//! Integer value noise on a seeded lattice. No floats: outputs are
//! fixed-point in [-32768, 32768] and interpolation is integer, so every
//! target produces the identical field (spec §5).

use super::rng::SeededRng;

/// Hash a lattice point to a value in [-32768, 32768].
fn lattice(seed: u64, x: i64, y: i64) -> i32 {
    let mut rng = SeededRng::new(
        seed ^ (x as u64).wrapping_mul(0x8DA6_B343)
            ^ (y as u64).wrapping_mul(0xD816_3841_A371_69BD),
    );
    (rng.next_u64() & 0xFFFF) as i32 - 32768
}

/// Smoothstep in 1/256 units: 3t² - 2t³ for t in [0, 256].
fn smooth(t: i64) -> i64 {
    (3 * t * t * 256 - 2 * t * t * t) / (256 * 256)
}

/// Value noise sampled at (x, y) in 1/256 lattice-cell units.
/// Returns a value in [-32768, 32768].
pub fn value_noise(seed: u64, x: i64, y: i64) -> i32 {
    let (cx, cy) = (x.div_euclid(256), y.div_euclid(256));
    let (fx, fy) = (x.rem_euclid(256), y.rem_euclid(256));
    let (sx, sy) = (smooth(fx), smooth(fy));

    let n00 = lattice(seed, cx, cy) as i64;
    let n10 = lattice(seed, cx + 1, cy) as i64;
    let n01 = lattice(seed, cx, cy + 1) as i64;
    let n11 = lattice(seed, cx + 1, cy + 1) as i64;

    let top = n00 + (n10 - n00) * sx / 256;
    let bottom = n01 + (n11 - n01) * sx / 256;
    (top + (bottom - top) * sy / 256) as i32
}

/// Fractal sum: `octaves` layers of value noise, each at double frequency
/// and half amplitude. Output stays within [-32768, 32768].
pub fn fbm(seed: u64, x: i64, y: i64, octaves: u32) -> i32 {
    let mut total: i64 = 0;
    let mut amplitude: i64 = 32768;
    let mut divisor: i64 = 0;
    for octave in 0..octaves {
        let f = 1i64 << octave;
        total +=
            value_noise(seed.wrapping_add(octave as u64), x * f, y * f) as i64 * amplitude / 32768;
        divisor += amplitude;
        amplitude /= 2;
    }
    (total * 32768 / divisor.max(1)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(value_noise(7, 1000, 2000), value_noise(7, 1000, 2000));
        assert_eq!(fbm(7, 1000, 2000, 4), fbm(7, 1000, 2000, 4));
    }

    #[test]
    fn continuous_at_cell_edges() {
        // Values one step apart across a lattice boundary stay close.
        let a = value_noise(7, 255, 128);
        let b = value_noise(7, 256, 128);
        assert!((a - b).abs() < 2048, "discontinuity: {a} vs {b}");
    }

    #[test]
    fn in_range() {
        for i in 0..500 {
            let v = fbm(3, i * 37, i * 91, 4);
            assert!((-32768..=32768).contains(&v));
        }
    }
}
