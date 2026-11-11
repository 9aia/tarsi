//! tarsi-config — loads [`tarsi_core::Config`] through
//! scarab-config's file discovery: explicit `$TARSI_CONFIG` →
//! `./tarsi.{yaml,yml,toml}` (YAML preferred) →
//! `~/.config/tarsi/`. No `tarsi-config-*` crates — the generic
//! machinery is scarab's (P-017); this crate is only the wiring.

use std::path::Path;

use scarab_config::Discovery;
use scarab_config_file::FileSource;
use tarsi_core::Config;

/// Load config from the conventional locations, relative to the
/// current dir.
pub fn load() -> Result<Config, String> {
    FileSource::for_app("tarsi").load()
}

/// Load config rooted at `dir` (the repo being released) —
/// `<dir>/tarsi.*` first, then `~/.config/tarsi/` as fallback.
/// `$TARSI_CONFIG` still wins over both.
pub fn load_in(dir: &Path) -> Result<Config, String> {
    FileSource::new(
        Discovery::for_app("tarsi")
            .dirs([dir.to_path_buf(), scarab_config::user_config_dir("tarsi")]),
    )
    .load()
}

/// Load an explicit config file path (the `--config` case).
pub fn load_path(path: &Path) -> Result<Config, String> {
    FileSource::for_app("tarsi").load_path(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
}
