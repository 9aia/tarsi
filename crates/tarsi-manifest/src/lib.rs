//! tarsi-manifest — the filesystem edge of unit resolution.
//! Reads `Cargo.toml` (workspace members, package versions,
//! path dependencies) and produces [`ReleaseUnitSpec`]s for
//! [`tarsi_core::plan_release`]. Also owns manifest version
//! writes (`cargo set-version` equivalent) via `toml_edit` so
//! formatting/comments survive.
//!
//! Pure parsing lives here; this is the only tarsi crate allowed
//! to touch `Cargo.toml` files.

use std::path::{Path, PathBuf};

use semver::Version;
use tarsi_core::{PublishTarget, ReleaseUnitSpec};

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}: toml: {1}")]
    Toml(PathBuf, String),
    #[error("{0}: no [package] name/version")]
    NoPackage(PathBuf),
    #[error("workspace member glob {0:?} matched nothing")]
    EmptyGlob(String),
}

fn read_toml(path: &Path) -> Result<toml::Table, ManifestError> {
    let text =
        std::fs::read_to_string(path).map_err(|e| ManifestError::Io(path.to_path_buf(), e))?;
    toml::from_str(&text).map_err(|e| ManifestError::Toml(path.to_path_buf(), e.to_string()))
}

fn err_io(p: &Path, e: std::io::Error) -> ManifestError {
    ManifestError::Io(p.to_path_buf(), e)
}

/// Workspace members declared in `<root>/Cargo.toml`
/// (`workspace.members`, default `["."]`… actually cargo requires
/// members; empty → root package only).
fn member_globs(root: &Path) -> Result<Vec<String>, ManifestError> {
    let manifest = root.join("Cargo.toml");
    let table = read_toml(&manifest)?;
    let Some(workspace) = table.get("workspace").and_then(|w| w.as_table()) else {
        // Single-package repo — the root IS the unit.
        return Ok(vec![".".to_string()]);
    };
    let Some(members) = workspace.get("members").and_then(|m| m.as_array()) else {
        return Ok(vec![".".to_string()]);
    };
    Ok(members
        .iter()
        .filter_map(|m| m.as_str().map(str::to_string))
        .collect())
}

/// Expand member globs (`crates/*`, `crates/foo`) to concrete dirs.
fn expand_members(root: &Path, globs: &[String]) -> Result<Vec<PathBuf>, ManifestError> {
    let mut dirs = Vec::new();
    for g in globs {
        if g.contains('*') {
            let (base, _) = g.split_once('*').unwrap_or((g, ""));
            let base_path = root.join(base.trim_end_matches('/'));
            let rd = std::fs::read_dir(&base_path).map_err(|e| err_io(&base_path, e))?;
            let mut any = false;
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_dir() && p.join("Cargo.toml").exists() {
                    any = true;
                    dirs.push(p);
                }
            }
            if !any {
                return Err(ManifestError::EmptyGlob(g.clone()));
            }
        } else {
            let p = root.join(g);
            if p.join("Cargo.toml").exists() {
                dirs.push(p);
            }
        }
    }
    dirs.sort();
    dirs.dedup();
    Ok(dirs)
}

struct MemberInfo {
    name: String,
    version: Version,
    /// Version resolves from `[workspace.package].version`
    /// (`version.workspace = true`, or the inherited fallback) —
    /// such units share one version file and must release in
    /// lockstep (I-002). Literal per-package versions are
    /// independent.
    version_shared: bool,
    publishable: bool,
    /// Names of sibling members this one path-depends on.
    path_deps: Vec<String>,
}

