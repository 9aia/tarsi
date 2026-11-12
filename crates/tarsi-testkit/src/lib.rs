//! tarsi-testkit — the behavioral backbone for tarsi's ports and
//! the `tarsi-serve` op surface (PL-014 B3/D5):
//!
//! - [`fixtures`] — commit-range JSON fixtures exercising every
//!   bump class; [`fixtures::synthetic_commits`] parses them into
//!   [`tarsi_core::commit::Commit`]s.
//! - [`golden`] — golden plan cases: specs + commits → expected
//!   bump/version/tag/changelog per unit. [`GoldenCase::check`]
//!   diffs a [`tarsi_core::ReleasePlan`] against the expectation —
//!   the spec the legacy TS behavior maps onto (B3). Cases cover
//!   synthetic ranges plus one real org range (tarsi's own
//!   `crates/tarsi-serve` history).
//! - [`repo`] — real-git fixture builders: temp workspace repos
//!   (multi-unit cargo workspace, bare `origin`, conventional
//!   commits, tags) for anything that must exercise `lasius-vcs`
//!   for real.
//! - [`serve`] — the `tarsi-serve` op conformance suite: every
//!   `assert_*` runs one op contract against a `&dyn Vcs` +
//!   fixture dir, so any VCS impl (or a future serve refactor)
//!   proves the same wire behavior. [`serve::assert_conformance`]
//!   runs the read-only ops; [`serve::assert_release_conformance`]
//!   adds the mutating `release` path (needs `origin` + a
//!   no-publish `tarsi.toml`).
//!
//! Downstream crates consume this as a dev-dependency and call the
//! `assert_*` functions from their own tests — same pattern as
//! `scarab-testkit` / `sepia-testkit`.

pub mod fixtures;
pub mod golden;
pub mod repo;
pub mod serve;

pub use fixtures::synthetic_commits;
pub use golden::{cases as golden_cases, GoldenCase, UnitExpect};
pub use repo::{git, git_try, ls_remote, write, Fixture};
