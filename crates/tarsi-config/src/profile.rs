//! Release profiles — the `type: tarsi` content packs (PL-014 D2).
//!
//! A **profile** is a directory containing a `tarsi.{yaml,yml,toml}`
//! config file — the same document a repo's own `tarsi.toml` carries.
//! Three sources are searched, in order:
//!
//! 1. `$TARSI_PROFILES_DIR/<name>/` — an explicit profiles root.
//! 2. `~/.config/tarsi/profiles/<name>/` — the user profiles dir
//!    (`@scope/<name>` nests one level deeper for scoped packs).
//! 3. The **tamias store** — `type: tarsi` packs installed via
//!    `tamias install` land unpacked at
//!    `<store>/[@scope/]<name>@<label>/`; any entry carrying a
//!    `tarsi.*` file is a profile candidate. Store roots resolve
//!    like `tamias` itself: `$TAMIAS_HOME/store` →
//!    `$XDG_DATA_HOME/tamias/store` → `~/.local/share/tamias/store`
//!    → `./.tamias/store`.
//!
//! Selection precedence for a *name* is: explicit argument →
//! `$TARSI_PROFILE` → the `profile = "…"` key in the repo/user
//! config. When a profile resolves, loading becomes *layered*
//! (scarab `LayeredFileSource`): repo `tarsi.*` beats profile
//! `tarsi.*` beats `~/.config/tarsi/` beats built-in defaults —
//! objects merge key-wise, scalars/arrays replace.
//!
//! Profile pack layout (`type: tarsi` tamias manifest):
//!
//! ```text
//! tamias.yaml        # pack manifest — name/version/type: tarsi/
//!                    # description (the profile's human label)
//! tarsi.toml         # the profile — a plain Config document
//! styleguide.md      # optional extras the profile may bundle
//! ```
//!
//! The payload's `tarsi.*` must NOT carry a `[profile]` table —
//! `Config.profile` is the *selector* (`profile = "name"`, a
//! string), and a table there fails deserialization. Pack
//! metadata lives in `tamias.yaml` (`description`) or an optional
//! top-level `description` key in the `tarsi.*` file.

use std::path::{Path, PathBuf};

use scarab_config::{expand_tilde, user_config_dir, ConfigFormat};
use scarab_config_toml::TomlFormat;
use scarab_config_yaml::YamlFormat;
use serde_json::Value;

/// Env lookup — injectable so tests never touch the process env.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Config-file extensions a profile dir may carry, in the same
/// preference order `FileSource` uses (YAML before TOML).
const CONFIG_EXTS: [&str; 3] = ["yaml", "yml", "toml"];

/// One discovered profile — what `tarsi-serve profiles` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileInfo {
    /// Profile name — dir name for dir sources, `[@scope/]name`
    /// for store entries.
    pub name: String,
    /// Store version label (`0.2.0`, `latest`, …); `None` for
    /// dir-based profiles which have no versioning.
    pub version: Option<String>,
    /// Human description — `tamias.yaml`'s `description`, else a
    /// top-level `description` in the profile's `tarsi.*` file.
    pub description: Option<String>,
    /// The profile dir (the dir containing `tarsi.*`).
    pub source: PathBuf,
}

/// A resolved store entry: `[@scope/]name@label` → dir.
#[derive(Debug)]
struct StoreEntry {
    /// `name` or `@scope/name`.
    name: String,
    /// The `@label` suffix (version label or ref alias).
    label: String,
    dir: PathBuf,
}

/// The `tarsi.*` config file inside `dir`, if any (preference
/// order: yaml → yml → toml).
fn config_file_in(dir: &Path) -> Option<PathBuf> {
    CONFIG_EXTS
        .iter()
        .map(|e| dir.join(format!("tarsi.{e}")))
        .find(|p| p.is_file())
}