fn read_member(dir: &Path, ws_version: Option<&Version>) -> Result<MemberInfo, ManifestError> {
    let manifest = dir.join("Cargo.toml");
    let table = read_toml(&manifest)?;
    let package = table
        .get("package")
        .and_then(|p| p.as_table())
        .ok_or_else(|| ManifestError::NoPackage(manifest.clone()))?;
    let name = package
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| ManifestError::NoPackage(manifest.clone()))?
        .to_string();
    // `version.workspace = true` inherits workspace.package.version.
    // Anything that isn't a literal string (the inherit table, or
    // a missing key falling back to the workspace version) shares
    // the workspace version file → `version_shared`.
    let version_item = package.get("version");
    let version_shared = !version_item.is_some_and(|v| v.is_str());
    let version = match version_item {
        Some(v) if v.is_str() => Version::parse(v.as_str().unwrap_or("0.0.0"))
            .map_err(|e| ManifestError::Toml(manifest.clone(), e.to_string()))?,
        Some(v) if v.is_table() => ws_version
            .cloned()
            .ok_or_else(|| ManifestError::NoPackage(manifest.clone()))?,
        _ => ws_version
            .cloned()
            .ok_or_else(|| ManifestError::NoPackage(manifest.clone()))?,
    };
    let publishable = package
        .get("publish")
        .and_then(|p| p.as_bool())
        .unwrap_or(true);
    // Path deps → sibling unit names. Deps can rename the package
    // (`foo = { package = "real-name", path = "../x" }`).
    let mut path_deps = Vec::new();
    for section in ["dependencies", "build-dependencies"] {
        if let Some(deps) = table.get(section).and_then(|d| d.as_table()) {
            for (dep_name, spec) in deps {
                // Only table specs can carry `path = "…"`.
                let Some(t) = spec.as_table() else { continue };
                if t.get("path").is_none() {
                    continue;
                }
                let real = t
                    .get("package")
                    .and_then(|p| p.as_str())
                    .unwrap_or(dep_name);
                path_deps.push(real.to_string());
            }
        }
    }
    // Workspace-inherited deps (`workspace = true`) — resolve names
    // from [workspace.dependencies] path entries.
    if let Some(ws_deps) = table
        .get("workspace")
        .and_then(|w| w.as_table())
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
    {
        for section in ["dependencies", "build-dependencies"] {
            if let Some(deps) = table.get(section).and_then(|d| d.as_table()) {
                for (dep_name, spec) in deps {
                    if spec
                        .as_table()
                        .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
                    {
                        if let Some(real) = ws_deps
                            .get(dep_name)
                            .and_then(|d| d.as_table())
                            .filter(|t| t.get("path").is_some())
                            .and_then(|t| t.get("package"))
                            .and_then(|p| p.as_str())
                            .or(Some(dep_name.as_str()))
                        {
                            path_deps.push(real.to_string());
                        }
                    }
                }
            }
        }
    }
    path_deps.sort();
    path_deps.dedup();
    Ok(MemberInfo {
        name,
        version,
        version_shared,
        publishable,
        path_deps,
    })
}

/// Scan `<root>` (a cargo workspace or single package) into
/// [`ReleaseUnitSpec`]s — versions, paths, dep topology, and
/// publish targets (`publish = false` manifests → no targets).
///
/// `tag_format`/`bump`/`publish` come from config overrides
/// applied by the caller ([`apply_config`]).
pub fn cargo_workspace_units(root: &Path) -> Result<Vec<ReleaseUnitSpec>, ManifestError> {
    let root = root.canonicalize().map_err(|e| err_io(root, e))?;
    let globs = member_globs(&root)?;
    let dirs = expand_members(&root, &globs)?;
    // workspace.package.version for `version.workspace = true`.
    let ws_version = read_toml(&root.join("Cargo.toml")).ok().and_then(|t| {
        t.get("workspace")
            .and_then(|w| w.as_table())
            .and_then(|w| w.get("package"))
            .and_then(|p| p.as_table())
            .and_then(|p| p.get("version"))
            .and_then(|v| v.as_str())
            .and_then(|s| Version::parse(s).ok())
    });
    let mut members = Vec::new();
    for dir in &dirs {
        members.push((dir.clone(), read_member(dir, ws_version.as_ref())?));
    }
    // Map package name → spec; path deps become depends_on filtered
    // to known members.
    let names: std::collections::BTreeSet<String> =
        members.iter().map(|(_, m)| m.name.clone()).collect();
    let mut specs = Vec::with_capacity(members.len());
    for (dir, m) in members {
        let rel = dir
            .strip_prefix(&root)
            .unwrap_or(&dir)
            .to_string_lossy()
            .replace('\\', "/");
        let path = if rel.is_empty() { ".".to_string() } else { rel };
        specs.push(ReleaseUnitSpec {
            name: m.name.clone(),
            path,
            current_version: m.version,
            tag_format: "{unit}-v{version}".to_string(),
            bump: None,
            publish: if m.publishable {
                vec![PublishTarget::CratesIo, PublishTarget::GithubRelease]
            } else {
                Vec::new()
            },
            depends_on: m
                .path_deps
                .into_iter()
                .filter(|d| names.contains(d))
                .collect(),
            version_shared: m.version_shared,
        });
    }
    if specs.is_empty() {
        return Err(ManifestError::EmptyGlob("*".to_string()));
    }
    Ok(specs)
}

