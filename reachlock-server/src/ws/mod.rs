//! WebSocket surface: shared state, router, connection handling.

pub mod handler;
pub mod session;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::routing::{any, get};
use axum::Router;
use reachlock_core::network::ServerMessage;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::services::seed::{MemorySeedStore, SeedStore};
use crate::services::verify::VerifyService;

pub struct AppState {
    pub seeds: Box<dyn SeedStore>,
    pub verify: VerifyService,
    /// Universe-wide fanout: tick events, presence. Every session forwards
    /// what it receives from here to its own socket.
    pub events: broadcast::Sender<ServerMessage>,
    connected: AtomicUsize,
}

impl AppState {
    pub fn new(_config: &Config) -> Self {
        let (events, _) = broadcast::channel(256);
        AppState {
            seeds: Box::new(MemorySeedStore::default()),
            verify: VerifyService::default(),
            events,
            connected: AtomicUsize::new(0),
        }
    }

    pub fn connected_count(&self) -> usize {
        self.connected.load(Ordering::Relaxed)
    }

    pub(crate) fn session_started(&self) {
        self.connected.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn session_ended(&self) {
        self.connected.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/ws", any(handler::upgrade))
        .with_state(state)
}
