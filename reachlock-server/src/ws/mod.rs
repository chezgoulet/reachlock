//! WebSocket surface: shared state, router, connection handling.

pub mod handler;
pub mod session;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use reachlock_core::network::ServerMessage;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::services::auth::{
    DevLoginRequest, DevLoginResponse, MemorySessionStore, SessionInfo, SessionStore,
};
use crate::services::seed::{MemorySeedStore, SeedStore};
use crate::services::verify::VerifyService;

pub struct AppState {
    pub seeds: Box<dyn SeedStore>,
    pub sessions: Box<dyn SessionStore>,
    pub verify: VerifyService,
    /// Universe-wide fanout: tick events, presence. Every session forwards
    /// what it receives from here to its own socket.
    pub events: broadcast::Sender<ServerMessage>,
    /// When true, the WS handshake demands a token minted by `/auth/dev`.
    pub auth_required: bool,
    connected: AtomicUsize,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        let (events, _) = broadcast::channel(256);
        // Store selection: the memory stores are the zero-infra default and
        // mirror the Postgres semantics. `config.db_url` selects the sqlx
        // stores when built `--features postgres` (wired in the pg module);
        // the live path is exercised in CI, not here.
        AppState {
            seeds: Box::new(MemorySeedStore::default()),
            sessions: Box::new(MemorySessionStore::default()),
            verify: VerifyService::default(),
            events,
            auth_required: config.auth_required,
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
        .route("/auth/dev", post(auth_dev))
        .route("/ws", any(handler::upgrade))
        .with_state(state)
}

/// `POST /auth/dev { username, universe? }` — dev-only token issuance
/// (spec §8, S03). Not a security boundary; mints a bearer token the WS
/// handshake then presents as `?token=…`.
async fn auth_dev(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DevLoginRequest>,
) -> Json<DevLoginResponse> {
    let player_id = req.username;
    let token = state.sessions.issue(SessionInfo {
        player_id: player_id.clone(),
        universe: req.universe,
    });
    Json(DevLoginResponse { token, player_id })
}
