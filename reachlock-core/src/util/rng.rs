//! Seeded RNG and fixed-point primitives (spec §5, Determinism Guarantee).

/// Fixed-point value: 1 unit = 1/1024 world units. All gameplay-critical
/// math stays in integers; `to_f32` exists only for the visual bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fixed(pub i64);

impl Fixed {
    pub const SCALE: i64 = 1024;

    pub const fn from_int(v: i64) -> Self {
        Fixed(v * Self::SCALE)
    }

    /// Visual-only escape hatch for the bridge layer. Never feed the result
    /// back into gameplay state.
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }
}

/// SplitMix64: deterministic, allocation-free, identical on every target.
/// No `rand` dependency in the spike — the full StdRng decision is spec §5's,
/// but the determinism harness will pin whatever we choose bit-for-bit.
pub struct SeededRng {
    state: u64,
}

impl SeededRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, n) via Lemire reduction — integer-only, no float bias.
    pub fn next_below(&mut self, n: u64) -> u64 {
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }
}
