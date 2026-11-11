//! Semver bump policy — `major`/`minor`/`patch`/`none` plus the
//! conventional-commit inference that picks one.

use semver::Version;
use serde::{Deserialize, Serialize};

/// A version-bump decision. Ordered by severity so `max` picks the
/// strongest signal across a commit range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bump {
    /// No release — docs/chore/test-only changes, or an explicit
    /// `none` override.
    None,
    Patch,
    Minor,
    Major,
}

impl Bump {
    /// The strongest bump in `self`/`other`.
    #[must_use]
    pub fn max(self, other: Self) -> Self {
        Ord::max(self, other)
    }

    /// Apply to `version` — `None` returns it unchanged.
    #[must_use]
    pub fn apply(self, version: &Version) -> Version {
        let mut v = version.clone();
        match self {
            Bump::None => {}
            Bump::Patch => v.patch += 1,
            Bump::Minor => {
                v.minor += 1;
                v.patch = 0;
            }
            Bump::Major => {
                v.major += 1;
                v.minor = 0;
                v.patch = 0;
            }
        }
        v.pre = semver::Prerelease::EMPTY;
        v.build = semver::BuildMetadata::EMPTY;
        v
    }

    /// Whether this bump produces a release at all.
    #[must_use]
    pub fn releases(self) -> bool {
        !matches!(self, Bump::None)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn apply_resets_lower_components() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(Bump::Patch.apply(&v).to_string(), "1.2.4");
        assert_eq!(Bump::Minor.apply(&v).to_string(), "1.3.0");
        assert_eq!(Bump::Major.apply(&v).to_string(), "2.0.0");
        assert_eq!(Bump::None.apply(&v).to_string(), "1.2.3");
    }

    #[test]
    fn apply_strips_pre_and_build() {
        let v = Version::parse("1.2.3-rc.1+build.5").unwrap();
        assert_eq!(Bump::Patch.apply(&v).to_string(), "1.2.4");
    }

    #[test]
    fn ordering_is_by_severity() {
        assert!(Bump::Major > Bump::Minor);
        assert!(Bump::Minor > Bump::Patch);
        assert!(Bump::Patch > Bump::None);
        assert_eq!(Bump::Patch.max(Bump::Major), Bump::Major);
    }
}
