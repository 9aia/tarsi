//! tarsi-testkit self-conformance: the golden corpus must pass
//! against `tarsi-core`'s real planner, and the serve-op suite
//! must pass against `lasius-vcs-git` on real temp repos. Other
//! crates run the same `assert_*` functions against their own
//! VCS impls.
#![allow(clippy::unwrap_used)]

use lasius_vcs_git::GitVcs;
use serde_json::json;
use tarsi_testkit::{golden_cases, repo, serve};

#[test]
fn golden_corpus_passes() {
    for case in golden_cases() {
        let bad = case.check();
        assert!(
            bad.is_empty(),
            "golden case {} failed:\n{}",
            case.name,
            bad.join("\n")
        );
    }
}

#[test]
fn synthetic_fixture_parses() {
    let commits = tarsi_testkit::synthetic_commits();
    assert_eq!(commits.len(), 5);
    assert_eq!(commits[0].subject, "feat(core): add widget API");
    assert!(commits[0].conventional().is_some());
}

#[test]
fn serve_read_ops_conform_on_git() {
    let fx = repo::workspace_repo();
    serve::assert_conformance(&GitVcs, fx.path());
}

#[test]
fn serve_release_conforms_on_git() {
    let fx = repo::workspace_repo();
    repo::write(fx.path(), "tarsi.toml", repo::NO_PUBLISH_TOML);
    serve::assert_release_tier(&GitVcs, fx.path());
}

#[test]
fn serve_granular_ops_conform_on_git() {
    let fx = repo::workspace_repo();
    repo::write(fx.path(), "tarsi.toml", repo::NO_PUBLISH_TOML);
    serve::assert_granular_tier(&GitVcs, fx.path());
}

#[test]
fn serve_sync_refs_conforms_on_git() {
    let fx = repo::single_unit_repo();
    serve::assert_sync_refs_conformance(&GitVcs, fx.path());
}

#[test]
fn serve_sync_refs_skips_mixed_versions_on_git() {
    let fx = repo::workspace_repo();
    serve::assert_sync_refs_skips_mixed_versions(&GitVcs, fx.path());
}

#[test]
fn single_unit_repo_plans_one_release() {
    let fx = repo::single_unit_repo();
    let r = serve::ok(
        &GitVcs,
        "plan",
        json!({ "dir": fx.path().to_string_lossy() }),
    );
    let units = r["plan"]["units"].as_array().unwrap();
    assert_eq!(units.len(), 1);
    let u = &units[0];
    assert_eq!(u["spec"]["name"], "app");
    assert_eq!(u["bump"], "minor");
    assert_eq!(u["new_version"], "0.2.0");
    assert_eq!(u["tag"], "app-v0.2.0");
    assert!(u["changelog_markdown"]
        .as_str()
        .unwrap()
        .contains("add widget API"));
}
