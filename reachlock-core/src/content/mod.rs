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

pub mod envelope;
pub mod priority;
pub mod resolve;
pub mod seed;
pub mod validate;

pub use envelope::{AssetType, ContentFile, ContentPayload};
pub use priority::Priority;
pub use resolve::{resolve, Resolved, SeedParams};
pub use seed::content_seed;
pub use validate::{validate_content, ValidationError};
