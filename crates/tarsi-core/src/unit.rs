//! Release-unit spec — one independently-versioned publishable
//! thing in a repo (a cargo workspace member, a single package, or
//! a configured path prefix).

use std::collections::BTreeMap;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::bump::Bump;

/// Where a unit's artifacts go on release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublishTarget {
    /// `cargo publish` to crates.io.
    CratesIo,
    /// GitHub Release with rendered notes.
    GithubRelease,
}

/// Static description of one release unit — what config/manifests
/// provide before `plan_release` computes the dynamic parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseUnitSpec {
    /// Unit name — also the tag stem (`{name}-v{version}`).
    pub name: String,
    /// Repo-relative path prefix for commit attribution
    /// (`"crates/tamias-core"`); `""` or `"."` = whole repo.
    #[serde(default)]
    pub path: String,
    /// Version at plan time (from the unit's manifest).
    pub current_version: Version,
    /// Tag template; `{unit}` and `{version}` placeholders.
    /// Default `{unit}-v{version}` — the org convention
    /// (`tamias-core-v0.4.2`).
    #[serde(default = "default_tag_format")]
    pub tag_format: String,
    /// Explicit bump override; `None` = infer from commits.
    #[serde(default)]
    pub bump: Option<Bump>,
    /// Publish destinations, in preference order.
    #[serde(default)]
    pub publish: Vec<PublishTarget>,
    /// Names of units that must publish before this one (dep
    /// topology). Populated from `cargo metadata` or declared in
    /// config; the planner topo-sorts on it.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

fn default_root() -> String {
    ".".to_string()
}

/// The built-in tag template — `{unit}-v{semver}`, the org
/// convention (`tamias-core-v0.4.2`). Specs carrying this exact
/// value are treated as *unset* by [`crate::config::apply_overrides`]
/// so `release.tag_format`/profile conventions can take over.
pub const DEFAULT_TAG_FORMAT: &str = "{unit}-v{version}";

fn default_tag_format() -> String {
    DEFAULT_TAG_FORMAT.to_string()
}

impl ReleaseUnitSpec {
    /// Render the tag for a resolved version.
    #[must_use]
    pub fn tag(&self, version: &Version) -> String {
        self.tag_format
            .replace("{unit}", &self.name)
            .replace("{version}", &version.to_string())
    }
}

/// How a repo declares its units — resolved to concrete specs by
/// the caller (manifest scan or config).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UnitsSpec {
    /// Every member of the cargo workspace at `root` (default
    /// `Cargo.toml`), versions read from member manifests.
    CargoWorkspace {
        #[serde(default = "default_root")]
        root: String,
    },
    /// Explicitly listed units.
    Explicit { units: Vec<ReleaseUnitSpec> },
}

/// Convenience: unit name → spec lookup used by the planner.
pub type UnitMap<'a> = BTreeMap<&'a str, &'a ReleaseUnitSpec>;
