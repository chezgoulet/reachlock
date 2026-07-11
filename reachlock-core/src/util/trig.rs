//! Integer trigonometry: sine/cosine on a const-evaluated quarter-wave
//! table. Coarse but bit-identical on every target — which is the point.
//! Angles are in turns/65536; outputs in [-32768, 32768].

pub fn isin(turn: u16) -> i32 {
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

pub fn icos(turn: u16) -> i32 {
    isin(turn.wrapping_add(16384))
}

/// sin(i/64 * π/2) * 32768, computed at compile time with integer math
/// (Taylor series in 1/2^16 fixed point). Const-evaluated: no runtime float,
/// no cross-target drift.
const fn build_quarter_table() -> [i32; 65] {
    let mut table = [0i32; 65];
    let mut i = 0;
    while i <= 64 {
        // angle in radians * 2^16: (i/64) * (π/2) * 65536
        let x = i as i64 * 102944 / 64; // π/2 * 65536 = 102943.7
                                        // sin(x) ≈ x - x³/6 + x⁵/120 - x⁷/5040, in 2^16 fixed point
        let x2 = x * x / 65536;
        let x3 = x2 * x / 65536;
        let x5 = x3 * x2 / 65536;
        let x7 = x5 * x2 / 65536;
        let s = x - x3 / 6 + x5 / 120 - x7 / 5040;
        table[i as usize] = (s * 32768 / 65536) as i32;
        i += 1;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinal_points() {
        assert_eq!(isin(0), 0);
        assert!((isin(16384) - 32768).abs() < 128, "sin(90°) ≈ 1");
        assert!(isin(32768).abs() < 128, "sin(180°) ≈ 0");
        assert!((isin(49152) + 32768).abs() < 128, "sin(270°) ≈ -1");
    }

    #[test]
    fn odd_symmetry() {
        for turn in (0u16..32768).step_by(1000) {
            let neg = turn.wrapping_neg();
            assert!((isin(turn) + isin(neg)).abs() <= 2, "sin(-x) = -sin(x)");
        }
    }
}
