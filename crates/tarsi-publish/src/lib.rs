//! tarsi-publish — the publish port. One [`Publisher`] per forge
//! target; `tarsi-serve`/drivers iterate [`tarsi_core::ReleasePlan`]
//! units in order and dispatch by [`PublishTarget`]. All impls are
//! sync; secrets arrive as env-var *names* or `cuora://` refs
//! resolved through [`token::TokenResolver`] — values never in
//! config or logs (cuora boundary).

pub mod token;

pub use token::{CuoraTokenResolver, TokenResolver};

use std::path::PathBuf;

use tarsi_core::PublishTarget;
use thiserror::Error;

/// A file to attach to the release — already resolved by the
/// caller (glob expansion + stat happen before the job is built;
/// publishers stay pure consumers of concrete paths).
#[derive(Debug, Clone)]
pub struct PublishAsset {
    /// Path to read at publish time (absolute, or relative to the
    /// process cwd — caller's choice).
    pub path: PathBuf,
    /// Upload file name — the `?name=` query on the asset endpoint
    /// and the identity used for idempotent replace.
    pub name: String,
    /// Declared content type; `None` → publisher guesses from the
    /// file extension.
    pub content_type: Option<String>,
    /// Byte size — for dry-run reporting; publishers may re-stat.
    pub size: u64,
}

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
    /// Files to attach (release assets). Empty = notes-only
    /// release. Targets that don't support assets ignore this.
    pub assets: Vec<PublishAsset>,
    /// Commit the release tag points at (already pushed). Forges
    /// that create tags on release send it as `target_commitish`
    /// so they never guess a commit; `None` on dry-runs.
    pub commitish: Option<String>,
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
