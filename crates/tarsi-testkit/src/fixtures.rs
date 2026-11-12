//! Commit-range fixtures — JSON descriptions of synthetic and real
//! commit ranges, parsed into [`tarsi_core::commit::Commit`] for
//! golden cases and port tests.

use tarsi_core::commit::Commit;

/// A small synthetic commit range exercising every bump class.
pub const SYNTHETIC_COMMITS: &str = r#"[
    {"sha":"aaaaaaa1","subject":"feat(core): add widget API","body":"","paths":["crates/core/src/lib.rs"]},
    {"sha":"bbbbbbb2","subject":"fix(cli): crash on empty args","body":"","paths":["crates/cli/src/main.rs"]},
    {"sha":"ccccccc3","subject":"feat(cli)!: drop --legacy flag","body":"","paths":["crates/cli/src/lib.rs"]},
    {"sha":"ddddddd4","subject":"chore: bump deps","body":"","paths":["Cargo.lock"]},
    {"sha":"eeeeeee5","subject":"docs: update readme","body":"","paths":["README.md"]}
]"#;

/// A docs/chore-only range — nothing releasable.
pub const CHORE_ONLY_COMMITS: &str = r#"[
    {"sha":"f1111111","subject":"chore: tidy imports","body":"","paths":["crates/lib/src/lib.rs"]},
    {"sha":"f2222222","subject":"docs: fix typo","body":"","paths":["crates/lib/README.md"]},
    {"sha":"f3333333","subject":"test: cover edge case","body":"","paths":["crates/lib/tests/it.rs"]}
]"#;

/// Parse a fixture JSON array into commits (newest-first order
/// preserved — callers supply the order).
///
/// # Panics
/// Fixture constants are compile-time-known-good; a parse failure
/// is a testkit bug, not input error.
#[must_use]
pub fn parse_commits(json: &str) -> Vec<Commit> {
    serde_json::from_str(json).expect("tarsi-testkit: bad commit fixture")
}

/// [`SYNTHETIC_COMMITS`] as typed commits.
#[must_use]
pub fn synthetic_commits() -> Vec<Commit> {
    parse_commits(SYNTHETIC_COMMITS)
}

/// [`CHORE_ONLY_COMMITS`] as typed commits.
#[must_use]
pub fn chore_only_commits() -> Vec<Commit> {
    parse_commits(CHORE_ONLY_COMMITS)
}