/// Apply tarsi `Config` onto resolved specs — tag format default,
/// per-unit overrides (bump/tag/publish/skip).
pub fn apply_config(specs: Vec<ReleaseUnitSpec>, cfg: &tarsi_core::Config) -> Vec<ReleaseUnitSpec> {
    tarsi_core::config::apply_overrides(specs, cfg)
}

/// Write `version` into `<manifest>/Cargo.toml` `[package]`,
/// preserving the rest of the file (toml_edit). Returns true when
/// the file changed.
pub fn write_version(manifest: &Path, version: &Version) -> Result<bool, ManifestError> {
    let path = manifest.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| err_io(&path, e))?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ManifestError::Toml(path.clone(), e.to_string()))?;
    let pkg = doc
        .get_mut("package")
        .and_then(|p| p.as_table_mut())
        .ok_or_else(|| ManifestError::NoPackage(path.clone()))?;
    let item = pkg
        .get_mut("version")
        .ok_or_else(|| ManifestError::NoPackage(path.clone()))?;
    // `version.workspace = true` can't be written per-package —
    // caller must handle workspace-versioned packages separately.
    if item.is_str() {
        let new = version.to_string();
        if item.as_str() == Some(new.as_str()) {
            return Ok(false);
        }
        *item = toml_edit::value(new);
        std::fs::write(&path, doc.to_string()).map_err(|e| err_io(&path, e))?;
        return Ok(true);
    }
    Err(ManifestError::Toml(
        path,
        "version is workspace-inherited; bump workspace.package.version instead".to_string(),
    ))
}

/// Write `version` into `[workspace.package]` when members inherit
/// it — used when every releasing unit shares one version.
pub fn write_workspace_version(root: &Path, version: &Version) -> Result<bool, ManifestError> {
    let path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| err_io(&path, e))?;
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| ManifestError::Toml(path.clone(), e.to_string()))?;
    let Some(pkg) = doc
        .get_mut("workspace")
        .and_then(|w| w.as_table_mut())
        .and_then(|w| w.get_mut("package"))
        .and_then(|p| p.as_table_mut())
    else {
        return Ok(false);
    };
    let Some(item) = pkg.get_mut("version") else {
        return Ok(false);
    };
    if !item.is_str() {
        return Ok(false);
    }
    let new = version.to_string();
    if item.as_str() == Some(new.as_str()) {
        return Ok(false);
    }
    *item = toml_edit::value(new);
    std::fs::write(&path, doc.to_string()).map_err(|e| err_io(&path, e))?;
    Ok(true)
}

/// Per-unit version write that handles `version.workspace = true`:
/// if the member inherits, bump the workspace version instead
/// (only correct when all releasing units share it — callers check).
pub fn write_unit_version(
    root: &Path,
    unit_path: &str,
    version: &Version,
) -> Result<bool, ManifestError> {
    let manifest = root.join(unit_path);
    let path = manifest.join("Cargo.toml");
    let text = std::fs::read_to_string(&path).map_err(|e| err_io(&path, e))?;
    let doc: toml::Table =
        toml::from_str(&text).map_err(|e| ManifestError::Toml(path.clone(), e.to_string()))?;
    let inherits = doc
        .get("package")
        .and_then(|p| p.as_table())
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_table())
        .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)));
    if inherits {
        return write_workspace_version(root, version);
    }
    write_version(&manifest, version)
}

