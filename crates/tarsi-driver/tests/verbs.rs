//! End-to-end wire tests — spawn the real `tamias-driver-tarsi`
//! binary and check the JSON reports tamias would read.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;

fn driver() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tamias-driver-tarsi"))
}

fn run(args: &[&str], envs: &[(&str, &str)]) -> (bool, serde_json::Value, String) {
    let mut cmd = driver();
    cmd.args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let json = serde_json::from_str(stdout.trim()).unwrap_or(serde_json::Value::Null);
    (out.status.success(), json, stderr)
}

fn write_payload(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("tamias.yaml"),
        format!("name: {name}\nversion: \"1.0.0\"\ntype: tarsi\ndescription: e2e\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join("tarsi.toml"),
        "[release]\nstyleguide = \"keep-a-changelog\"\n",
    )
    .unwrap();
}

#[test]
fn capabilities_answers_tamias_discovery() {
    let (ok, json, _) = run(&["capabilities"], &[]);
    assert!(ok);
    assert_eq!(json["types"], serde_json::json!(["tarsi"]));
    assert!(json["verbs"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("install")));
}

#[test]
fn config_schema_is_draft_2020_12() {
    let (ok, json, _) = run(&["config-schema"], &[]);
    assert!(ok);
    assert_eq!(
        json["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
}

#[test]
fn install_uninstall_round_trip_via_profiles_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("payload");
    write_payload(&payload, "e2e-profile");
    let profs = tmp.path().join("profs");
    let home = tmp.path().join("home");
    let cfg = tmp.path().join("driver.yaml");
    std::fs::write(&cfg, format!("profilesDir: {}\n", profs.display())).unwrap();
    let dir_s = payload.display().to_string();
    let name = "e2e-profile";
    let envs: Vec<(&str, String)> = vec![
        ("HOME", home.display().to_string()),
        ("TARSI_PROFILES_DIR", profs.display().to_string()),
        (
            "TAMIAS_HOME",
            tmp.path().join("tamias").display().to_string(),
        ),
    ];
    let env_refs: Vec<(&str, &str)> = envs
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect::<Vec<_>>();

    // install → copy lands + resolves through the discovery API.
    let (ok, json, err) = run(
        &[
            "install",
            "--dir",
            &dir_s,
            "--name",
            name,
            "--config",
            &cfg.display().to_string(),
        ],
        &env_refs,
    );
    assert!(ok, "{err}");
    assert_eq!(json["ok"], true);
    let dest = profs.join("e2e-profile");
    assert!(dest.join("tarsi.toml").is_file());
    assert!(dest.join(".tamias-driver-tarsi.json").is_file());

    // status → the dir profile resolves (source: profiles-dir).
    let store_dir = tmp.path().join("tamias/store/e2e-profile@1.0.0");
    write_payload(&store_dir, "e2e-profile");
    let (ok, json, err) = run(
        &[
            "status",
            "--dir",
            &store_dir.display().to_string(),
            "--name",
            name,
        ],
        &env_refs,
    );
    assert!(ok, "{err}");
    // Dir profiles beat the store — the copy wins resolution.
    assert_eq!(json["resolved"], serde_json::json!(dest));
    assert_eq!(json["source"], "profiles-dir");

    // uninstall → managed copy removed; the store entry is tamias's.
    let (ok, _json, err) = run(
        &[
            "uninstall",
            "--dir",
            &store_dir.display().to_string(),
            "--name",
            name,
        ],
        &env_refs,
    );
    assert!(ok, "{err}");
    assert!(!dest.exists());
    // status now resolves the store entry instead.
    let (ok, json, _) = run(
        &[
            "status",
            "--dir",
            &store_dir.display().to_string(),
            "--name",
            name,
        ],
        &env_refs,
    );
    assert!(ok);
    assert_eq!(json["source"], "tamias-store");
    assert_eq!(json["resolved"], serde_json::json!(store_dir));
}

#[test]
fn install_without_config_is_store_only() {
    let tmp = tempfile::tempdir().unwrap();
    let payload = tmp.path().join("payload");
    write_payload(&payload, "store-only");
    let home = tmp.path().join("home");
    let tamias_home = tmp.path().join("tamias");
    let envs: Vec<(&str, &str)> = vec![
        ("HOME", home.to_str().unwrap()),
        ("TAMIAS_HOME", tamias_home.to_str().unwrap()),
    ];
    let (ok, json, err) = run(
        &[
            "install",
            "--dir",
            &payload.display().to_string(),
            "--name",
            "store-only",
        ],
        &envs,
    );
    assert!(ok, "{err}");
    assert_eq!(json["placement"], "tamias-store");
    assert!(!home.join(".config/tarsi/profiles").exists());
}

#[test]
fn missing_flags_exit_2() {
    let mut cmd = driver();
    let out = cmd.arg("install").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn bad_payload_fails_loudly() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let home = tmp.path().join("home");
    let envs: Vec<(&str, &str)> = vec![("HOME", home.to_str().unwrap())];
    let (ok, _, err) = run(
        &[
            "install",
            "--dir",
            &empty.display().to_string(),
            "--name",
            "ghost",
        ],
        &envs,
    );
    assert!(!ok);
    assert!(err.contains("tarsi."), "{err}");
}
