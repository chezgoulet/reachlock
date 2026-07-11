//! System generation (spec §5, §14 Mode 3, §17 deep-space fidelity):
//! seed → a whole star system as plain data — star, orbits, asteroid
//! fields, station slots, exactly one gate, a starfield. Composes the
//! hull/station/planet generators by reference (seed + params); it does
//! not embed their meshes, keeping `GeneratedSystem` a small, serializable
//! contract that S01's authored content and S21's frontier both consume.

use serde::{Deserialize, Serialize};

use super::station::StationKind;
use super::FixedVec2;
use crate::seed::types::Biome;
use crate::util::color::{generate_palette, ColorRgba8};
use crate::util::rng::{Fixed, SeededRng};
use crate::util::trig::{icos, isin};

/// Level of detail for a system. Sparse trims stations, asteroid fields,
/// and starfield density for deep-space systems the player is unlikely to
/// linger in (spec §17, "variable fidelity").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fidelity {
    Full,
    Sparse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StarClass {
    Dwarf,
    Main,
    Giant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Star {
    pub class: StarClass,
    pub color: ColorRgba8,
}

/// One planet's orbital slot: params to hand to `generate_planet`, plus the
/// fixed-point position this system placed it at. The planet mesh/texture
/// itself is generated on demand by whoever renders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orbit {
    /// Orbital radius from the star, whole world units.
    pub radius: i64,
    /// Planet body radius, whole world units (fed to `generate_planet`).
    pub planet_radius: i64,
    pub biome: Biome,
    pub seed: u64,
    pub position: FixedVec2,
}

/// A scatter of rocks the client renders with small `HullClass::Rock`
/// hulls; `density` is a rock count, not a float.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsteroidField {
    pub center: FixedVec2,
    pub radius: i64,
    pub density: u32,
    pub biome: Biome,
}

/// Where a station sits, and the params to hand to `generate_station`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StationSlot {
    pub position: FixedVec2,
    pub kind: StationKind,
    pub seed: u64,
}

/// A whole star system as plain data (spec §5, §14 Mode 3). Frozen
/// contract: S01's authored content and S21's frontier both deserialize
/// into this struct — additive changes only, never remove/rename a field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedSystem {
    pub star: Star,
    pub orbits: Vec<Orbit>,
    pub asteroid_fields: Vec<AsteroidField>,
    pub station_slots: Vec<StationSlot>,
    pub gate_position: FixedVec2,
    /// Seed for the client's on-demand starfield (see `generate_starfield`)
    /// — cheap to regenerate, so it isn't embedded here.
    pub starfield_seed: u64,
    pub threat_level: u8,
}

// --- orbital spacing -------------------------------------------------

/// First orbit band starts this far from the star.
const ORBIT_START_RADIUS: i64 = 480;
/// Width of the band a planet's radius is drawn from.
const ORBIT_BAND_WIDTH: i64 = 220;
/// Minimum gap enforced between adjacent orbit bands (the "orbits don't
/// overlap" contract).
const ORBIT_BAND_GAP: i64 = 140;

/// Minimum clearance kept between a placed station/field and any orbit
/// position, and the bound on rejection-sampling attempts to find it.
const PLACEMENT_MIN_GAP: i64 = 130;
const MAX_PLACEMENT_ATTEMPTS: u32 = 8;

/// Radius beyond the outermost orbit band the gate is placed at.
const GATE_MARGIN: i64 = 260;
const GATE_JITTER: i64 = 220;

/// Seed → a whole star system (spec §5, §14 Mode 3). Pure function:
/// same seed, same biome, same fidelity, same system, on every target.
pub fn generate_system(seed: u64, biome: Biome, fidelity: Fidelity) -> GeneratedSystem {
    let mut rng = SeededRng::new(seed);

    let star = generate_star(seed);

    let (orbits, system_edge) = generate_orbits(&mut rng, seed, biome, fidelity);
    let asteroid_fields = generate_asteroid_fields(&mut rng, biome, fidelity, &orbits);
    let station_slots = generate_stations(&mut rng, seed, biome, fidelity, &orbits);

    let gate_radius = system_edge + GATE_MARGIN + rng.next_below(GATE_JITTER as u64) as i64;
    let gate_turn = rng.next_below(65536) as u16;
    let gate_position = polar(gate_radius, gate_turn);

    let threat_level = generate_threat_level(&mut rng, biome);
    let starfield_seed = seed ^ 0x5741_9333_0000_0001;

    GeneratedSystem {
        star,
        orbits,
        asteroid_fields,
        station_slots,
        gate_position,
        starfield_seed,
        threat_level,
    }
}

