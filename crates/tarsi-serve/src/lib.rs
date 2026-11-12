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
        "release" => {
            let v = execute_release(vcs, &dir, params);
            // A failed release reports `ok:false` inside the result;
            // surface it as a failed op too so `--yes` consumers and
            // the ndjson loop see the failure, not a silent 200.
            if v.get("ok").and_then(Value::as_bool) == Some(false) {
                let msg = v
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("release failed")
                    .to_string();
                Response {
                    ok: false,
                    result: Some(v),
                    error: Some(msg),
                }
            } else {
                Response::ok(v)
            }
        }
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
    let plan_json = plan_to_json(&plan, &cfg);
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

/// Whether a publish target is enabled in config — `spec.publish`
/// declares *intent*; `publish.<target>.enabled` is the switch
/// execution honors. Surfaced in plan output so consumers see the
/// effective state, not just the declaration.
fn target_enabled(cfg: &Config, t: &PublishTarget) -> bool {
    match t {
        PublishTarget::CratesIo => cfg.publish.crates.enabled,
        PublishTarget::GithubRelease => cfg.publish.ghreleases.enabled,
    }
}

fn plan_to_json(plan: &ReleasePlan, cfg: &Config) -> Value {
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
                "publish_targets": u.spec.publish.iter().map(|t| json!({
                    "target": t,
                    "enabled": target_enabled(cfg, t),
                })).collect::<Vec<_>>(),
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

/// How far the mutation phase got — rollback undoes exactly this
/// much, nothing more.
#[derive(Default)]
struct ReleaseState {
    /// HEAD before the release commit (rollback target).
    pre_sha: Option<String>,
    /// The bump commit exists locally.
    committed: bool,
    /// Local tags created for this release.
    tags: Vec<String>,
    /// The branch was pushed to `origin`.
    branch_pushed: bool,
    /// Tags already pushed to `origin` (subset of `tags`, in order).
    tags_pushed: Vec<String>,
    /// Changelog files this run *created* (didn't exist before) —
    /// `reset --hard` won't remove untracked files, so rollback
    /// deletes them explicitly.
    created_files: Vec<PathBuf>,
}

/// `release` op — executes plan steps. `dry_run` defaults TRUE:
/// real mutation requires `{"dry_run": false}`.
///
/// Ordering is load-bearing: the bump commit **and** the tags are
/// pushed *before* any publish call. GitHub Releases auto-creates
/// `refs/tags/<tag>` at the remote HEAD when the tag doesn't exist
/// yet — publishing first would pin the remote tag to the
/// pre-release commit forever. Pushing first means the release
/// attaches to the tag we created.
///
/// Every mutation step propagates errors (no `let _ =`): the first
/// failure rolls back everything done so far (remote tags/branch,
/// local commit/tags, created changelog files) and the op reports
/// `ok:false`. If *some* publish target succeeded before another
/// failed, rollback is skipped — deleting the tag would orphan a
/// live release — and the report asks for manual cleanup instead.
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
            "ok": true,
            "dry_run": dry_run,
            "plan": plan_v["plan"],
            "steps_run": [],
            "publish": [],
            "note": "nothing to release",
        });
    }

    let mut steps_run: Vec<Value> = Vec::new();
    let cfg = load_config(dir).unwrap_or_default();
    let gh_repo = resolve_gh_repo(dir, &cfg);

    if dry_run {
        let publish_reports = run_publishers(dir, &cfg, &gh_repo, &releasing, true, None);
        let failed = publish_reports
            .iter()
            .filter(|r| r["ok"].as_bool() == Some(false))
            .count();
        let mut v = json!({
            "ok": failed == 0,
            "dry_run": true,
            "plan": plan_v["plan"],
            "steps_run": steps_run,
            "publish": publish_reports,
        });
        if failed > 0 {
            v["error"] = json!(format!("{failed} publish target(s) failed"));
        }
        return v;
    }

    let mut st = ReleaseState {
        pre_sha: vcs.head(dir),
        ..ReleaseState::default()
    };

    if let Err(e) = mutate_release(vcs, dir, &releasing, &mut st, &mut steps_run) {
        let rolled_back = rollback(vcs, dir, &st);
        return json!({
            "ok": false,
            "error": e,
            "dry_run": false,
            "plan": plan_v["plan"],
            "steps_run": steps_run,
            "publish": [],
            "rolled_back": rolled_back,
        });
    }

    // Everything is on the remote now — publish against it. The
    // bump sha is the tag target; ghreleases sends it as
    // `target_commitish` so the release can never guess wrong.
    let commitish = vcs.head(dir);
    let publish_reports = run_publishers(dir, &cfg, &gh_repo, &releasing, false, commitish);
    let failures: Vec<&Value> = publish_reports
        .iter()
        .filter(|r| r["ok"].as_bool() == Some(false))
        .collect();
    if !failures.is_empty() {
        let any_ok = publish_reports
            .iter()
            .any(|r| r["ok"].as_bool() == Some(true) && r["disabled"].as_bool() != Some(true));
        // Roll back only when *nothing* published — otherwise the
        // tag delete would orphan a live release; leave the state
        // and say so.
        let rolled_back = if any_ok {
            json!({
                "attempted": false,
                "reason": "a publish target succeeded — partial state left for manual cleanup",
            })
        } else {
            rollback(vcs, dir, &st)
        };
        return json!({
            "ok": false,
            "error": format!("{} publish target(s) failed", failures.len()),
            "dry_run": false,
            "plan": plan_v["plan"],
            "steps_run": steps_run,
            "publish": publish_reports,
            "rolled_back": rolled_back,
        });
    }

    json!({
        "ok": true,
        "dry_run": false,
        "plan": plan_v["plan"],
        "steps_run": steps_run,
        "publish": publish_reports,
    })
}

