//! reachlock-core — shared library, no rendering deps (spec §3).
//!
//! Everything here is pure and deterministic: generators, the seed
//! protocol, the contract engine, universe tiers, and the network message
//! vocabulary. The client wraps this in Bevy; the server wraps it in Axum;
//! neither adds gameplay logic of its own.

pub mod contract;
pub mod determinism;
pub mod generator;
pub mod network;
pub mod seed;
pub mod universe;
pub mod util;
