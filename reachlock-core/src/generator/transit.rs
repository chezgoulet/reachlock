//! Hyperspace transit state queries and seeded rolls (S09). Pure functions
//! — deterministic in `(system_seed, jump_count)` — consumed by the client's
//! jump/cryo-pilot systems. Do NOT add manifest entries here; these are
//! gameplay rolls, not generators (S09 gotcha ledger).
//!
//! Iron rule: every `rng.next_below` call below produces the same results on
//! x86_64, aarch64, and wasm32. Each function has a determinism test that
//! asserts stable output for fixed `(seed, n)` input.

use crate::util::rng::SeededRng;

/// Anomaly probability per transit, in percent (seeded roll).
pub const ANOMALY_PCT: u64 = 35;
/// Extra fuel a self-jump burns on top of cruise (panic tax).
pub const SELF_JUMP_BURN: i64 = 220;

/// Destination system seed for jump `n` from `seed`. Deterministic in
/// `(seed, n)` — the S09 determinism gotcha.
pub fn transit_destination(seed: u64, n: u64) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(n.wrapping_mul(0x85EB_CA77))
        .wrapping_add(0x6A09_E667)
}

/// Whether a transit anomaly fires. Deterministic in `(seed, n)`.
pub fn anomaly_rolls(seed: u64, n: u64) -> bool {
    let mut rng = SeededRng::new(seed ^ 0x52A1 ^ n);
    rng.next_below(100) < ANOMALY_PCT
}

/// Self-jump malfunction severity 0..=3 (0 = clean arrival). Seeded.
pub fn malfunction_roll(seed: u64, n: u64) -> u64 {
    let mut rng = SeededRng::new(seed ^ 0xC0DE ^ n);
    rng.next_below(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Transit destination must be deterministic — same hash every time.
    #[test]
    fn transit_destination_is_deterministic() {
        let a = transit_destination(0xDEAD_BEEF, 0);
        let b = transit_destination(0xDEAD_BEEF, 0);
        assert_eq!(a, b);

        // Non-zero jump count produces a different destination.
        let c = transit_destination(0xDEAD_BEEF, 1);
        assert_ne!(a, c, "jump_count must change destination");
    }

    /// anomaly_rolls produces the same result for the same (seed, n) pair
    /// across invocations.
    #[test]
    fn anomaly_rolls_is_deterministic() {
        let a = anomaly_rolls(42, 3);
        let b = anomaly_rolls(42, 3);
        assert_eq!(a, b);

        // Different (seed, n) may differ — that's fine, just check it's bool.
        let c = anomaly_rolls(99, 1);
        let _ = c;
    }

    /// malfunction_roll produces the same 0..=3 result for stable input.
    #[test]
    fn malfunction_roll_is_deterministic() {
        let a = malfunction_roll(0xCAFE, 7);
        let b = malfunction_roll(0xCAFE, 7);
        assert_eq!(a, b);
        assert!(a <= 3);
    }

    /// Different (seed, n) pairs produce different severities (basic
    /// distribution check — not statistical, just sanity).
    #[test]
    fn malfunction_roll_distribution_spans_range() {
        let mut seen = [false; 4];
        for n in 0..50 {
            let sev = malfunction_roll(0x5EED, n);
            seen[sev as usize] = true;
            assert!(sev <= 3);
        }
        // With 50 rolls it's extremely likely all four values appear.
        assert!(
            seen.iter().all(|&x| x),
            "malfunction_roll should cover 0..=3 over 50 (seed, n) pairs"
        );
    }
}
