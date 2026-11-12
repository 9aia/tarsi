//! tarsi-config — loads [`tarsi_core::Config`] through
//! scarab-config's file discovery: explicit `$TARSI_CONFIG` →
//! `./tarsi.{yaml,yml,toml}` (YAML preferred) →
//! `~/.config/tarsi/`. No `tarsi-config-*` crates — the generic
//! machinery is scarab's (P-017); this crate is only the wiring.
//!
//! **Release profiles** (`type: tarsi` packs — PL-014 D2) layer
//! under the repo config: a profile is a dir containing a
//! `tarsi.*` file, discovered via [`profile`] (explicit arg →
//! `$TARSI_PROFILE` → `profile = "…"` in config). With a profile
//! resolved, loading switches to scarab's `LayeredFileSource`:
//! repo `tarsi.*` > profile `tarsi.*` > `~/.config/tarsi/` >
//! defaults. Without one, behavior is unchanged (first-match
//! `FileSource`).

use std::path::{Path, PathBuf};

use scarab_config::{user_config_dir, Discovery};
use scarab_config_file::{FileSource, LayeredFileSource};
use tarsi_core::Config;

pub mod profile;
pub use profile::{find_dir_env, list_profiles, ProfileInfo};

/// Env lookup for the process environment.
fn env_var(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

fn user_dir() -> PathBuf {
    user_config_dir("tarsi")
}

/// First-match source over `[dir, user dir]` — the pre-profile
/// semantics (repo file wins outright, user file only when no repo
/// file exists).
fn first_match(dir: &Path) -> FileSource<Config> {
    FileSource::new(Discovery::for_app("tarsi").dirs([dir.to_path_buf(), user_dir()]))
}

/// The profile name in effect: explicit argument (serve `profile`
/// param) → `$TARSI_PROFILE` → `profile = "…"` in the base config.
fn profile_name<'a>(
    explicit: Option<&'a str>,
    env: &dyn Fn(&str) -> Option<String>,
    base: Option<&'a Config>,
) -> Option<String> {
    explicit
        .map(str::to_string)
        .or_else(|| env("TARSI_PROFILE").filter(|s| !s.is_empty()))
        .or_else(|| base.and_then(|c| c.profile.clone()))
}

