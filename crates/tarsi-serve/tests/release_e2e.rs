//! Release-op e2e against a real temp git repo + bare `origin`:
//! ordering (push before publish), scoped staging, changelog
//! splice, and publish-failure rollback. No network — publish
//! targets are disabled or fail fast on a missing token env.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use lasius_vcs::Vcs;
use lasius_vcs_git::GitVcs;
use serde_json::json;
use tarsi_serve::{dispatch, Request};

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// Like [`git`] but allows failure (probes).
fn git_try(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// Repo + bare origin + workspace fixture (same shape as
/// `plan_e2e`), with local git identity so `Vcs::commit` works
/// without global config, and `origin` pointing at the bare repo.
/// Returns `(repo_root, origin_path)`; tempdirs are leaked into
/// the test's lifetime via `Box::leak`-free ownership: callers
/// keep the `TempDir`s.
fn fixture() -> (tempfile::TempDir, tempfile::TempDir, PathBuf, PathBuf) {
    let repo = tempfile::tempdir().unwrap();
    let origin = tempfile::tempdir().unwrap();
    let root = repo.path().to_path_buf();
    let origin_git = origin.path().join("origin.git");

    git(origin.path(), &["init", "-q", "--bare", "origin.git"]);

    write(&root, "Cargo.toml", "[workspace]\nmembers=[\"crates/*\"]\nresolver=\"2\"\n[workspace.package]\nversion=\"0.1.0\"\n");
    for (n, dep) in [("aaa", ""), ("bbb", "aaa = { path = \"../aaa\" }")] {
        write(
            &root,
            &format!("crates/{n}/Cargo.toml"),
            &format!("[package]\nname=\"{n}\"\nversion.workspace=true\n\n[dependencies]\n{dep}\n"),
        );
        write(&root, &format!("crates/{n}/src/lib.rs"), "// lib\n");
    }
    git(&root, &["init", "-q", "-b", "main"]);
    // Identity for `Vcs::commit` (it shells plain `git commit`).
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "user.email", "t@t"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "chore: scaffold"]);
    git(&root, &["tag", "aaa-v0.1.0"]);
    git(&root, &["tag", "bbb-v0.1.0"]);
    git(
        &root,
        &["remote", "add", "origin", origin_git.to_str().unwrap()],
    );
    // new work since the tags
    write(&root, "crates/aaa/src/lib.rs", "// feat\n");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "feat(aaa): add thing"]);
    write(&root, "crates/bbb/src/lib.rs", "// fix\n");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-qm", "fix(bbb): crash on empty"]);
    (repo, origin, root, origin_git)
}

/// `sha<TAB>ref` map from `git ls-remote origin`.
fn ls_remote(root: &Path) -> std::collections::BTreeMap<String, String> {
    git(root, &["ls-remote", "origin"])
        .lines()
        .filter_map(|l| {
            let (sha, r) = l.split_once('\t')?;
            Some((r.to_string(), sha.to_string()))
        })
        .collect()
}

fn release(root: &Path) -> serde_json::Value {
    let req = Request {
        op: "release".into(),
        params: json!({ "dir": root.to_string_lossy(), "dry_run": false }),
    };
    let resp = dispatch(&GitVcs, &req);
    assert!(resp.ok, "op failed: {:?} / {:?}", resp.error, resp.result);
    resp.result.unwrap()
}

const NO_PUBLISH: &str =
    "[publish.crates]\nenabled = false\n[publish.ghreleases]\nenabled = false\n";

