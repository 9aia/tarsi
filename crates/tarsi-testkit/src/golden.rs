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
//! - `real-myco-range` — the real `git log` range of the myco repo
//!   between release tags `myco-v0.1.1..myco-v0.1.2` (11 commits,
//!   whole-repo unit at path `.`).
//! - `real-tamias-core-range` — the real `git log` range of the
//!   tamias repo between per-unit tags
//!   `tamias-core-v0.1.3..tamias-core-v0.1.4` (4 commits, unit path
//!   `crates/tamias-core` — exercises path-prefix attribution and a
//!   `release:` noise commit that parses as `Other`).
//!
//! Strategy cases (P-030) — `strategy: None` cases are the
//! `conventional` parity gate; these exercise other strategies:
//!
//! - `semantic-release-angular` — the angular commit-analyzer
//!   preset as data: `revert`→patch joins the bump table,
//!   BREAKING CHANGES/Features/Bug Fixes/… section headings.
//! - `custom-deps-strategy` — an inline rule table naming the
//!   arbitrary raw type `deps` (unreachable via `CommitKind`)
//!   plus a `fallback` bump for unmatched units.

use semver::Version;
use tarsi_core::bump::Bump;
use tarsi_core::commit::Commit;
use tarsi_core::plan::{plan_release_with_strategy, ReleasePlan};
use tarsi_core::strategy::{
    BumpRule, BumpRules, ChangelogRules, SectionRule, SectionSpec, Strategy,
};
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
    /// Release strategy (P-030) — `None` = `conventional`, the
    /// parity baseline every pre-existing case exercises.
    pub strategy: Option<Strategy>,
    /// Expected units **in plan order** (topo-sorted).
    pub expect_units: Vec<UnitExpect>,
    /// Expected `plan.unattributed` length.
    pub expect_unattributed: usize,
}

