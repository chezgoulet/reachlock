//! Ship hull generation: seed → closed polygon mesh.

use serde::{Deserialize, Serialize};

use super::{FixedVec2, GeneratedMesh};
use crate::util::rng::{Fixed, SeededRng};
use crate::util::trig::{icos, isin};

/// Ship size class. Scales the hull radius band. Serde (snake_case) because
/// S17's authored hull frames name their class in `.ron` content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

/// Flight-handling parameters (spec §14 Mode 3; S09 freeze, iron-rule #2:
/// fixed-point, no floats in the gameplay struct). Consumed by the client as
/// `f32` at the bridge via [`HullHandling::f32`]. S17 (editor) and S19
/// (combat) both read this struct — it is load-bearing.
///
/// Every field is fixed-point at 1/1024. `mass`/`thrust`/`turn_rate`/
/// `drift_damping`/`boost_mult` are 1/1024; `fuel_burn` is integer units
/// per second at cruise. `boost_mult == 1024` means 1.0x thrust.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HullHandling {
    pub mass: i64,
    pub thrust: i64,
    pub turn_rate: i64,
    pub drift_damping: i64,
    pub boost_mult: i64,
    pub fuel_burn: i64,
}

impl HullHandling {
    /// Derive per `HullClass` with a small seed jitter so two ships of the
    /// same class still handle a hair differently. Deterministic in
    /// `(seed, class)` — no wall-clock, no manifest entry (it's a derived
    /// table, not a generator output).
    pub fn for_class(seed: u64, class: HullClass) -> Self {
        let mut rng = SeededRng::new(seed ^ 0x4A5D_1234);
        let mut j = |base: i64, spread: i64| {
            base + (rng.next_below((spread as u64) * 2 + 1) as i64) - spread
        };
        let base = match class {
            // (mass, thrust, turn_rate, drift_damping, boost_mult, fuel_burn)
            HullClass::Shuttle => (700, 1500, 260, 760, 1500, 9),
            HullClass::Freighter => (2600, 1300, 90, 540, 1280, 16),
            HullClass::Corvette => (1400, 1700, 200, 700, 1408, 12),
            HullClass::Station => (9_999, 0, 0, 0, 1024, 0),
            HullClass::Rock => (500, 0, 0, 0, 1024, 0),
        };
        Self {
            mass: j(base.0, 60),
            thrust: j(base.1, 80),
            turn_rate: j(base.2, 20),
            drift_damping: j(base.3, 40),
            boost_mult: j(base.4, 64),
            fuel_burn: j(base.5, 3).max(1),
        }
    }

    /// Fixed-point (1/1024) → f32 for the render/bridge layer only.
    pub fn f32(v: i64) -> f32 {
        v as f32 / 1024.0
    }
}

#[cfg(test)]
mod handling_tests {
    use super::*;

    #[test]
    fn handling_is_class_distinct_and_seed_stable() {
        let a = HullHandling::for_class(1, HullClass::Corvette);
        let b = HullHandling::for_class(1, HullClass::Freighter);
        assert_eq!(a, HullHandling::for_class(1, HullClass::Corvette));
        assert_ne!(a.thrust, b.thrust);
        assert!(b.mass > a.mass, "freighter is heavier");
    }

    #[test]
    fn f32_conversion() {
        assert_eq!(HullHandling::f32(1024), 1.0);
        assert_eq!(HullHandling::f32(2048), 2.0);
    }
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