/// Parse a config/manifest file into a `serde_json::Value` by
/// extension — the same dispatch `LayeredFileSource` uses.
fn parse_value(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let ext = path.extension()?.to_str()?;
    let parsed = match ext {
        "yaml" | "yml" => YamlFormat.parse(&text),
        "toml" => TomlFormat.parse(&text),
        _ => return None,
    };
    parsed.ok()
}

/// `description` out of a dir's `tamias.yaml|yml` or `tarsi.*`
/// (manifest wins — it is the pack's declared label).
fn description_in(dir: &Path) -> Option<String> {
    for name in ["tamias.yaml", "tamias.yml"] {
        let p = dir.join(name);
        if p.is_file() {
            if let Some(d) = parse_value(&p).and_then(|v| {
                v.get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }) {
                return Some(d);
            }
        }
    }
    let p = config_file_in(dir)?;
    parse_value(&p).and_then(|v| {
        v.get("description")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

/// Profile names must be path-safe: no traversal, no absolute
/// paths, no NUL. `@scope/name` and plain names are fine.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains("..")
        && !name.contains('\0')
}

/// Candidate profile dirs from the two *directory* conventions —
/// `$TARSI_PROFILES_DIR` and `~/.config/tarsi/profiles/`. Each
/// child dir (one level; `@scope` nests one deeper) that carries a
/// `tarsi.*` file is a profile named by its dir name.
fn dir_profiles(env: Env<'_>) -> Vec<(String, PathBuf)> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(d) = env("TARSI_PROFILES_DIR") {
        if !d.is_empty() {
            roots.push(PathBuf::from(d));
        }
    }
    // `~/.config/tarsi/profiles/` — HOME comes from the injected
    // env so tests stay hermetic; `expand_tilde` is the fallback
    // (it reads the process env).
    let home = env("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env("USERPROFILE")
                .filter(|h| !h.is_empty())
                .map(PathBuf::from)
        });
    roots.push(match home {
        Some(h) => h.join(".config/tarsi/profiles"),
        None => expand_tilde(&user_config_dir("tarsi")).join("profiles"),
    });

    let mut out = Vec::new();
    for root in roots {
        let Ok(children) = std::fs::read_dir(&root) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            if !path.is_dir() {
                continue;
            }
            let fname = child.file_name().to_string_lossy().to_string();
            if fname.starts_with('@') {
                // Scoped pack dir: `@scope/<name>/tarsi.*`
                if let Ok(grand) = std::fs::read_dir(&path) {
                    for g in grand.flatten() {
                        let gpath = g.path();
                        if gpath.is_dir()
                            && config_file_in(&gpath).is_some()
                            && safe_name(&format!("{fname}/{}", g.file_name().to_string_lossy()))
                        {
                            out.push((
                                format!("{fname}/{}", g.file_name().to_string_lossy()),
                                gpath,
                            ));
                        }
                    }
                }
            } else if config_file_in(&path).is_some() && safe_name(&fname) {
                out.push((fname, path));
            }
        }
    }
    out
}

/// Tamias store roots, mirroring `tamias`'s own resolution
/// (`TAMIAS_HOME` → `XDG_DATA_HOME` → `~/.local/share` → cwd
/// fallback), deduplicated.
fn store_roots(env: Env<'_>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(h) = env("TAMIAS_HOME") {
        if !h.is_empty() {
            roots.push(PathBuf::from(h).join("store"));
        }
    }
    if let Some(x) = env("XDG_DATA_HOME") {
        if !x.is_empty() {
            roots.push(PathBuf::from(x).join("tamias/store"));
        }
    }
    if let Some(h) = env("HOME") {
        if !h.is_empty() {
            roots.push(PathBuf::from(h).join(".local/share/tamias/store"));
        }
    }
    roots.push(PathBuf::from(".tamias/store"));
    roots.dedup();
    roots
}

