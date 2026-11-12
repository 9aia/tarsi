//! Golden plan corpus (PL-014 B3) — specs + commit ranges →
//! expected bump/version/tag/changelog per unit. This is the spec
//! the legacy TS `tarsi` behavior maps onto: every case is data,
//! [`GoldenCase::check`] diffs the real [`plan_release`] output
//! against it and returns human-readable mismatches (empty =
//! pass).
//!
//! Cases:
//!
//! - `mixed-single-unit` — one whole-repo unit over
//!   [`fixtures::SYNTHETIC_COMMITS`]: a breaking `feat!` forces
//!   major; chore/docs ride along into the changelog.
//! - `workspace-topo` — two units with a dep edge: path-prefix
//!   attribution, per-unit bump inference, topo publish order,
//!   and the unattributed tail (repo-root files).
//! - `chore-only-none` — docs/chore/test range → `Bump::None`,
//!   no release, version/tag still computed for the report.
//! - `real-tarsi-serve-range` — the real `git log` range of
//!   `crates/tarsi-serve` between scaffold and the D3 driver
//!   commit (8 commits, newest first as a VCS port returns them).

use semver::Version;
use tarsi_core::bump::Bump;
use tarsi_core::commit::Commit;
use tarsi_core::plan::{plan_release, ReleasePlan};
use tarsi_core::unit::{PublishTarget, ReleaseUnitSpec};

use crate::fixtures;

/// Expected outcome for one unit, in plan (publish) order.
#[derive(Debug, Clone)]
pub struct UnitExpect {
    pub name: String,
    pub bump: Bump,
    pub new_version: String,
    pub tag: String,
    /// Substrings the rendered changelog must contain.
    pub changelog_contains: Vec<String>,
}

/// One golden case: inputs + expectations.
#[derive(Debug, Clone)]
pub struct GoldenCase {
    pub name: &'static str,
    pub specs: Vec<ReleaseUnitSpec>,
    /// Newest-first, as the VCS port returns them.
    pub commits: Vec<Commit>,
    /// Expected units **in plan order** (topo-sorted).
    pub expect_units: Vec<UnitExpect>,
    /// Expected `plan.unattributed` length.
    pub expect_unattributed: usize,
}

impl GoldenCase {
    /// Run the real planner over this case's inputs.
    ///
    /// # Errors
    /// Propagates [`plan_release`] errors (spec/cycle problems).
    pub fn plan(&self) -> Result<ReleasePlan, tarsi_core::plan::PlanError> {
        // Fixed date keeps rendered changelogs deterministic.
        plan_release(&self.specs, &self.commits, Some("2026-01-01".to_string()))
    }

    /// Diff the real plan against expectations — empty `Vec` =
    /// pass, otherwise one human-readable string per mismatch.
    #[must_use]
    pub fn check(&self) -> Vec<String> {
        let mut bad = Vec::new();
        let plan = match self.plan() {
            Ok(p) => p,
            Err(e) => return vec![format!("plan_release failed: {e}")],
        };
        if plan.units.len() != self.expect_units.len() {
            bad.push(format!(
                "unit count: got {} ({:?}), want {}",
                plan.units.len(),
                plan.units.iter().map(|u| &u.spec.name).collect::<Vec<_>>(),
                self.expect_units.len()
            ));
        }
        for (i, exp) in self.expect_units.iter().enumerate() {
            let Some(u) = plan.units.get(i) else {
                bad.push(format!("missing unit {} at position {i}", exp.name));
                continue;
            };
            if u.spec.name != exp.name {
                bad.push(format!(
                    "unit[{i}] name: got {:?}, want {:?}",
                    u.spec.name, exp.name
                ));
                continue;
            }
            if u.bump != exp.bump {
                bad.push(format!(
                    "{}: bump got {:?}, want {:?}",
                    exp.name, u.bump, exp.bump
                ));
            }
            if u.new_version.to_string() != exp.new_version {
                bad.push(format!(
                    "{}: version got {}, want {}",
                    exp.name, u.new_version, exp.new_version
                ));
            }
            if u.tag != exp.tag {
                bad.push(format!(
                    "{}: tag got {:?}, want {:?}",
                    exp.name, u.tag, exp.tag
                ));
            }
            let md = u.changelog.render();
            for needle in &exp.changelog_contains {
                if !md.contains(needle.as_str()) {
                    bad.push(format!(
                        "{}: changelog missing {needle:?}\n---\n{md}\n---",
                        exp.name
                    ));
                }
            }
        }
        if plan.unattributed.len() != self.expect_unattributed {
            bad.push(format!(
                "unattributed: got {}, want {}",
                plan.unattributed.len(),
                self.expect_unattributed
            ));
        }
        bad
    }
}

fn spec(name: &str, path: &str, version: &str, deps: &[&str]) -> ReleaseUnitSpec {
    ReleaseUnitSpec {
        name: name.to_string(),
        path: path.to_string(),
        current_version: Version::parse(version).expect("golden spec version"),
        tag_format: tarsi_core::unit::DEFAULT_TAG_FORMAT.to_string(),
        bump: None,
        publish: vec![PublishTarget::CratesIo, PublishTarget::GithubRelease],
        depends_on: deps.iter().map(|s| (*s).to_string()).collect(),
    }
}

