//! The LLM Contract System (spec §6). Player-authored rules run first, pure
//! and instant; the LLM is a leaf-node fallback for situations the rules
//! cannot resolve. Every online evaluation is signed into a hash chain the
//! server can verify.

pub mod co_deliberation;
pub mod engine;
pub mod metadata;
pub mod protocol;
pub mod signature;
pub mod types;
pub mod validation;

pub use co_deliberation::{
    CoDeliberation, CoDeliberationMetrics, CoResolution, CrewDeliberant, CrewPosition,
    CrewRelationship, DeliberationTurn, GameEvent, RelationshipEvent, RelationshipEventType,
    RelationshipState, StepOutcome, MAX_ROUNDS,
};
pub use engine::{evaluate, EvalContext, Outcome};
pub use metadata::{
    ContractLibraryEntry, ContractMetadata, ContractStory, CraftingWarning, CrewRole,
};
pub use types::{Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger};
pub use validation::validate_contract;