fn generate_star(seed: u64) -> Star {
    // Distinct stream from the top-level layout rng, same pattern as
    // station.rs's exterior-vs-interior split.
    let mut rng = SeededRng::new(seed ^ 0x5741_5220);
    let class = match rng.next_below(3) {
        0 => StarClass::Dwarf,
        1 => StarClass::Main,
        _ => StarClass::Giant,
    };
    let color = generate_palette(seed).primary;
    Star { class, color }
}

fn planet_count_range(biome: Biome) -> (usize, usize) {
    match biome {
        Biome::Core => (3, 6),
        Biome::Frontier => (2, 5),
        Biome::Nebula => (1, 4),
        Biome::Derelict => (1, 3),
        Biome::DeepSpace => (0, 3),
    }
}

fn station_count_range(biome: Biome) -> (usize, usize) {
    match biome {
        Biome::Core => (1, 3),
        Biome::Frontier => (1, 3),
        Biome::Nebula => (0, 2),
        Biome::Derelict => (0, 1),
        Biome::DeepSpace => (0, 1),
    }
}

fn asteroid_count_range(biome: Biome) -> (usize, usize) {
    match biome {
        Biome::Core => (0, 1),
        Biome::Frontier => (1, 2),
        Biome::Nebula => (2, 4),
        Biome::Derelict => (1, 3),
        Biome::DeepSpace => (0, 2),
    }
}

fn station_kind_pool(biome: Biome) -> &'static [StationKind] {
    match biome {
        Biome::Core => &[
            StationKind::Trade,
            StationKind::Trade,
            StationKind::Military,
        ],
        Biome::Frontier => &[
            StationKind::Trade,
            StationKind::Mining,
            StationKind::Military,
        ],
        Biome::Nebula => &[StationKind::Mining, StationKind::Trade],
        Biome::Derelict => &[StationKind::Mining],
        Biome::DeepSpace => &[StationKind::Military, StationKind::Mining],
    }
}

/// Sparse fidelity trims a (min, max) count range: at most one, and half
/// the ceiling (spec §17 "variable fidelity").
fn sparse_range(range: (usize, usize)) -> (usize, usize) {
    let (lo, hi) = range;
    let lo = lo.min(1);
    let hi = (hi / 2).max(lo);
    (lo, hi)
}

fn count_in_range(rng: &mut SeededRng, range: (usize, usize)) -> usize {
    let (lo, hi) = range;
    lo + rng.next_below((hi - lo + 1) as u64) as usize
}

fn generate_orbits(
    rng: &mut SeededRng,
    seed: u64,
    biome: Biome,
    fidelity: Fidelity,
) -> (Vec<Orbit>, i64) {
    let range = match fidelity {
        Fidelity::Full => planet_count_range(biome),
        Fidelity::Sparse => sparse_range(planet_count_range(biome)),
    };
    let count = count_in_range(rng, range);

    let mut orbits = Vec::with_capacity(count);
    let mut band_lo = ORBIT_START_RADIUS;
    for i in 0..count {
        let radius = band_lo + rng.next_below(ORBIT_BAND_WIDTH as u64) as i64;
        let turn = rng.next_below(65536) as u16;
        let position = polar(radius, turn);
        let planet_radius = 24 + rng.next_below(56) as i64;
        let planet_seed = child_seed(seed, 0x0091_47A1, i as u64);
        orbits.push(Orbit {
            radius,
            planet_radius,
            biome,
            seed: planet_seed,
            position,
        });
        band_lo += ORBIT_BAND_WIDTH + ORBIT_BAND_GAP;
    }
    (orbits, band_lo)
}

fn generate_asteroid_fields(
    rng: &mut SeededRng,
    biome: Biome,
    fidelity: Fidelity,
    orbits: &[Orbit],
) -> Vec<AsteroidField> {
    let range = match fidelity {
        Fidelity::Full => asteroid_count_range(biome),
        Fidelity::Sparse => sparse_range(asteroid_count_range(biome)),
    };
    let count = count_in_range(rng, range);

    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        let center = place_clear_of_orbits(rng, orbits);
        let radius = 80 + rng.next_below(140) as i64;
        let density = 6 + rng.next_below(14) as u32;
        fields.push(AsteroidField {
            center,
            radius,
            density,
            biome,
        });
    }
    fields
}

fn generate_stations(
    rng: &mut SeededRng,
    seed: u64,
    biome: Biome,
    fidelity: Fidelity,
    orbits: &[Orbit],
) -> Vec<StationSlot> {
    let range = match fidelity {
        Fidelity::Full => station_count_range(biome),
        Fidelity::Sparse => sparse_range(station_count_range(biome)),
    };
    let count = count_in_range(rng, range);
    let pool = station_kind_pool(biome);

    let mut slots = Vec::with_capacity(count);
    for i in 0..count {
        let position = place_clear_of_orbits(rng, orbits);
        let kind = pool[rng.next_below(pool.len() as u64) as usize];
        let station_seed = child_seed(seed, 0x57A7_1014, i as u64);
        slots.push(StationSlot {
            position,
            kind,
            seed: station_seed,
        });
    }
    slots
}

