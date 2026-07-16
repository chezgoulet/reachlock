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
use crate::services::byok::ByokRegistration;
use crate::services::contracts::{ContractStore, MemoryContractStore};
use crate::services::llm_proxy::LlmService;
use crate::services::seed::{MemorySeedStore, SeedStore};
use crate::services::verify::VerifyService;

pub struct AppState {
    pub seeds: Box<dyn SeedStore>,
    pub sessions: Box<dyn SessionStore>,
    pub verify: VerifyService,
    /// S16B: server-side contract backup (`contract.sync` persists here —
    /// memory by default, Postgres via the pg module).
    pub contracts: Box<dyn ContractStore>,
    /// S14: LLM providers + rate limiting + BYOK + latency telemetry.
    pub llm: LlmService,
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
            contracts: Box::new(MemoryContractStore::default()),
            llm: LlmService::from_env(),
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
        .route("/byok", post(byok_register))
        .route("/metrics", get(metrics))
        .route("/ws", any(handler::upgrade))
        .with_state(state)
}

/// `GET /metrics` — deliberation latency histogram, Prometheus text format.
async fn metrics(State(state): State<Arc<AppState>>) -> String {
    state.llm.metrics.render()
}

/// `POST /byok` — register the caller's own provider endpoint + API key
/// (Byok tier, spec §7). Authenticated by the same bearer token the WS
/// handshake uses. The key is encrypted at rest and never logged.
async fn byok_register(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(reg): Json<ByokRegistration>,
) -> (axum::http::StatusCode, &'static str) {
    use axum::http::StatusCode;
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let Some(info) = token.and_then(|t| state.sessions.resolve(t)) else {
        return (StatusCode::UNAUTHORIZED, "invalid or missing bearer token");
    };
    match state.llm.byok.register(&info.player_id, &reg) {
        Ok(()) => (StatusCode::NO_CONTENT, ""),
        Err(crate::services::byok::ByokError::NotConfigured) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "BYOK disabled: server has no REACHLOCK_BYOK_KEY",
        ),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "key storage failed"),
    }
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
