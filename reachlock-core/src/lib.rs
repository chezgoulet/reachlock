//! reachlock-core — shared library, no rendering deps (spec §3).
//!
//! Everything here is pure and deterministic: generators, the seed
//! protocol, the contract engine, universe tiers, and the network message
//! vocabulary. The client wraps this in Bevy; the server wraps it in Axum;
//! neither adds gameplay logic of its own.

pub mod agency;
pub mod content;
pub mod contract;
pub mod crisis;
pub mod determinism;
pub mod dialogue;
pub mod economy;
pub mod faction;
pub mod generator;
pub mod item;
pub mod network;
pub mod seed;
pub mod sim;
pub mod soul;
pub mod universe;
pub mod util;