/// Rejection-sample a point beyond the orbit bands, far enough from every
/// planet position that scenery doesn't spawn on top of a planet. Bounded:
/// after `MAX_PLACEMENT_ATTEMPTS` the last candidate wins regardless.
fn place_clear_of_orbits(rng: &mut SeededRng, orbits: &[Orbit]) -> FixedVec2 {
    let outer = orbits
        .iter()
        .map(|o| o.radius)
        .max()
        .unwrap_or(ORBIT_START_RADIUS)
        + ORBIT_BAND_WIDTH;
    let span = outer.max(1);

    let mut candidate = polar(ORBIT_START_RADIUS / 2, 0);
    for _ in 0..MAX_PLACEMENT_ATTEMPTS {
        let radius = rng.next_below(span as u64) as i64;
        let turn = rng.next_below(65536) as u16;
        candidate = polar(radius, turn);
        if orbits
            .iter()
            .all(|o| dist_sq(candidate, o.position) >= gap_sq())
        {
            break;
        }
    }
    candidate
}

fn gap_sq() -> i64 {
    let g = Fixed::from_int(PLACEMENT_MIN_GAP).0;
    g * g
}

fn dist_sq(a: FixedVec2, b: FixedVec2) -> i64 {
    let dx = a.x.0 - b.x.0;
    let dy = a.y.0 - b.y.0;
    dx * dx + dy * dy
}

fn generate_threat_level(rng: &mut SeededRng, biome: Biome) -> u8 {
    let base: u32 = match biome {
        Biome::Core => 10,
        Biome::Frontier => 40,
        Biome::Nebula => 60,
        Biome::Derelict => 70,
        Biome::DeepSpace => 90,
    };
    (base + rng.next_below(20) as u32).min(255) as u8
}

fn polar(radius: i64, turn: u16) -> FixedVec2 {
    let r = Fixed::from_int(radius);
    FixedVec2 {
        x: Fixed(r.0 * icos(turn) as i64 / 32768),
        y: Fixed(r.0 * isin(turn) as i64 / 32768),
    }
}

/// Derive a stable child seed for a slot: distinct per (seed, tag, index)
/// so sibling planets/stations never accidentally share a stream.
fn child_seed(seed: u64, tag: u64, index: u64) -> u64 {
    seed ^ tag ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

// --- starfield ---------------------------------------------------------

/// One background star: cheap data the client draws as a parallax layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarfieldPoint {
    pub position: FixedVec2,
    pub brightness: u8,
    pub tint: ColorRgba8,
}

const STARFIELD_EXTENT: i64 = 4000;
const STARFIELD_FULL_COUNT: usize = 256;
const STARFIELD_SPARSE_COUNT: usize = 64;

