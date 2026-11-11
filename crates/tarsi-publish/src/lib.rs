//! tarsi-publish — the publish port. One [`Publisher`] per forge
//! target; `tarsi-serve`/drivers iterate [`tarsi_core::ReleasePlan`]
//! units in order and dispatch by [`PublishTarget`]. All impls are
//! sync; secrets arrive as env-var *names* resolved by the caller
//! (cuora boundary — values never in config or logs).

use std::path::PathBuf;

use tarsi_core::PublishTarget;
use thiserror::Error;

/// Everything a publisher needs for one unit — already resolved
/// by the caller (plan + config + secrets).
#[derive(Debug, Clone)]
pub struct PublishJob {
    /// Unit name (`tamias-core`).
    pub unit: String,
    /// New version being released.
    pub version: String,
    /// Repo-relative manifest dir (`crates/tamias-core`).
    pub manifest_dir: PathBuf,
    /// Tag created for this unit (`tamias-core-v0.4.2`).
    pub tag: String,
    /// Rendered release notes (changelog block / ghrelease body).
    pub notes: String,
    /// When true, impls validate + report but perform no remote
    /// writes. Dry-run is the safe default on integration surfaces.
    pub dry_run: bool,
}

/// Outcome of one publish attempt — surfaced in serve responses.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PublishReport {
    pub unit: String,
    pub target: PublishTarget,
    pub ok: bool,
    /// Human-readable detail (URL, "already exists", retry count…).
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("{0}")]
    Msg(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(String),
}

/// A forge/registry publish target.
pub trait Publisher: Send + Sync {
    /// Which [`PublishTarget`] this impl serves.
    fn target(&self) -> PublishTarget;
    /// Publish one unit. Implementations MUST honor `job.dry_run`.
    fn publish(&self, job: &PublishJob) -> Result<PublishReport, PublishError>;
}

/// Pick publishers for a plan's targets, preserving unit order —
/// callers run `publish` per unit in `plan_release` order.
pub fn publishers_for(
    targets: &[PublishTarget],
    builders: &dyn Fn(PublishTarget) -> Option<Box<dyn Publisher>>,
) -> Vec<(PublishTarget, Box<dyn Publisher>)> {
    targets
        .iter()
        .filter_map(|t| builders(*t).map(|p| (*t, p)))
        .collect()
}
