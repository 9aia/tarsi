//! The release plan — pure computation from unit specs + commits.
//! `plan_release` attributes commits to units by path prefix,
//! infers bumps, topo-sorts publish order, and emits the ordered
//! [`ReleaseStep`]s a driver/serve op would execute.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::bump::Bump;
use crate::changelog::{Changelog, ChangelogEntry};
use crate::commit::{Commit, ConventionalCommit};
use crate::strategy::Strategy;
use crate::unit::ReleaseUnitSpec;

/// One unit's resolved release decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitPlan {
    pub spec: ReleaseUnitSpec,
    /// Inferred or overridden bump.
    pub bump: Bump,
    pub new_version: Version,
    pub tag: String,
    /// Conventional commits attributed to this unit (newest first,
    /// as supplied).
    pub commits: Vec<Commit>,
    /// Parsed views of `commits` (same order).
    pub conventional: Vec<ConventionalCommit>,
    /// Rendered changelog block for this unit's release.
    pub changelog: Changelog,
}

impl UnitPlan {
    /// Whether this unit actually releases (bump != none).
    #[must_use]
    pub fn releases(&self) -> bool {
        self.bump.releases()
    }
}

/// An ordered, executable step in the release. Steps are data —
/// `tarsi-serve`/drivers interpret them; core never performs I/O.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum ReleaseStep {
    /// Splice each releasing unit's changelog into its
    /// `CHANGELOG.md` (or the configured path).
    WriteChangelogs,
    /// Bump versions in manifests (`cargo set-version`-equivalent).
    BumpManifests,
    /// Commit the version bump.
    CommitBump,
    /// Create the unit's tag(s) — one per releasing unit.
    Tag,
    /// Publish crates in dependency order (per-unit `cargo publish`).
    PublishCrates,
    /// Create GitHub Releases with rendered notes.
    PublishGithubReleases,
    /// Push commits + tags.
    Push,
}

/// The full plan for one release invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePlan {
    /// Units in publish order (topo-sorted on `depends_on`).
    pub units: Vec<UnitPlan>,
    /// Commits that matched no unit path — surfaced so callers can
    /// warn rather than silently drop them.
    pub unattributed: Vec<Commit>,
}

impl ReleasePlan {
    /// Ordered steps this plan implies. Steps appear once per plan
    /// (not per unit) except publish steps which name their unit.
    #[must_use]
    pub fn steps(&self) -> Vec<ReleaseStep> {
        // Push precedes publish — execution pushes the bump commit
        // and tags *before* any forge call (GitHub Releases
        // auto-create `refs/tags/<tag>` at remote HEAD otherwise).
        let mut steps = vec![
            ReleaseStep::WriteChangelogs,
            ReleaseStep::BumpManifests,
            ReleaseStep::CommitBump,
            ReleaseStep::Tag,
            ReleaseStep::Push,
        ];
        for u in self.units.iter().filter(|u| u.releases()) {
            if u.spec
                .publish
                .contains(&crate::unit::PublishTarget::CratesIo)
            {
                steps.push(ReleaseStep::PublishCrates);
                break;
            }
        }
        for u in self.units.iter().filter(|u| u.releases()) {
            if u.spec
                .publish
                .contains(&crate::unit::PublishTarget::GithubRelease)
            {
                steps.push(ReleaseStep::PublishGithubReleases);
                break;
            }
        }
        steps
    }

    /// Units that will actually release.
    pub fn releasing(&self) -> impl Iterator<Item = &UnitPlan> {
        self.units.iter().filter(|u| u.releases())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("unit {0:?} depends on unknown unit {1:?}")]
    UnknownDep(String, String),
    #[error("dependency cycle among units: {0}")]
    Cycle(String),
    #[error("duplicate unit name {0:?}")]
    DuplicateUnit(String),
    #[error("no release units resolved — check units config")]
    Empty,
}

/// Plan a release under the `conventional` strategy — the
/// pre-P-030 default. Equivalent to
/// [`plan_release_with_strategy`] with [`Strategy::conventional`].
///
/// `specs` may come from [`UnitsSpec::CargoWorkspace`] resolved by
/// the caller, or be passed directly — this function is pure.
pub fn plan_release(
    specs: &[ReleaseUnitSpec],
    commits: &[Commit],
    date: Option<String>,
) -> Result<ReleasePlan, PlanError> {
    plan_release_with_strategy(specs, commits, date, &Strategy::conventional())
}

