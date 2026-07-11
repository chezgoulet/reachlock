//! The LLM Contract System (spec §6). Player-authored rules run first, pure
//! and instant; the LLM is a leaf-node fallback for situations the rules
//! cannot resolve. Every online evaluation is signed into a hash chain the
//! server can verify.

pub mod engine;
pub mod protocol;
pub mod signature;
pub mod types;

pub use engine::{evaluate, EvalContext, Outcome};
pub use types::{Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger};
