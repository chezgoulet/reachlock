//! reachlock-core — shared library, no rendering deps (spec §3).
//!
//! Everything here is pure and deterministic: generators, the seed
//! protocol, the contract engine, universe tiers, and the network message
//! vocabulary. The client wraps this in Bevy; the server wraps it in Axum;
//! neither adds gameplay logic of its own.

pub mod content;
pub mod contract;
pub mod determinism;
pub mod economy;
pub mod faction;
pub mod generator;
pub mod item;
pub mod network;
pub mod seed;
pub mod universe;
pub mod util;

#[cfg(test)]
mod diag {
    use crate::generator::hull::{generate_hull_class, HullClass};
    use crate::generator::system::{generate_starfield, generate_system, Fidelity};
    use crate::seed::types::Biome;
    use crate::util::color::generate_palette;

    #[test]
    fn dump_system_seed() {
        let seed = 0x5EED_0001u64;
        // Palette
        let palette = generate_palette(seed);
        eprintln!("DIAG palette primary: r={},g={},b={},a={}", palette.primary.r, palette.primary.g, palette.primary.b, palette.primary.a);
        eprintln!("DIAG palette accent: r={},g={},b={},a={}", palette.accent.r, palette.accent.g, palette.accent.b, palette.accent.a);
        eprintln!("DIAG palette structure: r={},g={},b={},a={}", palette.structure.r, palette.structure.g, palette.structure.b, palette.structure.a);
        // Ship hull size
        let hull = generate_hull_class(seed ^ 0x51119, HullClass::Corvette);
        let max_extent = hull.vertices.iter()
            .map(|v| v.x.0.abs().max(v.y.0.abs()))
            .max()
            .unwrap_or(0);
        eprintln!("DIAG corvette max_extent={} (units) num_vertices={} num_indices={}", max_extent, hull.vertices.len(), hull.indices.len());
        // System
        let system = generate_system(seed, Biome::Frontier, Fidelity::Full);
        for (i, orbit) in system.orbits.iter().enumerate() {
            let pos_x = orbit.position.x.0 as f64 / 65536.0;
            let pos_y = orbit.position.y.0 as f64 / 65536.0;
            eprintln!("DIAG orbit[{}] pos=({:.0},{:.0}) radius={} biome={:?}", i, pos_x, pos_y, orbit.planet_radius, orbit.biome);
        }
        for (i, slot) in system.station_slots.iter().enumerate() {
            let pos_x = slot.position.x.0 as f64 / 65536.0;
            let pos_y = slot.position.y.0 as f64 / 65536.0;
            eprintln!("DIAG station[{}] pos=({:.0},{:.0}) seed={} kind={:?}", i, pos_x, pos_y, slot.seed, slot.kind);
        }
        let gate_x = system.gate_position.x.0 as f64 / 65536.0;
        let gate_y = system.gate_position.y.0 as f64 / 65536.0;
        eprintln!("DIAG gate pos=({:.0},{:.0})", gate_x, gate_y);
        eprintln!("DIAG starfield points={}", generate_starfield(seed, Fidelity::Full).len());
        // Check if any planet is near the spawn point
        let spawn = (0i64, 0i64);
        for (i, slot) in system.station_slots.iter().enumerate() {
            let dist = ((slot.position.x.0 as f64).powi(2) + (slot.position.y.0 as f64).powi(2)).sqrt();
            eprintln!("DIAG station[{}] distance_from_origin={:.0}", i, dist / 65536.0);
        }
    }
}