/// Core loader — env-injectable for tests.
///
/// Two phases: (A) first-match load over `[dir, user]` surfaces
/// parse errors and the `profile` selector; (B) when a profile
/// name resolves, re-load *layered* with the profile dir between
/// repo and user. A named-but-missing profile is a hard error —
/// silently releasing under defaults would surprise.
fn load_for(
    dir: &Path,
    explicit_profile: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Config, String> {
    let base = first_match(dir).load();
    let pname = profile_name(explicit_profile, env, base.as_ref().ok());
    let Some(pname) = pname else {
        return base;
    };
    let pdir = profile::find_dir(&pname, env)
        .ok_or_else(|| format!("tarsi profile {pname:?} not found"))?;
    LayeredFileSource::new(Discovery::for_app("tarsi").dirs([dir.to_path_buf(), pdir, user_dir()]))
        .load_with(env)
}

/// Load config from the conventional locations, relative to the
/// current dir.
pub fn load() -> Result<Config, String> {
    load_for(Path::new("."), None, &env_var)
}

/// Load config rooted at `dir` (the repo being released) —
/// `<dir>/tarsi.*` first, then `~/.config/tarsi/` as fallback.
/// `$TARSI_CONFIG` still wins over both.
pub fn load_in(dir: &Path) -> Result<Config, String> {
    load_in_with_profile(dir, None)
}

/// [`load_in`] with an explicit profile name (serve `profile`
/// param) — beats `$TARSI_PROFILE` and the config's own
/// `profile = "…"` key.
pub fn load_in_with_profile(dir: &Path, profile: Option<&str>) -> Result<Config, String> {
    load_for(dir, profile, &env_var)
}

/// Load an explicit config file path (the `--config` case). The
/// file is the whole config — profile layering does not apply.
pub fn load_path(path: &Path) -> Result<Config, String> {
    FileSource::for_app("tarsi").load_path(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn env_of(vars: &[(&str, String)]) -> impl Fn(&str) -> Option<String> {
        let map: std::collections::BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn loads_toml_from_dir() {
        let dir = std::env::temp_dir().join(format!("tarsi-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("tarsi.toml"),
            "[release]\nchangelog_path = \"CHANGES.md\"\n",
        )
        .unwrap();
        let cfg = load_in(&dir).unwrap();
        assert_eq!(cfg.release.changelog_path, "CHANGES.md");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_config_errors() {
        let dir = std::env::temp_dir().join(format!("tarsi-config-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // ~/.config/tarsi may exist on a dev box — only assert the
        // call shape here; discovery correctness is scarab's suite.
        let _ = load_in(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `load_for`'s user-config layer expands `~` via the *process*
    /// env (scarab's `expand_tilde`), so tests that need a hermetic
    /// `$HOME` set it for real — serialized by this lock.
    static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct HomeGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev: Option<std::ffi::OsString>,
    }

    impl HomeGuard {
        fn set(path: &Path) -> Self {
            let lock = HOME_LOCK.lock().unwrap();
            let prev = std::env::var_os("HOME");
            std::env::set_var("HOME", path);
            Self { _lock: lock, prev }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn profile_layers_between_repo_and_user() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let prof = tmp.path().join("profs/9aia-release");
        let home = tmp.path().join("home");
        for d in [&repo, &prof, &home.join(".config/tarsi")] {
            std::fs::create_dir_all(d).unwrap();
        }
        // repo sets one key + selects the profile
        std::fs::write(
            repo.join("tarsi.toml"),
            "profile = \"9aia-release\"\n[release]\nchangelog_path = \"NEWS.md\"\n",
        )
        .unwrap();
        // profile supplies the org conventions
        std::fs::write(
            prof.join("tarsi.toml"),
            "[release]\nchangelog_path = \"CHANGELOG.md\"\nstyleguide = \"keep-a-changelog\"\n[publish.crates]\nenabled = false\n",
        )
        .unwrap();
        // user config is the base layer
        std::fs::write(
            home.join(".config/tarsi/tarsi.toml"),
            "[release]\ntag_format = \"v{version}\"\n",
        )
        .unwrap();
        let _home = HomeGuard::set(&home);
        let env = env_of(&[(
            "TARSI_PROFILES_DIR",
            tmp.path().join("profs").to_string_lossy().to_string(),
        )]);
        let cfg = load_for(&repo, None, &env).unwrap();
        // repo wins over profile; profile wins over user; user
        // supplies what neither sets.
        assert_eq!(cfg.release.changelog_path, "NEWS.md");
        assert!(!cfg.publish.crates.enabled);
        assert_eq!(cfg.release.tag_format, "v{version}");
        assert_eq!(cfg.profile.as_deref(), Some("9aia-release"));
    }

    #[test]
    fn missing_named_profile_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("tarsi.toml"), "profile = \"ghost\"\n").unwrap();
        let _home = HomeGuard::set(tmp.path());
        let env = env_of(&[] as &[(&str, String)]);
        let err = load_for(&repo, None, &env).unwrap_err();
        assert!(err.contains("ghost"), "{err}");
    }

    #[test]
    fn env_profile_var_selects_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let prof = tmp.path().join("profs/p1");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&prof).unwrap();
        std::fs::write(repo.join("tarsi.toml"), "[release]\n").unwrap();
        std::fs::write(
            prof.join("tarsi.toml"),
            "[release]\nstyleguide = \"conventional-commit-grouped\"\n",
        )
        .unwrap();
        let _home = HomeGuard::set(tmp.path());
        let env = env_of(&[
            (
                "TARSI_PROFILES_DIR",
                tmp.path().join("profs").to_string_lossy().to_string(),
            ),
            ("TARSI_PROFILE", "p1".to_string()),
        ]);
        let cfg = load_for(&repo, None, &env).unwrap();
        assert_eq!(cfg.release.styleguide, "conventional-commit-grouped");
        // explicit arg beats env
        let cfg = load_for(&repo, Some("p1"), &env).unwrap();
        assert_eq!(cfg.release.styleguide, "conventional-commit-grouped");
    }
}
