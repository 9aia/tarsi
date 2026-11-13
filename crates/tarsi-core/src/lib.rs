//! tarsi-core — the pure release domain. No I/O, no git, no
//! network: callers feed it commits (from `tarsi-vcs`) and unit
//! specs (from manifests/config) and get back a [`ReleasePlan`]
//! plus rendered changelogs. Everything forge- or VCS-specific
//! lives in the port crates.

pub mod bump;
pub mod changelog;
pub mod commit;
pub mod config;
pub mod plan;
pub mod strategy;
pub mod unit;

pub use bump::Bump;
pub use changelog::{Changelog, ChangelogEntry, ChangelogSection};
pub use commit::{parse_conventional, Commit, CommitKind, ConventionalCommit};
pub use config::Config;
pub use plan::{
    plan_release, plan_release_with_strategy, PlanError, ReleasePlan, ReleaseStep, UnitPlan,
};
pub use strategy::{
    BumpRule, BumpRules, ChangelogRules, SectionRule, SectionSpec, Strategy, StrategyRef,
};
pub use unit::{PublishTarget, ReleaseUnitSpec};
