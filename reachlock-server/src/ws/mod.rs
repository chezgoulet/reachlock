//! WebSocket surface: shared state, router, connection handling.

pub mod handler;
pub mod session;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use reachlock_core::network::ServerMessage;
use reachlock_core::seed::types::SystemId;
use reachlock_core::universe::tier::UniverseTier;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::services::auth::{
    DevLoginRequest, DevLoginResponse, MemorySessionStore, SessionInfo, SessionStore,
};
use crate::services::byok::ByokRegistration;
use crate::services::contracts::{ContractStore, MemoryContractStore};
use crate::services::llm_proxy::LlmService;
use crate::services::seed::{MemorySeedStore, SeedStore};
use crate::services::verify::VerifyService;
use crate::services::voice::VoiceRegistry;

/// A map from (universe, system_id) to session message senders in that scope.
type SystemSenders = HashMap<(UniverseTier, SystemId), Vec<tokio::sync::mpsc::Sender<ServerMessage>>>;

/// S23: per-system presence registry. Holds outgoing message senders for
/// every session currently in a given (universe, system) pair. Scoped
/// messages (player position, chat, join/leave) go through this instead of
/// the global broadcast channel.
pub struct PresenceManager {
    by_system: RwLock<SystemSenders>,
}

impl Default for PresenceManager {
    fn default() -> Self {
        PresenceManager {
            by_system: RwLock::new(HashMap::new()),
        }
    }
}

impl PresenceManager {
    /// Register a session's sender in a system scope.
    pub async fn join(
        &self,
        universe: UniverseTier,
        system_id: SystemId,
        tx: tokio::sync::mpsc::Sender<ServerMessage>,
    ) {
        let mut map = self.by_system.write().await;
        map.entry((universe, system_id)).or_default().push(tx);
    }

    /// Unregister a session's sender (best-effort — only removes by identity).
    pub async fn leave(&self, universe: UniverseTier, system_id: &SystemId, tx: &tokio::sync::mpsc::Sender<ServerMessage>) {
        let mut map = self.by_system.write().await;
        if let Some(senders) = map.get_mut(&(universe, system_id.clone())) {
            senders.retain(|s| !s.same_channel(tx));
            if senders.is_empty() {
                map.remove(&(universe, system_id.clone()));
            }
        }
    }

    /// Broadcast a message to all sessions in the given (universe, system).
    pub async fn broadcast(&self, universe: UniverseTier, system_id: &SystemId, msg: &ServerMessage) {
        let map = self.by_system.read().await;
        if let Some(senders) = map.get(&(universe, system_id.clone())) {
            for sender in senders {
                let _ = sender.send(msg.clone()).await;
            }
        }
    }

    /// Iterate all sessions across all systems (for admin/global operations).
    pub async fn broadcast_all(&self, msg: &ServerMessage) {
        let map = self.by_system.read().await;
        for senders in map.values() {
            for sender in senders {
                let _ = sender.send(msg.clone()).await;
            }
        }
    }
}

pub struct AppState {
    pub seeds: Box<dyn SeedStore>,
    pub sessions: Box<dyn SessionStore>,
    pub verify: VerifyService,
    pub contracts: Box<dyn ContractStore>,
    pub llm: LlmService,
    /// Universe-wide fanout: tick events, presence.
    pub events: broadcast::Sender<ServerMessage>,
    /// S23: per-system presence registry for scoped messages.
    pub presence: PresenceManager,
    /// When true, the WS handshake demands a token minted by `/auth/dev`.
    pub auth_required: bool,
    connected: AtomicUsize,
    /// S29: voice chat room registry.
    pub voice: VoiceRegistry,
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
            presence: PresenceManager::default(),
            auth_required: config.auth_required,
            connected: AtomicUsize::new(0),
            voice: VoiceRegistry::default(),
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