/// The mutation phase: changelogs → manifests → scoped stage →
/// commit → tags → **push commit+tags** → (publish happens after,
/// in [`execute_release`]). Errors are `"step: detail"` strings;
/// `st` tracks progress for [`rollback`].
fn mutate_release(
    vcs: &dyn Vcs,
    dir: &Path,
    releasing: &[Value],
    st: &mut ReleaseState,
    steps_run: &mut Vec<Value>,
) -> Result<(), String> {
    for u in releasing {
        if write_changelog(dir, u)? {
            st.created_files.push(changelog_path(dir, u));
        }
    }
    steps_run.push(json!("write-changelogs"));
    for u in releasing {
        let path = u["spec"]["path"].as_str().unwrap_or(".");
        let ver = u["new_version"].as_str().unwrap_or_default();
        if let Ok(v) = semver::Version::parse(ver) {
            tarsi_manifest::write_unit_version(dir, path, &v)
                .map_err(|e| format!("bump-manifests: {e}"))?;
        }
    }
    steps_run.push(json!("bump-manifests"));
    stage_release_paths(vcs, dir, releasing)?;
    vcs.commit(dir, "chore(release): bump versions", None, true)
        .map_err(|e| format!("commit-bump: {e}"))?;
    st.committed = true;
    steps_run.push(json!("commit-bump"));
    for u in releasing {
        let tag = u["tag"].as_str().unwrap_or_default().to_string();
        if !vcs.tag(dir, &tag, None) {
            return Err(format!("tag {tag}: git tag failed"));
        }
        st.tags.push(tag);
    }
    steps_run.push(json!("tag"));
    // Push the commit AND the tags before publishing — see the
    // ordering note on `execute_release`.
    vcs.push_branch(dir, "origin", None)
        .map_err(|e| format!("push: {e}"))?;
    st.branch_pushed = true;
    for t in &st.tags {
        vcs.exec_out(dir, &["push", "origin", t])
            .map_err(|e| format!("push-tag {t}: {e}"))?;
        st.tags_pushed.push(t.clone());
    }
    steps_run.push(json!("push"));
    Ok(())
}

