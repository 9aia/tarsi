//! `[release.sync_refs]` — pack-ref/doc version sync (T-039).
//!
//! Repos used to do this in `scripts/release.sh` *before* calling
//! `tarsi-serve release`: learn the planned version, `sed`
//! `name@old` → `name@new` in `libraries/*/tamias.yaml` +
//! README/SKILL install examples, commit, *then* release — so the
//! tagged tree carried consistent refs. This stage folds that into
//! the pipeline: it runs inside `bump`/`release` after manifests
//! are bumped and before the release commit, so synced files ride
//! the same commit the tag points at.
//!
//! Safety model (mirrors the shims):
//!
//! - **Opt-in** — no `[release.sync_refs]` rules, no-op.
//! - **Single-version only** — sync needs one unambiguous
//!   `old → new` pair. A plan whose releasing units disagree on
//!   `new_version` (or `current_version`) is skipped with a
//!   reason, exactly like the shims' `len(vs)==1` python guard.
//! - **Idempotent** — rules match the *old* version; once refs
//!   say `new`, nothing matches and the stage reports zero
//!   changes. Re-running `bump`/`release` after a successful sync
//!   is a no-op for this stage.
//! - **Dry-run safe** — `dry_run: true` computes the same report
//!   (files + replacement counts) without writing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{json, Value};
use tarsi_core::config::{SyncRefRule, SyncRefsConfig};

/// What the stage did (or would do). Serialized into the `bump`
/// result's `sync_refs` key and the `release` result's.
#[derive(Debug, Clone)]
pub struct SyncOutcome {
    /// The report value embedded in op results.
    pub report: Value,
    /// Repo-root-relative paths whose content changed (or would
    /// change under dry-run) — the caller stages these into the
    /// release commit.
    pub changed_files: Vec<String>,
}

