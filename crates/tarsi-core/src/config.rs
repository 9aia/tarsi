//! The `tarsi.toml`/`tarsi.yml` config model — pure serde types.
//! Loading/discovery lives in scarab-config (`FileSource`); this
//! module only defines what the file *means*. Secret **names**
//! appear here (`token_env`); values never do — cuora boundary.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bump::Bump;
use crate::unit::{PublishTarget, ReleaseUnitSpec, UnitsSpec};

/// Top-level config — `tarsi.toml` or `tarsi.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Named release profile to layer *under* this config — a
    /// `type: tarsi` tamias pack or a dir under
    /// `~/.config/tarsi/profiles/`. Resolved by `tarsi-config`
    /// (profile dir becomes a config layer between the repo file
    /// and the user file); ignored by planning itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub release: ReleaseConfig,
    #[serde(default)]
    pub publish: PublishConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseConfig {
    /// How release units resolve. Default: cargo workspace at root.
    #[serde(default)]
    pub units: Option<UnitsSpec>,
    /// Default tag template for units that don't override it.
    #[serde(default = "default_tag_format")]
    pub tag_format: String,
    /// Changelog file spliced per unit — resolved relative to each
    /// unit's path (`crates/foo` + `CHANGES.md` →
    /// `crates/foo/CHANGES.md`).
    #[serde(default = "default_changelog_path")]
    pub changelog_path: String,
    /// keep-a-changelog | conventional-commit-grouped | …
    #[serde(default = "default_styleguide")]
    pub styleguide: String,
    /// Repo-wide default bump when inference finds nothing.
    #[serde(default)]
    pub default_bump: Option<Bump>,
    /// Path globs excluded from commit→unit attribution
    /// (e.g. `["docs/**", "*.md"]`).
    #[serde(default)]
    pub diff_exclude: Vec<String>,
    /// Per-unit overrides keyed by unit name.
    #[serde(default)]
    pub unit_overrides: BTreeMap<String, UnitOverride>,
    /// Pack-ref/doc version sync — rewrite stale `name@old` /
    /// manifest `version:` refs in repo files *before* the release
    /// commit lands, so the tagged tree is self-consistent (T-039;
    /// replaces the per-repo `scripts/release.sh` pre-step). Empty
    /// `rules` = disabled — repos opt in.
    #[serde(default)]
    pub sync_refs: SyncRefsConfig,
}

impl Default for ReleaseConfig {
    fn default() -> Self {
        Self {
            units: None,
            tag_format: default_tag_format(),
            changelog_path: default_changelog_path(),
            styleguide: default_styleguide(),
            default_bump: None,
            diff_exclude: Vec::new(),
            unit_overrides: BTreeMap::new(),
            sync_refs: SyncRefsConfig::default(),
        }
    }
}

fn default_tag_format() -> String {
    crate::unit::DEFAULT_TAG_FORMAT.to_string()
}
fn default_changelog_path() -> String {
    "CHANGELOG.md".to_string()
}
fn default_styleguide() -> String {
    "keep-a-changelog".to_string()
}

/// `[release.sync_refs]` — the pack-ref sync stage config. Runs
/// inside `bump`/`release` after manifests are bumped and before
/// the release commit, so synced refs ride the tagged tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRefsConfig {
    /// Rewrite rules, applied in order. Empty = the stage is
    /// disabled entirely (no files touched, no step reported).
    #[serde(default)]
    pub rules: Vec<SyncRefRule>,
}

/// One sync rewrite: every regex match of `find` in each matched
/// file is replaced by the literal `replace`.
///
/// `{old}`/`{new}` placeholders expand to the releasing units'
/// shared current/new versions — `{old}`/`{new}` inside `find`
/// are regex-escaped before compilation (so `.` stays literal),
/// inside `replace` they expand literally. Patterns are not
/// auto-bounded — `app@{old}` still matches the `app@1.2.3`
/// prefix of `app@1.2.30`, exactly like the old `sed` pre-steps;
/// add `\b` (or similar) when a boundary is needed. Line anchors
/// need the regex multi-line flag: `(?m)^version: "{old}"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRefRule {
    /// Repo-root-relative globs of files to rewrite
    /// (`"libraries/myco/tamias.yaml"`, `"README.md"`,
    /// `".agents/skills/myco/SKILL.md"`). Non-matching globs are
    /// skipped — a repo can carry rules for files it doesn't have.
    #[serde(default)]
    pub files: Vec<String>,
    /// Regex matching the stale ref. Required.
    pub find: String,
    /// Literal replacement text. Required — an empty string would
    /// silently delete refs, so serde rejects a missing key.
    pub replace: String,
}

