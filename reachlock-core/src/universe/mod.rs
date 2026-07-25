//! Multi-universe tiers (spec §7): fair competition brackets differentiated
//! only by inference capability. No billing logic — the enum is the
//! architectural hook, nothing more.

pub mod rules;
pub mod tier;

pub use tier::UniverseTier;
