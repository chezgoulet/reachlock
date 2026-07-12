//! Priority ladder (spec §10, Priority System). Declaration order IS
//! discriminant order IS priority order — keep the three in sync if this
//! ever grows, or `derive(Ord)` and `value()` disagree.

use serde::{Deserialize, Serialize};

/// Which version of an asset renders when more than one source exists for
/// the same object. Higher always beats lower; see [`crate::content::resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Default: no authored content exists, the generator produces the asset.
    Procedural = 0,
    /// Prefer this version; falls back to procedural if no content applies.
    Curated = 50,
    /// Temporary authoritative. Respects `ContentFile::expires_at`.
    Event = 75,
    /// Always renders. Overrides everything else unconditionally.
    Authoritative = 100,
}

impl Priority {
    /// The spec §10 table value (0 / 50 / 75 / 100).
    pub fn value(self) -> u16 {
        self as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_match_spec_table() {
        assert_eq!(Priority::Procedural.value(), 0);
        assert_eq!(Priority::Curated.value(), 50);
        assert_eq!(Priority::Event.value(), 75);
        assert_eq!(Priority::Authoritative.value(), 100);
    }

    #[test]
    fn ordering_follows_the_table() {
        assert!(Priority::Authoritative > Priority::Event);
        assert!(Priority::Event > Priority::Curated);
        assert!(Priority::Curated > Priority::Procedural);
    }

    #[test]
    fn serde_names_are_snake_case() {
        assert_eq!(
            serde_json::to_string(&Priority::Authoritative).unwrap(),
            "\"authoritative\""
        );
    }
}
