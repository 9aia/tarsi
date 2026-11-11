//! tarsi-publish-dryrun — a [`Publisher`] that reports what *would*
//! happen for any target. The default on integration surfaces:
//! plans and serve ops run dry until explicitly told otherwise.

use tarsi_core::PublishTarget;
use tarsi_publish::{PublishJob, PublishReport, Publisher};

/// Reports every job as a successful dry-run.
#[derive(Debug, Default, Clone, Copy)]
pub struct DryRunPublisher;

impl Publisher for DryRunPublisher {
    fn target(&self) -> PublishTarget {
        // Dry-run is target-agnostic — callers pick per-target
        // wrappers or use `publish_all` helpers; this impl answers
        // for whichever target the job implies via `job.tag`.
        // We return CratesIo as a placeholder; serve uses
        // `DryRunPublisher::for_target` instead.
        PublishTarget::CratesIo
    }

    fn publish(&self, job: &PublishJob) -> Result<PublishReport, tarsi_publish::PublishError> {
        let assets = asset_suffix(job);
        Ok(PublishReport {
            unit: job.unit.clone(),
            target: self.target(),
            ok: true,
            detail: format!(
                "dry-run: would publish {} {} to {}{}",
                job.unit,
                job.version,
                job.manifest_dir.display(),
                assets
            ),
        })
    }
}

impl DryRunPublisher {
    /// A dry-run publisher pinned to a specific target.
    #[must_use]
    pub fn for_target(target: PublishTarget) -> TargetDryRun {
        TargetDryRun(target)
    }
}

/// Per-target dry-run wrapper.
#[derive(Debug, Clone, Copy)]
pub struct TargetDryRun(pub PublishTarget);

impl Publisher for TargetDryRun {
    fn target(&self) -> PublishTarget {
        self.0
    }

    fn publish(&self, job: &PublishJob) -> Result<PublishReport, tarsi_publish::PublishError> {
        let assets = asset_suffix(job);
        Ok(PublishReport {
            unit: job.unit.clone(),
            target: self.0,
            ok: true,
            detail: format!(
                "dry-run: would publish {} {} ({:?}){}",
                job.unit, job.version, self.0, assets
            ),
        })
    }
}

/// `, N asset(s): a, b` suffix for dry-run details — empty when
/// the job carries no assets.
fn asset_suffix(job: &PublishJob) -> String {
    if job.assets.is_empty() {
        return String::new();
    }
    let names = job
        .assets
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(" +{} asset(s): {names}", job.assets.len())
}
