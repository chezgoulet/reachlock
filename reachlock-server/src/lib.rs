//! reachlock-server library surface: everything the binary wires together,
//! importable by integration tests (and eventually by a combined
//! host-mode client for LAN play).

pub mod config;
pub mod observability;
pub mod services;
pub mod ws;

pub use config::Config;
pub use ws::{router, AppState};
