//! Changelog model: strategy-ordered sections, entries grouped
//! under them, markdown rendering for splicing into
//! `CHANGELOG.md`. Section identity/order/headings come from the
//! release [`Strategy`](crate::strategy::Strategy) — P-030 made
//! the keep-a-changelog section map data, so this module no longer
//! hardcodes `Section` kinds; the `conventional` builtin
//! reproduces the old map byte-identically.

use serde::{Deserialize, Serialize};

use crate::commit::ConventionalCommit;

/// One rendered section: stable `key` (the strategy's section
/// identity), display `heading` (`### ` text), and its entries.
/// Sections render in `Changelog.sections` order — the strategy's
/// table order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangelogSection {
    pub key: String,
    pub heading: String,
    pub entries: Vec<ChangelogEntry>,
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

/// Sections → entries for one release. `sections` is an ordered
/// `Vec` — order is the strategy's render order (the old
/// `BTreeMap<Section, _>` relied on the enum's declaration order;
/// string keys can't carry that, so the strategy's table order is
/// stored explicitly).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changelog {
    /// Rendered under `## [version] - date`; empty version =
    /// `[Unreleased]`.
    pub version: String,
    /// ISO date; `None` = omit the date suffix.
    pub date: Option<String>,
    /// Ordered sections — pre-seeded from the strategy's section
    /// table so render order is stable regardless of which
    /// section a commit lands in first.
    pub sections: Vec<ChangelogSection>,
}

impl Changelog {
    /// An empty changelog with the strategy's sections pre-seeded
    /// in render order — every section exists (empty) so `push`
    /// appends in place and `render` emits table order.
    #[must_use]
    pub fn for_strategy(
        sections: &[crate::strategy::SectionSpec],
        version: String,
        date: Option<String>,
    ) -> Self {
        Self {
            version,
            date,
            sections: sections
                .iter()
                .map(|s| ChangelogSection {
                    key: s.key.clone(),
                    heading: s.heading.clone(),
                    entries: Vec::new(),
                })
                .collect(),
        }
    }

    /// Append `entry` to section `key`. The section is created at
    /// the end (with `heading` as its `### ` text) when absent —
    /// callers that pre-seed via [`Changelog::for_strategy`] never
    /// hit that path.
    pub fn push(&mut self, key: &str, heading: &str, entry: ChangelogEntry) {
        match self.sections.iter_mut().find(|s| s.key == key) {
            Some(s) => s.entries.push(entry),
            None => self.sections.push(ChangelogSection {
                key: key.to_string(),
                heading: heading.to_string(),
                entries: vec![entry],
            }),
        }
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
        for section in &self.sections {
            if section.entries.is_empty() {
                continue;
            }
            out.push_str("\n### ");
            out.push_str(&section.heading);
            out.push_str("\n\n");
            for e in &section.entries {
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

fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::commit::parse_conventional;
    use crate::strategy::Strategy;

    #[test]
    fn sections_render_in_strategy_order() {
        let strategy = Strategy::conventional();
        let mut cl = Changelog::for_strategy(
            &strategy.changelog.sections,
            "1.0.0".into(),
            Some("2026-11-10".into()),
        );
        // Push Fixed before Added — render order must still be
        // the strategy's table order (Added before Fixed… via
        // Changed between them; only non-empty sections render).
        for subject in ["fix: crash on empty input", "feat: new thing"] {
            let cc = parse_conventional(subject).unwrap();
            let sec = strategy.section_for(&cc).unwrap();
            cl.push(
                &sec.key,
                &sec.heading,
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
    fn empty_sections_are_skipped_and_unknown_keys_append() {
        let mut cl = Changelog::default();
        cl.push(
            "custom",
            "Custom Heading",
            ChangelogEntry {
                sha: "x".into(),
                scope: None,
                description: "thing".into(),
                breaking: false,
            },
        );
        let out = cl.render();
        assert!(out.contains("### Custom Heading"));
        assert!(out.contains("- thing"));
        // Empty sections never render headings.
        assert!(!out.contains("### Added"));
    }
}
