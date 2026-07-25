//! NPC soul system (spec §15; S13). Souls are DATA, not live LLM
//! connections: a [`types::SoulFile`] defines *who* an NPC is (identity,
//! personality, emotional baseline, memories, relationships, goals,
//! breaking points, secrets); the contract engine (spec §6) decides *how*
//! they react; S16 decides *what they say*.
//!
//! Layering (the S13 gotcha, honored):
//! - Authored soul files are immutable content (`content/souls/*.ron`,
//!   loaded through the content pipeline as [`crate::content::ContentPayload::Soul`]).
//! - Live state ([`runtime::SoulState`]) is a separate serde struct keyed by
//!   soul id — it goes in the save; the authored file never changes.
//! - Emotional triggers, breaking points, secret reveals, and mutation
//!   triggers all reuse [`crate::contract::types::Condition`] evaluated by
//!   [`crate::contract::engine::condition_holds`] — one predicate language,
//!   not two (the v1 mistake we're not repeating).
//! - Everything numeric is fixed-point `i64` (1024 = 1.0), per iron rule #2.

pub mod compression;
pub mod memory;
pub mod runtime;
pub mod types;

pub use compression::{
    compress, select_strategy, should_compress, CompressedContext, CompressionStrategy,
};
pub use memory::{
    RelationshipMemory, SignificantEvent, SignificantEventType, TrustTrajectory, TrustTrend,
};
pub use runtime::{
    apply_event, apply_mutation, inject_soul_fields, load_soul_mutations, SoulEvent, SoulOutput,
    SoulState,
};
pub use types::{SoulChange, SoulFile, SoulMutation};
