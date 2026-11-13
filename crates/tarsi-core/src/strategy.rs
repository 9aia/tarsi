//! Release strategies (P-030) — bump inference and changelog
//! sectioning as pure serde **data**, selectable per repo/profile
//! via `[release] strategy` in `tarsi.toml`.
//!
//! A [`Strategy`] is two rule tables:
//!
//! - [`BumpRules`] — ordered-agnostic `rules[]` of
//!   `{type?, scope?, breaking?, not_type?, bump}`. A commit
//!   contributes the **max** `bump` across its matching rules
//!   (semantic-release commit-analyzer's "highest release type
//!   wins"); a unit folds max across its commits. `fallback` is
//!   the bump when inference finds nothing.
//! - [`ChangelogRules`] — ordered `sections[]` of
//!   `{key, heading, rules[]}`; table order is both match
//!   precedence (first matching section wins) and render order.
//!   Commits matching no section are dropped.
//!
//! Matchers run against the [`ConventionalCommit`]'s **raw type
//! token** (`raw_type` — `deps`, `i18n`, … are nameable, unlike
//! the closed [`CommitKind`] enum), its scope, and its breaking
//! flag:
//!
//! - `type` — exact raw-type token, or `"*"`/absent for any.
//! - `scope` — exact scope, trailing-`*` prefix glob, or
//!   `"*"`/absent for any (including no scope).
//! - `breaking` — `true`/`false` pins the `!`/`BREAKING CHANGE`
//!   flag; absent matches either.
//! - `not_type` — raw-type tokens that veto the match. This is
//!   how `conventional` expresses "breaking commits of
//!   otherwise-silent kinds land in Changed" without stealing
//!   `fix!`/`perf!`/`feat!` from their own sections.
//!
//! Builtin strategies are compile-time data ([`Strategy::conventional`],
//! [`Strategy::semantic_release_angular`]); custom strategies are
//! inline TOML/YAML tables — no plugin binaries, no trait objects.
//! What can't be data (render templates beyond
//! section/heading/order, non-semver versioning, non-conventional
//! parsing, anything needing execution) stays code — see P-030.

use serde::{Deserialize, Serialize};

use crate::bump::Bump;
use crate::commit::ConventionalCommit;

/// The `[release] strategy` config value — either a builtin name
/// or an inline rules table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StrategyRef {
    /// Builtin strategy name: `"conventional"`,
    /// `"semantic-release-angular"`.
    Named(String),
    /// Inline strategy table (`[release.strategy.bump]` /
    /// `[release.strategy.changelog]`). Each sub-table defaults to
    /// the `conventional` rules when absent.
    Inline(Strategy),
}

impl StrategyRef {
    /// Resolve to concrete rules. Unknown builtin names are a
    /// config error — silently falling back would hide typos.
    ///
    /// # Errors
    /// When `Named` isn't a known builtin.
    pub fn resolve(&self) -> Result<Strategy, String> {
        match self {
            Self::Named(name) => Strategy::named(name),
            Self::Inline(s) => Ok(s.clone()),
        }
    }
}

/// A complete release strategy — bump policy + changelog layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strategy {
    #[serde(default = "default_bump_rules")]
    pub bump: BumpRules,
    #[serde(default = "default_changelog_rules")]
    pub changelog: ChangelogRules,
}

impl Default for Strategy {
    /// Absent/empty strategy = `conventional` — the pre-P-030
    /// behavior, so existing `tarsi.toml`s are unchanged.
    fn default() -> Self {
        Self::conventional()
    }
}

fn default_bump_rules() -> BumpRules {
    Strategy::conventional().bump
}

fn default_changelog_rules() -> ChangelogRules {
    Strategy::conventional().changelog
}

