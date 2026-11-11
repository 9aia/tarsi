//! tarsi-testkit — shared fixtures for port conformance and the
//! golden corpus (real commit ranges → expected plans/changelogs).
//! See PL-014 B3: this is the spec the legacy TS behavior maps onto.

/// Load a JSON fixture from `tests/fixtures/` (dev-only path —
/// fixtures ship with the crate for downstream test use via
/// `include_str!` on the published source is NOT guaranteed, so
/// fixtures are embedded below for v1).
pub mod fixtures {
    /// A small synthetic commit range exercising every bump class.
    pub const SYNTHETIC_COMMITS: &str = r#"[
        {"sha":"aaaaaaa1","subject":"feat(core): add widget API","body":"","paths":["crates/core/src/lib.rs"]},
        {"sha":"bbbbbbb2","subject":"fix(cli): crash on empty args","body":"","paths":["crates/cli/src/main.rs"]},
        {"sha":"ccccccc3","subject":"feat(cli)!: drop --legacy flag","body":"","paths":["crates/cli/src/lib.rs"]},
        {"sha":"ddddddd4","subject":"chore: bump deps","body":"","paths":["Cargo.lock"]},
        {"sha":"eeeeeee5","subject":"docs: update readme","body":"","paths":["README.md"]}
    ]"#;
}