/// Stage only the files tarsi touched — `CHANGELOG.md` +
/// `Cargo.toml` per releasing unit, plus the workspace-root
/// `Cargo.toml`/`Cargo.lock` and `tarsi.*` config candidates.
/// Never `git add -A`: unrelated dirty files must not ride the
/// release commit.
fn stage_release_paths(vcs: &dyn Vcs, dir: &Path, releasing: &[Value]) -> Result<(), String> {
    let mut paths = std::collections::BTreeSet::new();
    for u in releasing {
        let p = u["spec"]["path"].as_str().unwrap_or(".");
        let p = if p.is_empty() { "." } else { p };
        for f in ["CHANGELOG.md", "Cargo.toml"] {
            paths.insert(format!("{p}/{f}"));
        }
    }
    for f in [
        "Cargo.toml",
        "Cargo.lock",
        "tarsi.toml",
        "tarsi.yaml",
        "tarsi.yml",
    ] {
        paths.insert(f.to_string());
    }
    let existing: Vec<String> = paths.into_iter().filter(|p| dir.join(p).exists()).collect();
    if existing.is_empty() {
        return Ok(());
    }
    let mut args: Vec<&str> = vec!["add", "--"];
    args.extend(existing.iter().map(String::as_str));
    vcs.exec_out(dir, &args)
        .map(|_| ())
        .map_err(|e| format!("stage-bump: {e}"))
}

/// Undo everything [`ReleaseState`] records — best-effort, per-step
/// results reported. Remote cleanup runs before local so a failed
/// local reset can't strand pushed tags.
fn rollback(vcs: &dyn Vcs, dir: &Path, st: &ReleaseState) -> Value {
    fn record(steps: &mut Vec<Value>, step: String, res: Result<(), String>) {
        steps.push(match res {
            Ok(()) => json!({ "step": step, "ok": true }),
            Err(e) => json!({ "step": step, "ok": false, "error": e }),
        });
    }
    let mut steps: Vec<Value> = Vec::new();
    for t in &st.tags_pushed {
        record(
            &mut steps,
            format!("delete-remote-tag {t}"),
            vcs.exec_out(dir, &["push", "origin", "--delete", t])
                .map(|_| ())
                .map_err(|e| e.to_string()),
        );
    }
    if st.branch_pushed {
        match (&st.pre_sha, vcs.current_branch(dir)) {
            (Some(pre), Some(branch)) => {
                let refspec = format!("{pre}:refs/heads/{branch}");
                record(
                    &mut steps,
                    format!("reset-remote-branch {branch}"),
                    vcs.exec_out(dir, &["push", "--force-with-lease", "origin", &refspec])
                        .map(|_| ())
                        .map_err(|e| e.to_string()),
                );
            }
            _ => steps.push(json!({
                "step": "reset-remote-branch",
                "ok": false,
                "error": "skipped: no pre-release sha or detached HEAD — reset the remote branch manually",
            })),
        }
    }
    if st.committed {
        if let Some(pre) = &st.pre_sha {
            record(
                &mut steps,
                "reset-local".to_string(),
                vcs.exec_out(dir, &["reset", "--hard", pre])
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
            );
        }
    }
    // Remove changelog files this run created — `reset --hard`
    // only restores tracked files. Only delete when still
    // untracked (a pre-existing CHANGELOG is tracked and was
    // already restored by the reset).
    for f in &st.created_files {
        let rel = f
            .strip_prefix(dir)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        let tracked = vcs
            .exec_out(dir, &["ls-files", "--error-unmatch", "--", &rel])
            .is_ok();
        if !tracked {
            // `reset --hard` already removes files that only existed
            // in the rolled-back commit — absent is success.
            let res = if f.exists() {
                std::fs::remove_file(f).map_err(|e| e.to_string())
            } else {
                Ok(())
            };
            record(&mut steps, format!("remove-created {rel}"), res);
        }
    }
    for t in &st.tags {
        record(
            &mut steps,
            format!("delete-local-tag {t}"),
            if vcs.delete_tag(dir, t) {
                Ok(())
            } else {
                Err("git tag -d failed".to_string())
            },
        );
    }
    let ok = steps.iter().all(|s| s["ok"].as_bool() == Some(true));
    json!({ "attempted": true, "ok": ok, "steps": steps })
}