/// Bump policy: match rules + the no-match fallback.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BumpRules {
    /// Bump applied to a unit whose commits match no rule (or
    /// that has no releasable commits at all). `None`/absent =
    /// no release — `"none"` spells the same explicitly and also
    /// blocks `release.default_bump` inheritance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Bump>,
    /// Match rules — order irrelevant (max-match fold).
    #[serde(default)]
    pub rules: Vec<BumpRule>,
}

/// One bump rule: matcher fields + the `bump` a match contributes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BumpRule {
    /// Raw conventional type token; absent/`"*"` = any type.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Scope matcher — exact or trailing-`*` prefix glob;
    /// absent/`"*"` = any scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Pin the breaking flag; absent = either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaking: Option<bool>,
    /// Raw-type tokens that veto the match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_type: Vec<String>,
    /// The bump this rule contributes.
    pub bump: Bump,
}

/// Changelog policy: ordered section table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangelogRules {
    /// Sections in render + match-precedence order.
    #[serde(default)]
    pub sections: Vec<SectionSpec>,
}

/// One changelog section: stable `key` (data identity), display
/// `heading` (`### Heading`), and the rules that route commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionSpec {
    /// Stable identifier — `"added"`, `"features"`, …
    pub key: String,
    /// Rendered `### ` heading text.
    pub heading: String,
    /// Match rules — a commit lands here when ANY rule matches
    /// and no earlier section claimed it.
    #[serde(default)]
    pub rules: Vec<SectionRule>,
}

/// Changelog section matcher — same fields as [`BumpRule`] minus
/// `bump`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionRule {
    /// Raw conventional type token; absent/`"*"` = any type.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Scope matcher — exact or trailing-`*` prefix glob;
    /// absent/`"*"` = any scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Pin the breaking flag; absent = either.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breaking: Option<bool>,
    /// Raw-type tokens that veto the match.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_type: Vec<String>,
}

/// Shared matcher evaluation for [`BumpRule`]/[`SectionRule`].
fn rule_matches(
    kind: Option<&str>,
    scope: Option<&str>,
    breaking: Option<bool>,
    not_type: &[String],
    cc: &ConventionalCommit,
) -> bool {
    if let Some(b) = breaking {
        if cc.breaking != b {
            return false;
        }
    }
    if not_type.iter().any(|t| t == &cc.raw_type) {
        return false;
    }
    if let Some(k) = kind {
        if k != "*" && k != cc.raw_type {
            return false;
        }
    }
    if let Some(s) = scope {
        if s != "*" && !scope_matches(s, cc.scope.as_deref()) {
            return false;
        }
    }
    true
}

/// Scope match: exact, or trailing-`*` prefix glob (`"core-*"`).
/// An absent commit scope only matches the `"*"` wildcard, which
/// callers handle before reaching here.
fn scope_matches(pattern: &str, scope: Option<&str>) -> bool {
    let Some(sc) = scope else {
        return false;
    };
    match pattern.strip_suffix('*') {
        Some(prefix) if !prefix.is_empty() => sc.starts_with(prefix),
        _ => pattern == sc,
    }
}

fn bump_rule(
    kind: Option<&str>,
    scope: Option<&str>,
    breaking: Option<bool>,
    not_type: &[&str],
    bump: Bump,
) -> BumpRule {
    BumpRule {
        kind: kind.map(str::to_string),
        scope: scope.map(str::to_string),
        breaking,
        not_type: not_type.iter().map(|s| (*s).to_string()).collect(),
        bump,
    }
}

fn section_rule(
    kind: Option<&str>,
    scope: Option<&str>,
    breaking: Option<bool>,
    not_type: &[&str],
) -> SectionRule {
    SectionRule {
        kind: kind.map(str::to_string),
        scope: scope.map(str::to_string),
        breaking,
        not_type: not_type.iter().map(|s| (*s).to_string()).collect(),
    }
}

fn section(key: &str, heading: &str, rules: Vec<SectionRule>) -> SectionSpec {
    SectionSpec {
        key: key.to_string(),
        heading: heading.to_string(),
        rules,
    }
}