/// Per-unit config overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitOverride {
    #[serde(default)]
    pub bump: Option<Bump>,
    #[serde(default)]
    pub tag_format: Option<String>,
    #[serde(default)]
    pub publish: Option<Vec<PublishTarget>>,
    /// Skip this unit entirely (e.g. a lib that versions with its
    /// parent).
    #[serde(default)]
    pub skip: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishConfig {
    #[serde(default)]
    pub crates: CratesPublish,
    #[serde(default)]
    pub ghreleases: GhReleasePublish,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CratesPublish {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Retry crates.io 429s with backoff (release.sh semantics).
    #[serde(default = "default_true")]
    pub retry_429: bool,
    /// Max publish attempts per unit.
    #[serde(default = "default_attempts")]
    pub max_attempts: u32,
}

impl Default for CratesPublish {
    fn default() -> Self {
        Self {
            enabled: true,
            retry_429: true,
            max_attempts: default_attempts(),
        }
    }
}

fn default_attempts() -> u32 {
    5
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhReleasePublish {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Env var NAME holding the GitHub token — resolved via
    /// environment/cuora at publish time; the value never appears
    /// in config, manifests, or logs.
    #[serde(default = "default_token_env")]
    pub token_env: String,
    /// Repo override `owner/name`; default = git remote.
    #[serde(default)]
    pub repo: Option<String>,
    /// Repo-root-relative glob patterns for release assets —
    /// `{unit}` and `{version}` expand per releasing unit, e.g.
    /// `["dist/{unit}-{version}-*", "{unit}/checksums.txt"]`.
    /// A pattern with no glob magic must match an existing file;
    /// magic patterns matching nothing are skipped. Empty =
    /// notes-only releases.
    #[serde(default)]
    pub assets: Vec<String>,
}

impl Default for GhReleasePublish {
    fn default() -> Self {
        Self {
            enabled: true,
            token_env: default_token_env(),
            repo: None,
            assets: Vec::new(),
        }
    }
}

fn default_token_env() -> String {
    "GITHUB_TOKEN".to_string()
}
fn default_true() -> bool {
    true
}

/// Merge config `unit_overrides` onto resolved specs — produces
/// the final specs `plan_release` consumes. Pure; manifest
/// resolution stays with the caller.
#[must_use]
pub fn apply_overrides(mut specs: Vec<ReleaseUnitSpec>, cfg: &Config) -> Vec<ReleaseUnitSpec> {
    specs.retain(|s| {
        !cfg.release
            .unit_overrides
            .get(&s.name)
            .is_some_and(|o| o.skip)
    });
    for s in &mut specs {
        // Repo/profile-level default tag format: specs still
        // carrying the built-in default (or none) adopt
        // `release.tag_format`. Per-unit `unit_overrides` below
        // always win, as does any spec format that differs from
        // the built-in default.
        if s.tag_format.is_empty() || s.tag_format == crate::unit::DEFAULT_TAG_FORMAT {
            s.tag_format = cfg.release.tag_format.clone();
        }
        if let Some(o) = cfg.release.unit_overrides.get(&s.name) {
            if o.bump.is_some() {
                s.bump = o.bump;
            }
            if let Some(t) = &o.tag_format {
                s.tag_format = t.clone();
            }
            if let Some(p) = &o.publish {
                s.publish = p.clone();
            }
        }
    }
    specs
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip() {
        let text = r#"
[release]
changelog_path = "CHANGES.md"
default_bump = "patch"

[release.unit_overrides."tamias-core"]
skip = true

[publish.ghreleases]
token_env = "GH_TOKEN"
assets = ["dist/{unit}-{version}-*"]
"#;
        let cfg: Config = toml::from_str(text).unwrap();
        assert_eq!(cfg.release.changelog_path, "CHANGES.md");
        assert_eq!(cfg.release.default_bump, Some(Bump::Patch));
        assert!(cfg
            .release
            .unit_overrides
            .get("tamias-core")
            .is_some_and(|o| o.skip));
        assert_eq!(cfg.publish.ghreleases.token_env, "GH_TOKEN");
        assert_eq!(
            cfg.publish.ghreleases.assets,
            vec!["dist/{unit}-{version}-*".to_string()]
        );
        assert!(cfg.publish.crates.enabled);
    }

    #[test]
    fn sync_refs_round_trip() {
        let text = r#"
[release.sync_refs]
rules = [
  { files = ["libraries/app/tamias.yaml"], find = "(?m)^version: \"{old}\"", replace = "version: \"{new}\"" },
  { files = ["README.md", ".agents/skills/app/SKILL.md"], find = "app@{old}", replace = "app@{new}" },
]
"#;
        let cfg: Config = toml::from_str(text).unwrap();
        let rules = &cfg.release.sync_refs.rules;
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].files, vec!["libraries/app/tamias.yaml"]);
        assert_eq!(rules[1].replace, "app@{new}");
        // Missing find/replace is a hard config error.
        assert!(
            toml::from_str::<Config>("[release.sync_refs]\nrules = [{ files = [\"x\"] }]").is_err()
        );
        // Empty rules = disabled.
        assert!(Config::default().release.sync_refs.rules.is_empty());
    }

    #[test]
    fn defaults_are_zero_key() {
        let cfg = Config::default();
        assert!(!cfg.publish.dry_run);
        assert_eq!(cfg.release.tag_format, "{unit}-v{version}");
    }
}
