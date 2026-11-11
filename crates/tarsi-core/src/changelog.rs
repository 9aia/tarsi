//! Keep-a-changelog model: fixed section order, entries grouped
//! under sections, markdown rendering for splicing into
//! `CHANGELOG.md`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::commit::{CommitKind, ConventionalCommit};

/// The keep-a-changelog sections, in render order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Section {
    Added,
    Changed,
    Deprecated,
    Removed,
    Fixed,
    Security,
}

impl Section {
    /// Canonical heading text (`## [Unreleased]` style headings are
    /// emitted by the renderer, not stored here).
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Changed => "Changed",
            Self::Deprecated => "Deprecated",
            Self::Removed => "Removed",
            Self::Fixed => "Fixed",
            Self::Security => "Security",
        }
    }
}

/// One changelog line's worth of data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangelogEntry {
    /// Short sha for `(#123)`-style attribution later; empty = omit.
    pub sha: String,
    pub scope: Option<String>,
    pub description: String,
    pub breaking: bool,
}

impl ChangelogEntry {
    /// Build from a parsed conventional commit + its sha.
    #[must_use]
    pub fn from_commit(sha: &str, cc: &ConventionalCommit) -> Self {
        Self {
            sha: sha.to_string(),
            scope: cc.scope.clone(),
            description: cc.description.clone(),
            breaking: cc.breaking,
        }
    }
}

/// Sections → entries for one release. `BTreeMap` keyed by `Section`
/// keeps canonical order (the enum's `Ord` matches declaration
/// order).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changelog {
    /// Rendered under `## [version] - date`; empty version =
    /// `[Unreleased]`.
    pub version: String,
    /// ISO date; `None` = omit the date suffix.
    pub date: Option<String>,
    pub sections: BTreeMap<Section, Vec<ChangelogEntry>>,
}

impl Changelog {
    /// Insert an entry into its mapped section. Returns `false`
    /// when the commit kind isn't changelog-worthy (docs/chore/
    /// test/style/ci/build/revert with no breaking flag) — the
    /// entry is dropped.
    pub fn push(&mut self, kind: CommitKind, entry: ChangelogEntry) -> bool {
        let Some(section) = section_for(kind, entry.breaking) else {
            return false;
        };
        self.sections.entry(section).or_default().push(entry);
        true
    }

    /// Render to keep-a-changelog markdown:
    ///
    /// ```text
    /// ## [1.2.0] - 2026-11-10
    ///
    /// ### Added
    ///
    /// - **scope:** description (`abc1234`)
    /// ```
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        let heading = if self.version.is_empty() {
            "[Unreleased]".to_string()
        } else {
            match &self.date {
                Some(d) => format!("[{}] - {}", self.version, d),
                None => format!("[{}]", self.version),
            }
        };
        out.push_str("## ");
        out.push_str(&heading);
        out.push('\n');
        for (section, entries) in &self.sections {
            if entries.is_empty() {
                continue;
            }
            out.push_str("\n### ");
            out.push_str(section.heading());
            out.push_str("\n\n");
            for e in entries {
                out.push_str("- ");
                if e.breaking {
                    out.push_str("**BREAKING** ");
                }
                if let Some(scope) = &e.scope {
                    out.push_str(&format!("**{scope}:** "));
                }
                out.push_str(&e.description);
                if !e.sha.is_empty() {
                    out.push_str(&format!(" (`{}`)", short_sha(&e.sha)));
                }
                out.push('\n');
            }
        }
        out
    }
}

/// Map commit kind → changelog section. `None` = not
/// changelog-worthy. Breaking changes on otherwise-silent kinds
/// land in `Changed`.
fn section_for(kind: CommitKind, breaking: bool) -> Option<Section> {
    match kind {
        CommitKind::Feat => Some(Section::Added),
        CommitKind::Fix | CommitKind::Perf => Some(Section::Fixed),
        CommitKind::Refactor => Some(Section::Changed),
        CommitKind::Docs
        | CommitKind::Style
        | CommitKind::Test
        | CommitKind::Build
        | CommitKind::Ci
        | CommitKind::Chore
        | CommitKind::Revert
        | CommitKind::Other => {
            if breaking {
                Some(Section::Changed)
            } else {
                None
            }
        }
    }
}

fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::commit::parse_conventional;

    #[test]
    fn sections_render_in_canonical_order() {
        let mut cl = Changelog {
            version: "1.0.0".into(),
            date: Some("2026-11-10".into()),
            ..Changelog::default()
        };
        for (subject, kind_expected) in [
            ("fix: crash on empty input", CommitKind::Fix),
            ("feat: new thing", CommitKind::Feat),
        ] {
            let cc = parse_conventional(subject).unwrap();
            cl.push(
                kind_expected,
                ChangelogEntry::from_commit("abcdef1234567890", &cc),
            );
        }
        let out = cl.render();
        let added = out.find("### Added").unwrap();
        let fixed = out.find("### Fixed").unwrap();
        assert!(added < fixed, "Added must render before Fixed");
        assert!(out.contains("## [1.0.0] - 2026-11-10"));
        assert!(out.contains("new thing (`abcdef1`)"));
    }

    #[test]
    fn non_conventional_and_chore_are_dropped() {
        let mut cl = Changelog::default();
        assert!(!cl.push(
            CommitKind::Chore,
            ChangelogEntry {
                sha: "x".into(),
                scope: None,
                description: "bump deps".into(),
                breaking: false,
            }
        ));
        assert!(cl.sections.is_empty());
    }
}
