//! Ship hull generation: seed → closed polygon mesh.

use super::{FixedVec2, GeneratedMesh};
use crate::util::rng::{Fixed, SeededRng};
use crate::util::trig::{icos, isin};

/// Ship size class. Scales the hull radius band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HullClass {
    Shuttle,
    Freighter,
    Corvette,
    Station,
    /// A small, irregular chunk — the system generator's asteroid fields
    /// reuse this hull class for rocks instead of a bespoke generator.
    Rock,
}

impl HullClass {
    /// (min, max) hull radius in whole world units.
    fn radius_band(self) -> (i64, i64) {
        match self {
            HullClass::Shuttle => (24, 40),
            HullClass::Freighter => (48, 80),
            HullClass::Corvette => (40, 64),
            HullClass::Station => (96, 160),
            HullClass::Rock => (4, 12),
        }
    }

    fn side_band(self) -> (u64, u64) {
        match self {
            HullClass::Shuttle => (5, 8),
            HullClass::Freighter => (7, 11),
            HullClass::Corvette => (6, 9),
            HullClass::Station => (10, 16),
            HullClass::Rock => (5, 7),
        }
    }
}

/// Generate a hull with the default class (used by the spike demo and
/// anywhere that doesn't care about class yet).
pub fn generate_hull(seed: u64) -> GeneratedMesh {
    generate_hull_class(seed, HullClass::Corvette)
}

/// Generate a closed hull outline: a fan-triangulated irregular polygon
/// whose vertex radii derive entirely from the seed. Mirror-symmetric about
/// the x axis so ships read as ships, not asteroids.
pub fn generate_hull_class(seed: u64, class: HullClass) -> GeneratedMesh {
    let mut rng = SeededRng::new(seed);
    let (side_lo, side_hi) = class.side_band();
    let sides = (side_lo + rng.next_below(side_hi - side_lo + 1)) as usize;
    let (r_lo, r_hi) = class.radius_band();

    // Radii for the upper half; the lower half mirrors them.
    let half = sides / 2 + 1;
    let mut radii = Vec::with_capacity(half);
    for _ in 0..half {
        radii.push(Fixed::from_int(
            r_lo + rng.next_below((r_hi - r_lo) as u64) as i64,
        ));
    }

    let mut vertices = Vec::with_capacity(sides + 1);
    vertices.push(FixedVec2 {
        x: Fixed(0),
        y: Fixed(0),
    });
    for i in 0..sides {
        let turn = (i as u64 * 65536 / sides as u64) as u16;
        // Mirror: index i and (sides - i) share a radius.
        let ri = i.min(sides - i);
        let radius = radii[ri.min(half - 1)];
        vertices.push(FixedVec2 {
            x: Fixed(radius.0 * icos(turn) as i64 / 32768),
            y: Fixed(radius.0 * isin(turn) as i64 / 32768),
        });
    }

    let mut indices = Vec::with_capacity(sides * 3);
    for i in 0..sides {
        let a = 1 + i as u32;
        let b = 1 + ((i + 1) % sides) as u32;
        indices.extend_from_slice(&[0, a, b]);
    }

    GeneratedMesh { vertices, indices }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_hull() {
        assert_eq!(generate_hull(0xDEAD_BEEF), generate_hull(0xDEAD_BEEF));
    }

    #[test]
    fn different_seeds_differ() {
        assert_ne!(generate_hull(1), generate_hull(2));
    }

    #[test]
    fn classes_scale() {
        let shuttle = generate_hull_class(7, HullClass::Shuttle);
        let station = generate_hull_class(7, HullClass::Station);
        let max_r = |m: &GeneratedMesh| {
            m.vertices
                .iter()
                .map(|v| v.x.0.abs().max(v.y.0.abs()))
                .max()
                .unwrap()
        };
        assert!(max_r(&station) > max_r(&shuttle));
    }

    #[test]
    fn rock_is_smaller_than_any_ship_class() {
        let rock = generate_hull_class(7, HullClass::Rock);
        let shuttle = generate_hull_class(7, HullClass::Shuttle);
        let max_r = |m: &GeneratedMesh| {
            m.vertices
                .iter()
                .map(|v| v.x.0.abs().max(v.y.0.abs()))
                .max()
                .unwrap()
        };
        assert!(max_r(&rock) < max_r(&shuttle));
    }

    #[test]
    fn mirror_symmetry() {
        let mesh = generate_hull(42);
        let n = mesh.vertices.len() - 1; // minus center
        for i in 1..n {
            let a = mesh.vertices[1 + i];
            let b = mesh.vertices[1 + (n - i)];
            assert_eq!(a.x.0, b.x.0, "x mirrors at vertex {i}");
            assert!((a.y.0 + b.y.0).abs() <= 2, "y negates at vertex {i}");
        }
    }

    /// Golden vector: if this changes, generator output changed on SOME
    /// target — the determinism harness compares it across x86/ARM/wasm.
    #[test]
    fn golden_hull_seed_42() {
        let mesh = generate_hull(42);
        let checksum: i64 = mesh.vertices.iter().fold(0i64, |acc, v| {
            acc.wrapping_mul(31).wrapping_add(v.x.0 ^ v.y.0)
        });
        assert_eq!(
            (mesh.vertices.len(), mesh.indices.len(), checksum),
            golden::SEED_42
        );
    }

    mod golden {
        /// (vertex count, index count, vertex checksum) for seed 42.
        /// Captured on x86_64; must match on every target.
        pub const SEED_42: (usize, usize, i64) = (9, 24, 1212647953801875);
    }
}
