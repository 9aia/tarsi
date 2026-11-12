//! `tarsi-serve` op conformance — the wire contract every VCS
//! impl (and every serve refactor) must satisfy. Each `assert_*`
//! drives [`tarsi_serve::dispatch`] against a caller-supplied
//! `&dyn lasius_vcs::Vcs` and a fixture dir (see [`crate::repo`]);
//! failures panic with the op + response, passes return the
//! response value for further assertions.
//!
//! Two tiers:
//!
//! - [`assert_conformance`] — read-only ops: `manifest`,
//!   `profiles`, `plan`, `changelog`, `notes`, plus the
//!   unknown-op error path. Safe on [`crate::repo::workspace_repo`]
//!   output as-is.
//! - [`assert_release_conformance`] — the mutating `release` op:
//!   requires a fixture with `origin` ([`crate::repo::workspace_repo`]
//!   / [`crate::repo::single_unit_repo`] both provide one) and a
//!   [`crate::repo::NO_PUBLISH_TOML`] `tarsi.toml` written by the
//!   caller, so no forge is touched. Asserts the step order,
//!   remote branch+tag state, changelog creation, and manifest
//!   bumps.

use std::path::Path;

use lasius_vcs::Vcs;
use serde_json::{json, Value};
use tarsi_serve::{dispatch, Request, Response};

use crate::repo;

/// One op call → response. Panics are the caller's job — this
/// returns the raw [`Response`].
#[must_use]
pub fn call(vcs: &dyn Vcs, op: &str, params: Value) -> Response {
    dispatch(
        vcs,
        &Request {
            op: op.to_string(),
            params,
        },
    )
}

/// `call` + unwrap the result value; panics on `ok:false`.
///
/// # Panics
/// When the op reports `ok:false`.
#[must_use]
pub fn ok(vcs: &dyn Vcs, op: &str, params: Value) -> Value {
    let resp = call(vcs, op, params);
    assert!(
        resp.ok,
        "op {op:?} failed: {:?} / result {:?}",
        resp.error, resp.result
    );
    resp.result.expect("ok response carries result")
}

fn dir_params(dir: &Path) -> Value {
    json!({ "dir": dir.to_string_lossy() })
}

/// `manifest` answers the op directory — must list every op the
/// serve surface implements.
///
/// # Panics
/// On contract violation.
pub fn assert_manifest_op(vcs: &dyn Vcs, dir: &Path) {
    let r = ok(vcs, "manifest", dir_params(dir));
    for op in ["plan", "changelog", "notes", "release"] {
        assert!(
            r["ops"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|o| o == op)),
            "manifest ops missing {op:?}: {r}"
        );
    }
}

/// `profiles` answers a list (env-dependent content — only the
/// shape is contractual).
///
/// # Panics
/// On contract violation.
pub fn assert_profiles_op(vcs: &dyn Vcs, dir: &Path) {
    let r = ok(vcs, "profiles", dir_params(dir));
    assert!(
        r["profiles"].is_array(),
        "profiles result must carry a list: {r}"
    );
}

