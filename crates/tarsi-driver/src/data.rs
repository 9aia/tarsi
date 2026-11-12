//! data — the pavo read plane. Every query is one `tarsi-serve`
//! one-shot op (`tarsi-serve <op> --dir <d> --set k=v …`), the same
//! wire peregrin campaigns and humans use — the driver never opens
//! tarsi state itself ("pavo renders what the serve reports").
//!
//! Read-only by construction: the only ops called are `manifest`,
//! `profiles`, `plan`, and `changelog` — `release` is never
//! dispatched, and `plan`/`changelog` mutate nothing (the serve's
//! `dry_run` default applies regardless).
//!
//! The binary resolves as `$TARSI_SERVE` when set (tests and
//! non-PATH installs point it at a specific build), else
//! `tarsi-serve` on PATH — the `libraries/tarsi` pack deploys it
//! under that name.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};

use crate::Env;

/// The `tarsi-serve` binary: `$TARSI_SERVE` override, else PATH.
fn serve_bin(env: Env<'_>) -> String {
    env("TARSI_SERVE")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "tarsi-serve".to_string())
}

/// `--dir`/`--set` params → the one-shot argv tail. `--set` values
/// ride verbatim (`k=v`); hypha-serve-cli JSON-parses or string-
/// falls-back each value, so bare strings like profile names and
/// `HEAD~3..HEAD` ranges pass through untouched.
fn argv_for(op: &str, dir: &Path, sets: &[(&str, &str)]) -> Vec<String> {
    let mut argv = vec![
        op.to_string(),
        "--dir".to_string(),
        dir.display().to_string(),
    ];
    for (k, v) in sets {
        argv.push("--set".to_string());
        argv.push(format!("{k}={v}"));
    }
    argv
}

/// Run `tarsi-serve <op>` once and return the op's `result`
/// payload — the `{"ok":…,"result":…,"error":…}` envelope is
/// unwrapped here: `ok:true` → `result`, anything else → `Err`
/// carrying the serve's error (or the process failure).
///
/// # Errors
/// Binary missing/unspawnable, non-zero exit without a parseable
/// envelope, or an `ok:false` serve response.
pub fn serve_op(
    op: &str,
    dir: &Path,
    sets: &[(&str, &str)],
    env: Env<'_>,
) -> Result<Value, String> {
    let bin = serve_bin(env);
    let argv = argv_for(op, dir, sets);
    let out = Command::new(&bin)
        .args(&argv)
        .output()
        .map_err(|e| format!("tarsi: cannot run `{bin}` ({op}): {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The one-shot prints exactly one JSON line; take the last
    // non-empty one so stray noise can't corrupt the parse.
    let line = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let envelope: Value = serde_json::from_str(line.trim()).map_err(|e| {
        format!(
            "tarsi: `{bin} {}` returned unparseable output ({e}): {}",
            argv.join(" "),
            line.trim()
        )
    })?;
    if envelope["ok"].as_bool() == Some(true) {
        return Ok(envelope["result"].clone());
    }
    let msg = envelope["error"]
        .as_str()
        .map(str::to_string)
        .or_else(|| (!stderr.trim().is_empty()).then(|| stderr.trim().to_string()))
        .unwrap_or_else(|| format!("`{bin} {}` exited {}", argv.join(" "), out.status));
    Err(format!("tarsi: {msg}"))
}

/// `params["dir"]` → the repo dir the plan ops run against;
/// absent/empty → `.` (the serve's own default, resolved against
/// the driver's cwd — pavo hosts launch drivers from a stable
/// workspace root).
pub fn repo_dir(params: &Map<String, Value>) -> PathBuf {
    params
        .get("dir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The `--set` pairs plan/changelog ops honor: `profile` (select a
/// `type: tarsi` release profile — beats `$TARSI_PROFILE` and the
/// repo config's `profile` key) and `range` (`"a..b"` overrides
/// the since-last-tag commit window for every unit).
pub fn plan_sets(params: &Map<String, Value>) -> Vec<(String, String)> {
    let mut sets = Vec::new();
    for key in ["profile", "range"] {
        if let Some(v) = params.get(key).and_then(Value::as_str) {
            if !v.is_empty() {
                sets.push((key.to_string(), v.to_string()));
            }
        }
    }
    sets
}

/// `plan_sets` → the `&[(&str,&str)]` slice `serve_op` takes.
fn set_refs(sets: &[(String, String)]) -> Vec<(&str, &str)> {
    sets.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

/// The `plan` op payload for `params` — `{specs, plan}` as
/// tarsi-serve reports it.
pub fn plan_payload(params: &Map<String, Value>, env: Env<'_>) -> Result<Value, String> {
    let dir = repo_dir(params);
    let sets = plan_sets(params);
    serve_op("plan", &dir, &set_refs(&sets), env)
}

/// The `changelog` op payload — `{changelogs: [{unit, version,
/// tag, markdown}]}` for every releasing unit.
pub fn changelog_payload(params: &Map<String, Value>, env: Env<'_>) -> Result<Value, String> {
    let dir = repo_dir(params);
    let sets = plan_sets(params);
    serve_op("changelog", &dir, &set_refs(&sets), env)
}

/// The `profiles` op payload — `{profiles: [{name, version,
/// description, source}]}` across every discovery root (dir
/// profiles, `~/.config/tarsi`, the tamias store).
pub fn profiles_payload(env: Env<'_>) -> Result<Value, String> {
    serve_op("profiles", Path::new("."), &[], env)
}

/// The `manifest` op payload — `{name, version, ops}`: the serve
/// reachability probe behind the status view.
pub fn manifest_payload(env: Env<'_>) -> Result<Value, String> {
    serve_op("manifest", Path::new("."), &[], env)
}
