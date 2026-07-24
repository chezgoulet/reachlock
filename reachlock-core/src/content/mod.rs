//! Authored content pipeline (spec §10): hand-crafted assets flow through
//! the exact same plain-data structs and bridge path as procedurally
//! generated ones — "the bridge doesn't know the difference."
//!
//! - [`envelope::ContentFile`] is the on-disk `.ron` shape every authored
//!   asset deserializes into.
//! - [`priority::Priority`] is the ladder that decides which version wins
//!   when more than one source exists for the same object.
//! - [`resolve::resolve`] is the single function that applies that ladder.
//! - [`seed::content_seed`] derives the canonical seed authored content is
//!   pinned to (spec §10, Seed Integration).
//! - [`validate`] holds the structural integrity checks the CLI's
//!   `content validate` command runs before schema validation.
//!
//! These are frozen contracts (spec §13, iron rule #7): the field names on
//! `ContentFile` and the generator structs it wraps ARE the authoring
//! format. Changing them orphans every `.ron` file under `content/`.

pub mod dialogue;
pub mod dungeon;
pub mod envelope;
pub mod event;
pub mod priority;
pub mod recipe;
pub mod resolve;
pub mod seed;
pub mod validate;

pub use dialogue::{Dialogue, DialogueChoice, DialogueNode, NodeType};
pub use dungeon::{Dungeon, DungeonPuzzle, DungeonRoom};
pub use envelope::{AssetType, ContentFile, ContentPayload, NpcSpawn};
pub use event::{Consequence, Event, EventStage, TriggerCondition};
pub use priority::Priority;
pub use recipe::{Ingredient, OutputConfig, Recipe, SkillRequirement};
pub use resolve::{resolve, Resolved, SeedParams};
pub use seed::content_seed;
pub use validate::{validate_content, ValidationError};