#[test]
fn release_pushes_commit_and_tags_before_publish() {
    let (_repo, _origin, root, _og) = fixture();
    write(&root, "tarsi.toml", NO_PUBLISH);
    // Unrelated dirty state that must NOT ride the release commit.
    write(&root, "scratch-notes.txt", "unrelated\n");
    write(&root, "crates/aaa/dirty.txt", "unrelated\n");

    let pre_head = git(&root, &["rev-parse", "HEAD"]);
    let r = release(&root);
    assert_eq!(r["ok"], true, "{r}");
    let steps: Vec<&str> = r["steps_run"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(
        steps,
        vec![
            "write-changelogs",
            "bump-manifests",
            "commit-bump",
            "tag",
            "push"
        ]
    );

    // Remote state: branch + BOTH tags exist and the tags point
    // at the bump commit (HEAD), not the pre-release head — the
    // bug-1 regression.
    let bump = git(&root, &["rev-parse", "HEAD"]);
    assert_ne!(bump, pre_head);
    let remote = ls_remote(&root);
    assert_eq!(
        remote.get("refs/heads/main").map(String::as_str),
        Some(bump.as_str()),
        "{remote:?}"
    );
    for tag in ["aaa-v0.2.0", "bbb-v0.1.1"] {
        assert_eq!(
            remote.get(&format!("refs/tags/{tag}")).map(String::as_str),
            Some(bump.as_str()),
            "remote {tag} must point at the bump commit: {remote:?}"
        );
        // local tag agrees
        assert_eq!(git(&root, &["rev-parse", tag]), bump);
    }

    // Scoped staging: the release commit touched only tarsi's
    // files — the dirty scratch files are still untracked.
    let names = git(&root, &["show", "--pretty=format:", "--name-only", "HEAD"]);
    assert!(names.contains("CHANGELOG.md"), "{names}");
    assert!(names.contains("Cargo.toml"), "{names}");
    assert!(!names.contains("scratch-notes.txt"), "{names}");
    assert!(!names.contains("dirty.txt"), "{names}");
    assert!(git_try(
        &root,
        &["status", "--porcelain", "--", "scratch-notes.txt"]
    ));
    let status = git(&root, &["status", "--porcelain"]);
    assert!(status.contains("scratch-notes.txt"), "{status}");
    assert!(status.contains("dirty.txt"), "{status}");

    // Changelog splice: exactly one blank line after `# Changelog`.
    let cl = std::fs::read_to_string(root.join("crates/aaa/CHANGELOG.md")).unwrap();
    assert!(cl.starts_with("# Changelog\n\n##"), "{cl:?}");
    assert!(!cl.contains("# Changelog\n\n\n"), "{cl:?}");
}

#[test]
fn release_publish_failure_rolls_back_local_and_remote() {
    let (_repo, _origin, root, _og) = fixture();
    std::env::remove_var("TARSI_TEST_UNSET_RELEASE_TOKEN");
    write(
        &root,
        "tarsi.toml",
        "[publish.crates]\nenabled = false\n[publish.ghreleases]\nrepo = \"o/r\"\ntoken_env = \"TARSI_TEST_UNSET_RELEASE_TOKEN\"\n",
    );

    let pre_head = git(&root, &["rev-parse", "HEAD"]);
    let pre_tags = GitVcs.tags(&root);
    let req = Request {
        op: "release".into(),
        params: json!({ "dir": root.to_string_lossy(), "dry_run": false }),
    };
    let resp = dispatch(&GitVcs, &req);
    assert!(!resp.ok, "op should fail: {:?}", resp.result);
    let r = resp.result.unwrap();
    assert_eq!(r["ok"], false);
    assert_eq!(r["rolled_back"]["ok"], true, "{r}");
    // ghreleases failed on the missing token for both units.
    let gh_failures = r["publish"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["target"] == "github-release" && p["ok"] == false)
        .count();
    assert_eq!(gh_failures, 2, "{r}");

    // Local: HEAD back at pre-release, tags gone, created
    // changelogs removed, manifests restored.
    assert_eq!(git(&root, &["rev-parse", "HEAD"]), pre_head);
    assert_eq!(GitVcs.tags(&root), pre_tags);
    assert!(!root.join("crates/aaa/CHANGELOG.md").exists());
    assert!(!root.join("crates/bbb/CHANGELOG.md").exists());
    let ws = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(ws.contains("0.1.0"), "{ws}");

    // Remote: branch reset to pre-release sha, release tags gone.
    let remote = ls_remote(&root);
    assert_eq!(
        remote.get("refs/heads/main").map(String::as_str),
        Some(pre_head.as_str()),
        "{remote:?}"
    );
    for tag in ["aaa-v0.2.0", "bbb-v0.1.1"] {
        assert!(
            !remote.contains_key(&format!("refs/tags/{tag}")),
            "remote tag {tag} must be deleted: {remote:?}"
        );
    }
}

#[test]
fn release_dry_run_still_mutates_nothing() {
    let (_repo, _origin, root, _og) = fixture();
    write(&root, "tarsi.toml", NO_PUBLISH);
    let req = Request {
        op: "release".into(),
        params: json!({ "dir": root.to_string_lossy() }), // dry_run defaults true
    };
    let resp = dispatch(&GitVcs, &req);
    assert!(resp.ok);
    let r = resp.result.unwrap();
    assert_eq!(r["dry_run"], true);
    assert_eq!(GitVcs.tags(&root).len(), 2);
    assert!(!root.join("crates/aaa/CHANGELOG.md").exists());
    assert!(ls_remote(&root).is_empty(), "nothing pushed on dry-run");
}
