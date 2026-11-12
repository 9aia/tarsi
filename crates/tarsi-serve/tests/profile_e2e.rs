//! Profile e2e: `type: tarsi` release profiles layer under repo
//! config — `params.profile`, `$TARSI_PROFILE`, and the config's
//! `profile = "…"` key all resolve through `tarsi-config`.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use lasius_vcs_git::GitVcs;
use serde_json::json;
use tarsi_serve::{dispatch, Request};

/// Env-mutating tests in this binary serialize on this lock —
/// `TARSI_PROFILES_DIR`/`HOME` are process-global.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    vars: Vec<(&'static str, Option<String>)>,
}

impl EnvGuard {
    fn set(vars: &[(&'static str, &str)]) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let saved = vars
            .iter()
            .map(|(k, _)| (*k, std::env::var(k).ok()))
            .collect();
        for (k, v) in vars {
            std::env::set_var(k, v);
        }
        Self {
            _lock: lock,
            vars: saved,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (k, prev) in &self.vars {
            match prev {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}

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

/// Minimal fixture: one-unit workspace, tagged, one feat commit.
fn fixture() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=[\"crates/*\"]\nresolver=\"2\"\n[workspace.package]\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    let u = root.join("crates/u");
    std::fs::create_dir_all(u.join("src")).unwrap();
    std::fs::write(
        u.join("Cargo.toml"),
        "[package]\nname=\"u\"\nversion.workspace=true\n",
    )
    .unwrap();
    std::fs::write(u.join("src/lib.rs"), "// lib\n").unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "chore: scaffold"]);
    git(root, &["tag", "u-v0.1.0"]);
    std::fs::write(u.join("src/lib.rs"), "// feat\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "feat: add thing"]);
    d
}

fn plan(dir: &Path, params: serde_json::Value) -> serde_json::Value {
    let mut p = json!({ "dir": dir.to_string_lossy() });
    p.as_object_mut()
        .unwrap()
        .extend(params.as_object().unwrap().clone());
    let resp = dispatch(
        &GitVcs,
        &Request {
            op: "plan".into(),
            params: p,
        },
    );
    assert!(resp.ok, "{:?}", resp.error);
    resp.result.unwrap()
}

#[test]
fn profile_param_layers_config_and_lists() {
    let d = fixture();
    // A `type: tarsi`-shaped profile dir: tamias.yaml + tarsi.toml.
    let profs = tempfile::tempdir().unwrap();
    let prof = profs.path().join("org-release");
    std::fs::create_dir_all(&prof).unwrap();
    std::fs::write(
        prof.join("tamias.yaml"),
        "name: org-release\ntype: tarsi\ndescription: Org release conventions\n",
    )
    .unwrap();
    std::fs::write(
        prof.join("tarsi.toml"),
        // distinctive tag format + disabled crates publish — both
        // observable in the plan JSON.
        "[release]\ntag_format = \"rel/{unit}/v{version}\"\n[publish.crates]\nenabled = false\n",
    )
    .unwrap();
    let home = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[
        ("TARSI_PROFILES_DIR", profs.path().to_str().unwrap()),
        ("HOME", home.path().to_str().unwrap()),
    ]);

    // `profiles` op sees the installed profile.
    let resp = dispatch(
        &GitVcs,
        &Request {
            op: "profiles".into(),
            params: json!({}),
        },
    );
    assert!(resp.ok);
    let profiles = resp.result.unwrap()["profiles"].as_array().unwrap().clone();
    let hit = profiles
        .iter()
        .find(|p| p["name"] == "org-release")
        .expect("org-release listed");
    assert_eq!(hit["description"], "Org release conventions");

    // plan with params.profile → profile's tag_format + publish
    // flags apply (repo has no tarsi.toml at all).
    let r = plan(d.path(), json!({ "profile": "org-release" }));
    let u = &r["plan"]["units"][0];
    assert_eq!(u["tag"], "rel/u/v0.2.0", "{u}");
    let crates = u["publish_targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["target"] == "crates-io")
        .unwrap()
        .clone();
    assert_eq!(crates["enabled"], false, "{crates}");
}

#[test]
fn profile_key_in_repo_config_and_missing_profile_errors() {
    let d = fixture();
    let profs = tempfile::tempdir().unwrap();
    let prof = profs.path().join("p1");
    std::fs::create_dir_all(&prof).unwrap();
    std::fs::write(
        prof.join("tarsi.toml"),
        "[release]\ntag_format = \"k/{unit}/{version}\"\n",
    )
    .unwrap();
    let home = tempfile::tempdir().unwrap();
    let _env = EnvGuard::set(&[
        ("TARSI_PROFILES_DIR", profs.path().to_str().unwrap()),
        ("HOME", home.path().to_str().unwrap()),
    ]);

    // repo tarsi.toml selects the profile via `profile = "…"`.
    std::fs::write(d.path().join("tarsi.toml"), "profile = \"p1\"\n").unwrap();
    let r = plan(d.path(), json!({}));
    assert_eq!(r["plan"]["units"][0]["tag"], "k/u/0.2.0");

    // A named-but-missing profile is a hard error, not defaults.
    let resp = dispatch(
        &GitVcs,
        &Request {
            op: "plan".into(),
            params: json!({ "dir": d.path().to_string_lossy(), "profile": "ghost" }),
        },
    );
    assert!(!resp.ok);
    assert!(
        resp.error.unwrap().contains("ghost"),
        "missing profile must surface"
    );
}