impl GoldenCase {
    /// Run the real planner over this case's inputs.
    ///
    /// # Errors
    /// Propagates [`plan_release_with_strategy`] errors (spec/cycle problems).
    pub fn plan(&self) -> Result<ReleasePlan, tarsi_core::plan::PlanError> {
        // Fixed date keeps rendered changelogs deterministic.
        let strategy = self.strategy.clone().unwrap_or_default();
        plan_release_with_strategy(
            &self.specs,
            &self.commits,
            Some("2026-01-01".to_string()),
            &strategy,
        )
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

fn commit(sha: &str, subject: &str, paths: &[&str]) -> Commit {
    Commit {
        sha: sha.into(),
        subject: subject.into(),
        body: String::new(),
        paths: paths.iter().map(|s| (*s).to_string()).collect(),
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
            strategy: None,
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
            strategy: None,
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
            strategy: None,
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
            strategy: None,
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
        GoldenCase {
            name: "real-myco-range",
            // Whole-repo unit — the myco-v* tag tracks the shared
            // workspace version (tarsi.toml: versions are
            // workspace-inherited), so the release consumed the
            // entire range.
            specs: vec![spec("myco", ".", "0.1.1", &[])],
            // `git log --format='%h|%s' --name-only
            // myco-v0.1.1..myco-v0.1.2` in the myco repo, newest
            // first. Bodies omitted — none carry a BREAKING CHANGE
            // footer, so `body` stays empty (same convention as
            // real-tarsi-serve-range).
            commits: vec![
                Commit {
                    sha: "4246abf".into(),
                    subject: "release: bump crate versions".into(),
                    body: String::new(),
                    paths: vec!["Cargo.lock".into(), "Cargo.toml".into()],
                },
                Commit {
                    sha: "c9e8c7c".into(),
                    subject: "fix(release): single workspace-version bump; tag version from root manifest".into(),
                    body: String::new(),
                    paths: vec!["scripts/release.sh".into()],
                },
                Commit {
                    sha: "8ad39b0".into(),
                    subject: "feat(sync): adopt remote ledgers, sync verb, behind-remote guard".into(),
                    body: String::new(),
                    paths: vec![
                        ".kuda/index.yaml".into(),
                        ".kuda/modules/docs.yaml".into(),
                        ".kuda/modules/myco-config-toml.yaml".into(),
                        ".kuda/modules/myco-config.yaml".into(),
                        ".kuda/modules/myco-core.yaml".into(),
                        ".kuda/modules/myco-driver.yaml".into(),
                        ".kuda/modules/myco-git.yaml".into(),
                        ".kuda/modules/myco-index-redb.yaml".into(),
                        ".kuda/modules/myco-index.yaml".into(),
                        ".kuda/modules/myco-ops.yaml".into(),
                        ".kuda/modules/myco-shell.yaml".into(),
                        ".kuda/modules/myco-store-md.yaml".into(),
                        ".kuda/modules/myco-store.yaml".into(),
                        ".kuda/modules/myco-testkit.yaml".into(),
                        ".kuda/modules/myco-tui.yaml".into(),
                        ".kuda/modules/myco.yaml".into(),
                        "crates/myco-cli/src/check.rs".into(),
                        "crates/myco-cli/src/commit.rs".into(),
                        "crates/myco-cli/src/init.rs".into(),
                        "crates/myco-cli/src/lib.rs".into(),
                        "crates/myco-cli/src/registry.rs".into(),
                        "crates/myco-cli/src/sync.rs".into(),
                        "crates/myco-cli/tests/edges.rs".into(),
                        "crates/myco-git/src/lib.rs".into(),
                        "crates/myco-ops/src/items.rs".into(),
                        "crates/myco-ops/src/ops.rs".into(),
                    ],
                },
                Commit {
                    sha: "07fb8c7".into(),
                    subject: "docs: boundary — bupha is the CI repo; gate migrating from peregrin ci-gate".into(),
                    body: String::new(),
                    paths: vec![
                        "AGENTS.md".into(),
                        "CONTRIBUTING.md".into(),
                        "README.md".into(),
                    ],
                },
                Commit {
                    sha: "54c151b".into(),
                    subject: "fix(cli): view about-text lists the next preset".into(),
                    body: String::new(),
                    paths: vec!["crates/myco-cli/src/registry.rs".into()],
                },
                Commit {
                    sha: "1ffade7".into(),
                    subject: "chore(kuda): regen cards for priority-token emit change (I-032)".into(),
                    body: String::new(),
                    paths: vec![
                        ".kuda/index.yaml".into(),
                        ".kuda/modules/docs.yaml".into(),
                        ".kuda/modules/myco-core.yaml".into(),
                        ".kuda/modules/myco-store-md.yaml".into(),
                        ".kuda/modules/myco.yaml".into(),
                        ".kuda/modules/project.yaml".into(),
                    ],
                },
                Commit {
                    sha: "5c96eff".into(),
                    subject: "fix(store-md): frontmatter priority emits pri: tokens, not serde names (I-032)".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/myco-cli/tests/id_repair.rs".into(),
                        "crates/myco-cli/tests/items.rs".into(),
                        "crates/myco-cli/tests/parity.rs".into(),
                        "crates/myco-core/src/priority.rs".into(),
                        "crates/myco-store-md/src/itemfile.rs".into(),
                        "crates/myco-testkit/fixtures/items/adrs/0001-markdown-store.md".into(),
                        "crates/myco-testkit/fixtures/items/plans/pl-0001-v2-rollout.md".into(),
                        "crates/myco-testkit/fixtures/items/proposals/proposal-0002-agent-verbs.md".into(),
                        "crates/myco-testkit/tests/snapshots/golden__items__adrs__0001-markdown-store_md.snap".into(),
                        "crates/myco-testkit/tests/snapshots/golden__items__plans__pl-0001-v2-rollout_md.snap".into(),
                        "crates/myco-testkit/tests/snapshots/golden__items__proposals__proposal-0002-agent-verbs_md.snap".into(),
                    ],
                },
                Commit {
                    sha: "0ef8b79".into(),
                    subject: "feat: engine-as-lib + tamias driver ops pattern (kuda parity)".into(),
                    body: String::new(),
                    paths: vec![
                        ".kuda/index.yaml".into(),
                        ".kuda/modules/docs.yaml".into(),
                        ".kuda/modules/myco-driver.yaml".into(),
                        ".kuda/modules/myco.yaml".into(),
                        ".kuda/modules/project.yaml".into(),
                        "AGENTS.md".into(),
                        "Cargo.lock".into(),
                        "crates/myco-cli/src/continuity.rs".into(),
                        "crates/myco-cli/src/items.rs".into(),
                        "crates/myco-cli/src/lib.rs".into(),
                        "crates/myco-cli/src/main.rs".into(),
                        "crates/myco-cli/src/serve.rs".into(),
                        "crates/myco-driver/Cargo.toml".into(),
                        "crates/myco-driver/src/main.rs".into(),
                        "crates/myco-driver/src/ops.rs".into(),
                        "crates/myco-driver/src/surface.rs".into(),
                    ],
                },
                Commit {
                    sha: "be47760".into(),
                    subject: "docs: bupha retired — gate is the peregrin ci-gate campaign; releases manual".into(),
                    body: String::new(),
                    paths: vec![
                        "AGENTS.md".into(),
                        "CONTRIBUTING.md".into(),
                        "README.md".into(),
                        "scripts/ci.sh".into(),
                        "scripts/release.sh".into(),
                    ],
                },
                Commit {
                    sha: "df1cee2".into(),
                    subject: "feat: item scheduling fields due/est/cycle (T-081, P-012 ph1)".into(),
                    body: String::new(),
                    paths: vec![
                        ".agents/skills/myco/SKILL.md".into(),
                        ".kuda/index.yaml".into(),
                        ".kuda/modules/myco-config-toml.yaml".into(),
                        ".kuda/modules/myco-config.yaml".into(),
                        ".kuda/modules/myco-core.yaml".into(),
                        ".kuda/modules/myco-driver.yaml".into(),
                        ".kuda/modules/myco-git.yaml".into(),
                        ".kuda/modules/myco-index-redb.yaml".into(),
                        ".kuda/modules/myco-index.yaml".into(),
                        ".kuda/modules/myco-ops.yaml".into(),
                        ".kuda/modules/myco-shell.yaml".into(),
                        ".kuda/modules/myco-store-md.yaml".into(),
                        ".kuda/modules/myco-store.yaml".into(),
                        ".kuda/modules/myco-testkit.yaml".into(),
                        ".kuda/modules/myco-tui.yaml".into(),
                        ".kuda/modules/myco.yaml".into(),
                        ".kuda/modules/project.yaml".into(),
                        "crates/myco-cli/src/itemcmd.rs".into(),
                        "crates/myco-cli/src/items.rs".into(),
                        "crates/myco-cli/src/movecmd.rs".into(),
                        "crates/myco-cli/src/registry.rs".into(),
                        "crates/myco-cli/src/schema.rs".into(),
                        "crates/myco-cli/src/serve.rs".into(),
                        "crates/myco-cli/src/view.rs".into(),
                        "crates/myco-cli/tests/parity.rs".into(),
                        "crates/myco-cli/tests/snapshots/views__schema_document_snapshot.snap".into(),
                        "crates/myco-core/src/item.rs".into(),
                        "crates/myco-core/src/lib.rs".into(),
                        "crates/myco-core/src/view.rs".into(),
                        "crates/myco-ops/src/items.rs".into(),
                        "crates/myco-store-md/src/itemfile.rs".into(),
                        "crates/myco-store-md/src/line.rs".into(),
                        "crates/myco-testkit/fixtures/items/issues/issue-0003-memory-leak.md".into(),
                        "crates/myco-testkit/fixtures/todo/basic.md".into(),
                        "crates/myco-testkit/tests/snapshots/golden__items__issues__issue-0003-memory-leak_md.snap".into(),
                        "crates/myco-testkit/tests/snapshots/golden__todo__basic_md.snap".into(),
                        "crates/myco-testkit/tests/snapshots/invalid__todo__invalid__unknown-token_md.snap".into(),
                        "crates/myco-tui/src/app.rs".into(),
                        "crates/myco-tui/src/render.rs".into(),
                    ],
                },
                Commit {
                    sha: "deff1b5".into(),
                    subject: "fix(dist): cargo-dist 0.33 compat — ci/installers as empty arrays".into(),
                    body: String::new(),
                    paths: vec!["dist-workspace.toml".into()],
                },
            ],
            // Three feats → minor; the real myco-v0.1.2 tag was a
            // manual patch bump, but the corpus asserts what tarsi
            // infers from the range, not history reproduction.
            strategy: None,
            expect_units: vec![unit(
                "myco",
                Bump::Minor,
                "0.2.0",
                "myco-v0.2.0",
                &[
                    "adopt remote ledgers, sync verb, behind-remote guard",
                    "engine-as-lib + tamias driver ops pattern",
                    "item scheduling fields due/est/cycle",
                    "frontmatter priority emits pri: tokens",
                    "single workspace-version bump",
                    "view about-text lists the next preset",
                    "cargo-dist 0.33 compat",
                ],
            )],
            // Whole-repo unit path "." attributes every commit.
            expect_unattributed: 0,
        },
        GoldenCase {
            name: "real-tamias-core-range",
            specs: vec![spec("tamias-core", "crates/tamias-core", "0.1.3", &[])],
            // `git log --format='%h|%s' --name-only
            // tamias-core-v0.1.3..tamias-core-v0.1.4 --
            // crates/tamias-core` in the tamias repo, newest first.
            // Per-unit tag range; bodies omitted (no BREAKING CHANGE
            // footers).
            commits: vec![
                Commit {
                    sha: "255f44c".into(),
                    subject: "feat(driver): deploy.d operator config overrides + tamias deploy --set (T-026)".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tamias-core/src/lib.rs".into(),
                        "crates/tamias-core/src/overlay.rs".into(),
                    ],
                },
                Commit {
                    sha: "fe7b618".into(),
                    subject: "release: bump crate versions".into(),
                    body: String::new(),
                    paths: vec!["crates/tamias-core/Cargo.toml".into()],
                },
                Commit {
                    sha: "6383087".into(),
                    subject: "fix(core): pack_tgz never emits GNUSparse tar entries".into(),
                    body: String::new(),
                    paths: vec!["crates/tamias-core/src/archive.rs".into()],
                },
                Commit {
                    sha: "ad841f5".into(),
                    subject: "feat: declared bins lifecycle — bins[] manifests, tools/bin payloads, tamias bin verbs".into(),
                    body: String::new(),
                    paths: vec![
                        "crates/tamias-core/src/lib.rs".into(),
                        "crates/tamias-core/src/manifest.rs".into(),
                    ],
                },
            ],
            // Two feats → minor (real tag was a workspace patch bump).
            // The `release:` commit parses as Other — attributed to
            // the unit by path but dropped from bump + changelog.
            strategy: None,
            expect_units: vec![unit(
                "tamias-core",
                Bump::Minor,
                "0.2.0",
                "tamias-core-v0.2.0",
                &[
                    "deploy.d operator config overrides",
                    "pack_tgz never emits GNUSparse tar entries",
                    "declared bins lifecycle",
                ],
            )],
            // Every commit in the range touches crates/tamias-core.
            expect_unattributed: 0,
        },
        GoldenCase {
            name: "semantic-release-angular",
            specs: vec![spec("app", ".", "0.3.1", &[])],
            strategy: Some(Strategy::semantic_release_angular()),
            commits: vec![
                commit("a1", "feat(core)!: redesign api", &[]),
                commit("a2", "revert: bad deploy", &[]),
                commit("a3", "perf: speed up parse", &[]),
                commit("a4", "fix: crash on empty", &[]),
                commit("a5", "feat: add search", &[]),
                commit("a6", "chore: tidy", &[]),
                commit("a7", "docs: readme", &[]),
            ],
            // breaking feat! → major wins the fold; revert→patch is
            // an angular-only rule conventional lacks.
            expect_units: vec![unit(
                "app",
                Bump::Major,
                "1.0.0",
                "app-v1.0.0",
                &[
                    "### BREAKING CHANGES",
                    "### Features",
                    "### Bug Fixes",
                    "### Performance Improvements",
                    "### Reverts",
                    "redesign api",
                    "add search",
                    "crash on empty",
                    "speed up parse",
                    "bad deploy",
                ],
            )],
            expect_unattributed: 0,
        },
        GoldenCase {
            name: "custom-deps-strategy",
            specs: vec![
                spec("app", "crates/app", "0.1.0", &[]),
                spec("lib", "crates/lib", "0.4.2", &[]),
            ],
            strategy: Some(Strategy {
                bump: BumpRules {
                    // fallback = the wired `default_bump` semantic.
                    fallback: Some(Bump::Patch),
                    rules: vec![BumpRule {
                        kind: Some("deps".into()),
                        scope: None,
                        breaking: None,
                        not_type: vec![],
                        bump: Bump::Minor,
                    }],
                },
                changelog: ChangelogRules {
                    sections: vec![SectionSpec {
                        key: "deps".into(),
                        heading: "Dependencies".into(),
                        rules: vec![SectionRule {
                            kind: Some("deps".into()),
                            scope: None,
                            breaking: None,
                            not_type: vec![],
                        }],
                    }],
                },
            }),
            commits: vec![
                commit("d1", "deps: bump serde", &["crates/app/Cargo.toml"]),
                commit("c1", "chore: tidy", &["crates/lib/src/lib.rs"]),
            ],
            // app: `deps` is CommitKind::Other — only the raw_type
            // rule can see it → minor. lib: chore matches nothing
            // → fallback patch.
            expect_units: vec![
                unit("app", Bump::Minor, "0.2.0", "app-v0.2.0", &["### Dependencies", "bump serde"]),
                unit("lib", Bump::Patch, "0.4.3", "lib-v0.4.3", &[]),
            ],
            expect_unattributed: 0,
        },
    ]
}
