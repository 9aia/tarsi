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
//!   `profiles`, `plan`, `changelog`, `notes`, `check`, `schema`,
//!   plus the unknown-op error path. Safe on
//!   [`crate::repo::workspace_repo`] output as-is.
//! - [`assert_release_conformance`] — the mutating `release` op:
//!   requires a fixture with `origin` ([`crate::repo::workspace_repo`]
//!   / [`crate::repo::single_unit_repo`] both provide one) and a
//!   [`crate::repo::NO_PUBLISH_TOML`] `tarsi.toml` written by the
//!   caller, so no forge is touched. Asserts the step order,
//!   remote branch+tag state, changelog creation, and manifest
//!   bumps.
//! - [`assert_granular_tier`] — the granular ops (`bump` → `tag`
//!   → `publish`) chained via `params.units` passthrough, the
//!   node-shaped composition peregrin campaigns use. Same fixture
//!   requirements as [`assert_release_tier`].

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
    for op in [
        "plan",
        "changelog",
        "notes",
        "check",
        "schema",
        "bump",
        "tag",
        "publish",
        "release",
    ] {
        assert!(
            r["ops"]
                .as_array()
                .is_some_and(|ops| ops.iter().any(|o| o == op)),
            "manifest ops missing {op:?}: {r}"
        );
    }
}

/// `schema` answers the op surface as data — every op in
/// `manifest.ops` must appear with summary/params/returns.
///
/// # Panics
/// On contract violation.
pub fn assert_schema_op(vcs: &dyn Vcs, dir: &Path) {
    let r = ok(vcs, "schema", dir_params(dir));
    let ops = r["ops"].as_object().expect("schema.ops must be an object");
    for op in [
        "manifest",
        "profiles",
        "plan",
        "changelog",
        "notes",
        "check",
        "schema",
        "bump",
        "tag",
        "publish",
        "release",
    ] {
        let entry = ops
            .get(op)
            .unwrap_or_else(|| panic!("schema missing op {op:?}: {r}"));
        assert!(entry["summary"].is_string(), "schema {op} summary: {entry}");
        assert!(entry["params"].is_string(), "schema {op} params: {entry}");
        assert!(entry["returns"].is_string(), "schema {op} returns: {entry}");
    }
}

