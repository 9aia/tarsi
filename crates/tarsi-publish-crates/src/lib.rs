//! tarsi-publish-crates — crates.io publisher. Runs
//! `cargo publish --manifest-path <unit>/Cargo.toml` per unit;
//! the *caller* supplies units already in dependency order
//! (plan_release topo-sorts). Retries on crates.io 429/rate-limit
//! output with linear backoff; "already exists" counts as success
//! (idempotent re-runs — release.sh semantics).

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use tarsi_core::PublishTarget;
use tarsi_publish::{
    CuoraTokenResolver, PublishError, PublishJob, PublishReport, Publisher, TokenResolver,
};

/// The env var cargo reads for the default-registry token — a
/// resolved token is always injected under this name regardless
/// of which `token_env` fallback supplied it.
pub const CARGO_TOKEN_ENV: &str = "CARGO_REGISTRY_TOKEN";

/// crates.io publisher. `repo_root` anchors relative manifest dirs.
#[derive(Clone)]
pub struct CratesPublisher {
    pub repo_root: PathBuf,
    /// Max attempts per unit when retry_429 is on.
    pub max_attempts: u32,
    pub retry_429: bool,
    /// Env var NAME consulted for the token when `token_ref` is
    /// unset or misses (default `CARGO_REGISTRY_TOKEN`).
    pub token_env: String,
    /// Optional `cuora://<scope>/<name>` secret ref — takes
    /// precedence over `token_env`.
    pub token_ref: Option<String>,
    /// Token resolver — defaults to [`CuoraTokenResolver`]; tests
    /// inject a fake.
    pub resolver: Arc<dyn TokenResolver>,
}

impl CratesPublisher {
    #[must_use]
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            max_attempts: 5,
            retry_429: true,
            token_env: CARGO_TOKEN_ENV.to_string(),
            token_ref: None,
            resolver: Arc::new(CuoraTokenResolver::new()),
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
        // Resolve the token once per unit — after the dry-run
        // early-return so dry-runs never touch cuora/env secrets.
        let token = self
            .resolver
            .resolve(&self.token_env, self.token_ref.as_deref())?;
        let attempts = if self.retry_429 { self.max_attempts } else { 1 };
        let mut last_err = String::new();
        for attempt in 1..=attempts {
            let mut cmd = Command::new("cargo");
            cmd.arg("publish").arg("--manifest-path").arg(&manifest);
            // Inject under the name cargo actually reads — the
            // configured `token_env` only names the fallback.
            if let Some(t) = &token {
                cmd.env(CARGO_TOKEN_ENV, t);
            }
            let out = cmd.output()?;
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
            commitish: None,
            dry_run: true,
        };
        let r = p.publish(&job).unwrap();
        assert!(r.ok && r.detail.contains("dry-run"));
    }

    fn job(dry_run: bool) -> PublishJob {
        PublishJob {
            unit: "u".into(),
            version: "1.0.0".into(),
            manifest_dir: "crates/u".into(),
            tag: "u-v1.0.0".into(),
            notes: String::new(),
            assets: Vec::new(),
            commitish: None,
            dry_run,
        }
    }

    /// Resolver seam — fixed token or error, env ignored.
    struct FakeResolver(Result<Option<String>, String>);

    impl TokenResolver for FakeResolver {
        fn resolve(
            &self,
            _token_env: &str,
            _token_ref: Option<&str>,
        ) -> Result<Option<String>, PublishError> {
            match &self.0 {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(PublishError::Msg(e.clone())),
            }
        }
    }

    #[test]
    fn resolved_token_is_injected_as_cargo_token_env() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        // Fake `cargo`: records CARGO_REGISTRY_TOKEN, exits 0.
        let capture = dir.path().join("token.txt");
        let fake = dir.path().join("cargo");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s' \"${{CARGO_REGISTRY_TOKEN:-}}\" > '{}'\nexit 0\n",
                capture.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Prepend the stub dir to PATH for this test process.
        let old_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{old_path}", dir.path().display()));

        let mut p = CratesPublisher::new(dir.path().to_path_buf());
        p.retry_429 = false;
        p.resolver = Arc::new(FakeResolver(Ok(Some("sekret-token".into()))));
        p.token_ref = Some("cuora://tamias/crates-token".into());
        // manifest_dir is repo-root-relative; point repo_root at
        // the tempdir so the (nonexistent) manifest path is fine —
        // the fake cargo never reads it.
        let r = p.publish(&job(false));
        std::env::set_var("PATH", old_path);
        let r = r.unwrap();
        assert!(r.ok, "{}", r.detail);
        assert_eq!(std::fs::read_to_string(&capture).unwrap(), "sekret-token");
    }

    #[test]
    fn resolver_error_fails_before_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = CratesPublisher::new(dir.path().to_path_buf());
        p.retry_429 = false;
        p.resolver = Arc::new(FakeResolver(Err("bad token_ref".into())));
        let err = p.publish(&job(false)).unwrap_err();
        assert!(err.to_string().contains("bad token_ref"));
    }
}
