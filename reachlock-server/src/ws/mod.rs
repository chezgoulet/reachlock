//! WebSocket surface: shared state, router, connection handling.

pub mod admin;
pub mod handler;
pub mod session;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use reachlock_core::network::ServerMessage;
use reachlock_core::seed::types::SystemId;
use reachlock_core::universe::tier::UniverseTier;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::services::audit::{AuditLog, MemoryAuditLog};
use crate::services::auth::{
    DevLoginRequest, DevLoginResponse, MemorySessionStore, SessionInfo, SessionStore,
};
use crate::services::byok::ByokRegistration;
use crate::services::contracts::{ContractStore, MemoryContractStore};
use crate::services::health::HealthAggregator;
use crate::services::llm_proxy::LlmService;
use crate::services::seed::{MemorySeedStore, SeedStore};
use crate::services::verify::VerifyService;
use crate::services::billing::{MemorySubscriptionStore, SubscriptionStore, StripeWebhook, SubscriptionStatus, create_checkout_session, create_portal_session, verify_stripe_webhook};
use crate::services::content::ContentService;
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
    /// S26: audit log of admin actions.
    pub audit: Box<dyn AuditLog>,
    /// S26: Prometheus metrics registry.
    pub prometheus: prometheus::Registry,
    /// S26: health check aggregator.
    pub health: std::sync::Arc<HealthAggregator>,
    /// When true, the WS handshake demands a token minted by `/auth/dev`.
    pub auth_required: bool,
    /// S28: subscription entitlements and offline token store.
    pub billing: Box<dyn SubscriptionStore>,
    connected: AtomicUsize,
    /// S29: voice chat room registry.
    pub voice: VoiceRegistry,
    /// Authored content distribution for wasm clients (spec §10).
    pub content: ContentService,
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
            audit: Box::new(MemoryAuditLog::default()),
            prometheus: crate::observability::init_prometheus(),
            health: std::sync::Arc::new(HealthAggregator::default()),
            auth_required: config.auth_required,
            connected: AtomicUsize::new(0),
            voice: VoiceRegistry::default(),
            billing: Box::new(MemorySubscriptionStore::default()),
            // Content distribution reads `mods/` from the working directory
            // (same source the native client loads). Override with
            // REACHLOCK_MODS_DIR if the server runs elsewhere.
            content: ContentService::new(
                std::env::var("REACHLOCK_MODS_DIR").unwrap_or_else(|_| "mods".to_string()),
            ),
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
    let admin_routes = admin::admin_routes();
    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        .route("/auth/dev", post(auth_dev))
        .route("/byok", post(byok_register))
        .route("/ws", any(handler::upgrade))
        // S28: Stripe webhook (no auth — signed by Stripe).
        .route("/stripe/webhook", post(stripe_webhook_handler))
        // S28: billing endpoints (bearer token auth).
        .route("/billing/checkout", post(billing_checkout))
        .route("/billing/portal", post(billing_portal))
        .route("/billing/entitlement-token", post(billing_entitlement_token))
        .merge(admin_routes)
        .with_state(state)
}

/// `GET /metrics` — Prometheus text exposition (S26).
async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    use prometheus::TextEncoder;
    let encoder = TextEncoder::new();
    let mut buffer = String::new();
    encoder.encode_utf8(&state.prometheus.gather(), &mut buffer).unwrap_or_default();
    buffer
}

/// S26: aggregate health check across all backends.
async fn health_handler(State(state): State<Arc<AppState>>) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let agg = state.health.aggregate();
    let code = if agg.status == "ok" {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(serde_json::to_value(&agg).unwrap_or_default()))
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

// ---------------------------------------------------------------------------
// S28: Stripe webhook handler
// ---------------------------------------------------------------------------

