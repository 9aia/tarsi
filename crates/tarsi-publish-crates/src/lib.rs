//! tarsi-publish-crates — crates.io publisher. Runs
//! `cargo publish --manifest-path <unit>/Cargo.toml` per unit;
//! the *caller* supplies units already in dependency order
//! (plan_release topo-sorts). Retries on crates.io 429/rate-limit
//! output with linear backoff; "already exists" counts as success
//! (idempotent re-runs — release.sh semantics).

use std::process::Command;
use std::time::Duration;

use tarsi_core::PublishTarget;
use tarsi_publish::{PublishError, PublishJob, PublishReport, Publisher};

/// crates.io publisher. `repo_root` anchors relative manifest dirs.
#[derive(Debug, Clone)]
pub struct CratesPublisher {
    pub repo_root: PathBuf,
    /// Max attempts per unit when retry_429 is on.
    pub max_attempts: u32,
    pub retry_429: bool,
}

use std::path::PathBuf;

impl CratesPublisher {
    #[must_use]
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            max_attempts: 5,
            retry_429: true,
        }
    }
}

fn is_rate_limited(stderr: &str) -> bool {
    stderr.contains("429") || stderr.contains("rate limit") || stderr.contains("too many requests")
}

fn is_already_published(stderr: &str) -> bool {
    stderr.contains("already exists")
        || stderr.contains("is already uploaded")
        || stderr.contains("previously uploaded")
}

impl Publisher for CratesPublisher {
    fn target(&self) -> PublishTarget {
        PublishTarget::CratesIo
    }

    fn publish(&self, job: &PublishJob) -> Result<PublishReport, PublishError> {
        let manifest = self.repo_root.join(&job.manifest_dir).join("Cargo.toml");
        if job.dry_run {
            return Ok(PublishReport {
                unit: job.unit.clone(),
                target: self.target(),
                ok: true,
                detail: format!("dry-run: would cargo publish {}", manifest.display()),
            });
        }
        let attempts = if self.retry_429 { self.max_attempts } else { 1 };
        let mut last_err = String::new();
        for attempt in 1..=attempts {
            let out = Command::new("cargo")
                .arg("publish")
                .arg("--manifest-path")
                .arg(&manifest)
                .output()?;
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            if out.status.success() {
                return Ok(PublishReport {
                    unit: job.unit.clone(),
                    target: self.target(),
                    ok: true,
                    detail: format!("published {} (attempt {attempt})", job.version),
                });
            }
            if is_already_published(&stderr) {
                return Ok(PublishReport {
                    unit: job.unit.clone(),
                    target: self.target(),
                    ok: true,
                    detail: "already published".to_string(),
                });
            }
            last_err = stderr.lines().take(3).collect::<Vec<_>>().join(" | ");
            if !(self.retry_429 && is_rate_limited(&last_err)) || attempt == attempts {
                break;
            }
            std::thread::sleep(Duration::from_secs(u64::from(attempt) * 15));
        }
        Err(PublishError::Msg(format!(
            "cargo publish {} failed: {last_err}",
            job.unit
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_detection() {
        assert!(is_rate_limited("error: 429 Too Many Requests"));
        assert!(is_rate_limited("you have hit your rate limit"));
        assert!(!is_rate_limited("error: no such package"));
        assert!(is_already_published(
            "error: crate version 1.0.0 is already uploaded"
        ));
    }

    #[test]
    fn dry_run_never_spawns() {
        let p = CratesPublisher::new(PathBuf::from("/nonexistent"));
        let job = PublishJob {
            unit: "u".into(),
            version: "1.0.0".into(),
            manifest_dir: "crates/u".into(),
            tag: "u-v1.0.0".into(),
            notes: String::new(),
            assets: Vec::new(),
            dry_run: true,
        };
        let r = p.publish(&job).unwrap();
        assert!(r.ok && r.detail.contains("dry-run"));
    }
}
