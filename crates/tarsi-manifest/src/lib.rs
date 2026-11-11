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
    let version = match package.get("version") {
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
        d
    }

    #[test]
    fn workspace_members_become_specs() {
        let d = fixture();
        let specs = cargo_workspace_units(d.path()).unwrap();
        assert_eq!(specs.len(), 3);
        let cli = specs.iter().find(|s| s.name == "cli").unwrap();
        assert_eq!(cli.depends_on, vec!["core"]);
        assert_eq!(cli.current_version.to_string(), "1.2.3");
        assert_eq!(cli.path, "crates/cli");
        assert_eq!(cli.publish.len(), 2);
        let internal = specs.iter().find(|s| s.name == "internal").unwrap();
        assert!(internal.publish.is_empty());
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
        let specs = cargo_workspace_units(d.path()).unwrap();
        assert!(specs
            .iter()
            .all(|s| s.current_version.to_string() == "2.0.0"));
        let _ = cli;
    }
}
