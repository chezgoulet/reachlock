use serde::{Deserialize, Serialize};

use crate::seed::types::Seed;
use crate::universe::tier::UniverseTier;

/// 3D galactic coordinate. Attached to every system (charted and uncharted)
/// for distance calculations and `deep_space_seed` derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GalaxyCoord {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl GalaxyCoord {
    /// Squared Euclidean distance between two coordinates (i64 avoids overflow
    /// for plausible galaxy sizes; if the galaxy ever exceeds ~300k units on
    /// any axis the computation should switch to i128).
    pub fn dist_sq(&self, other: &GalaxyCoord) -> i64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx.saturating_mul(dx)
            .saturating_add(dy.saturating_mul(dy))
            .saturating_add(dz.saturating_mul(dz))
    }

    /// Stable hash string for uncharted system ids: `uncharted_{hex}` where
    /// hex is the FNV-1a hash of the packed coordinates + the universe tier.
    pub fn coord_hash(&self, universe: UniverseTier) -> String {
        let mut h = 0xCBF2_9CE4_8422_2325u64;
        for v in [self.x, self.y, self.z, universe as u8 as i64] {
            for b in v.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
        format!("{:016x}", h)
    }
}

/// FROZEN PROTOCOL. Generates a deterministic seed for any uncharted galactic
/// coordinate. Must never change — golden-tested across x86/aarch64/wasm32.
/// Joins `derive_seed` from `seed/resolver.rs` as the second frozen derivation.
///
/// Algorithm: FNV-1a over the 24-byte packed representation `(x, y, z, universe_tag)`
/// then mask to 53 bits. This produces a uniformly distributed seed within the
/// allowed range (≤2^53) without any single host having prior knowledge of what
/// a coordinate resolves to.
pub fn deep_space_seed(coord: GalaxyCoord, universe: UniverseTier) -> Seed {
    let mut h = 0xCBF2_9CE4_8422_2325u64;
    for v in [coord.x, coord.y, coord.z, universe as u8 as i64] {
        for b in v.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    Seed::new(h)
}

/// Distance (in galaxy units) beyond which the fidelity gradient switches to
/// Sparse generation and threat scaling applies.
pub const SPARSE_THRESHOLD: i64 = 500;
/// Distance beyond which the biome becomes DeepSpace (no charted-systems
/// content, pure procedural voids).
pub const DEEP_SPACE_THRESHOLD: i64 = 1500;
/// Threat boost divisor: each `THREAT_SCALE` units of distance adds 1 to the
/// system's threat level (beyond the normal biome-based roll).
pub const THREAT_SCALE: i64 = 200;
/// At what distance DeepSpace systems are so remote the threat is maxed.
pub const MAX_THREAT_DISTANCE: i64 = 5000;
/// Faction territory influence radius around each charted system (galaxy units).
pub const TERRITORY_INFLUENCE: i64 = 800;

/// Find the minimum squared distance from `coord` to any charted system in
/// the gate network. Returns `None` if the network is empty or has no positions.
/// This drives the fidelity gradient and threat scaling for deep-space systems.
pub fn nearest_charted_distance_sq(
    coord: &GalaxyCoord,
    system_positions: &[GalaxyCoord],
) -> Option<i64> {
    system_positions.iter().map(|cp| coord.dist_sq(cp)).min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universe::tier::UniverseTier;

    #[test]
    fn deep_space_seed_is_deterministic() {
        let coord = GalaxyCoord {
            x: 42,
            y: -100,
            z: 3000,
        };
        let a = deep_space_seed(coord, UniverseTier::Classic);
        let b = deep_space_seed(coord, UniverseTier::Classic);
        assert_eq!(a, b);
    }

    #[test]
    fn deep_space_seed_differs_by_coord() {
        let a = deep_space_seed(GalaxyCoord { x: 0, y: 0, z: 0 }, UniverseTier::Classic);
        let b = deep_space_seed(GalaxyCoord { x: 1, y: 0, z: 0 }, UniverseTier::Classic);
        assert_ne!(a, b);
    }

    #[test]
    fn deep_space_seed_differs_by_universe() {
        let coord = GalaxyCoord {
            x: 42,
            y: 100,
            z: -200,
        };
        let a = deep_space_seed(coord, UniverseTier::Classic);
        let b = deep_space_seed(coord, UniverseTier::Spectrum);
        assert_ne!(a, b);
    }

    #[test]
    fn seed_within_53_bit_range() {
        for x in [0i64, -1, 1000, -5000, 1_000_000_000] {
            for y in [0i64, 1, -2000, 7000] {
                for z in [0i64, -1, 3000, -8000] {
                    let s = deep_space_seed(GalaxyCoord { x, y, z }, UniverseTier::Classic);
                    assert!(
                        s.value() < (1 << 53),
                        "seed {} exceeds 53 bits for ({x}, {y}, {z})",
                        s.value()
                    );
                }
            }
        }
    }

    #[test]
    fn coord_hash_is_stable() {
        let c = GalaxyCoord {
            x: 100,
            y: 200,
            z: 300,
        };
        assert_eq!(
            c.coord_hash(UniverseTier::Classic),
            c.coord_hash(UniverseTier::Classic)
        );
    }

    #[test]
    fn nearest_distance_returns_closest() {
        let positions = [
            GalaxyCoord { x: 0, y: 0, z: 0 },
            GalaxyCoord { x: 100, y: 0, z: 0 },
            GalaxyCoord { x: 0, y: 0, z: 500 },
        ];
        let query = GalaxyCoord { x: 10, y: 0, z: 0 };
        let dist = nearest_charted_distance_sq(&query, &positions);
        assert_eq!(dist, Some(100)); // squared distance to (0,0,0) or (100,0,0)?
                                     // 10^2 = 100 vs 90^2 = 8100, so closest is (0,0,0) at 100.
        assert_eq!(dist, Some(100));
    }

    #[test]
    fn nearest_distance_empty() {
        assert_eq!(
            nearest_charted_distance_sq(&GalaxyCoord { x: 0, y: 0, z: 0 }, &[]),
            None
        );
    }

    #[test]
    fn coord_hash_differs_by_universe() {
        let c = GalaxyCoord { x: 1, y: 2, z: 3 };
        assert_ne!(
            c.coord_hash(UniverseTier::Classic),
            c.coord_hash(UniverseTier::Spectrum)
        );
    }
}