/// Navigate `doc` along `path` of table keys to a [`TableLike`].
fn table_at<'a>(
    doc: &'a mut toml_edit::DocumentMut,
    path: &[&str],
) -> Option<&'a mut dyn toml_edit::TableLike> {
    let mut item = doc.as_item_mut();
    for key in path {
        item = item.as_table_like_mut()?.get_mut(key)?;
    }
    item.as_table_like_mut()
}

/// The version a dep's manifest currently declares — literal
/// `[package].version`, or `[workspace.package].version` when the
/// package inherits (`version.workspace = true`).
fn dep_manifest_version(manifest: &Path, ws_version: Option<&str>) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let doc: toml::Table = toml::from_str(&text).ok()?;
    let v = doc.get("package")?.get("version")?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if v.as_table()
        .is_some_and(|t| t.get("workspace") == Some(&toml::Value::Boolean(true)))
    {
        return ws_version.map(str::to_string);
    }
    None
}

/// Rewrite `version` reqs on path deps so each names the target
/// package's *current* manifest version. `cargo publish` ships path
/// deps with their `version` req — after a bump the req must match
/// the new version (`^0.1.5` rejects `0.2.0`, and cargo's own
/// resolver fails the same way on `cargo check`). Run after
/// [`write_unit_version`] writes land. Covers
/// `[workspace.dependencies]` in the root manifest plus
/// `dependencies`/`build-dependencies`/`dev-dependencies` in every
/// workspace member; only entries carrying both `path` and a string
/// `version` are touched. Returns true when any file changed.
pub fn sync_path_dep_reqs(root: &Path) -> Result<bool, ManifestError> {
    let root = root.canonicalize().map_err(|e| err_io(root, e))?;
    let ws_version = read_toml(&root.join("Cargo.toml")).ok().and_then(|t| {
        t.get("workspace")?
            .get("package")?
            .get("version")?
            .as_str()
            .map(str::to_string)
    });
    let mut manifests = vec![(root.join("Cargo.toml"), true)];
    let globs = member_globs(&root)?;
    for dir in expand_members(&root, &globs)? {
        manifests.push((dir.join("Cargo.toml"), false));
    }
    let mut changed = false;
    for (manifest, is_root) in manifests {
        if !manifest.exists() {
            continue;
        }
        let Some(base) = manifest.parent() else {
            continue;
        };
        let sections: &[&[&str]] = if is_root {
            &[&["workspace", "dependencies"]]
        } else {
            &[
                &["dependencies"],
                &["build-dependencies"],
                &["dev-dependencies"],
            ]
        };
        let text = std::fs::read_to_string(&manifest).map_err(|e| err_io(&manifest, e))?;
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| ManifestError::Toml(manifest.clone(), e.to_string()))?;
        let mut file_changed = false;
        for section in sections {
            let Some(table) = table_at(&mut doc, section) else {
                continue;
            };
            let entries: Vec<(String, String, String, String)> = table
                .iter()
                .filter_map(|(key, item)| {
                    let t = item.as_table_like()?;
                    let path = t.get("path")?.as_str()?.to_string();
                    let req = t.get("version")?.as_str()?.to_string();
                    let name = t
                        .get("package")
                        .and_then(|p| p.as_str())
                        .unwrap_or(key)
                        .to_string();
                    Some((key.to_string(), path, name, req))
                })
                .collect();
            for (key, dep_path, _name, req) in entries {
                let dep_manifest = base.join(&dep_path).join("Cargo.toml");
                let Some(actual) = dep_manifest_version(&dep_manifest, ws_version.as_deref())
                else {
                    continue;
                };
                if actual == req {
                    continue;
                }
                let Some(spec) = table
                    .get_mut(&key)
                    .and_then(|i| i.as_table_like_mut())
                    .and_then(|t| t.get_mut("version"))
                else {
                    continue;
                };
                *spec = toml_edit::value(actual);
                file_changed = true;
            }
        }
        if file_changed {
            std::fs::write(&manifest, doc.to_string()).map_err(|e| err_io(&manifest, e))?;
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.2.3"
"#,
        )
        .unwrap();
        for (name, deps) in [
            ("core", ""),
            ("cli", "core = { path = \"../core\" }"),
            ("internal", ""),
        ] {
            let dir = root.join("crates").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("Cargo.toml"),
                format!(
                    "[package]\nname = \"{name}\"\nversion.workspace = true\n\n[dependencies]\n{deps}\n"
                ),
            )
            .unwrap();
        }
        // internal is publish=false
        std::fs::write(
            root.join("crates/internal/Cargo.toml"),
            "[package]\nname = \"internal\"\nversion.workspace = true\npublish = false\n",
        )
        .unwrap();
        // literal pins its own version — NOT workspace-shared
        let dir = root.join("crates/literal");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"literal\"\nversion = \"9.9.9\"\n",
        )
        .unwrap();
        d
    }

    #[test]
    fn workspace_members_become_specs() {
        let d = fixture();
        let specs = cargo_workspace_units(d.path()).unwrap();
        assert_eq!(specs.len(), 4);
        let cli = specs.iter().find(|s| s.name == "cli").unwrap();
        assert_eq!(cli.depends_on, vec!["core"]);
        assert_eq!(cli.current_version.to_string(), "1.2.3");
        assert_eq!(cli.path, "crates/cli");
        assert_eq!(cli.publish.len(), 2);
        // version.workspace = true → shared version file →
        // lockstep release group (I-002). publish=false doesn't
        // change that — the manifest version still moves.
        assert!(cli.version_shared);
        let internal = specs.iter().find(|s| s.name == "internal").unwrap();
        assert!(internal.publish.is_empty());
        assert!(internal.version_shared);
        // Literal version → independent versioning.
        let literal = specs.iter().find(|s| s.name == "literal").unwrap();
        assert!(!literal.version_shared);
        assert_eq!(literal.current_version.to_string(), "9.9.9");
    }

    #[test]
    fn write_version_round_trip() {
        let d = fixture();
        let cli = d.path().join("crates/cli");
        // version.workspace = true → write_workspace_version path
        let changed =
            write_unit_version(d.path(), "crates/cli", &Version::parse("2.0.0").unwrap()).unwrap();
        assert!(changed);
        // workspace version now 2.0.0 for all inheriting members
        // (the literal-version member keeps its own version)
        let specs = cargo_workspace_units(d.path()).unwrap();
        assert!(specs
            .iter()
            .filter(|s| s.version_shared)
            .all(|s| s.current_version.to_string() == "2.0.0"));
        assert_eq!(
            specs
                .iter()
                .find(|s| s.name == "literal")
                .unwrap()
                .current_version
                .to_string(),
            "9.9.9"
        );
        let _ = cli;
    }

    #[test]
    fn sync_path_dep_reqs_tracks_bumped_versions() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        std::fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.2.3"

[workspace.dependencies]
core = { path = "crates/core", version = "1.2.3" }
"#,
        )
        .unwrap();
        for name in ["core", "cli"] {
            let dir = root.join("crates").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("Cargo.toml"),
                if name == "cli" {
                    "[package]\nname = \"cli\"\nversion.workspace = true\n\n[dependencies]\ncore.workspace = true\n"
                } else {
                    "[package]\nname = \"core\"\nversion.workspace = true\n"
                },
            )
            .unwrap();
        }
        // Bump the workspace version, then sync — the req must follow.
        write_unit_version(root, "crates/cli", &Version::parse("2.0.0").unwrap()).unwrap();
        assert!(sync_path_dep_reqs(root).unwrap());
        let text = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
        assert!(
            text.contains("core = { path = \"crates/core\", version = \"2.0.0\" }"),
            "{text}"
        );
        // Idempotent — second run changes nothing.
        assert!(!sync_path_dep_reqs(root).unwrap());
    }
}