/// `plan` on the workspace fixture: two units in topo order
/// (`aaa` before `bbb` — bbb path-deps aaa), minor/patch bumps,
/// rendered changelogs, declared publish targets annotated with
/// their effective `enabled` state. Returns the plan value.
///
/// # Panics
/// On contract violation.
#[must_use]
pub fn assert_plan_op(vcs: &dyn Vcs, dir: &Path) -> Value {
    let r = ok(vcs, "plan", dir_params(dir));
    let units = r["plan"]["units"]
        .as_array()
        .expect("plan.units must be an array");
    assert_eq!(units.len(), 2, "workspace fixture has two units: {r}");
    let names: Vec<&str> = units
        .iter()
        .map(|u| u["spec"]["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(names, vec!["aaa", "bbb"], "topo order (dep first): {r}");
    let aaa = &units[0];
    assert_eq!(aaa["bump"], "minor", "{aaa}");
    assert_eq!(aaa["new_version"], "0.2.0", "{aaa}");
    assert_eq!(aaa["tag"], "aaa-v0.2.0", "{aaa}");
    assert_eq!(aaa["releases"], true, "{aaa}");
    assert!(
        aaa["changelog_markdown"]
            .as_str()
            .is_some_and(|m| m.contains("add thing")),
        "aaa changelog renders its commit: {aaa}"
    );
    assert!(
        aaa["publish_targets"]
            .as_array()
            .is_some_and(|ts| ts.iter().any(|t| t["enabled"].is_boolean())),
        "publish targets carry effective enabled state: {aaa}"
    );
    let bbb = &units[1];
    assert_eq!(bbb["bump"], "patch", "{bbb}");
    assert_eq!(bbb["new_version"], "0.1.1", "{bbb}");
    assert_eq!(bbb["tag"], "bbb-v0.1.1", "{bbb}");
    r
}

/// `changelog` returns one entry per *releasing* unit with its
/// rendered markdown; `notes` concatenates the same markdown.
///
/// # Panics
/// On contract violation.
pub fn assert_changelog_notes_ops(vcs: &dyn Vcs, dir: &Path) {
    let r = ok(vcs, "changelog", dir_params(dir));
    let changelogs = r["changelogs"]
        .as_array()
        .expect("changelogs must be an array");
    assert_eq!(changelogs.len(), 2, "both units release: {r}");
    for c in changelogs {
        assert!(c["unit"].is_string(), "{c}");
        assert!(c["version"].is_string(), "{c}");
        assert!(c["tag"].is_string(), "{c}");
        assert!(
            c["markdown"].as_str().is_some_and(|m| m.contains("## [")),
            "markdown renders a version heading: {c}"
        );
    }

    let n = ok(vcs, "notes", dir_params(dir));
    let notes = n["notes"].as_str().expect("notes is a string");
    assert!(
        notes.contains("add thing"),
        "notes carry aaa's entry: {notes}"
    );
    assert!(
        notes.contains("crash on empty"),
        "notes carry bbb's: {notes}"
    );
}

/// `params.range` overrides the since-last-tag range for every
/// unit — `"<tag>..HEAD"` style rev syntax passes through to the
/// VCS port.
///
/// # Panics
/// On contract violation.
pub fn assert_range_param(vcs: &dyn Vcs, dir: &Path) {
    // Only the newest commit (fix(bbb)) is in range → aaa sees
    // nothing releasable, bbb still patches.
    let mut params = dir_params(dir);
    params["range"] = json!("HEAD~1..HEAD");
    let r = ok(vcs, "plan", params);
    let units = r["plan"]["units"]
        .as_array()
        .expect("plan.units must be an array");
    let aaa = units
        .iter()
        .find(|u| u["spec"]["name"] == "aaa")
        .expect("aaa unit");
    assert_eq!(aaa["releases"], false, "aaa out of range: {aaa}");
    let bbb = units
        .iter()
        .find(|u| u["spec"]["name"] == "bbb")
        .expect("bbb unit");
    assert_eq!(bbb["bump"], "patch", "{bbb}");
}

/// An unknown op is an error, not a panic or silent ok.
///
/// # Panics
/// On contract violation.
pub fn assert_unknown_op_errors(vcs: &dyn Vcs, dir: &Path) {
    let resp = call(vcs, "definitely-not-an-op", dir_params(dir));
    assert!(!resp.ok, "unknown op must fail: {resp:?}");
    assert!(
        resp.error.is_some(),
        "unknown op carries an error message: {resp:?}"
    );
}

/// The full read-only conformance tier — safe on a fresh
/// [`repo::workspace_repo`].
///
/// # Panics
/// On any contract violation.
pub fn assert_conformance(vcs: &dyn Vcs, dir: &Path) {
    assert_manifest_op(vcs, dir);
    assert_profiles_op(vcs, dir);
    let _plan = assert_plan_op(vcs, dir);
    assert_changelog_notes_ops(vcs, dir);
    assert_range_param(vcs, dir);
    assert_unknown_op_errors(vcs, dir);
}

/// `release` with no `dry_run` param defaults to dry-run: reports
/// the plan, runs no steps, mutates nothing (no tags, no
/// changelogs, no pushes).
///
/// # Panics
/// On contract violation.
pub fn assert_release_defaults_to_dry_run(vcs: &dyn Vcs, dir: &Path) {
    let before_tags = repo::git(dir, &["tag", "--list"]);
    let r = ok(vcs, "release", dir_params(dir));
    assert_eq!(r["dry_run"], true, "dry_run must default true: {r}");
    assert!(
        r["steps_run"].as_array().is_some_and(Vec::is_empty),
        "dry-run runs no steps: {r}"
    );
    assert_eq!(
        repo::git(dir, &["tag", "--list"]),
        before_tags,
        "dry-run creates no tags"
    );
    assert!(
        !dir.join("crates/aaa/CHANGELOG.md").exists(),
        "dry-run writes no changelog"
    );
    assert!(repo::ls_remote(dir).is_empty(), "dry-run pushes nothing");
}

/// The mutating `release` path (`dry_run: false`) on a fixture
/// with `origin` and publish targets disabled: changelogs →
/// manifests → scoped commit → tags → push, in that order; remote
/// branch and tags point at the bump commit; workspace version
/// bumped; nothing else staged.
///
/// # Panics
/// On contract violation.
pub fn assert_release_conformance(vcs: &dyn Vcs, dir: &Path) {
    let pre_head = repo::git(dir, &["rev-parse", "HEAD"]);
    let mut params = dir_params(dir);
    params["dry_run"] = json!(false);
    let r = ok(vcs, "release", params);
    assert_eq!(r["ok"], true, "release failed: {r}");
    assert_eq!(r["dry_run"], false, "{r}");

    let steps: Vec<&str> = r["steps_run"]
        .as_array()
        .expect("steps_run array")
        .iter()
        .map(|s| s.as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        steps,
        vec![
            "write-changelogs",
            "bump-manifests",
            "commit-bump",
            "tag",
            "push"
        ],
        "step order is contractual: {r}"
    );

    // Remote: branch + both tags at the bump commit.
    let bump = repo::git(dir, &["rev-parse", "HEAD"]);
    assert_ne!(bump, pre_head, "release must create a bump commit");
    let remote = repo::ls_remote(dir);
    assert_eq!(
        remote.get("refs/heads/main").map(String::as_str),
        Some(bump.as_str()),
        "remote main at bump commit: {remote:?}"
    );
    for tag in ["aaa-v0.2.0", "bbb-v0.1.1"] {
        assert_eq!(
            remote.get(&format!("refs/tags/{tag}")).map(String::as_str),
            Some(bump.as_str()),
            "remote {tag} at bump commit: {remote:?}"
        );
    }

    // Changelogs created with keep-a-changelog shape.
    for unit in ["aaa", "bbb"] {
        let cl = dir.join(format!("crates/{unit}/CHANGELOG.md"));
        let text = std::fs::read_to_string(&cl).unwrap_or_else(|e| panic!("{}: {e}", cl.display()));
        assert!(text.starts_with("# Changelog\n\n##"), "{text:?}");
        assert!(!text.contains("# Changelog\n\n\n"), "{text:?}");
    }

    // Workspace version: both units inherit
    // `version.workspace = true`, so each `write_unit_version`
    // rewrites the same `[workspace.package] version` — last
    // releasing unit wins (bbb → 0.1.1). This is the documented
    // ADR-0003 shared-workspace-version limitation, not a bug the
    // conformance suite should paper over.
    let ws = std::fs::read_to_string(dir.join("Cargo.toml")).expect("workspace manifest");
    assert!(ws.contains("0.1.1"), "workspace version bumped: {ws}");
    assert!(!ws.contains("0.1.0"), "old version gone: {ws}");
}

/// The mutating tier — call after writing
/// [`repo::NO_PUBLISH_TOML`] to `<dir>/tarsi.toml` on a fresh
/// [`repo::workspace_repo`].
///
/// # Panics
/// On any contract violation.
pub fn assert_release_tier(vcs: &dyn Vcs, dir: &Path) {
    assert_release_defaults_to_dry_run(vcs, dir);
    assert_release_conformance(vcs, dir);
}
