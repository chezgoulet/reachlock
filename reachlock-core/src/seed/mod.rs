//! The Seed Protocol (spec §4): a seed is a 53-bit integer key from which a
//! deterministic generator reproduces a game object anywhere — offline, LAN,
//! or online.

pub mod resolver;
pub mod types;

pub use resolver::derive_seed;
pub use types::{Biome, ObjectType, PlayerId, Seed, SystemId};