/// `check` on the workspace fixture: every releasing unit gets
/// `tag-available`/`manifest-present`/`changelog-not-drifted`
/// checks plus informational publish-target entries; all pass on
/// a fresh repo. Returns the check result.
///
/// # Panics
/// On contract violation.
#[must_use]
pub fn assert_check_op(vcs: &dyn Vcs, dir: &Path) -> Value {
    let r = ok(vcs, "check", dir_params(dir));
    assert_eq!(r["ok"], true, "fresh fixture must pass check: {r}");
    assert_eq!(r["releasing"], 2, "both units release: {r}");
    let checks = r["checks"].as_array().expect("checks array");
    for unit in ["aaa", "bbb"] {
        for name in ["tag-available", "manifest-present", "changelog-not-drifted"] {
            let hit = checks
                .iter()
                .find(|c| c["check"] == name && c["unit"] == unit);
            assert!(
                hit.is_some_and(|c| c["ok"].as_bool() == Some(true)),
                "check {name} for {unit} must pass: {r}"
            );
        }
    }
    assert!(
        checks.iter().any(|c| c["check"]
            .as_str()
            .is_some_and(|s| s.starts_with("publish-"))),
        "publish target checks present: {r}"
    );
    assert!(r["plan"]["units"].is_array(), "check echoes the plan: {r}");
    r
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
    assert_schema_op(vcs, dir);
    let _plan = assert_plan_op(vcs, dir);
    assert_changelog_notes_ops(vcs, dir);
    let _check = assert_check_op(vcs, dir);
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

/// `bump` with no `dry_run` param defaults to dry-run: reports the
/// planned units, mutates nothing (HEAD unchanged, no changelog,
/// no manifest bump).
///
/// # Panics
/// On contract violation.
pub fn assert_bump_defaults_to_dry_run(vcs: &dyn Vcs, dir: &Path) {
    let pre_head = repo::git(dir, &["rev-parse", "HEAD"]);
    let r = ok(vcs, "bump", dir_params(dir));
    assert_eq!(r["dry_run"], true, "bump dry_run must default true: {r}");
    assert_eq!(
        r["bump"]["commit"],
        Value::Null,
        "dry-run bump has no commit: {r}"
    );
    let units = r["units"]
        .as_array()
        .expect("bump echoes units for chaining");
    assert_eq!(units.len(), 2, "both units releasing: {r}");
    assert_eq!(
        repo::git(dir, &["rev-parse", "HEAD"]),
        pre_head,
        "dry-run bump creates no commit"
    );
    assert!(
        !dir.join("crates/aaa/CHANGELOG.md").exists(),
        "dry-run bump writes no changelog"
    );
}

/// `tag` and `publish` dry-run defaults: no tags created, no
/// forge calls (reports marked dry-run/disabled).
///
/// # Panics
/// On contract violation.
pub fn assert_tag_publish_dry_run_defaults(vcs: &dyn Vcs, dir: &Path) {
    let before_tags = repo::git(dir, &["tag", "--list"]);
    let r = ok(vcs, "tag", dir_params(dir));
    assert_eq!(r["dry_run"], true, "tag dry_run must default true: {r}");
    assert_eq!(
        repo::git(dir, &["tag", "--list"]),
        before_tags,
        "dry-run tag creates no tags"
    );

    let p = ok(vcs, "publish", dir_params(dir));
    assert_eq!(p["dry_run"], true, "publish dry_run must default true: {p}");
    assert!(
        p["publish"]
            .as_array()
            .is_some_and(|rs| { rs.iter().all(|r| r["ok"].as_bool() == Some(true)) }),
        "dry-run publish reports all targets ok: {p}"
    );
}

/// The granular chain — `bump` → `tag`(push) → `publish` — the
/// node-shaped composition a peregrin campaign drives. Each op
/// consumes the previous op's `units` echo via `params.units`
/// (re-planning mid-chain would see the bump commit / new tags).
/// Asserts the same end state as [`assert_release_conformance`]:
/// bump commit on remote main, both tags at that commit, changelogs
/// written, manifests bumped, publish reports collected.
///
/// # Panics
/// On contract violation.
pub fn assert_granular_release_chain(vcs: &dyn Vcs, dir: &Path) {
    let pre_head = repo::git(dir, &["rev-parse", "HEAD"]);

    // Step 1 — bump: changelogs + manifests + release commit.
    let mut params = dir_params(dir);
    params["dry_run"] = json!(false);
    let b = ok(vcs, "bump", params);
    assert_eq!(b["ok"], true, "bump failed: {b}");
    let bump_commit = b["bump"]["commit"]
        .as_str()
        .expect("bump returns the release commit sha")
        .to_string();
    assert_ne!(bump_commit, pre_head, "bump creates a commit");
    let steps: Vec<&str> = b["bump"]["steps_run"]
        .as_array()
        .expect("bump steps_run")
        .iter()
        .map(|s| s.as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        steps,
        vec!["write-changelogs", "bump-manifests", "commit-bump"],
        "bump step order: {b}"
    );
    let units = b["units"].as_array().expect("bump echoes units");
    assert_eq!(units.len(), 2, "{b}");

    // Step 2 — tag at the bump commit, pushed (required before
    // publish; ghreleases pins `target_commitish`).
    let mut params = dir_params(dir);
    params["dry_run"] = json!(false);
    params["push"] = json!(true);
    params["units"] = b["units"].clone();
    params["sha"] = json!(bump_commit);
    let t = ok(vcs, "tag", params);
    assert_eq!(t["ok"], true, "tag failed: {t}");
    let tag_steps: Vec<&str> = t["tag"]["steps_run"]
        .as_array()
        .expect("tag steps_run")
        .iter()
        .map(|s| s.as_str().unwrap_or(""))
        .collect();
    assert_eq!(tag_steps, vec!["tag", "push"], "tag step order: {t}");
    assert_eq!(
        t["tag"]["pushed"]["branch"], true,
        "tag push pushes the branch: {t}"
    );
    let tag_units = t["units"].as_array().expect("tag echoes units");

    // Step 3 — publish against the pushed state.
    let mut params = dir_params(dir);
    params["dry_run"] = json!(false);
    params["units"] = json!(tag_units);
    params["commitish"] = json!(bump_commit);
    let p = ok(vcs, "publish", params);
    assert_eq!(p["ok"], true, "publish failed: {p}");
    let publish = p["publish"].as_array().expect("publish reports");
    assert!(
        publish.iter().all(|r| r["ok"].as_bool() == Some(true)),
        "all publish targets ok (disabled counts): {p}"
    );
    assert!(
        p["targets"].as_array().is_some_and(|ts| !ts.is_empty()),
        "publish names touched targets: {p}"
    );

    // End state — identical to the composed `release` op.
    let remote = repo::ls_remote(dir);
    assert_eq!(
        remote.get("refs/heads/main").map(String::as_str),
        Some(bump_commit.as_str()),
        "remote main at bump commit: {remote:?}"
    );
    for tag in ["aaa-v0.2.0", "bbb-v0.1.1"] {
        assert_eq!(
            remote.get(&format!("refs/tags/{tag}")).map(String::as_str),
            Some(bump_commit.as_str()),
            "remote {tag} at bump commit: {remote:?}"
        );
    }
    for unit in ["aaa", "bbb"] {
        let cl = dir.join(format!("crates/{unit}/CHANGELOG.md"));
        let text = std::fs::read_to_string(&cl).unwrap_or_else(|e| panic!("{}: {e}", cl.display()));
        assert!(text.starts_with("# Changelog\n\n##"), "{text:?}");
    }
    let ws = std::fs::read_to_string(dir.join("Cargo.toml")).expect("workspace manifest");
    assert!(ws.contains("0.1.1"), "workspace version bumped: {ws}");

    // Post-release `check` is clean: the new tags bound each
    // unit's range, so nothing is left to release (idempotent —
    // a second release attempt plans zero units).
    let resp = call(vcs, "check", dir_params(dir));
    assert!(resp.ok, "post-release check must pass: {resp:?}");
    assert_eq!(
        resp.result.as_ref().map(|r| &r["releasing"]),
        Some(&json!(0)),
        "post-release check finds nothing to release: {resp:?}"
    );
}

/// `check` drift detection: a changelog that already carries the
/// version heading the plan wants to write fails
/// `changelog-not-drifted` (and the op reports `ok:false`). The
/// fixture file is removed afterwards so the release chain is
/// unaffected.
///
/// # Panics
/// On contract violation.
pub fn assert_check_detects_drift(vcs: &dyn Vcs, dir: &Path) {
    // aaa plans minor → 0.2.0; pre-seed its changelog with that
    // heading — the "already released" drift signature.
    let cl = dir.join("crates/aaa/CHANGELOG.md");
    std::fs::write(
        &cl,
        "# Changelog\n\n## [0.2.0] - 2026-01-01\n\nalready here\n",
    )
    .expect("seed drifted changelog");
    let resp = call(vcs, "check", dir_params(dir));
    std::fs::remove_file(&cl).expect("remove drifted changelog");
    assert!(!resp.ok, "drifted changelog must fail check: {resp:?}");
    let checks = resp
        .result
        .as_ref()
        .and_then(|r| r["checks"].as_array())
        .expect("check result carries checks");
    let drift = checks
        .iter()
        .find(|c| c["check"] == "changelog-not-drifted" && c["unit"] == "aaa")
        .expect("aaa drift check present");
    assert_eq!(drift["ok"], false, "aaa drift check fails: {drift}");
    // bbb is untouched — its checks still pass.
    let bbb_ok = checks
        .iter()
        .filter(|c| c["unit"] == "bbb")
        .all(|c| c["ok"].as_bool() == Some(true));
    assert!(bbb_ok, "bbb checks unaffected by aaa drift: {checks:?}");
}

/// The granular tier — call after writing
/// [`repo::NO_PUBLISH_TOML`] to `<dir>/tarsi.toml` on a fresh
/// [`repo::workspace_repo`].
///
/// # Panics
/// On any contract violation.
pub fn assert_granular_tier(vcs: &dyn Vcs, dir: &Path) {
    assert_check_detects_drift(vcs, dir);
    assert_bump_defaults_to_dry_run(vcs, dir);
    assert_tag_publish_dry_run_defaults(vcs, dir);
    assert_granular_release_chain(vcs, dir);
}
