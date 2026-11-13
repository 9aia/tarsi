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
fn plan_annotates_disabled_publish_targets() {
    let d = fixture();
    write(
        d.path(),
        "tarsi.toml",
        "[publish.crates]\nenabled = false\n[publish.ghreleases]\nenabled = false\n",
    );
    let req = Request {
        op: "plan".into(),
        params: json!({ "dir": d.path().to_string_lossy() }),
    };
    let resp = dispatch(&GitVcs, &req);
    assert!(resp.ok, "{:?}", resp.error);
    let r = resp.result.unwrap();
    let units = r["plan"]["units"].as_array().unwrap();
    for u in units {
        let targets = u["publish_targets"].as_array().unwrap();
        assert!(
            targets.iter().any(|t| t["target"] == "github-release"),
            "declared targets still listed: {targets:?}"
        );
        assert!(
            targets.iter().all(|t| t["enabled"] == false),
            "disabled config must surface as enabled:false: {targets:?}"
        );
    }
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

#[test]
fn release_dry_run_reports_configured_assets() {
    let d = fixture();
    // Artifact + ghreleases asset glob; {unit}/{version} substitute
    // per releasing unit.
    write(d.path(), "dist/aaa-0.2.0-linux.tar.gz", "fake");
    write(
        d.path(),
        "tarsi.toml",
        "[publish.ghreleases]\nassets = [\"dist/{unit}-*-linux.tar.gz\"]\n",
    );
    let req = Request {
        op: "release".into(),
        params: json!({ "dir": d.path().to_string_lossy(), "dry_run": true }),
    };
    let resp = dispatch(&GitVcs, &req);
    let r = resp.result.unwrap();
    assert_eq!(r["dry_run"], true);
    let publish = r["publish"].as_array().unwrap();
    let gh = publish
        .iter()
        .find(|p| p["target"] == "github-release" && p["unit"] == "aaa")
        .expect("ghreleases report for aaa");
    assert_eq!(gh["ok"], true, "{gh}");
    let detail = gh["detail"].as_str().unwrap();
    assert!(detail.contains("dry-run"), "{detail}");
    assert!(detail.contains("aaa-0.2.0-linux.tar.gz"), "{detail}");
    assert!(detail.contains("application/gzip"), "{detail}");
    // bbb's version differs → its glob matches nothing → no assets.
    let gh_bbb = publish
        .iter()
        .find(|p| p["target"] == "github-release" && p["unit"] == "bbb")
        .expect("ghreleases report for bbb");
    assert!(!gh_bbb["detail"].as_str().unwrap().contains("assets:"));
}

#[test]
fn plan_honors_named_release_strategy() {
    let d = fixture();
    write(
        d.path(),
        "tarsi.toml",
        "[release]\nstrategy = \"semantic-release-angular\"\n",
    );
    let req = Request {
        op: "plan".into(),
        params: json!({ "dir": d.path().to_string_lossy() }),
    };
    let resp = dispatch(&GitVcs, &req);
    assert!(resp.ok, "{:?}", resp.error);
    let r = resp.result.unwrap();
    // The resolved strategy echoes on the plan.
    assert_eq!(r["plan"]["strategy"]["name"], "semantic-release-angular");
    let units = r["plan"]["units"].as_array().unwrap();
    let bbb = units
        .iter()
        .find(|u| u["spec"]["name"] == "bbb")
        .expect("bbb unit");
    // angular's section headings differ from keep-a-changelog's.
    let md = bbb["changelog_markdown"].as_str().unwrap();
    assert!(md.contains("### Bug Fixes"), "{md}");
    assert!(!md.contains("### Fixed"), "{md}");
}

#[test]
fn plan_honors_inline_strategy_default_bump_and_diff_exclude() {
    let d = fixture();
    // A docs-only commit that would land in `unattributed` without
    // diff_exclude; `*.md`/`docs/**` drop it pre-attribution.
    write(d.path(), "docs/guide.md", "# guide\n");
    git(d.path(), &["add", "-A"]);
    git(d.path(), &["commit", "-qm", "fix: docs typo"]);
    write(
        d.path(),
        "tarsi.toml",
        r#"
[release]
default_bump = "patch"
diff_exclude = ["docs/**", "*.md"]

[release.strategy.bump]
[[release.strategy.bump.rules]]
type = "feat"
bump = "major"
"#,
    );
    let req = Request {
        op: "plan".into(),
        params: json!({ "dir": d.path().to_string_lossy() }),
    };
    let resp = dispatch(&GitVcs, &req);
    assert!(resp.ok, "{:?}", resp.error);
    let r = resp.result.unwrap();
    assert_eq!(r["plan"]["strategy"]["name"], "custom");
    let units = r["plan"]["units"].as_array().unwrap();
    let aaa = units
        .iter()
        .find(|u| u["spec"]["name"] == "aaa")
        .expect("aaa unit");
    // Custom rule: feat → major (conventional would say minor).
    assert_eq!(aaa["bump"], "major");
    assert_eq!(aaa["new_version"], "1.0.0");
    let bbb = units
        .iter()
        .find(|u| u["spec"]["name"] == "bbb")
        .expect("bbb unit");
    // fix matches no custom rule → inferred none → default_bump
    // fallback → patch.
    assert_eq!(bbb["bump"], "patch");
    assert_eq!(bbb["new_version"], "0.1.1");
    // The docs-only commit was filtered pre-attribution — nothing
    // unattributed.
    assert_eq!(r["plan"]["unattributed"].as_array().unwrap().len(), 0);
}
