//! Golden-fixture e2e: real temp git repo + workspace → `plan` op.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

use lasius_vcs::Vcs;
use lasius_vcs_git::GitVcs;
use serde_json::json;
use tarsi_serve::{dispatch, Request};

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?}");
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// Fixture repo: workspace `aaa` (depended on by `bbb`), tagged
/// `aaa-v0.1.0`/`bbb-v0.1.0`, then new conventional commits.
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(root, "Cargo.toml", "[workspace]\nmembers=[\"crates/*\"]\nresolver=\"2\"\n[workspace.package]\nversion=\"0.1.0\"\n");
    for (n, dep) in [("aaa", ""), ("bbb", "aaa = { path = \"../aaa\" }")] {
        write(
            root,
            &format!("crates/{n}/Cargo.toml"),
            &format!("[package]\nname=\"{n}\"\nversion.workspace=true\n\n[dependencies]\n{dep}\n"),
        );
        write(root, &format!("crates/{n}/src/lib.rs"), "// lib\n");
    }
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "chore: scaffold"]);
    git(root, &["tag", "aaa-v0.1.0"]);
    git(root, &["tag", "bbb-v0.1.0"]);
    // new work since the tags
    write(root, "crates/aaa/src/lib.rs", "// feat\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "feat(aaa): add thing"]);
    write(root, "crates/bbb/src/lib.rs", "// fix\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "fix(bbb): crash on empty"]);
    d
}

#[test]
fn plan_since_tags_attributes_and_orders() {
    let d = fixture();
    let req = Request {
        op: "plan".into(),
        params: json!({ "dir": d.path().to_string_lossy() }),
    };
    let resp = dispatch(&GitVcs, &req);
    assert!(resp.ok, "{:?}", resp.error);
    let r = resp.result.unwrap();
    let units = r["plan"]["units"].as_array().unwrap();
    // topo: aaa before bbb (bbb path-deps aaa)
    let names: Vec<&str> = units
        .iter()
        .map(|u| u["spec"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["aaa", "bbb"]);
    let aaa = &units[0];
    assert_eq!(aaa["bump"], "minor");
    assert_eq!(aaa["new_version"], "0.2.0");
    assert_eq!(aaa["tag"], "aaa-v0.2.0");
    // bbb only saw a fix → patch; aaa saw a feat → minor. The two
    // disagree — exactly the workspace-version limitation ADR-0003
    // documents (shared workspace.package.version can't express it).
    let bbb = &units[1];
    assert_eq!(bbb["bump"], "patch");
    assert_eq!(bbb["new_version"], "0.1.1");
    // changelog markdown rendered for the releasing unit
    assert!(aaa["changelog_markdown"]
        .as_str()
        .unwrap()
        .contains("add thing"));
}

#[test]
fn release_dry_run_reports_without_mutating() {
    let d = fixture();
    let req = Request {
        op: "release".into(),
        params: json!({ "dir": d.path().to_string_lossy(), "dry_run": true }),
    };
    let resp = dispatch(&GitVcs, &req);
    let r = resp.result.unwrap();
    assert_eq!(r["dry_run"], true);
    assert!(r["steps_run"].as_array().unwrap().is_empty());
    // nothing written: no new tags, manifests untouched
    assert_eq!(GitVcs.tags(d.path()).len(), 2);
    let manifest = std::fs::read_to_string(d.path().join("Cargo.toml")).unwrap();
    assert!(manifest.contains("0.1.0"));
}