/// `POST /stripe/webhook` — Stripe event subscription updates.
async fn stripe_webhook_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> (axum::http::StatusCode, &'static str) {
    let webhook_secret = match std::env::var("REACHLOCK_STRIPE_WEBHOOK_SECRET") {
        Ok(s) => s,
        Err(_) => return (StatusCode::SERVICE_UNAVAILABLE, "stripe not configured"),
    };
    let sig_header = match headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(h) => h,
        None => return (StatusCode::BAD_REQUEST, "missing stripe-signature header"),
    };

    let event_id = match verify_stripe_webhook(&body, sig_header, &webhook_secret) {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, e),
    };

    if state.billing.is_webhook_processed(&event_id) {
        return (StatusCode::OK, "already processed");
    }
    state.billing.mark_webhook_processed(&event_id);

    // Parse event and update entitlement
    let event: StripeWebhook = match serde_json::from_slice(&body) {
        Ok(e) => e,
        Err(_) => return (StatusCode::BAD_REQUEST, "unparseable webhook"),
    };

    let sub_obj = &event.data.object;
    let metadata = sub_obj.metadata.as_ref();
    let player_id = metadata.and_then(|m| m.get("player_id"));
    let tier_str = metadata.and_then(|m| m.get("universe_tier"));

    let (player_id, tier) = match (player_id, tier_str) {
        (Some(pid), Some(t)) => (pid.clone(), t.clone()),
        _ => return (StatusCode::OK, "no player metadata — ignored"),
    };

    let tier_parsed: UniverseTier = match tier.parse() {
        Ok(t) => t,
        Err(_) => return (StatusCode::OK, "unknown tier in metadata"),
    };

    let status = sub_obj
        .status
        .as_deref()
        .map(SubscriptionStatus::from_stripe)
        .unwrap_or(SubscriptionStatus::Incomplete);

    let period_end = sub_obj
        .current_period_end
        .map(|ts| chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default())
        .unwrap_or_else(chrono::Utc::now);

    use crate::services::billing::PlayerSubscription;
    state.billing.upsert(PlayerSubscription {
        player_id: player_id.clone(),
        stripe_customer_id: sub_obj.customer.clone(),
        tier: tier_parsed,
        status,
        current_period_end: period_end,
        created_at: chrono::Utc::now(),
    });

    (StatusCode::OK, "ok")
}

// ---------------------------------------------------------------------------
// S28: Billing API endpoints
// ---------------------------------------------------------------------------

/// Authenticate a bearer token from the Authorization header.
fn resolve_bearer_token(headers: &axum::http::HeaderMap, state: &AppState) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| {
            state.sessions.resolve(token).map(|info| info.player_id.clone())
        })
}

/// Helper: extract bearer token, return 401 if missing.
macro_rules! require_auth {
    ($headers:expr, $state:expr) => {
        match resolve_bearer_token($headers, $state) {
            Some(pid) => pid,
            None => return Err((
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "unauthorized"})),
            )),
        }
    };
}

/// `POST /billing/checkout` — create a Stripe Checkout session URL.
async fn billing_checkout(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let player_id = require_auth!(&headers, &state);
    let tier_str = body["universe_tier"].as_str().unwrap_or("fairplay");
    let tier: UniverseTier = tier_str.parse().map_err(|_| {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "bad tier"})))
    })?;

    match create_checkout_session(&player_id, tier).await {
        Ok(url) => Ok(Json(serde_json::json!({"url": url}))),
        Err(e) => Err((StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": e})))),
    }
}

/// `POST /billing/portal` — create a Stripe Customer Portal session.
async fn billing_portal(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let player_id = require_auth!(&headers, &state);
    let sub = state.billing.get(&player_id).ok_or((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "no_subscription"})),
    ))?;
    match create_portal_session(&sub.stripe_customer_id).await {
        Ok(url) => Ok(Json(serde_json::json!({"url": url}))),
        Err(e) => Err((StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": e})))),
    }
}

/// `POST /billing/entitlement-token` — mint an offline entitlement token (30 days).
async fn billing_entitlement_token(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<serde_json::Value>)> {
    let player_id = require_auth!(&headers, &state);
    let sub = state.billing.get(&player_id).ok_or((
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "no_subscription"})),
    ))?;
    let token = match crate::services::billing::mint_offline_token(&player_id, sub.tier) {
        Ok(t) => t,
        Err(e) => return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e})),
        )),
    };
    Ok(Json(serde_json::to_value(&token).unwrap_or_default()))
}