/// Resolve the single `old → new` version pair the rules rewrite
/// between. Releasing units are plan-unit JSON carrying
/// `spec.current_version` and `new_version`. `Err(reason)` reports
/// a skip condition, not a failure — the release proceeds without
/// syncing, like the shims.
fn version_pair(releasing: &[Value]) -> Result<(String, String), String> {
    let mut olds = std::collections::BTreeSet::new();
    let mut news = std::collections::BTreeSet::new();
    for u in releasing {
        olds.insert(
            u["spec"]["current_version"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        news.insert(u["new_version"].as_str().unwrap_or_default().to_string());
    }
    if olds.len() != 1 || news.len() != 1 {
        return Err(format!(
            "mixed-version plan ({} old × {} new across {} releasing units)",
            olds.len(),
            news.len(),
            releasing.len()
        ));
    }
    let old = olds.into_iter().next().unwrap_or_default();
    let new = news.into_iter().next().unwrap_or_default();
    if old == new {
        return Err("no version change".to_string());
    }
    Ok((old, new))
}

/// Compile one rule's `find` template: `{old}`/`{new}` expand to
/// the regex-escaped versions so a literal `1.2.3` can't match
/// `1.2.30`. `replace` stays a literal template expanded later.
fn compile_find(rule: &SyncRefRule, idx: usize, old: &str, new: &str) -> Result<Regex, String> {
    let pat = rule
        .find
        .replace("{old}", &regex::escape(old))
        .replace("{new}", &regex::escape(new));
    Regex::new(&pat).map_err(|e| format!("sync_refs rule {idx}: bad find regex {pat:?}: {e}"))
}

/// The literal replacement for one rule — `{old}`/`{new}` expand
/// unescaped (replacement text is literal, not regex).
fn expand_replace(rule: &SyncRefRule, old: &str, new: &str) -> String {
    rule.replace.replace("{old}", old).replace("{new}", new)
}

/// Expand a rule's `files` globs to concrete files under `dir`,
/// sorted + deduped. Non-magic patterns that don't exist and
/// magic patterns matching nothing both yield nothing — missing
/// files are not an error (a repo may carry rules for files it
/// doesn't have).
fn expand_files(dir: &Path, files: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for pat in files {
        let full = dir.join(pat);
        let full_str = full.to_string_lossy().to_string();
        match glob::glob(&full_str) {
            Ok(paths) => {
                for p in paths.flatten() {
                    if p.is_file() {
                        out.push(p);
                    }
                }
            }
            Err(e) => return Err(format!("sync_refs: bad file glob {pat:?}: {e}")),
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Repo-relative `/`-separated path for staging/reporting.
fn rel_path(dir: &Path, p: &Path) -> String {
    p.strip_prefix(dir)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Run the sync stage. `releasing` is the plan-unit JSON array the
/// ops already carry; `dry_run` computes the report without
/// writing. Never fails on *skip* conditions (mixed versions, no
/// rules) — only on I/O or bad config, which the caller turns into
/// a step error.
pub fn run(
    dir: &Path,
    cfg: &SyncRefsConfig,
    releasing: &[Value],
    dry_run: bool,
) -> Result<SyncOutcome, String> {
    if cfg.rules.is_empty() {
        return Ok(SyncOutcome {
            report: json!({ "status": "disabled" }),
            changed_files: Vec::new(),
        });
    }
    let (old, new) = match version_pair(releasing) {
        Ok(pair) => pair,
        Err(reason) => {
            return Ok(SyncOutcome {
                report: json!({
                    "status": "skipped",
                    "reason": reason,
                    "old": Value::Null,
                    "new": Value::Null,
                    "changes": [],
                }),
                changed_files: Vec::new(),
            });
        }
    };

    // Compile all rules up front — a bad regex fails before any
    // file is touched.
    let compiled: Vec<(usize, Regex, String)> = cfg
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            compile_find(r, i, &old, &new).map(|re| (i, re, expand_replace(r, &old, &new)))
        })
        .collect::<Result<_, _>>()?;

    // Apply rules in order over an in-memory overlay so later
    // rules see earlier rules' rewrites (and dry-run reports what
    // a real run would do). Writes happen once, at the end.
    let mut contents: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut changes: Vec<Value> = Vec::new();
    let mut changed_files: Vec<String> = Vec::new();
    for (rule_idx, re, repl) in &compiled {
        let rule = &cfg.rules[*rule_idx];
        for path in expand_files(dir, &rule.files)? {
            let text = match contents.get(&path) {
                Some(t) => t.clone(),
                None => std::fs::read_to_string(&path)
                    .map_err(|e| format!("sync_refs: read {}: {e}", path.display()))?,
            };
            let n = re.find_iter(&text).count();
            if n == 0 {
                continue;
            }
            let new_text = re
                .replace_all(&text, |_: &regex::Captures| repl.as_str())
                .into_owned();
            if new_text == text {
                continue;
            }
            changes.push(json!({
                "rule": rule_idx,
                "file": rel_path(dir, &path),
                "replacements": n,
            }));
            let rel = rel_path(dir, &path);
            if !changed_files.contains(&rel) {
                changed_files.push(rel);
            }
            contents.insert(path, new_text);
        }
    }

    if !dry_run {
        for (path, text) in &contents {
            std::fs::write(path, text)
                .map_err(|e| format!("sync_refs: write {}: {e}", path.display()))?;
        }
    }

    Ok(SyncOutcome {
        report: json!({
            "status": if dry_run { "dry-run" } else { "applied" },
            "reason": Value::Null,
            "old": old,
            "new": new,
            "changes": changes,
        }),
        changed_files,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn unit_json(current: &str, new: &str) -> Value {
        json!({
            "spec": { "name": "app", "current_version": current },
            "new_version": new,
        })
    }

    fn rule(find: &str, replace: &str, files: &[&str]) -> SyncRefRule {
        SyncRefRule {
            files: files.iter().map(|s| (*s).to_string()).collect(),
            find: find.to_string(),
            replace: replace.to_string(),
        }
    }

    #[test]
    fn pair_requires_single_old_and_new() {
        assert_eq!(
            version_pair(&[unit_json("0.1.0", "0.2.0")]).unwrap(),
            ("0.1.0".to_string(), "0.2.0".to_string())
        );
        assert!(version_pair(&[unit_json("0.1.0", "0.2.0"), unit_json("0.1.0", "0.1.1")]).is_err());
        assert!(version_pair(&[unit_json("0.1.0", "0.1.0")]).is_err());
        assert!(version_pair(&[]).is_err());
    }

    #[test]
    fn empty_rules_disabled() {
        let d = tempfile::tempdir().unwrap();
        let out = run(d.path(), &SyncRefsConfig::default(), &[], true).unwrap();
        assert_eq!(out.report["status"], "disabled");
    }

    #[test]
    fn mixed_plan_skips() {
        let d = tempfile::tempdir().unwrap();
        let cfg = SyncRefsConfig {
            rules: vec![rule("app@{old}", "app@{new}", &["README.md"])],
        };
        let releasing = vec![unit_json("0.1.0", "0.2.0"), unit_json("0.1.0", "0.1.1")];
        let out = run(d.path(), &cfg, &releasing, false).unwrap();
        assert_eq!(out.report["status"], "skipped");
        assert!(out.changed_files.is_empty());
    }

    #[test]
    fn rewrites_refs_and_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("README.md"),
            "install: tamias install app@0.1.0\napp@0.1.0 again\n",
        )
        .unwrap();
        let cfg = SyncRefsConfig {
            rules: vec![rule("app@{old}", "app@{new}", &["README.md"])],
        };
        let releasing = vec![unit_json("0.1.0", "0.2.0")];
        let out = run(d.path(), &cfg, &releasing, false).unwrap();
        assert_eq!(out.report["status"], "applied");
        assert_eq!(out.changed_files, vec!["README.md".to_string()]);
        assert_eq!(out.report["changes"][0]["replacements"], 2);
        let text = std::fs::read_to_string(d.path().join("README.md")).unwrap();
        assert_eq!(text, "install: tamias install app@0.2.0\napp@0.2.0 again\n");
        // Second run: nothing left matching `old` → zero changes.
        let out2 = run(d.path(), &cfg, &releasing, false).unwrap();
        assert!(out2.changed_files.is_empty());
        assert_eq!(out2.report["changes"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn version_is_regex_escaped_but_not_bounded() {
        // `{old}` expands regex-escaped (the `.`s are literal), but
        // — like the shims' `sed s|name@old|…|g` — the pattern is
        // not auto-bounded: `app@1.2.3` still matches the prefix of
        // `app@1.2.30`. Refs that need a boundary carry one in
        // `find` (e.g. `app@{old}\b` — `regex` has no look-around;
        // `\b` suffices since versions are word chars).
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("r.md"), "app@1.2.30 app@1.2.3\n").unwrap();
        let cfg = SyncRefsConfig {
            rules: vec![rule("app@{old}\\b", "app@{new}", &["r.md"])],
        };
        let releasing = vec![unit_json("1.2.3", "1.2.4")];
        run(d.path(), &cfg, &releasing, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join("r.md")).unwrap(),
            "app@1.2.30 app@1.2.4\n"
        );
    }

    #[test]
    fn dry_run_reports_without_writing() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("r.md"), "app@0.1.0\n").unwrap();
        let cfg = SyncRefsConfig {
            rules: vec![rule("app@{old}", "app@{new}", &["r.md"])],
        };
        let releasing = vec![unit_json("0.1.0", "0.2.0")];
        let out = run(d.path(), &cfg, &releasing, true).unwrap();
        assert_eq!(out.report["status"], "dry-run");
        assert_eq!(out.changed_files, vec!["r.md".to_string()]);
        assert_eq!(
            std::fs::read_to_string(d.path().join("r.md")).unwrap(),
            "app@0.1.0\n"
        );
    }

    #[test]
    fn anchored_line_pattern() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("tamias.yaml"),
            "name: app\nversion: \"0.1.0\"\nnote: version: \"0.1.0\" stays\n",
        )
        .unwrap();
        let cfg = SyncRefsConfig {
            rules: vec![rule(
                "(?m)^version: \"{old}\"",
                "version: \"{new}\"",
                &["tamias.yaml"],
            )],
        };
        let releasing = vec![unit_json("0.1.0", "0.2.0")];
        run(d.path(), &cfg, &releasing, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(d.path().join("tamias.yaml")).unwrap(),
            "name: app\nversion: \"0.2.0\"\nnote: version: \"0.1.0\" stays\n"
        );
    }

    #[test]
    fn missing_files_and_globs_are_skipped() {
        let d = tempfile::tempdir().unwrap();
        let cfg = SyncRefsConfig {
            rules: vec![rule("x@{old}", "x@{new}", &["nope/**", "absent.md"])],
        };
        let releasing = vec![unit_json("0.1.0", "0.2.0")];
        let out = run(d.path(), &cfg, &releasing, false).unwrap();
        assert_eq!(out.report["status"], "applied");
        assert!(out.changed_files.is_empty());
    }

    #[test]
    fn bad_regex_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        let cfg = SyncRefsConfig {
            rules: vec![rule("([", "x", &["r.md"])],
        };
        let releasing = vec![unit_json("0.1.0", "0.2.0")];
        assert!(run(d.path(), &cfg, &releasing, false).is_err());
    }
}
