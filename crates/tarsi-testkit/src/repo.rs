//! Real-git fixture repos for VCS-touching tests — temp
//! workspaces with conventional commits, tags, and a bare
//! `origin`, built with the `git` CLI (no VCS-impl dependency, so
//! any [`lasius_vcs::Vcs`] adapter can be pointed at them).
//!
//! [`workspace_repo`] produces the canonical two-unit shape the
//! serve-op conformance suite expects: cargo workspace members
//! `aaa` (depended on by `bbb`), tagged `aaa-v0.1.0` /
//! `bbb-v0.1.0`, then one `feat(aaa)` + one `fix(bbb)` commit —
//! so `plan` yields a minor bump for `aaa` and a patch for `bbb`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// `tarsi.toml` disabling every publish target — mutation tests
/// write this so `release` exercises git steps without touching a
/// forge. JSON report entries still appear, marked `disabled`.
pub const NO_PUBLISH_TOML: &str =
    "[publish.crates]\nenabled = false\n[publish.ghreleases]\nenabled = false\n";

/// A fixture repo: the tempdir guard plus resolved paths. Drop
/// order doesn't matter — everything lives under the tempdirs.
pub struct Fixture {
    /// Workspace root (`root`).
    pub root: PathBuf,
    /// Bare `origin.git` path the `origin` remote points at.
    pub origin_git: PathBuf,
    _repo: TempDir,
    _origin: TempDir,
}

impl Fixture {
    /// The repo root as a path slice.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }
}

/// Run `git -C <dir> <args>` with a fixed identity; returns
/// stdout. Panics on failure — fixture setup is not fallible in
/// practice.
///
/// # Panics
/// git missing, or the command exits non-zero.
pub fn git(dir: &Path, args: &[&str]) -> String {
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
        .expect("tarsi-testkit: git not runnable");
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

/// Like [`git`] but reports success/failure — for probes.
#[must_use]
pub fn git_try(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write `body` to `dir/rel`, creating parents.
///
/// # Panics
/// On I/O error.
pub fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().expect("fixture path has parent"))
        .expect("tarsi-testkit: mkdir");
    std::fs::write(p, body).expect("tarsi-testkit: write fixture file");
}

/// `git add -A && git commit -m <msg>` in one step.
pub fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", msg]);
}

/// `sha<TAB>ref` map from `git ls-remote origin` — remote state
/// assertions for release conformance.
#[must_use]
pub fn ls_remote(dir: &Path) -> BTreeMap<String, String> {
    git(dir, &["ls-remote", "origin"])
        .lines()
        .filter_map(|l| {
            let (sha, r) = l.split_once('\t')?;
            Some((r.to_string(), sha.to_string()))
        })
        .collect()
}

/// The canonical fixture: cargo workspace `aaa` + `bbb` (bbb
/// path-deps aaa), one scaffold commit tagged `aaa-v0.1.0` /
/// `bbb-v0.1.0`, a bare `origin`, then `feat(aaa)` + `fix(bbb)`.
/// Local git identity is configured so `Vcs::commit` works
/// without global config.
#[must_use]
pub fn workspace_repo() -> Fixture {
    let repo = TempDir::new().expect("tarsi-testkit: tempdir");
    let origin = TempDir::new().expect("tarsi-testkit: tempdir");
    let root = repo.path().to_path_buf();
    let origin_git = origin.path().join("origin.git");

    git(origin.path(), &["init", "-q", "--bare", "origin.git"]);

    write(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers=[\"crates/*\"]\nresolver=\"2\"\n[workspace.package]\nversion=\"0.1.0\"\n",
    );
    for (n, dep) in [("aaa", ""), ("bbb", "aaa = { path = \"../aaa\" }")] {
        write(
            &root,
            &format!("crates/{n}/Cargo.toml"),
            &format!("[package]\nname=\"{n}\"\nversion.workspace=true\n\n[dependencies]\n{dep}\n"),
        );
        write(&root, &format!("crates/{n}/src/lib.rs"), "// lib\n");
    }
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "user.email", "t@t"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    commit_all(&root, "chore: scaffold");
    git(&root, &["tag", "aaa-v0.1.0"]);
    git(&root, &["tag", "bbb-v0.1.0"]);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            origin_git.to_str().expect("utf8 path"),
        ],
    );
    // New work since the tags.
    write(&root, "crates/aaa/src/lib.rs", "// feat\n");
    commit_all(&root, "feat(aaa): add thing");
    write(&root, "crates/bbb/src/lib.rs", "// fix\n");
    commit_all(&root, "fix(bbb): crash on empty");

    Fixture {
        root,
        origin_git,
        _repo: repo,
        _origin: origin,
    }
}

/// A single-package repo: one `Cargo.toml` + `src/lib.rs`, one
/// `feat:` commit, tag `app-v0.1.0`, bare `origin`. For
/// single-unit release conformance.
#[must_use]
pub fn single_unit_repo() -> Fixture {
    let repo = TempDir::new().expect("tarsi-testkit: tempdir");
    let origin = TempDir::new().expect("tarsi-testkit: tempdir");
    let root = repo.path().to_path_buf();
    let origin_git = origin.path().join("origin.git");

    git(origin.path(), &["init", "-q", "--bare", "origin.git"]);

    write(
        &root,
        "Cargo.toml",
        "[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    );
    write(&root, "src/lib.rs", "// lib\n");
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "user.email", "t@t"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    commit_all(&root, "chore: scaffold");
    git(&root, &["tag", "app-v0.1.0"]);
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            origin_git.to_str().expect("utf8 path"),
        ],
    );
    write(&root, "src/lib.rs", "// feat\n");
    commit_all(&root, "feat: add widget API");

    Fixture {
        root,
        origin_git,
        _repo: repo,
        _origin: origin,
    }
}