fn unit(
    name: &str,
    bump: Bump,
    new_version: &str,
    tag: &str,
    changelog_contains: &[&str],
) -> UnitExpect {
    UnitExpect {
        name: name.to_string(),
        bump,
        new_version: new_version.to_string(),
        tag: tag.to_string(),
        changelog_contains: changelog_contains
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    }
}

/// The whole golden corpus.
#[must_use]
pub fn cases() -> Vec<GoldenCase> {
    vec![
        GoldenCase {
            name: "mixed-single-unit",
            specs: vec![spec("app", ".", "0.3.1", &[])],
            commits: fixtures::synthetic_commits(),
            expect_units: vec![unit(
                "app",
                Bump::Major,
                "1.0.0",
                "app-v1.0.0",
                &["add widget API", "crash on empty args", "drop --legacy flag"],
            )],
            // Whole-repo unit path "." attributes every commit.
            expect_unattributed: 0,
        },
        GoldenCase {
            name: "workspace-topo",
            specs: vec![
                spec("core", "crates/core", "0.1.0", &[]),
                spec("cli", "crates/cli", "0.1.0", &["core"]),
            ],
            commits: fixtures::synthetic_commits(),
            expect_units: vec![
                // topo: core publishes before cli (cli depends on it)
                unit("core", Bump::Minor, "0.2.0", "core-v0.2.0", &["add widget API"]),
                // fix + breaking feat! → major
                unit(
                    "cli",
                    Bump::Major,
                    "1.0.0",
                    "cli-v1.0.0",
                    &["crash on empty args", "drop --legacy flag"],
                ),
            ],
            // Cargo.lock + README.md touch neither unit path.
            expect_unattributed: 2,
        },
        GoldenCase {
            name: "chore-only-none",
            specs: vec![spec("lib", "crates/lib", "1.4.0", &[])],
            commits: fixtures::chore_only_commits(),
            // chore/docs/test kinds aren't changelog-worthy — the
            // rendered block carries the version heading only.
            expect_units: vec![unit("lib", Bump::None, "1.4.0", "lib-v1.4.0", &[])],
            expect_unattributed: 0,
        },
        GoldenCase {
            name: "real-tarsi-serve-range",
            specs: vec![spec("tarsi-serve", "crates/tarsi-serve", "0.1.0", &[])],
            // `git log --format='%h|%s' --name-only` over
            // crates/tarsi-serve, scaffold → 6bac2ec (D3), newest
            // first. Real org history — the B3 "real commit ranges"
            // clause; more repos' ranges slot in beside it.
            commits: vec![
                Commit {
                    sha: "110a329".into(),
                    subject: "feat(config): type:tarsi release profiles — layered config + store discovery + 9aia-release pack".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tarsi-serve/src/lib.rs".into(),
                        "crates/tarsi-serve/tests/profile_e2e.rs".into(),
                    ],
                },
                Commit {
                    sha: "ddd64e6".into(),
                    subject: "fix(serve): T-033 cosmetic nits — changelog newline/order, plan publish enabled".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tarsi-serve/src/lib.rs".into(),
                        "crates/tarsi-serve/tests/plan_e2e.rs".into(),
                    ],
                },
                Commit {
                    sha: "b273aff".into(),
                    subject: "fix(serve): release op — push before publish, error propagation, scoped staging, rollback".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tarsi-serve/src/lib.rs".into(),
                        "crates/tarsi-serve/tests/release_e2e.rs".into(),
                    ],
                },
                Commit {
                    sha: "f75ea14".into(),
                    subject: "chore(deps): bump lasius-vcs(-git) pin to 0.4.0".into(),
                    body: String::new(),
                    paths: vec!["crates/tarsi-serve/Cargo.toml".into()],
                },
                Commit {
                    sha: "6c2f62b".into(),
                    subject: "feat(ghreleases): release asset upload — globs, dry-run report, idempotent replace".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tarsi-serve/src/lib.rs".into(),
                        "crates/tarsi-serve/tests/plan_e2e.rs".into(),
                    ],
                },
                Commit {
                    sha: "1cd7de4".into(),
                    subject: "refactor(serve): one-shot via hypha-serve-cli (P-020)".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tarsi-serve/Cargo.toml".into(),
                        "crates/tarsi-serve/src/main.rs".into(),
                    ],
                },
                Commit {
                    sha: "f06c93e".into(),
                    subject: "feat: tarsi.toml dogfood config + golden-fixture e2e (B3 partial)".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tarsi-serve/Cargo.toml".into(),
                        "crates/tarsi-serve/tests/plan_e2e.rs".into(),
                    ],
                },
                Commit {
                    sha: "e61513b".into(),
                    subject: "feat(serve): one-shot CLI mode — <op> --dir --set --yes".into(),
                    body: String::new(),
                    paths: vec!["crates/tarsi-serve/src/main.rs".into()],
                },
            ],
            expect_units: vec![unit(
                "tarsi-serve",
                Bump::Minor,
                "0.2.0",
                "tarsi-serve-v0.2.0",
                &[
                    "release profiles",
                    "push before publish",
                    "one-shot via hypha-serve-cli",
                    "release asset upload",
                ],
            )],
            // Every commit in the range touches crates/tarsi-serve.
            expect_unattributed: 0,
        },
    ]
}
