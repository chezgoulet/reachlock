//! Client↔server protocol types (spec §8). Shared by reachlock-client and
//! reachlock-server so the two cannot drift apart.

pub mod messages;

pub use messages::{ClientMessage, ServerMessage, VoiceSignalPayload, PROTOCOL_VERSION};