/// Plan a release under an explicit [`Strategy`] (P-030) — bump
/// inference and changelog sectioning are data-driven; the
/// `conventional` builtin reproduces the legacy hardcoded policy
/// byte-identically.
///
/// `specs` may come from [`UnitsSpec::CargoWorkspace`] resolved by
/// the caller, or be passed directly — this function is pure.
pub fn plan_release_with_strategy(
    specs: &[ReleaseUnitSpec],
    commits: &[Commit],
    date: Option<String>,
    strategy: &Strategy,
) -> Result<ReleasePlan, PlanError> {
    if specs.is_empty() {
        return Err(PlanError::Empty);
    }
    let mut seen = BTreeSet::new();
    for s in specs {
        if !seen.insert(s.name.as_str()) {
            return Err(PlanError::DuplicateUnit(s.name.clone()));
        }
        for dep in &s.depends_on {
            if !specs.iter().any(|o| &o.name == dep) {
                return Err(PlanError::UnknownDep(s.name.clone(), dep.clone()));
            }
        }
    }

    let mut units = Vec::with_capacity(specs.len());
    let mut unattributed = Vec::new();
    for spec in specs {
        let mine: Vec<Commit> = commits
            .iter()
            .filter(|c| commit_touches(c, &spec.path))
            .cloned()
            .collect();
        // Inference fold: a commit contributes the max bump across
        // its matching rules; the unit takes the max across commits.
        let mut inferred = Bump::None;
        let mut conventional = Vec::new();
        let mut changelog =
            Changelog::for_strategy(&strategy.changelog.sections, String::new(), date.clone());
        for c in &mine {
            let Some(cc) = c.conventional() else {
                continue;
            };
            conventional.push(cc.clone());
            inferred = inferred.max(strategy.bump_for(&cc));
            if let Some(sec) = strategy.section_for(&cc) {
                changelog.push(
                    &sec.key,
                    &sec.heading,
                    ChangelogEntry::from_commit(&c.sha, &cc),
                );
            }
        }
        // `spec.bump` (unit override) wins outright; otherwise the
        // inferred bump, falling back to the strategy's `fallback`
        // (`release.default_bump` is folded in by the caller) when
        // inference finds nothing.
        let bump = spec.bump.unwrap_or_else(|| {
            if inferred.releases() {
                inferred
            } else {
                strategy.bump.fallback.unwrap_or(Bump::None)
            }
        });
        let new_version = bump.apply(&spec.current_version);
        changelog.version = new_version.to_string();
        units.push(UnitPlan {
            tag: spec.tag(&new_version),
            spec: spec.clone(),
            bump,
            new_version,
            commits: mine,
            conventional,
            changelog,
        });
    }

    // Unattributed = commits touching no unit path. A commit with
    // empty `paths` is conservatively attributed to every unit
    // (unknown granularity), so it never lands here.
    for c in commits {
        if !specs.iter().any(|s| commit_touches(c, &s.path)) {
            unattributed.push(c.clone());
        }
    }

    let ordered = topo_sort(&units)?;
    Ok(ReleasePlan {
        units: ordered,
        unattributed,
    })
}

/// Path-prefix attribution. Empty commit `paths` = unknown →
/// matches every unit (can't prove it doesn't touch the unit).
/// Empty unit path = whole repo.
fn commit_touches(commit: &Commit, unit_path: &str) -> bool {
    if unit_path.is_empty() || unit_path == "." {
        return true;
    }
    if commit.paths.is_empty() {
        return true;
    }
    let prefix = unit_path.trim_end_matches('/');
    commit
        .paths
        .iter()
        .any(|p| p == prefix || p.starts_with(&format!("{prefix}/")))
}

// NOTE: resolving `UnitsSpec::CargoWorkspace` into concrete specs
// (manifest scanning) is intentionally NOT here — core stays pure.
// That resolution belongs to a manifest/`tarsi-config` layer that
// feeds `plan_release` explicit `ReleaseUnitSpec`s.