/// Seeded point cloud regenerated on demand from `GeneratedSystem::starfield_seed`
/// — not embedded in the contract because it's cheaper to recompute than
/// to ship over the wire.
pub fn generate_starfield(seed: u64, fidelity: Fidelity) -> Vec<StarfieldPoint> {
    let count = match fidelity {
        Fidelity::Full => STARFIELD_FULL_COUNT,
        Fidelity::Sparse => STARFIELD_SPARSE_COUNT,
    };
    let mut rng = SeededRng::new(seed ^ 0x5741_5246);
    let palette = generate_palette(seed ^ 0x5441_4E54);

    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        let x = rng.next_below((2 * STARFIELD_EXTENT) as u64) as i64 - STARFIELD_EXTENT;
        let y = rng.next_below((2 * STARFIELD_EXTENT) as u64) as i64 - STARFIELD_EXTENT;
        let brightness = 64 + rng.next_below(192) as u8;
        let tint = if rng.next_below(4) == 0 {
            palette.accent
        } else {
            ColorRgba8 {
                r: 235,
                g: 235,
                b: 245,
                a: 255,
            }
        };
        points.push(StarfieldPoint {
            position: FixedVec2 {
                x: Fixed(x),
                y: Fixed(y),
            },
            brightness,
            tint,
        });
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_system(9, Biome::Frontier, Fidelity::Full);
        let b = generate_system(9, Biome::Frontier, Fidelity::Full);
        assert_eq!(a, b);
    }

    #[test]
    fn exactly_one_gate() {
        // `gate_position` is a single field, not a Vec — this test locks
        // that the contract stays that way (one gate per system, always).
        let sys = generate_system(1234, Biome::DeepSpace, Fidelity::Full);
        let _: FixedVec2 = sys.gate_position;
    }

    #[test]
    fn orbits_dont_overlap() {
        for &seed in &[0u64, 1, 42, 0xDEAD_BEEF, (1u64 << 53) - 1] {
            for biome in [
                Biome::Core,
                Biome::Frontier,
                Biome::Nebula,
                Biome::Derelict,
                Biome::DeepSpace,
            ] {
                for fidelity in [Fidelity::Full, Fidelity::Sparse] {
                    let sys = generate_system(seed, biome, fidelity);
                    for pair in sys.orbits.windows(2) {
                        let gap = pair[1].radius - pair[0].radius;
                        assert!(
                            gap >= ORBIT_BAND_GAP,
                            "orbits overlap at seed {seed:#x} biome {biome:?}: \
                             {} then {} (gap {gap})",
                            pair[0].radius,
                            pair[1].radius,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn station_and_field_counts_within_spec_bounds() {
        for &seed in &[0u64, 1, 42, 0xDEAD_BEEF] {
            for biome in [
                Biome::Core,
                Biome::Frontier,
                Biome::Nebula,
                Biome::Derelict,
                Biome::DeepSpace,
            ] {
                let sys = generate_system(seed, biome, Fidelity::Full);
                assert!(sys.station_slots.len() <= 3, "stations: 0-3 by biome");
                assert!(sys.asteroid_fields.len() <= 4, "asteroid fields: 0-4");
            }
        }
    }

    /// `sparse_range` is the fidelity knob's whole contract (spec §17): its
    /// ceiling must never exceed the full-fidelity ceiling, for any range a
    /// biome table might produce.
    #[test]
    fn sparse_range_never_exceeds_full_range() {
        for range in [(0usize, 1usize), (1, 3), (2, 5), (3, 6), (0, 3), (1, 2)] {
            let (lo, hi) = sparse_range(range);
            assert!(lo <= hi, "sparse range inverted: {lo}..={hi}");
            assert!(hi <= range.1, "sparse ceiling exceeds full ceiling");
            assert!(lo <= 1, "sparse floor stays low-detail");
        }
    }

    #[test]
    fn sparse_counts_stay_within_sparse_bounds() {
        for &seed in &[0u64, 1, 42, 0xDEAD_BEEF] {
            for biome in [
                Biome::Core,
                Biome::Frontier,
                Biome::Nebula,
                Biome::Derelict,
                Biome::DeepSpace,
            ] {
                let sys = generate_system(seed, biome, Fidelity::Sparse);
                let (_, orbit_max) = sparse_range(planet_count_range(biome));
                let (_, station_max) = sparse_range(station_count_range(biome));
                let (_, field_max) = sparse_range(asteroid_count_range(biome));
                assert!(sys.orbits.len() <= orbit_max);
                assert!(sys.station_slots.len() <= station_max);
                assert!(sys.asteroid_fields.len() <= field_max);
            }
        }
    }

    #[test]
    fn biomes_differ() {
        let a = generate_system(3, Biome::Frontier, Fidelity::Full);
        let b = generate_system(3, Biome::DeepSpace, Fidelity::Full);
        assert_ne!(a, b);
    }

    #[test]
    fn starfield_deterministic_and_sparse_is_smaller() {
        let a = generate_starfield(7, Fidelity::Full);
        let b = generate_starfield(7, Fidelity::Full);
        assert_eq!(a, b);
        let sparse = generate_starfield(7, Fidelity::Sparse);
        assert!(sparse.len() < a.len());
    }

    #[test]
    fn round_trips_through_json() {
        let sys = generate_system(42, Biome::Frontier, Fidelity::Full);
        let json = serde_json::to_string(&sys).expect("serialize");
        let back: GeneratedSystem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(sys, back);
    }

    /// Golden vector: if this changes, generator output changed on SOME
    /// target — the determinism harness compares it across x86/ARM/wasm.
    #[test]
    fn golden_system_seed_42() {
        let sys = generate_system(42, Biome::Frontier, Fidelity::Full);
        let mut checksum: i64 = 0;
        for o in &sys.orbits {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(o.radius ^ o.position.x.0 ^ o.position.y.0);
        }
        for f in &sys.asteroid_fields {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(f.center.x.0 ^ f.center.y.0 ^ f.radius as i64);
        }
        for s in &sys.station_slots {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(s.position.x.0 ^ s.position.y.0 ^ s.kind as i64);
        }
        checksum = checksum
            .wrapping_mul(31)
            .wrapping_add(sys.gate_position.x.0 ^ sys.gate_position.y.0);
        assert_eq!(
            (
                sys.orbits.len(),
                sys.asteroid_fields.len(),
                sys.station_slots.len(),
                sys.threat_level,
                checksum,
            ),
            golden::SEED_42
        );
    }

    mod golden {
        /// (orbit count, field count, station count, threat, checksum) for
        /// seed 42 / Frontier / Full. Captured on x86_64; must match on
        /// every target.
        pub const SEED_42: (usize, usize, usize, u8, i64) = (4, 2, 2, 56, -372411290521051861);
    }
}