impl Strategy {
    /// Resolve a builtin by name.
    ///
    /// # Errors
    /// Unknown name — lists the builtins.
    pub fn named(name: &str) -> Result<Self, String> {
        match name {
            "conventional" => Ok(Self::conventional()),
            "semantic-release-angular" => Ok(Self::semantic_release_angular()),
            other => Err(format!(
                "unknown release strategy {other:?} — builtins: conventional, semantic-release-angular"
            )),
        }
    }

    /// The pre-P-030 policy, expressed as data: `feat`→minor,
    /// `fix`/`perf`→patch, breaking→major, everything else→none;
    /// keep-a-changelog sections Added/Changed/Fixed. Byte-identical
    /// to the hardcoded `bump_for`/`section_for` it replaces — the
    /// golden corpus is the parity gate.
    #[must_use]
    pub fn conventional() -> Self {
        Self {
            bump: BumpRules {
                fallback: None,
                rules: vec![
                    // breaking always wins — checked as max-match,
                    // so `feat!` folds major+minor → major.
                    bump_rule(None, None, Some(true), &[], Bump::Major),
                    bump_rule(Some("feat"), None, None, &[], Bump::Minor),
                    bump_rule(Some("fix"), None, None, &[], Bump::Patch),
                    bump_rule(Some("perf"), None, None, &[], Bump::Patch),
                ],
            },
            changelog: ChangelogRules {
                sections: vec![
                    section(
                        "added",
                        "Added",
                        vec![section_rule(Some("feat"), None, None, &[])],
                    ),
                    section(
                        "changed",
                        "Changed",
                        vec![
                            section_rule(Some("refactor"), None, None, &[]),
                            // Breaking commits of kinds without a
                            // section of their own → Changed. The
                            // not_type veto keeps `fix!`/`perf!`/
                            // `feat!` routing to Fixed/Added (those
                            // sections' precedence vs Changed is
                            // type-based, not breaking-based).
                            section_rule(None, None, Some(true), &["feat", "fix", "perf"]),
                        ],
                    ),
                    section(
                        "fixed",
                        "Fixed",
                        vec![
                            section_rule(Some("fix"), None, None, &[]),
                            section_rule(Some("perf"), None, None, &[]),
                        ],
                    ),
                ],
            },
        }
    }

    /// semantic-release's `@semantic-release/commit-analyzer`
    /// **angular preset** as data (P-030 §5): `feat`→minor,
    /// `fix`/`perf`/`revert`→patch, breaking→major, all other types
    /// ignored; changelog sections per angular's convention
    /// (BREAKING CHANGES / Features / Bug Fixes / Performance
    /// Improvements / Reverts). Pipeline parity (channels,
    /// dist-tags, plugins) is explicitly out of scope.
    #[must_use]
    pub fn semantic_release_angular() -> Self {
        Self {
            bump: BumpRules {
                fallback: None,
                rules: vec![
                    bump_rule(None, None, Some(true), &[], Bump::Major),
                    bump_rule(Some("feat"), None, None, &[], Bump::Minor),
                    bump_rule(Some("fix"), None, None, &[], Bump::Patch),
                    bump_rule(Some("perf"), None, None, &[], Bump::Patch),
                    bump_rule(Some("revert"), None, None, &[], Bump::Patch),
                ],
            },
            changelog: ChangelogRules {
                sections: vec![
                    section(
                        "breaking-changes",
                        "BREAKING CHANGES",
                        vec![section_rule(None, None, Some(true), &[])],
                    ),
                    section(
                        "features",
                        "Features",
                        vec![section_rule(Some("feat"), None, None, &[])],
                    ),
                    section(
                        "bug-fixes",
                        "Bug Fixes",
                        vec![section_rule(Some("fix"), None, None, &[])],
                    ),
                    section(
                        "performance",
                        "Performance Improvements",
                        vec![section_rule(Some("perf"), None, None, &[])],
                    ),
                    section(
                        "reverts",
                        "Reverts",
                        vec![section_rule(Some("revert"), None, None, &[])],
                    ),
                ],
            },
        }
    }

