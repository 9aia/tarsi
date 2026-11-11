//! tarsi-publish-ghreleases — GitHub Releases publisher. Creates a
//! release per unit tag with the rendered notes as the body.
//! Blocking `ureq` per the bupha-github convention; the token is
//! read from the env var *named* in config (`token_env`) — the
//! value never appears in config, logs, or args.

use tarsi_core::PublishTarget;
use tarsi_publish::{PublishError, PublishJob, PublishReport, Publisher};

/// GitHub Releases publisher for `owner/repo`.
#[derive(Debug, Clone)]
pub struct GhReleasePublisher {
    /// `owner/repo` — resolved from git remote or config.
    pub repo: String,
    /// Env var NAME holding the token (e.g. `GITHUB_TOKEN`).
    pub token_env: String,
    /// API base — override for GHES/tests.
    pub api_base: String,
}

impl GhReleasePublisher {
    #[must_use]
    pub fn new(repo: String, token_env: String) -> Self {
        Self {
            repo,
            token_env,
            api_base: "https://api.github.com".to_string(),
        }
    }

    fn token(&self) -> Result<String, PublishError> {
        std::env::var(&self.token_env).map_err(|_| {
            PublishError::Msg(format!(
                "{} is not set — resolve it via cuora/environment",
                self.token_env
            ))
        })
    }
}

impl Publisher for GhReleasePublisher {
    fn target(&self) -> PublishTarget {
        PublishTarget::GithubRelease
    }

    fn publish(&self, job: &PublishJob) -> Result<PublishReport, PublishError> {
        if job.dry_run {
            return Ok(PublishReport {
                unit: job.unit.clone(),
                target: self.target(),
                ok: true,
                detail: format!(
                    "dry-run: would create GH release {} on {}",
                    job.tag, self.repo
                ),
            });
        }
        let token = self.token()?;
        let url = format!("{}/repos/{}/releases", self.api_base, self.repo);
        let body = serde_json::json!({
            "tag_name": job.tag,
            "name": format!("{} {}", job.unit, job.version),
            "body": job.notes,
            "draft": false,
            "prerelease": false,
        });
        let resp = ureq::post(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "tarsi")
            .send_json(&body)
            .map_err(|e| PublishError::Http(e.to_string()))?;
        let json: serde_json::Value = resp
            .into_body()
            .read_json()
            .map_err(|e| PublishError::Http(e.to_string()))?;
        let html = json
            .get("html_url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();
        Ok(PublishReport {
            unit: job.unit.clone(),
            target: self.target(),
            ok: true,
            detail: html,
        })
    }
}