/// Kahn's algorithm over `depends_on`. Returns units in publish
/// order — dependencies first.
fn topo_sort(units: &[UnitPlan]) -> Result<Vec<UnitPlan>, PlanError> {
    let by_name: BTreeMap<&str, &UnitPlan> =
        units.iter().map(|u| (u.spec.name.as_str(), u)).collect();
    let mut indegree: BTreeMap<&str, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for u in units {
        indegree.entry(u.spec.name.as_str()).or_insert(0);
    }
    for u in units {
        for dep in &u.spec.depends_on {
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(u.spec.name.as_str());
            if let Some(d) = indegree.get_mut(u.spec.name.as_str()) {
                *d += 1;
            }
        }
    }
    // Stable-ish: BTreeMap iteration gives alphabetical tie-break.
    let mut queue: VecDeque<&str> = indegree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(&n, _)| n)
        .collect();
    let mut out = Vec::with_capacity(units.len());
    while let Some(name) = queue.pop_front() {
        let unit = by_name[name];
        out.push(unit.clone());
        if let Some(deps) = dependents.get(name) {
            for &dep_name in deps {
                if let Some(d) = indegree.get_mut(dep_name) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(dep_name);
                    }
                }
            }
        }
    }
    if out.len() != units.len() {
        let stuck: Vec<&str> = indegree
            .iter()
            .filter(|(_, &d)| d > 0)
            .map(|(&n, _)| n)
            .collect();
        return Err(PlanError::Cycle(stuck.join(", ")));
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::unit::PublishTarget;

    fn spec(name: &str, path: &str, deps: &[&str]) -> ReleaseUnitSpec {
        ReleaseUnitSpec {
            name: name.into(),
            path: path.into(),
            current_version: Version::parse("0.1.0").unwrap(),
            tag_format: "{unit}-v{version}".into(),
            bump: None,
            publish: vec![PublishTarget::CratesIo],
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn commit(sha: &str, subject: &str, paths: &[&str]) -> Commit {
        Commit {
            sha: sha.into(),
            subject: subject.into(),
            body: String::new(),
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn attributes_and_infers_bumps() {
        let specs = vec![
            spec("core", "crates/core", &[]),
            spec("cli", "crates/cli", &["core"]),
        ];
        let commits = vec![
            commit(
                "aaa1111",
                "feat(core): add thing",
                &["crates/core/src/lib.rs"],
            ),
            commit("bbb2222", "fix(cli): crash", &["crates/cli/src/main.rs"]),
            commit(
                "ccc3333",
                "feat(cli)!: break api",
                &["crates/cli/src/lib.rs"],
            ),
            commit("ddd4444", "docs: readme", &["README.md"]),
        ];
        let plan = plan_release(&specs, &commits, None).unwrap();
        // topo: core before cli
        assert_eq!(plan.units[0].spec.name, "core");
        assert_eq!(plan.units[1].spec.name, "cli");
        assert_eq!(plan.units[0].bump, Bump::Minor);
        assert_eq!(plan.units[0].new_version.to_string(), "0.2.0");
        assert_eq!(plan.units[0].tag, "core-v0.2.0");
        assert_eq!(plan.units[1].bump, Bump::Major);
        assert_eq!(plan.units[1].tag, "cli-v1.0.0");
        // docs commit attributed to both (path "" matches? no —
        // unit paths are non-empty; README.md touches neither)
        assert_eq!(plan.unattributed.len(), 1);
        assert_eq!(plan.unattributed[0].sha, "ddd4444");
    }

    #[test]
    fn chore_only_range_is_none_bump() {
        let specs = vec![spec("lib", "crates/lib", &[])];
        let commits = vec![commit("x1", "chore: tidy", &["crates/lib/a.rs"])];
        let plan = plan_release(&specs, &commits, None).unwrap();
        assert_eq!(plan.units[0].bump, Bump::None);
        assert!(!plan.units[0].releases());
        assert!(plan.releasing().next().is_none());
    }

    #[test]
    fn empty_paths_attributed_everywhere() {
        let specs = vec![spec("a", "crates/a", &[]), spec("b", "crates/b", &[])];
        let commits = vec![commit("x1", "feat: shared thing", &[])];
        let plan = plan_release(&specs, &commits, None).unwrap();
        assert!(plan.unattributed.is_empty());
        assert_eq!(
            plan.units.iter().filter(|u| u.bump == Bump::Minor).count(),
            2
        );
    }

    #[test]
    fn explicit_override_beats_inference() {
        let mut s = spec("lib", "crates/lib", &[]);
        s.bump = Some(Bump::Patch);
        let commits = vec![commit("x1", "feat: big", &["crates/lib/a.rs"])];
        let plan = plan_release(&[s], &commits, None).unwrap();
        assert_eq!(plan.units[0].bump, Bump::Patch);
    }

    #[test]
    fn errors() {
        assert!(matches!(
            plan_release(&[], &[], None),
            Err(PlanError::Empty)
        ));
        let mut s = spec("a", "", &[]);
        s.depends_on = vec!["ghost".into()];
        assert!(matches!(
            plan_release(&[s], &[], None),
            Err(PlanError::UnknownDep(_, _))
        ));
        let mut a = spec("a", "", &[]);
        a.depends_on = vec!["b".into()];
        let mut b = spec("b", "", &[]);
        b.depends_on = vec!["a".into()];
        assert!(matches!(
            plan_release(&[a, b], &[], None),
            Err(PlanError::Cycle(_))
        ));
    }

    #[test]
    fn steps_follow_publish_targets() {
        let specs = vec![spec("lib", "", &[])];
        let commits = vec![commit("x1", "feat: x", &[])];
        let plan = plan_release(&specs, &commits, None).unwrap();
        let steps = plan.steps();
        assert!(matches!(steps[0], ReleaseStep::WriteChangelogs));
        assert!(steps.contains(&ReleaseStep::PublishCrates));
        assert!(!steps.contains(&ReleaseStep::PublishGithubReleases));
        // Push before publish — matches execution order.
        let push = steps.iter().position(|s| *s == ReleaseStep::Push).unwrap();
        let publish = steps
            .iter()
            .position(|s| *s == ReleaseStep::PublishCrates)
            .unwrap();
        assert!(push < publish, "push must precede publish: {steps:?}");
    }
}