    /// The bump one commit contributes: **max** across matching
    /// rules (`Bump::None` when nothing matches). The unit-level
    /// fold and the `fallback` application live in `plan_release`.
    #[must_use]
    pub fn bump_for(&self, cc: &ConventionalCommit) -> Bump {
        self.bump
            .rules
            .iter()
            .filter(|r| {
                rule_matches(
                    r.kind.as_deref(),
                    r.scope.as_deref(),
                    r.breaking,
                    &r.not_type,
                    cc,
                )
            })
            .map(|r| r.bump)
            .max()
            .unwrap_or(Bump::None)
    }

    /// The changelog section a commit routes to: the **first**
    /// section (table order = precedence) with a matching rule.
    /// `None` = not changelog-worthy.
    #[must_use]
    pub fn section_for(&self, cc: &ConventionalCommit) -> Option<&SectionSpec> {
        self.changelog.sections.iter().find(|s| {
            s.rules.iter().any(|r| {
                rule_matches(
                    r.kind.as_deref(),
                    r.scope.as_deref(),
                    r.breaking,
                    &r.not_type,
                    cc,
                )
            })
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::commit::parse_conventional;

    fn cc(subject: &str) -> ConventionalCommit {
        parse_conventional(subject).unwrap()
    }

    #[test]
    fn conventional_bump_matches_legacy_inference() {
        let s = Strategy::conventional();
        assert_eq!(s.bump_for(&cc("feat: x")), Bump::Minor);
        assert_eq!(s.bump_for(&cc("feat(core)!: x")), Bump::Major);
        assert_eq!(s.bump_for(&cc("fix: x")), Bump::Patch);
        assert_eq!(s.bump_for(&cc("perf: x")), Bump::Patch);
        assert_eq!(s.bump_for(&cc("chore!: x")), Bump::Major);
        assert_eq!(s.bump_for(&cc("chore: x")), Bump::None);
        assert_eq!(s.bump_for(&cc("deps: bump")), Bump::None);
    }

    #[test]
    fn conventional_sections_match_legacy_map() {
        let s = Strategy::conventional();
        let key = |subj: &str| s.section_for(&cc(subj)).map(|s| s.key.as_str());
        assert_eq!(key("feat: x"), Some("added"));
        assert_eq!(key("feat!: x"), Some("added"));
        assert_eq!(key("fix: x"), Some("fixed"));
        assert_eq!(key("perf: x"), Some("fixed"));
        // fix!/perf! must NOT be stolen by Changed's breaking rule.
        assert_eq!(key("fix!: x"), Some("fixed"));
        assert_eq!(key("perf!: x"), Some("fixed"));
        assert_eq!(key("refactor: x"), Some("changed"));
        assert_eq!(key("chore: x"), None);
        assert_eq!(key("docs: x"), None);
        // Breaking on a silent kind → Changed (incl. unknown types).
        assert_eq!(key("chore!: x"), Some("changed"));
        assert_eq!(key("deps!: bump"), Some("changed"));
        assert_eq!(key("deps: bump"), None);
    }

    #[test]
    fn max_match_fold_picks_highest_rule() {
        let s = Strategy {
            bump: BumpRules {
                fallback: None,
                rules: vec![
                    bump_rule(Some("feat"), None, None, &[], Bump::Minor),
                    bump_rule(Some("feat"), Some("api"), None, &[], Bump::Major),
                ],
            },
            changelog: ChangelogRules::default(),
        };
        assert_eq!(s.bump_for(&cc("feat(api): x")), Bump::Major);
        assert_eq!(s.bump_for(&cc("feat(core): x")), Bump::Minor);
        assert_eq!(s.bump_for(&cc("fix: x")), Bump::None);
    }

    #[test]
    fn scope_glob_and_wildcards() {
        let s = Strategy {
            bump: BumpRules {
                fallback: None,
                rules: vec![
                    bump_rule(Some("feat"), Some("core-*"), None, &[], Bump::Major),
                    bump_rule(Some("feat"), Some("*"), None, &[], Bump::Minor),
                ],
            },
            changelog: ChangelogRules::default(),
        };
        assert_eq!(s.bump_for(&cc("feat(core-x): a")), Bump::Major);
        assert_eq!(s.bump_for(&cc("feat(ui): b")), Bump::Minor);
        assert_eq!(s.bump_for(&cc("feat: c")), Bump::Minor);
    }

    #[test]
    fn section_first_match_wins() {
        let s = Strategy {
            bump: BumpRules::default(),
            changelog: ChangelogRules {
                sections: vec![
                    section(
                        "all",
                        "Everything",
                        vec![section_rule(None, None, None, &[])],
                    ),
                    section(
                        "feat",
                        "Features",
                        vec![section_rule(Some("feat"), None, None, &[])],
                    ),
                ],
            },
        };
        // "all" precedes "feat" — feat lands in the first match.
        assert_eq!(s.section_for(&cc("feat: x")).unwrap().key, "all");
        assert_eq!(s.section_for(&cc("fix: x")).unwrap().key, "all");
    }

    #[test]
    fn not_type_vetoes_match() {
        let s = Strategy {
            bump: BumpRules::default(),
            changelog: ChangelogRules {
                sections: vec![section(
                    "rest",
                    "Rest",
                    vec![section_rule(None, None, Some(true), &["fix"])],
                )],
            },
        };
        assert!(s.section_for(&cc("chore!: x")).is_some());
        assert!(s.section_for(&cc("fix!: x")).is_none());
        assert!(s.section_for(&cc("fix: x")).is_none());
    }

    #[test]
    fn strategy_ref_resolves_builtin_and_inline() {
        assert_eq!(
            StrategyRef::Named("conventional".into()).resolve().unwrap(),
            Strategy::conventional()
        );
        assert_eq!(
            StrategyRef::Named("semantic-release-angular".into())
                .resolve()
                .unwrap(),
            Strategy::semantic_release_angular()
        );
        assert!(StrategyRef::Named("nope".into()).resolve().is_err());
        let inline = StrategyRef::Inline(Strategy {
            bump: BumpRules {
                fallback: Some(Bump::Patch),
                rules: vec![],
            },
            changelog: ChangelogRules::default(),
        });
        assert_eq!(inline.resolve().unwrap().bump.fallback, Some(Bump::Patch));
    }

    #[test]
    fn toml_string_and_table_forms() {
        // String form names a builtin.
        let cfg: crate::Config =
            toml::from_str("[release]\nstrategy = \"conventional\"\n").unwrap();
        assert_eq!(
            cfg.release.strategy,
            Some(StrategyRef::Named("conventional".into()))
        );
        // Table form is an inline strategy; absent sub-tables
        // default to the conventional rules.
        let cfg: crate::Config = toml::from_str(
            r#"
[release.strategy.bump]
fallback = "patch"

[[release.strategy.bump.rules]]
type = "feat"
bump = "major"
"#,
        )
        .unwrap();
        let Some(StrategyRef::Inline(s)) = cfg.release.strategy else {
            panic!("expected inline strategy");
        };
        assert_eq!(s.bump.fallback, Some(Bump::Patch));
        assert_eq!(s.bump.rules.len(), 1);
        assert_eq!(s.bump.rules[0].bump, Bump::Major);
        // changelog sub-table defaulted to conventional sections.
        assert_eq!(s.changelog, Strategy::conventional().changelog);
        // Absent strategy = None → conventional at resolution.
        let cfg: crate::Config = toml::from_str("[release]\n").unwrap();
        assert_eq!(cfg.release.strategy, None);
    }
}
