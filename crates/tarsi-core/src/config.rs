//! The `tarsi.toml`/`tarsi.yml` config model — pure serde types.
//! Loading/discovery lives in scarab-config (`FileSource`); this
//! module only defines what the file *means*. Secret **names**
//! and `cuora://` refs appear here (`token_env`, `token_ref`);
//! values never do — cuora boundary.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::bump::Bump;
use crate::strategy::StrategyRef;
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
    ///
    /// **Deprecated (P-030)** — superseded by
    /// [`ReleaseConfig::strategy`]'s `changelog` section table.
    /// Still parsed for backward compat; has no effect on
    /// planning (it never did — the field was always descriptive).
    #[serde(default = "default_styleguide")]
    pub styleguide: String,
    /// Release strategy (P-030): a builtin name
    /// (`"conventional"`, `"semantic-release-angular"`) or an
    /// inline rules table (`[release.strategy.bump]` /
    /// `[release.strategy.changelog]`). Absent = `conventional`,
    /// the pre-strategy behavior. Resolved inside `tarsi-serve`'s
    /// plan path so every op inherits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<StrategyRef>,
    /// Repo-wide default bump when inference finds nothing —
    /// honored as the strategy's `fallback` (P-030 wired the
    /// previously-dead field; an explicit `strategy.bump.fallback`
    /// wins).
    #[serde(default)]
    pub default_bump: Option<Bump>,
    /// Path globs excluded from commit→unit attribution
    /// (e.g. `["docs/**", "*.md"]`). Wired in P-030: matching
    /// paths are stripped from each commit before attribution;
    /// a commit left with no paths is dropped entirely.
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
            strategy: None,
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
    /// Force this unit's bump (beats inference).
    #[serde(default)]
    pub bump: Option<Bump>,
    /// Per-unit tag template.
    #[serde(default)]
    pub tag_format: Option<String>,
    /// Restrict publish targets (`"crates-io"`, `"github-release"`).
    #[serde(default)]
    pub publish: Option<Vec<PublishTarget>>,
    /// Skip this unit entirely (e.g. a lib that versions with its
    /// parent).
    #[serde(default)]
    pub skip: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishConfig {
    /// crates.io publish target.
    #[serde(default)]
    pub crates: CratesPublish,
    /// GitHub Releases publish target.
    #[serde(default)]
    pub ghreleases: GhReleasePublish,
    /// Schema-level dry-run flag; the serve op's `dry_run` param is
    /// what callers use.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CratesPublish {
    /// crates.io publishing on/off.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Retry crates.io 429s with backoff (release.sh semantics).
    #[serde(default = "default_true")]
    pub retry_429: bool,
    /// Max publish attempts per unit.
    #[serde(default = "default_attempts")]
    pub max_attempts: u32,
    /// Per-attempt wall-clock cap for `cargo publish` in seconds
    /// (verify build + upload). Heavy crates need minutes — the
    /// default covers tamias-serve/tamias-tui-class builds
    /// (I-003). `0` disables the cap.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Base delay in seconds between 429 retries — doubles per
    /// attempt up to `retry_max_secs`. crates.io bulk throttles
    /// ask for minutes-scale spacing (I-003).
    #[serde(default = "default_retry_base_secs")]
    pub retry_base_secs: u64,
    /// Ceiling in seconds for computed 429 backoff. A server
    /// `Retry-After` hint is honored verbatim and may exceed it.
    #[serde(default = "default_retry_max_secs")]
    pub retry_max_secs: u64,
    /// Env var NAME consulted for the crates.io token when
    /// `token_ref` is unset or misses — cargo reads
    /// `CARGO_REGISTRY_TOKEN`, and a resolved value is injected
    /// into the publish child's environment under that name. The
    /// value never appears in config, manifests, or logs.
    #[serde(default = "default_crates_token_env")]
    pub token_env: String,
    /// Optional `cuora://<scope>/<name>` secret ref — resolved via
    /// cuora at publish time and takes precedence over `token_env`.
    /// The value never appears in config or logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_ref: Option<String>,
}

impl Default for CratesPublish {
    fn default() -> Self {
        Self {
            enabled: true,
            retry_429: true,
            max_attempts: default_attempts(),
            timeout_secs: default_timeout_secs(),
            retry_base_secs: default_retry_base_secs(),
            retry_max_secs: default_retry_max_secs(),
            token_env: default_crates_token_env(),
            token_ref: None,
        }
    }
}

fn default_attempts() -> u32 {
    5
}
fn default_timeout_secs() -> u64 {
    15 * 60
}
fn default_retry_base_secs() -> u64 {
    30
}
fn default_retry_max_secs() -> u64 {
    5 * 60
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhReleasePublish {
    /// GitHub Releases on/off.
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
    /// Optional `cuora://<scope>/<name>` secret ref — resolved via
    /// cuora at publish time and takes precedence over `token_env`.
    /// The value never appears in config or logs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_ref: Option<String>,
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
            token_ref: None,
            assets: Vec::new(),
        }
    }
}

fn default_token_env() -> String {
    "GITHUB_TOKEN".to_string()
}
fn default_crates_token_env() -> String {
    "CARGO_REGISTRY_TOKEN".to_string()
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
    fn publish_token_fields() {
        let text = r#"
[publish.crates]
token_env = "CARGO_REGISTRY_TOKEN"
token_ref = "cuora://tamias/crates-token"

[publish.ghreleases]
token_ref = "cuora://org/GH_RELEASES_TOKEN"
"#;
        let cfg: Config = toml::from_str(text).unwrap();
        assert_eq!(cfg.publish.crates.token_env, "CARGO_REGISTRY_TOKEN");
        assert_eq!(
            cfg.publish.crates.token_ref.as_deref(),
            Some("cuora://tamias/crates-token")
        );
        assert_eq!(
            cfg.publish.ghreleases.token_ref.as_deref(),
            Some("cuora://org/GH_RELEASES_TOKEN")
        );
        // Defaults: crates reads CARGO_REGISTRY_TOKEN, ghreleases
        // GITHUB_TOKEN; no refs unless configured.
        let d = Config::default();
        assert_eq!(d.publish.crates.token_env, "CARGO_REGISTRY_TOKEN");
        assert_eq!(d.publish.ghreleases.token_env, "GITHUB_TOKEN");
        assert!(d.publish.crates.token_ref.is_none());
        assert!(d.publish.ghreleases.token_ref.is_none());
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
    fn crates_publish_timing_knobs() {
        let text = r#"
[publish.crates]
timeout_secs = 1200
retry_base_secs = 60
retry_max_secs = 600
"#;
        let cfg: Config = toml::from_str(text).unwrap();
        assert_eq!(cfg.publish.crates.timeout_secs, 1200);
        assert_eq!(cfg.publish.crates.retry_base_secs, 60);
        assert_eq!(cfg.publish.crates.retry_max_secs, 600);
        // Defaults cover heavy verify builds + bulk 429s (I-003).
        let d = Config::default().publish.crates;
        assert_eq!(d.timeout_secs, 900);
        assert_eq!(d.retry_base_secs, 30);
        assert_eq!(d.retry_max_secs, 300);
    }

    #[test]
    fn defaults_are_zero_key() {
        let cfg = Config::default();
        assert!(!cfg.publish.dry_run);
        assert_eq!(cfg.release.tag_format, "{unit}-v{version}");
    }
}
