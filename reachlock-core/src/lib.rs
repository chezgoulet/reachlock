//! reachlock-core — shared library, no rendering deps (spec §3).
//!
//! Spike scope: just enough generator to prove the
//! `seed → pure generator → plain data → bridge → Bevy` pipeline compiles
//! and runs identically on native and wasm32 targets.

pub mod generator;
pub mod util;
