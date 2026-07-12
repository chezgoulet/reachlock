//! Canonical seed derivation for authored content (spec §10, Seed
//! Integration): `hash("content_override", system_id, object_id)`.
//!
//! Deliberately distinct from `seed::resolver::derive_seed`: that function
//! hashes a discoverer + a rounded timestamp window for procedural
//! first-write-wins discovery. Authored content has neither — its identity
//! is fixed by the author, not the first player to arrive — so it gets its
//! own (smaller) derivation using the same hash primitives.

use crate::seed::resolver::{finalize, fnv1a, FNV_OFFSET};
use crate::seed::types::Seed;

/// `hash("content_override", system_id, object_id)`, masked to 53 bits so
/// it survives JSON float round-trips (spec §4). Stable across releases —
/// changing this orphans the seed on every authored `.ron` file that pinned
/// its `seed` field against it.
pub fn content_seed(system_id: &str, object_id: &str) -> u64 {
    let mut h = FNV_OFFSET;
    h = fnv1a(h, b"content_override");
    h = fnv1a(h, system_id.as_bytes());
    h = fnv1a(h, object_id.as_bytes());
    finalize(h) & Seed::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(
            content_seed("frontier-01", "sorrow_station"),
            content_seed("frontier-01", "sorrow_station")
        );
    }

    #[test]
    fn inputs_matter() {
        let base = content_seed("frontier-01", "sorrow_station");
        assert_ne!(base, content_seed("frontier-02", "sorrow_station"));
        assert_ne!(base, content_seed("frontier-01", "loup_garou"));
    }

    #[test]
    fn within_53_bits() {
        for i in 0..50u64 {
            let seed = content_seed("sys", &format!("obj-{i}"));
            assert!(seed <= Seed::MAX);
        }
    }

    /// Golden vectors — these values are part of the authoring protocol:
    /// `content/hulls/loup_garou.ron` and `content/stations/sorrow_station.ron`
    /// pin their `seed` field against exactly these numbers. If this test
    /// breaks, either the hash changed (update the .ron files deliberately
    /// and say so in the commit message) or a .ron file drifted.
    #[test]
    fn golden_derivation() {
        assert_eq!(
            content_seed("frontier-01", "loup_garou"),
            golden::LOUP_GAROU
        );
        assert_eq!(
            content_seed("frontier-01", "sorrow_station"),
            golden::SORROW_STATION
        );
    }

    mod golden {
        // Captured from `content_seed` when the derivation was frozen. The
        // authored `.ron` files pin their `seed` field against these.
        pub const LOUP_GAROU: u64 = 5_335_717_113_362_242;
        pub const SORROW_STATION: u64 = 4_218_130_448_322_139;
    }
}
