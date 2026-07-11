//! Deterministic primitives: fixed-point math, seeded RNG, integer noise,
//! palette generation. Everything here must be bit-identical on every target
//! (spec §5, Determinism Guarantee).

pub mod color;
pub mod noise;
pub mod rng;
pub mod trig;

pub use rng::{Fixed, SeededRng};