/// Run every unit × publish target, collecting per-target reports.
/// `commitish` is the pushed bump sha — sent to forges as the
/// release target so they never guess a commit.
fn run_publishers(
    dir: &Path,
    cfg: &Config,
    gh_repo: &Option<String>,
    releasing: &[Value],
    dry_run: bool,
    commitish: Option<String>,
) -> Vec<Value> {
    let mut publish_reports: Vec<Value> = Vec::new();
    for u in releasing {
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
            commitish: commitish.clone(),
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
                // `disabled: true` marks a skipped target — it must
                // not count as a successful publish when deciding
                // whether a failure warrants rollback.
                _ => Ok(
                    json!({ "unit": unit, "target": t, "ok": true, "detail": "disabled", "disabled": true }),
                ),
            };
            publish_reports.push(match entry {
                Ok(v) => v,
                Err(e) => json!({ "unit": unit, "target": t, "ok": false, "detail": e }),
            });
        }
    }
    publish_reports
}

/// `<unit path>/CHANGELOG.md` for a plan unit.
fn changelog_path(dir: &Path, unit: &Value) -> PathBuf {
    let path = unit["spec"]["path"].as_str().unwrap_or(".");
    dir.join(path).join("CHANGELOG.md")
}

/// Write (or prepend into) a unit's `CHANGELOG.md`.
/// Returns `Ok(true)` when the file was newly created — rollback
/// uses that to remove files `reset --hard` can't reach.
fn write_changelog(dir: &Path, unit: &Value) -> Result<bool, String> {
    let file = changelog_path(dir, unit);
    let md = unit["changelog_markdown"].as_str().unwrap_or_default();
    if md.is_empty() {
        return Ok(false);
    }
    let existed = file.exists();
    let existing = std::fs::read_to_string(&file).unwrap_or_default();
    let md = md.trim_end();
    let merged = if existing.trim().is_empty() {
        format!("# Changelog\n\n{md}\n")
    } else {
        splice_changelog(&existing, md)
    };
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&file, merged).map_err(|e| e.to_string())?;
    Ok(!existed)
}

