//! Spike generator: a seed becomes a ship hull polygon.
//!
//! Pure function, integer math only, plain-data output. The real hull
//! generator (spec §5) replaces this; the *shape* — seed in, `GeneratedMesh`
//! out, bridge converts — is what the spike locks in.

use crate::util::{Fixed, SeededRng};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedVec2 {
    pub x: Fixed,
    pub y: Fixed,
}

/// Plain-data mesh, target-independent. The client bridge converts this to a
/// Bevy `Mesh`; an authored-content file deserializes to the same struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedMesh {
    pub vertices: Vec<FixedVec2>,
    pub indices: Vec<u32>,
}

/// Generate a closed hull outline: a fan-triangulated irregular polygon whose
/// vertex radii derive entirely from the seed.
pub fn generate_hull(seed: u64) -> GeneratedMesh {
    let mut rng = SeededRng::new(seed);
    let sides = 6 + rng.next_below(7) as usize; // 6..=12 sides

    let mut vertices = Vec::with_capacity(sides + 1);
    vertices.push(FixedVec2 {
        x: Fixed(0),
        y: Fixed(0),
    });

    // Integer-only polar coordinates: angle as turns/65536, radius in fixed
    // units. Sine via a coarse integer table keeps every target bit-identical.
    for i in 0..sides {
        let turn = (i as u64 * 65536 / sides as u64) as u16;
        let radius = Fixed::from_int(40 + rng.next_below(40) as i64);
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

/// Spike stand-in for `generate_music` (spec §5): a seeded two-note square
/// wave as raw PCM. Integer math only — bit-identical on every target.
pub fn generate_tone(seed: u64, sample_rate: u32, seconds: u32) -> Vec<i16> {
    let mut rng = SeededRng::new(seed);
    let root_hz = 110 + rng.next_below(110) as u32; // A2..A3
    let fifth_hz = root_hz * 3 / 2;

    let total = (sample_rate * seconds) as usize;
    let mut samples = Vec::with_capacity(total);
    for i in 0..total {
        let hz = if i < total / 2 { root_hz } else { fifth_hz };
        let period = sample_rate / hz;
        let high = (i as u32 % period) < period / 2;
        samples.push(if high { 6000i16 } else { -6000i16 });
    }
    samples
}

/// Integer sine, input in turns/65536, output in [-32768, 32768].
/// Quarter-wave table, 64 entries, linear interpolation — coarse but
/// deterministic everywhere, which is the entire point.
fn isin(turn: u16) -> i32 {
    const QUARTER: [i32; 65] = build_quarter_table();
    let quadrant = turn >> 14;
    let idx = ((turn & 0x3FFF) >> 8) as usize; // 0..64 within the quadrant
    let frac = (turn & 0xFF) as i32;
    let (lo, hi) = match quadrant {
        0 => (QUARTER[idx], QUARTER[idx + 1]),
        1 => (QUARTER[64 - idx], QUARTER[63 - idx.min(63)]),
        2 => (-QUARTER[idx], -QUARTER[idx + 1]),
        _ => (-QUARTER[64 - idx], -QUARTER[63 - idx.min(63)]),
    };
    lo + (hi - lo) * frac / 256
}

fn icos(turn: u16) -> i32 {
    isin(turn.wrapping_add(16384))
}

/// sin(i/64 * π/2) * 32768, computed at compile time with integer math
/// (Taylor series in 1/2^16 fixed point). Const-evaluated: no runtime float,
/// no cross-target drift.
const fn build_quarter_table() -> [i32; 65] {
    let mut table = [0i32; 65];
    let mut i = 0;
    while i <= 64 {
        // angle in radians * 2^16: (i/64) * (π/2) * 65536 = i * 1608.49...
        let x = i as i64 * 102944 / 64; // π/2 * 65536 = 102943.7
                                        // sin(x) ≈ x - x³/6 + x⁵/120, in 2^16 fixed point
        let x2 = x * x / 65536;
        let x3 = x2 * x / 65536;
        let x5 = x3 * x2 / 65536;
        let s = x - x3 / 6 + x5 / 120;
        table[i as usize] = (s * 32768 / 65536) as i32;
        i += 1;
    }
    table
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

    /// Golden vector: if this changes, generator output changed on SOME
    /// target — the cross-platform harness (spec §5) extends this to
    /// x86/ARM/wasm comparison.
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
        pub const SEED_42: (usize, usize, i64) = (12, 33, 3259797634967353072);
    }
}
