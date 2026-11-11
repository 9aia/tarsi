//! tarsi-serve — the service surface. ndjson JSON-lines ops over
//! stdin/stdout (one `{"op": …, "params": …}` per line, one
//! `{"ok": …}`/`{"error": …}` response per line) — the same
//! protocol shape as `sepia serve` / `tamias-driver` verbs.
//!
//! Composition only: config via `tarsi-config` (scarab-config),
//! manifests via `tarsi-manifest`, history/tags/push via
//! `lasius-vcs` + `lasius-vcs-git`, planning via `tarsi-core`,
//! publishing via `tarsi-publish*` impls. No orchestration or UI
//! lives here — peregrin/pavo call these ops.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use lasius_vcs::Vcs;
use lasius_vcs_git::GitVcs;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tarsi_core::commit::Commit;
use tarsi_core::{plan_release, Config, PublishTarget, ReleasePlan, ReleaseUnitSpec};
use tarsi_publish::{PublishJob, Publisher};

#[derive(Debug, Deserialize)]
pub struct Request {
    pub op: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    fn ok(result: Value) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(msg.into()),
        }
    }
}

/// Run one request line → response value (pure-ish; all I/O is
/// through the injected `vcs` for testability).
pub fn dispatch(vcs: &dyn Vcs, req: &Request) -> Response {
    let params = &req.params;
    let dir = params
        .get("dir")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    match req.op.as_str() {
        "manifest" => Response::ok(json!({
            "name": "tarsi-serve",
            "version": env!("CARGO_PKG_VERSION"),
            "ops": ["manifest", "plan", "changelog", "notes", "release"],
        })),
        "plan" => match build_plan(vcs, &dir, params) {
            Ok(v) => Response::ok(v),
            Err(e) => Response::err(e),
        },
        "changelog" => match build_plan(vcs, &dir, params) {
            Ok(v) => {
                let changelogs: Vec<Value> = v["plan"]["units"]
                    .as_array()
                    .map(|us| {
                        us.iter()
                            .filter(|u| u["releases"].as_bool().unwrap_or(false))
                            .map(|u| {
                                json!({
                                    "unit": u["spec"]["name"],
                                    "version": u["new_version"],
                                    "tag": u["tag"],
                                    "markdown": u["changelog_markdown"],
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Response::ok(json!({ "changelogs": changelogs }))
            }
            Err(e) => Response::err(e),
        },
        "notes" => match build_plan(vcs, &dir, params) {
            Ok(v) => {
                let mut md = String::new();
                if let Some(us) = v["plan"]["units"].as_array() {
                    for u in us {
                        if u["releases"].as_bool().unwrap_or(false) {
                            if let Some(m) = u["changelog_markdown"].as_str() {
                                md.push_str(m);
                                md.push('\n');
                            }
                        }
                    }
                }
                Response::ok(json!({ "notes": md }))
            }
            Err(e) => Response::err(e),
        },
        "release" => Response::ok(execute_release(vcs, &dir, params)),
        other => Response::err(format!("unknown op {other:?}")),
    }
}

/// Load config + specs + commits → plan, serialized with rendered
/// changelog markdown per unit.
fn build_plan(vcs: &dyn Vcs, dir: &Path, params: &Value) -> Result<Value, String> {
    let cfg = load_config(dir)?;
    let specs = resolve_specs(dir, &cfg)?;
    let commits = gather_commits(vcs, dir, &specs, params)?;
    let date = today();
    let plan = plan_release(&specs, &commits, Some(date)).map_err(|e| e.to_string())?;
    let plan_json = plan_to_json(&plan);
    Ok(json!({ "specs": specs, "plan": plan_json }))
}

fn load_config(dir: &Path) -> Result<Config, String> {
    match tarsi_config::load_in(dir) {
        Ok(c) => Ok(c),
        Err(e) => {
            // No tarsi.* anywhere → defaults (cargo workspace scan).
            // A present-but-broken file still errors loudly.
            let local = ["tarsi.toml", "tarsi.yaml", "tarsi.yml"]
                .iter()
                .any(|f| dir.join(f).exists());
            if local {
                Err(e)
            } else {
                Ok(Config::default())
            }
        }
    }
}

fn resolve_specs(dir: &Path, cfg: &Config) -> Result<Vec<ReleaseUnitSpec>, String> {
    match cfg.release.units.as_ref() {
        Some(tarsi_core::unit::UnitsSpec::Explicit { units }) => {
            Ok(tarsi_manifest::apply_config(units.clone(), cfg))
        }
        _ => {
            let specs = tarsi_manifest::cargo_workspace_units(dir).map_err(|e| e.to_string())?;
            Ok(tarsi_manifest::apply_config(specs, cfg))
        }
    }
}

/// Union of commits across each unit's since-last-tag range.
/// `params.range` (`"a..b"`) overrides: same range for every unit.
fn gather_commits(
    vcs: &dyn Vcs,
    dir: &Path,
    specs: &[ReleaseUnitSpec],
    params: &Value,
) -> Result<Vec<Commit>, String> {
    let explicit = params
        .get("range")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for spec in specs {
        let rev = match &explicit {
            Some(r) => r.clone(),
            None => match latest_tag(vcs, dir, spec) {
                Some(t) => format!("{t}..HEAD"),
                None => "HEAD".to_string(),
            },
        };
        for ci in vcs.commits(dir, &rev, 0) {
            if seen.insert(ci.sha.clone()) {
                out.push(Commit {
                    sha: ci.sha,
                    subject: ci.subject,
                    body: ci.body,
                    paths: ci.paths,
                });
            }
        }
    }
    // Newest first overall.
    out.sort_by(|a, b| b.sha.cmp(&a.sha));
    Ok(out)
}

/// Latest `{unit}-vX.Y.Z`-style tag for a spec — derived from its
/// tag_format by splitting on `{version}`.
fn latest_tag(vcs: &dyn Vcs, dir: &Path, spec: &ReleaseUnitSpec) -> Option<String> {
    let (prefix, _) = spec
        .tag_format
        .split_once("{version}")
        .unwrap_or((&spec.tag_format, ""));
    let prefix = prefix.replace("{unit}", &spec.name);
    vcs.tags(dir)
        .into_iter()
        .filter(|t| t.starts_with(&prefix))
        .filter_map(|t| {
            let v = t[prefix.len()..].trim_start_matches('v');
            semver::Version::parse(v).ok().map(|sv| (t, sv))
        })
        .max_by(|a, b| a.1.cmp(&b.1))
        .map(|(t, _)| t)
}

fn plan_to_json(plan: &ReleasePlan) -> Value {
    let units: Vec<Value> = plan
        .units
        .iter()
        .map(|u| {
            json!({
                "spec": u.spec,
                "bump": u.bump,
                "new_version": u.new_version,
                "tag": u.tag,
                "releases": u.releases(),
                "changelog_markdown": u.changelog.render(),
                "commits": u.commits.len(),
            })
        })
        .collect();
    let unattributed: Vec<Value> = plan
        .unattributed
        .iter()
        .map(|c| json!({ "sha": c.sha, "subject": c.subject }))
        .collect();
    json!({ "units": units, "unattributed": unattributed, "steps": plan.steps() })
}

/// `release` op — executes plan steps. `dry_run` defaults TRUE:
/// real mutation requires `{"dry_run": false}`.
fn execute_release(vcs: &dyn Vcs, dir: &Path, params: &Value) -> Value {
    let dry_run = params
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let plan_v = match build_plan(vcs, dir, params) {
        Ok(v) => v,
        Err(e) => return json!({ "ok": false, "error": e }),
    };
    let releasing: Vec<Value> = plan_v["plan"]["units"]
        .as_array()
        .map(|us| {
            us.iter()
                .filter(|u| u["releases"].as_bool().unwrap_or(false))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if releasing.is_empty() {
        return json!({
            "dry_run": dry_run,
            "plan": plan_v["plan"],
            "steps_run": [],
            "publish": [],
            "note": "nothing to release",
        });
    }

    let mut steps_run: Vec<Value> = Vec::new();
    let mut publish_reports: Vec<Value> = Vec::new();

    if !dry_run {
        for u in &releasing {
            let _ = write_changelog(dir, u);
        }
        steps_run.push(json!("write-changelogs"));
        for u in &releasing {
            let path = u["spec"]["path"].as_str().unwrap_or(".");
            let ver = u["new_version"].as_str().unwrap_or_default();
            if let Ok(v) = semver::Version::parse(ver) {
                let _ = tarsi_manifest::write_unit_version(dir, path, &v);
            }
        }
        steps_run.push(json!("bump-manifests"));
        let _ = git(dir, &["add", "-A"]);
        let _ = git(dir, &["commit", "-m", "chore(release): bump versions"]);
        steps_run.push(json!("commit-bump"));
        for u in &releasing {
            let tag = u["tag"].as_str().unwrap_or_default();
            let _ = vcs.tag(dir, tag, None);
        }
        steps_run.push(json!("tag"));
    }

    let cfg = load_config(dir).unwrap_or_default();
    let gh_repo = resolve_gh_repo(dir, &cfg);
    for u in &releasing {
        let unit = u["spec"]["name"].as_str().unwrap_or_default().to_string();
        let version = u["new_version"].as_str().unwrap_or_default().to_string();
        // Expand `[publish.ghreleases] assets` globs once per unit —
        // `{unit}`/`{version}` substitute per release. Only done
        // when this unit actually targets GitHub Releases and the
        // publisher is enabled; expansion failure is reported on
        // the ghreleases entry, not fatal to other targets.
        let wants_gh = u["spec"]["publish"]
            .as_array()
            .is_some_and(|ps| ps.iter().any(|p| p.as_str() == Some("github-release")));
        let (assets, asset_err) = if wants_gh && cfg.publish.ghreleases.enabled {
            match tarsi_publish_ghreleases::expand_assets(
                dir,
                &cfg.publish.ghreleases.assets,
                &unit,
                &version,
            ) {
                Ok(a) => (a, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            }
        } else {
            (Vec::new(), None)
        };
        let job = PublishJob {
            unit: unit.clone(),
            version: version.clone(),
            manifest_dir: PathBuf::from(u["spec"]["path"].as_str().unwrap_or(".")),
            tag: u["tag"].as_str().unwrap_or_default().to_string(),
            notes: u["changelog_markdown"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            assets,
            dry_run,
        };
        for target in u["spec"]["publish"].as_array().cloned().unwrap_or_default() {
            let t =
                serde_json::from_value::<PublishTarget>(target).unwrap_or(PublishTarget::CratesIo);
            let entry = match t {
                PublishTarget::CratesIo if cfg.publish.crates.enabled => {
                    tarsi_publish_crates::CratesPublisher::new(dir.to_path_buf())
                        .publish(&job)
                        .map(|r| json!(r))
                        .map_err(|e| e.to_string())
                }
                PublishTarget::GithubRelease if cfg.publish.ghreleases.enabled => {
                    if let Some(e) = &asset_err {
                        Err(e.clone())
                    } else {
                        tarsi_publish_ghreleases::GhReleasePublisher::new(
                            gh_repo.clone().unwrap_or_default(),
                            cfg.publish.ghreleases.token_env.clone(),
                        )
                        .publish(&job)
                        .map(|r| json!(r))
                        .map_err(|e| e.to_string())
                    }
                }
                _ => Ok(json!({ "unit": unit, "target": t, "ok": true, "detail": "disabled" })),
            };
            publish_reports.push(match entry {
                Ok(v) => v,
                Err(e) => json!({ "unit": unit, "target": t, "ok": false, "detail": e }),
            });
        }
    }

    if !dry_run {
        let _ = vcs.push(dir, "origin", &[]);
        for u in &releasing {
            if let Some(tag) = u["tag"].as_str() {
                let _ = vcs.push_tag(dir, "origin", tag);
            }
        }
        steps_run.push(json!("push"));
    }

    json!({
        "dry_run": dry_run,
        "plan": plan_v["plan"],
        "steps_run": steps_run,
        "publish": publish_reports,
    })
}

fn write_changelog(dir: &Path, unit: &Value) -> Result<(), String> {
    let path = unit["spec"]["path"].as_str().unwrap_or(".");
    let md = unit["changelog_markdown"].as_str().unwrap_or_default();
    if md.is_empty() {
        return Ok(());
    }
    let file = dir.join(path).join("CHANGELOG.md");
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    let merged = if existing.trim().is_empty() {
        format!("# Changelog\n\n{md}\n")
    } else {
        // Insert after the leading `# ` title block (title + blank).
        let mut lines = existing.lines();
        let mut prefix = String::new();
        for line in lines.by_ref() {
            prefix.push_str(line);
            prefix.push('\n');
            if line.trim().is_empty() {
                break;
            }
        }
        let rest: String = lines.collect::<Vec<_>>().join("\n");
        format!("{prefix}\n{md}\n\n{rest}")
    };
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, merged).map_err(|e| e.to_string())
}

fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let st = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if st.success() {
        Ok(())
    } else {
        Err(format!("git {args:?} failed"))
    }
}

/// owner/repo from `origin` remote or config override.
fn resolve_gh_repo(dir: &Path, cfg: &Config) -> Option<String> {
    if let Some(r) = &cfg.publish.ghreleases.repo {
        return Some(r.clone());
    }
    let url = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    let url = String::from_utf8_lossy(&url.stdout).trim().to_string();
    // git@github.com:owner/repo.git | https://github.com/owner/repo.git
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
    let repo = path.trim_end_matches(".git").trim_end_matches('/');
    (!repo.is_empty()).then(|| repo.to_string())
}

fn today() -> String {
    // `date +%F` — serve is a thin composition root; no chrono dep.
    std::process::Command::new("date")
        .arg("+%F")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unreleased".to_string())
}

/// Serve loop — ndjson in, ndjson out. Returns when stdin closes.
pub fn serve() -> i32 {
    let vcs = GitVcs;
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(line) {
            Ok(req) => dispatch(&vcs, &req),
            Err(e) => Response::err(format!("bad request: {e}")),
        };
        let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap_or_default());
        let _ = out.flush();
    }
    0
}