/// Splice a rendered `## [x.y.z] - date` block into an existing
/// changelog, keeping strict keep-a-changelog order — title →
/// preamble → `## [Unreleased]` → releases newest-first. The new
/// section goes immediately *before* the first versioned
/// `## [..]` heading (so `[Unreleased]` and any preamble stay on
/// top); when only `[Unreleased]` exists it appends after it;
/// with no release headings at all it keeps the legacy splice
/// directly under the title block. Line-based rebuild preserves
/// the file's trailing newline (and trailing blank lines).
fn splice_changelog(existing: &str, md: &str) -> String {
    let mut lines = existing.lines();
    // Title block: the `# ` heading plus the blank line after it
    // (or everything, when the file has no blank line at all).
    let mut head: Vec<&str> = Vec::new();
    for line in lines.by_ref() {
        head.push(line);
        if line.trim().is_empty() {
            break;
        }
    }
    let rest: Vec<&str> = lines.collect();

    let is_versioned = |l: &&str| {
        let t = l.trim_start();
        t.starts_with("## [") && !t.starts_with("## [Unreleased]")
    };
    let is_unreleased = |l: &&str| l.trim_start().starts_with("## [Unreleased]");
    let insert_at = rest
        .iter()
        .position(is_versioned)
        .or_else(|| rest.iter().any(is_unreleased).then_some(rest.len()))
        .unwrap_or(0);

    let mut out = String::new();
    for l in &head {
        out.push_str(l);
        out.push('\n');
    }
    if !head.last().is_some_and(|l| l.trim().is_empty()) {
        out.push('\n'); // blank line after the title
    }
    for l in &rest[..insert_at] {
        out.push_str(l);
        out.push('\n');
    }
    if !out.ends_with("\n\n") {
        out.push('\n'); // exactly one blank line before the section
    }
    out.push_str(md);
    out.push('\n');
    if insert_at < rest.len() {
        out.push('\n'); // blank line after the section
        for l in &rest[insert_at..] {
            out.push_str(l);
            out.push('\n');
        }
    }
    out
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn unit(path: &str, md: &str) -> Value {
        json!({
            "spec": { "name": "u", "path": path },
            "changelog_markdown": md,
        })
    }

    #[test]
    fn changelog_splice_has_single_blank_after_title() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("CHANGELOG.md");
        std::fs::write(&f, "# Changelog\n\n## 0.1.0\n\nold entry\n").unwrap();
        let created = write_changelog(d.path(), &unit(".", "## 0.2.0\n\nnew entry\n")).unwrap();
        assert!(!created, "file existed — not a create");
        let s = std::fs::read_to_string(&f).unwrap();
        assert!(s.starts_with("# Changelog\n\n## 0.2.0"), "{s:?}");
        assert!(!s.contains("# Changelog\n\n\n"), "double blank: {s:?}");
        assert!(s.contains("## 0.1.0"), "old entry kept: {s:?}");
        assert!(s.ends_with('\n'), "trailing newline kept: {s:?}");
    }

    #[test]
    fn changelog_splice_keeps_unreleased_and_preamble_on_top() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("CHANGELOG.md");
        std::fs::write(
            &f,
            "# Changelog\n\nPreamble prose.\n\n## [Unreleased]\n\n### Added\n\n- pending\n\n## [0.1.0] - 2026-01-01\n\nold\n",
        )
        .unwrap();
        write_changelog(
            d.path(),
            &unit(".", "## [0.2.0] - 2026-02-01\n\nnew entry\n"),
        )
        .unwrap();
        let s = std::fs::read_to_string(&f).unwrap();
        let preamble = s.find("Preamble prose.").unwrap();
        let unreleased = s.find("## [Unreleased]").unwrap();
        let new_rel = s.find("## [0.2.0]").unwrap();
        let old = s.find("## [0.1.0]").unwrap();
        assert!(
            preamble < unreleased && unreleased < new_rel && new_rel < old,
            "keep-a-changelog order violated: {s:?}"
        );
        assert!(s.ends_with('\n'), "trailing newline kept: {s:?}");
    }

    #[test]
    fn changelog_splice_appends_after_lone_unreleased() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("CHANGELOG.md");
        std::fs::write(&f, "# Changelog\n\n## [Unreleased]\n\n- pending\n").unwrap();
        write_changelog(d.path(), &unit(".", "## [0.1.0]\n\nfirst\n")).unwrap();
        let s = std::fs::read_to_string(&f).unwrap();
        assert!(s.find("## [Unreleased]") < s.find("## [0.1.0]"), "{s:?}");
        assert!(s.ends_with("first\n"), "single trailing newline: {s:?}");
    }

    #[test]
    fn changelog_new_file_reports_created() {
        let d = tempfile::tempdir().unwrap();
        let created = write_changelog(d.path(), &unit("crates/u", "## 1.0.0\n\nnotes\n")).unwrap();
        assert!(created);
        let s = std::fs::read_to_string(d.path().join("crates/u/CHANGELOG.md")).unwrap();
        assert_eq!(s, "# Changelog\n\n## 1.0.0\n\nnotes\n");
    }

    #[test]
    fn changelog_empty_markdown_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let created = write_changelog(d.path(), &unit(".", "")).unwrap();
        assert!(!created);
        assert!(!d.path().join("CHANGELOG.md").exists());
    }
}