/// Every store entry carrying a `tarsi.*` file — the installed
/// `type: tarsi` packs. `[@scope/]name@label` dir names parse into
/// (name, label); entries without a config file are skipped.
fn store_profiles(env: Env<'_>) -> Vec<StoreEntry> {
    let mut out = Vec::new();
    for root in store_roots(env) {
        if !root.is_dir() {
            continue;
        }
        let Ok(children) = std::fs::read_dir(&root) else {
            continue;
        };
        for child in children.flatten() {
            let path = child.path();
            let fname = child.file_name().to_string_lossy().to_string();
            if !path.is_dir() {
                continue;
            }
            if let Some(scope) = fname.strip_prefix('@') {
                // Scope dir — entries one level deeper.
                let Ok(grand) = std::fs::read_dir(&path) else {
                    continue;
                };
                for g in grand.flatten() {
                    let gpath = g.path();
                    if let Some(e) =
                        parse_entry(&gpath, &g.file_name().to_string_lossy(), Some(scope))
                    {
                        out.push(e);
                    }
                }
            } else if let Some(e) = parse_entry(&path, &fname, None) {
                out.push(e);
            }
        }
    }
    out
}

/// `<name>@<label>` dir → StoreEntry, when it carries `tarsi.*`.
fn parse_entry(dir: &Path, fname: &str, scope: Option<&str>) -> Option<StoreEntry> {
    let (pkg, label) = fname.rsplit_once('@')?;
    if pkg.is_empty() || label.is_empty() || config_file_in(dir).is_none() {
        return None;
    }
    let name = match scope {
        Some(s) => format!("@{s}/{pkg}"),
        None => pkg.to_string(),
    };
    Some(StoreEntry {
        name,
        label: label.to_string(),
        dir: dir.to_path_buf(),
    })
}

/// Does store/dir profile `entry_name` satisfy requested `name`?
/// `name` may be `pkg`, `scope/pkg`, or `@scope/pkg`; a bare `pkg`
/// request matches scoped entries too (unscoped preferred by the
/// caller's sort).
fn name_matches(req: &str, entry: &str) -> bool {
    if req == entry {
        return true;
    }
    let bare_req = req.strip_prefix('@').unwrap_or(req);
    let (esc, epkg) = match entry.strip_prefix('@') {
        Some(rest) => match rest.split_once('/') {
            Some((s, p)) => (Some(s), p),
            None => (None, rest),
        },
        None => (None, entry),
    };
    match bare_req.split_once('/') {
        Some((s, p)) => esc == Some(s) && p == epkg,
        None => epkg == bare_req,
    }
}

/// Version-label rank for picking the newest installed profile:
/// semver labels outrank non-semver (`latest`, branch names),
/// higher semver first, then lexicographic for the rest.
fn label_rank(label: &str) -> (u8, semver::Version, String) {
    match semver::Version::parse(label) {
        Ok(v) => (1, v, label.to_string()),
        Err(_) => (0, semver::Version::new(0, 0, 0), label.to_string()),
    }
}

/// Resolve a profile *name* to its dir. Directory sources win over
/// the store; within the store the newest version label wins.
/// `None` = not found.
pub fn find_dir(name: &str, env: Env<'_>) -> Option<PathBuf> {
    if !safe_name(name) {
        return None;
    }
    for (n, dir) in dir_profiles(env) {
        if n == name && config_file_in(&dir).is_some() {
            return Some(dir);
        }
    }
    let entries = store_profiles(env);
    let mut matches: Vec<&StoreEntry> = entries
        .iter()
        .filter(|e| name_matches(name, &e.name))
        .collect();
    // Exact-name matches first, then unscoped before scoped, then
    // newest label.
    matches.sort_by(|a, b| {
        let exact = |e: &&StoreEntry| usize::from(e.name == name);
        exact(b)
            .cmp(&exact(a))
            .then_with(|| {
                usize::from(a.name.starts_with('@')).cmp(&usize::from(b.name.starts_with('@')))
            })
            .then_with(|| label_rank(&b.label).cmp(&label_rank(&a.label)))
            .then_with(|| a.dir.cmp(&b.dir))
    });
    matches.first().map(|e| e.dir.clone())
}

