//! Client networking (S02): mode/connection-state resources and the
//! dual-target transport. Bevy-facing systems live in
//! `crate::systems::network`; this module holds the plain (non-ECS) plumbing
//! so it stays unit-testable without spinning up an `App`.

pub mod mode;
pub mod transport;

pub use mode::{ConnectionState, NetMode};
pub use transport::{TransportEvent, WsTransport};
