//! Seed derivation: `hash(discoverer, system, object_type, biome,
//! timestamp_rounded)` (spec §4). FNV-1a over the canonical byte encoding,
//! finalized with SplitMix64 — stable across releases: changing this
//! function orphans every derived seed in every ledger, so DON'T.

use super::types::{Biome, ObjectType, PlayerId, Seed, SystemId};

const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

fn fnv1a(state: u64, bytes: &[u8]) -> u64 {
    let mut h = state;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    // Field separator so ("ab","c") never collides with ("a","bc").
    h ^= 0x1F;
    h.wrapping_mul(FNV_PRIME)
}

fn finalize(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Timestamps are rounded to this many seconds before hashing, so two
/// requests from the same discoverer in the same window derive the same
/// tentative seed (idempotent retries).
pub const TIMESTAMP_ROUND_SECS: u64 = 3600;

pub fn derive_seed(
    discoverer: &PlayerId,
    system: &SystemId,
    object_type: ObjectType,
    biome: Biome,
    unix_timestamp: u64,
) -> Seed {
    let rounded = unix_timestamp / TIMESTAMP_ROUND_SECS;
    let mut h = FNV_OFFSET;
    h = fnv1a(h, discoverer.0.as_bytes());
    h = fnv1a(h, system.0.as_bytes());
    h = fnv1a(h, object_type.as_str().as_bytes());
    h = fnv1a(h, biome.as_str().as_bytes());
    h = fnv1a(h, &rounded.to_le_bytes());
    Seed::new(finalize(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PlayerId, SystemId) {
        (
            PlayerId("player-boris".into()),
            SystemId("duskway-0417".into()),
        )
    }

    #[test]
    fn deterministic_within_window() {
        let (p, s) = fixture();
        let a = derive_seed(&p, &s, ObjectType::System, Biome::Frontier, 1_000_000);
        let b = derive_seed(&p, &s, ObjectType::System, Biome::Frontier, 1_000_100);
        assert_eq!(a, b, "same rounding window must derive the same seed");
    }

    #[test]
    fn fields_matter() {
        let (p, s) = fixture();
        let base = derive_seed(&p, &s, ObjectType::System, Biome::Frontier, 1_000_000);
        assert_ne!(
            base,
            derive_seed(&p, &s, ObjectType::Station, Biome::Frontier, 1_000_000)
        );
        assert_ne!(
            base,
            derive_seed(&p, &s, ObjectType::System, Biome::Nebula, 1_000_000)
        );
    }

    #[test]
    fn within_53_bits() {
        let (p, s) = fixture();
        for t in 0..50u64 {
            let seed = derive_seed(&p, &s, ObjectType::Ship, Biome::Core, t * 7919);
            assert!(seed.value() <= Seed::MAX);
        }
    }

    /// Golden vector — this value is part of the protocol. If this test
    /// breaks, seed derivation changed and every ledger entry is orphaned.
    #[test]
    fn golden_derivation() {
        let (p, s) = fixture();
        let seed = derive_seed(&p, &s, ObjectType::System, Biome::Frontier, 1_750_000_000);
        assert_eq!(seed.value(), golden::SYSTEM_FRONTIER);
    }

    mod golden {
        pub const SYSTEM_FRONTIER: u64 = 7928794229254937;
    }
}