/// Every installed/available profile, sorted by name then version.
pub fn list(env: Env<'_>) -> Vec<ProfileInfo> {
    let mut out: Vec<ProfileInfo> = dir_profiles(env)
        .into_iter()
        .map(|(name, dir)| ProfileInfo {
            description: description_in(&dir),
            name,
            version: None,
            source: dir,
        })
        .collect();
    out.extend(store_profiles(env).into_iter().map(|e| ProfileInfo {
        description: description_in(&e.dir),
        name: e.name,
        version: Some(e.label),
        source: e.dir,
    }));
    out.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.version.cmp(&b.version))
            .then_with(|| a.source.cmp(&b.source))
    });
    out
}

/// Process-env wrappers.
pub fn find_dir_env(name: &str) -> Option<PathBuf> {
    find_dir(name, &|k| std::env::var(k).ok())
}

/// Every profile visible to this process.
pub fn list_profiles() -> Vec<ProfileInfo> {
    list(&|k| std::env::var(k).ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn write_profile(dir: &Path, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("tarsi.toml"), body).unwrap();
    }

    #[test]
    fn finds_profile_under_tarsi_profiles_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let pd = tmp.path().join("profs/9aia-release");
        write_profile(&pd, "[release]\nstyleguide = \"keep-a-changelog\"\n");
        let env = env_of(&[
            (
                "TARSI_PROFILES_DIR",
                tmp.path().join("profs").to_str().unwrap(),
            ),
            ("HOME", tmp.path().to_str().unwrap()),
        ]);
        assert_eq!(find_dir("9aia-release", &env), Some(pd));
        assert_eq!(find_dir("nope", &env), None);
    }

    #[test]
    fn finds_profile_in_tamias_store_best_version() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        for label in ["0.1.0", "0.2.0"] {
            let d = store.join(format!("9aia-release@{label}"));
            write_profile(&d, "[release]\n");
            std::fs::write(
                d.join("tamias.yaml"),
                "name: 9aia-release\ntype: tarsi\nversion: \"1.0.0\"\n",
            )
            .unwrap();
        }
        let env = env_of(&[
            ("TAMIAS_HOME", tmp.path().to_str().unwrap()),
            ("HOME", tmp.path().to_str().unwrap()),
        ]);
        assert_eq!(
            find_dir("9aia-release", &env),
            Some(store.join("9aia-release@0.2.0"))
        );
        // Scoped entry: @scope/name@label
        let scoped = store.join("@acme/9aia-release@9.9.9");
        write_profile(&scoped, "[release]\n");
        assert_eq!(find_dir("@acme/9aia-release", &env), Some(scoped.clone()));
        // Bare name still resolves — unscoped preferred over scoped.
        assert_eq!(
            find_dir("9aia-release", &env),
            Some(store.join("9aia-release@0.2.0"))
        );
        let infos = list(&env);
        assert!(infos.iter().any(|i| i.name == "@acme/9aia-release"));
    }

    #[test]
    fn rejects_unsafe_names() {
        let tmp = tempfile::tempdir().unwrap();
        let env = env_of(&[
            ("TARSI_PROFILES_DIR", tmp.path().to_str().unwrap()),
            ("HOME", tmp.path().to_str().unwrap()),
        ]);
        assert_eq!(find_dir("../escape", &env), None);
        assert_eq!(find_dir("/abs", &env), None);
        assert_eq!(find_dir("", &env), None);
    }

    #[test]
    fn description_prefers_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("p");
        write_profile(&d, "description = \"from tarsi.toml\"\n");
        std::fs::write(
            d.join("tamias.yaml"),
            "name: p\ntype: tarsi\ndescription: from manifest\n",
        )
        .unwrap();
        assert_eq!(description_in(&d).as_deref(), Some("from manifest"));
    }
}
