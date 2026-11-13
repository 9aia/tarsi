//! Commit model + conventional-commit parsing. A [`Commit`] is the
//! VCS-neutral shape `tarsi-vcs` adapters produce; parsing the
//! subject into a [`ConventionalCommit`] is pure.

use serde::{Deserialize, Serialize};

/// Conventional-commit types we recognize. Unknown types still
/// parse (kind = `Other`) so exotic repos don't lose history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitKind {
    Feat,
    Fix,
    Perf,
    Refactor,
    Docs,
    Style,
    Test,
    Build,
    Ci,
    Chore,
    Revert,
    /// `!`-marked or footer-`BREAKING CHANGE` on an unrecognized
    /// type still counts as breaking.
    Other,
}

impl CommitKind {
    /// Parse the type token of a conventional subject.
    fn parse(s: &str) -> Self {
        match s {
            "feat" => Self::Feat,
            "fix" => Self::Fix,
            "perf" => Self::Perf,
            "refactor" => Self::Refactor,
            "docs" => Self::Docs,
            "style" => Self::Style,
            "test" => Self::Test,
            "build" => Self::Build,
            "ci" => Self::Ci,
            "chore" => Self::Chore,
            "revert" => Self::Revert,
            _ => Self::Other,
        }
    }
}

/// A parsed conventional commit: `type(scope)!: description`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConventionalCommit {
    pub kind: CommitKind,
    /// The exact type token from the subject (`feat`, `deps`,
    /// `i18n`, …) — strategy rules match on this, not the closed
    /// [`CommitKind`] enum, so arbitrary conventional types are
    /// nameable in config (P-030).
    pub raw_type: String,
    pub scope: Option<String>,
    /// `!` after type/scope, or a `BREAKING CHANGE:`/`BREAKING-CHANGE:`
    /// footer — callers fold footer detection into `breaking` when
    /// constructing (subject-only parse can't see the body).
    pub breaking: bool,
    pub description: String,
}

/// A commit as the VCS port reports it — sha + subject + touched
/// paths (used for release-unit attribution).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    pub sha: String,
    /// First line of the message.
    pub subject: String,
    /// Remaining message body (for `BREAKING CHANGE:` footers).
    #[serde(default)]
    pub body: String,
    /// Repo-relative paths touched; empty = unknown (attribution
    /// falls back to "matches every unit").
    #[serde(default)]
    pub paths: Vec<String>,
}

impl Commit {
    /// Parse the subject and fold body footers into `breaking`.
    #[must_use]
    pub fn conventional(&self) -> Option<ConventionalCommit> {
        let mut cc = parse_conventional(&self.subject)?;
        if self.body.contains("BREAKING CHANGE:") || self.body.contains("BREAKING-CHANGE:") {
            cc.breaking = true;
        }
        Some(cc)
    }
}

/// Parse `type[(scope)][!]: description`. Returns `None` for
/// non-conventional subjects (free-form messages).
pub fn parse_conventional(subject: &str) -> Option<ConventionalCommit> {
    let subject = subject.trim();
    let (head, description) = subject.split_once(':')?;
    let description = description.trim().to_string();
    if description.is_empty() {
        return None;
    }
    let (head, bang) = match head.strip_suffix('!') {
        Some(h) => (h, true),
        None => (head, false),
    };
    let (kind_s, scope) = match head.split_once('(') {
        Some((k, rest)) => {
            let scope = rest.strip_suffix(')')?;
            if scope.is_empty() {
                return None;
            }
            (k, Some(scope.to_string()))
        }
        None => (head, None),
    };
    // Type token must be a bare word — rejects "fix: a: b" style
    // noise and URLs in the head.
    if kind_s.is_empty() || !kind_s.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
        return None;
    }
    Some(ConventionalCommit {
        kind: CommitKind::parse(kind_s),
        raw_type: kind_s.to_string(),
        scope,
        breaking: bang,
        description,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_form() {
        let cc = parse_conventional("feat(driver): add bundled-driver consent gate").unwrap();
        assert_eq!(cc.kind, CommitKind::Feat);
        assert_eq!(cc.scope.as_deref(), Some("driver"));
        assert!(!cc.breaking);
        assert_eq!(cc.description, "add bundled-driver consent gate");
    }

    #[test]
    fn parses_breaking_bang_and_scope() {
        let cc = parse_conventional("feat(api)!: drop v1 endpoints").unwrap();
        assert!(cc.breaking);
        assert_eq!(cc.scope.as_deref(), Some("api"));
    }

    #[test]
    fn rejects_non_conventional() {
        assert!(parse_conventional("just a message").is_none());
        assert!(parse_conventional("fix:").is_none());
        assert!(parse_conventional("HTTPS://example.com").is_none());
        assert!(parse_conventional("feat(): empty scope").is_none());
    }

    #[test]
    fn body_footer_marks_breaking() {
        let c = Commit {
            sha: "abc".into(),
            subject: "chore: bump deps".into(),
            body: "details\n\nBREAKING CHANGE: config format moved".into(),
            paths: vec![],
        };
        let cc = c.conventional().unwrap();
        assert!(cc.breaking);
    }
}
