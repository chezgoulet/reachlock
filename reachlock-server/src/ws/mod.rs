//! WebSocket surface: shared state, router, connection handling.

pub mod admin;
pub mod handler;
pub mod session;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use axum::extract::{Path, Query, RawQuery, State};
use axum::http::StatusCode;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use reachlock_core::content::ContentFile;
use reachlock_core::network::ServerMessage;
use reachlock_core::seed::types::{Seed, SystemId};
use reachlock_core::universe::tier::UniverseTier;
use tokio::sync::broadcast;
use tokio::sync::RwLock;

use std::time::Instant;

use axum::response::{IntoResponse, Response};

use crate::config::Config;
use crate::services::audit::{AuditLog, MemoryAuditLog};
use crate::services::auth::{
    DevLoginRequest, DevLoginResponse, MemoryPlayerStore, MemorySessionStore, PlayerStore,
    SessionStore, TempTokenStore,
};
use crate::services::billing::{
    create_checkout_session, create_portal_session, verify_stripe_webhook, MemorySubscriptionStore,
    StripeWebhook, SubscriptionStatus, SubscriptionStore,
};
use crate::services::byok::ByokRegistration;
use crate::services::contracts::{ContractStore, MemoryContractStore};
use crate::services::email::{EmailBackend, NoopEmailBackend};
use crate::services::health::HealthAggregator;
use crate::services::library::{ContractLibrary, MemoryContractLibrary};
use crate::services::llm_proxy::LlmService;
use crate::services::seed::{MemorySeedStore, SeedStore};
use crate::services::verify::VerifyService;
use crate::services::voice::VoiceRegistry;

/// A map from (universe, system_id) to session message senders in that scope.
type SystemSenders =
    HashMap<(UniverseTier, SystemId), Vec<tokio::sync::mpsc::Sender<ServerMessage>>>;

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
    pub async fn leave(
        &self,
        universe: UniverseTier,
        system_id: &SystemId,
        tx: &tokio::sync::mpsc::Sender<ServerMessage>,
    ) {
        let mut map = self.by_system.write().await;
        if let Some(senders) = map.get_mut(&(universe, system_id.clone())) {
            senders.retain(|s| !s.same_channel(tx));
            if senders.is_empty() {
                map.remove(&(universe, system_id.clone()));
            }
        }
    }

    /// Broadcast a message to all sessions in the given (universe, system).
    pub async fn broadcast(
        &self,
        universe: UniverseTier,
        system_id: &SystemId,
        msg: &ServerMessage,
    ) {
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
    pub events: broadcast::Sender<ServerMessage>,
    pub presence: PresenceManager,
    pub audit: Box<dyn AuditLog>,
    pub prometheus: prometheus::Registry,
    pub health: std::sync::Arc<HealthAggregator>,
    pub auth_required: std::sync::atomic::AtomicBool,
    pub billing: Box<dyn SubscriptionStore>,
    connected: AtomicUsize,
    pub voice: VoiceRegistry,
    pub library: Box<dyn ContractLibrary>,
    /// S73: server uptime start instant.
    pub uptime_started: Instant,
    /// S73: server metrics for prometheus.
    pub metrics: std::sync::Arc<crate::services::metrics::ServerMetrics>,
    /// S49: Postgres connection pool. `None` when using in-memory stores.
    /// When Some, tick events are persisted to `universe_events`.
    #[cfg(feature = "postgres")]
    pub pg_pool: Option<sqlx::PgPool>,
    /// S50: Redis connection pool. `None` when using in-memory stores.
    /// When Some, sessions, rate limiting, and presence use Redis.
    #[cfg(feature = "redis")]
    pub redis_pool: Option<std::sync::Arc<crate::services::redis::RedisPool>>,
    /// S54: per-player message senders for targeted delivery (voice signaling).
    pub player_senders: std::sync::Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<String, tokio::sync::mpsc::Sender<ServerMessage>>,
        >,
    >,
    // S51: authentication stores.
    pub players: Box<dyn PlayerStore>,
    pub email: Box<dyn EmailBackend>,
    pub auth_config: std::sync::Arc<std::sync::RwLock<crate::services::auth::AuthConfig>>,
    pub verification_tokens: std::sync::Arc<Mutex<HashMap<String, (String, i64)>>>,
    pub reset_tokens: std::sync::Arc<Mutex<HashMap<String, (String, i64)>>>,
    pub temp_tokens: TempTokenStore,
    /// Confirmed TOTP enrollments. A player here is required to pass 2FA.
    pub totp_secrets: std::sync::Arc<Mutex<HashMap<String, String>>>,
    /// Enrollments awaiting `tfa_verify`. Kept separate from `totp_secrets`
    /// so starting enrollment cannot lock a player out: previously
    /// `tfa_enable` wrote straight into `totp_secrets`, and `login` gates on
    /// that map, so anyone who opened the 2FA screen and never scanned the QR
    /// could no longer log in.
    pub totp_pending: std::sync::Arc<Mutex<HashMap<String, String>>>,
    pub totp_recovery_codes: std::sync::Arc<Mutex<Vec<(String, String)>>>,
    pub oauth_flows: std::sync::Arc<Mutex<HashMap<String, String>>>,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        let (events, _) = broadcast::channel(256);
        let prometheus_registry = crate::observability::init_prometheus();
        let metrics = crate::services::metrics::ServerMetrics::new(&prometheus_registry);
        let cfg = crate::services::auth::AuthConfig::from_env();
        let smtp_url = std::env::var("REACHLOCK_SMTP_URL").ok();
        let from_addr = std::env::var("REACHLOCK_SMTP_FROM")
            .unwrap_or_else(|_| "noreply@reachlock.test".into());
        let email: Box<dyn EmailBackend> = if let Some(url) = &smtp_url {
            match crate::services::email::SmtpEmailBackend::new(url, &from_addr) {
                Ok(b) => Box::new(b),
                Err(e) => {
                    tracing::warn!("SMTP config failed ({e}), falling back to NoopEmailBackend");
                    Box::new(NoopEmailBackend)
                }
            }
        } else {
            let file_dir = std::env::var("REACHLOCK_EMAIL_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("data/emails"));
            Box::new(crate::services::email::FileEmailBackend::new(file_dir))
        };

        // Store selection: Postgres and Redis are independent.
        cfg_if::cfg_if! {
            if #[cfg(feature = "postgres")] {
                if let Some(url) = &config.db_url {
                    return Self::new_pg(url, events, email, cfg, config.auth_required, metrics, prometheus_registry);
                }
            }
        }

        AppState {
            seeds: Box::new(MemorySeedStore::default()),
            sessions: Box::new(MemorySessionStore::default()),
            verify: VerifyService::default(),
            contracts: Box::new(MemoryContractStore::default()),
            llm: LlmService::from_env(),
            events,
            #[cfg(feature = "postgres")]
            pg_pool: None,
            #[cfg(feature = "redis")]
            redis_pool: None,
            player_senders: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            presence: PresenceManager::default(),
            audit: Box::new(MemoryAuditLog::default()),
            prometheus: prometheus_registry,
            health: std::sync::Arc::new(HealthAggregator::default()),
            auth_required: std::sync::atomic::AtomicBool::new(config.auth_required),
            connected: AtomicUsize::new(0),
            voice: VoiceRegistry::default(),
            billing: Box::new(MemorySubscriptionStore::default()),
            library: Box::new(MemoryContractLibrary::default()),
            players: Box::new(MemoryPlayerStore::default()),
            email,
            auth_config: std::sync::Arc::new(std::sync::RwLock::new(cfg)),
            verification_tokens: std::sync::Arc::new(Mutex::new(HashMap::new())),
            reset_tokens: std::sync::Arc::new(Mutex::new(HashMap::new())),
            temp_tokens: TempTokenStore::new(),
            totp_secrets: std::sync::Arc::new(Mutex::new(HashMap::new())),
            totp_pending: std::sync::Arc::new(Mutex::new(HashMap::new())),
            totp_recovery_codes: std::sync::Arc::new(Mutex::new(Vec::new())),
            oauth_flows: std::sync::Arc::new(Mutex::new(HashMap::new())),
            uptime_started: Instant::now(),
            metrics,
        }
    }

    /// Try to initialize Redis stores (called after construction).
    /// Replaces memory stores with Redis-backed ones when Redis is reachable.
    #[cfg(feature = "redis")]
    pub fn try_init_redis(&mut self) {
        let url = match std::env::var("REACHLOCK_REDIS_URL")
            .ok()
            .filter(|s| !s.is_empty())
        {
            Some(u) => u,
            None => return,
        };
        let rt = tokio::runtime::Handle::current();
        let pool = match rt.block_on(crate::services::redis::RedisPool::new(&url)) {
            Ok(p) => std::sync::Arc::new(p),
            Err(e) => {
                tracing::warn!("REACHLOCK_REDIS_URL failed: {e}, falling back to in-memory");
                return;
            }
        };
        let pool_clone = pool.clone();
        self.sessions = Box::new(crate::services::redis::RedisSessionStore::new(pool_clone));
        self.llm.limiter = Box::new(crate::services::redis::RedisRateLimiter::new(pool.clone()));
        self.redis_pool = Some(pool);
        tracing::info!("Redis stores active");
    }

    /// Construct all stores from Postgres. Only available with the `postgres` feature.
    #[cfg(feature = "postgres")]
    fn new_pg(
        url: &str,
        events: broadcast::Sender<ServerMessage>,
        email: Box<dyn EmailBackend>,
        cfg: crate::services::auth::AuthConfig,
        auth_required: bool,
        metrics: std::sync::Arc<crate::services::metrics::ServerMetrics>,
        prometheus_registry: prometheus::Registry,
    ) -> Self {
        use crate::services::auth::pg::{PgPlayerStore, PgSessionStore};
        use crate::services::seed::pg::PgSeedStore;

        let rt = tokio::runtime::Handle::current();
        let pool = rt.block_on(async {
            let pool = sqlx::PgPool::connect(url)
                .await
                .expect("REACHLOCK_DB: connect failed");
            sqlx::migrate!("./migrations")
                .run(&pool)
                .await
                .expect("REACHLOCK_DB: migration failed");
            tracing::info!("Postgres stores active — migrations applied");
            pool
        });

        AppState {
            seeds: Box::new(PgSeedStore::new(pool.clone())),
            sessions: Box::new(PgSessionStore::new(pool.clone())),
            verify: VerifyService::default(),
            contracts: Box::new(MemoryContractStore::default()),
            llm: LlmService::from_env(),
            events,
            pg_pool: Some(pool.clone()),
            #[cfg(feature = "redis")]
            redis_pool: None,
            player_senders: std::sync::Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            presence: PresenceManager::default(),
            audit: Box::new(MemoryAuditLog::default()),
            prometheus: prometheus_registry,
            health: std::sync::Arc::new(HealthAggregator::default()),
            auth_required: std::sync::atomic::AtomicBool::new(auth_required),
            connected: AtomicUsize::new(0),
            voice: VoiceRegistry::default(),
            billing: Box::new(MemorySubscriptionStore::default()),
            library: Box::new(MemoryContractLibrary::default()),
            players: Box::new(PgPlayerStore::new(pool)),
            email,
            auth_config: std::sync::Arc::new(std::sync::RwLock::new(cfg)),
            verification_tokens: std::sync::Arc::new(Mutex::new(HashMap::new())),
            reset_tokens: std::sync::Arc::new(Mutex::new(HashMap::new())),
            temp_tokens: TempTokenStore::new(),
            totp_secrets: std::sync::Arc::new(Mutex::new(HashMap::new())),
            totp_pending: std::sync::Arc::new(Mutex::new(HashMap::new())),
            totp_recovery_codes: std::sync::Arc::new(Mutex::new(Vec::new())),
            oauth_flows: std::sync::Arc::new(Mutex::new(HashMap::new())),
            uptime_started: Instant::now(),
            metrics,
        }
    }

    pub fn connected_count(&self) -> usize {
        self.connected.load(Ordering::Relaxed)
    }

    pub(crate) fn session_started(&self) {
        self.connected.fetch_add(1, Ordering::Relaxed);
        self.metrics.connections_total.inc();
    }

    pub(crate) fn session_ended(&self) {
        self.connected.fetch_sub(1, Ordering::Relaxed);
    }
}

/// S51: shared error type for auth routes — (status, json_body).
/// Implement IntoResponse so Axum handlers can return `Result<_, AppError>`.
pub struct AppError {
    pub status: u16,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(serde_json::json!({"error": self.message}))).into_response()
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let admin_routes = admin::admin_routes();

    Router::new()
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler))
        // S51 auth endpoints
        .route("/auth/register", post(register_handler))
        .route("/auth/login", post(login_handler))
        .route("/auth/logout", post(logout_handler))
        .route("/auth/verify-email", post(verify_email_handler))
        .route(
            "/auth/resend-verification",
            post(resend_verification_handler),
        )
        .route("/auth/forgot-password", post(forgot_password_handler))
        .route("/auth/reset-password", post(reset_password_handler))
        .route("/auth/delete-account", post(delete_account_handler))
        .route("/auth/cancel-deletion", post(cancel_deletion_handler))
        .route("/auth/2fa/enable", post(tfa_enable_handler))
        .route("/auth/2fa/verify", post(tfa_verify_handler))
        .route("/auth/2fa/disable", post(tfa_disable_handler))
        .route("/auth/2fa/challenge", post(tfa_challenge_handler))
        .route(
            "/auth/oauth/google/device",
            post(oauth_google_device_handler),
        )
        .route(
            "/auth/oauth/github/device",
            post(oauth_github_device_handler),
        )
        .route("/auth/oauth/token", post(oauth_token_handler))
        // S57: seed discovery via HTTP
        .route("/seed/discover", post(seed_discover))
        // S57: content overrides for a system (in-memory stub)
        .route("/content/system/{system_id}", get(content_system))
        // S56: content file publish endpoint
        .route("/content/publish", post(content_publish_handler))
        // S51 dev auth (preserved)
        .route("/auth/dev", post(auth_dev))
        .route("/byok", post(byok_register))
        .route("/ws", any(handler::upgrade))
        // S28: Stripe webhook (no auth — signed by Stripe).
        .route("/stripe/webhook", post(stripe_webhook_handler))
        // S28: billing endpoints (bearer token auth).
        .route("/billing/checkout", post(billing_checkout))
        .route("/billing/portal", post(billing_portal))
        .route(
            "/billing/entitlement-token",
            post(billing_entitlement_token),
        )
        .merge(admin_routes)
        .layer(request_body_limit())
        .layer(cors_layer())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

/// Cap request bodies. Axum's default applies only to `Json`-extracted
/// handlers; this covers the whole surface.
fn request_body_limit() -> tower_http::limit::RequestBodyLimitLayer {
    const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;
    let max = std::env::var("REACHLOCK_MAX_BODY_BYTES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_BODY_BYTES);
    tower_http::limit::RequestBodyLimitLayer::new(max)
}

/// CORS policy.
///
/// Origins come from `REACHLOCK_ALLOWED_ORIGINS` (comma-separated); unset
/// means same-origin only, which is the right default now that the only
/// clients are native. Kept because the admin/ops surface and any future
/// browser-based tooling are served over HTTP from another origin. A literal
/// `*` is honoured but must never be combined with credentials.
fn cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};
    let raw = std::env::var("REACHLOCK_ALLOWED_ORIGINS").unwrap_or_default();
    let origins: Vec<&str> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let base = CorsLayer::new()
        .allow_methods(Any)
        .allow_headers(Any)
        .max_age(std::time::Duration::from_secs(3600));

    if origins.is_empty() {
        // Same-origin only: no Access-Control-Allow-Origin is emitted.
        return CorsLayer::new();
    }
    if origins.contains(&"*") {
        return base.allow_origin(Any);
    }
    let parsed: Vec<axum::http::HeaderValue> = origins
        .iter()
        .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
        .collect();
    base.allow_origin(parsed)
}

/// `GET /metrics` — Prometheus text exposition (S26/S73).
async fn metrics_handler(State(state): State<Arc<AppState>>) -> String {
    // Update uptime gauge before rendering.
    state
        .metrics
        .uptime_seconds
        .set(state.uptime_started.elapsed().as_secs_f64());
    state
        .metrics
        .connections_active
        .set(state.connected_count() as f64);
    use prometheus::TextEncoder;
    let encoder = TextEncoder::new();
    let mut buffer = String::new();
    encoder
        .encode_utf8(&state.prometheus.gather(), &mut buffer)
        .unwrap_or_default();
    buffer
}

/// S26/S73: aggregate health check across all backends.
/// Returns uptime, connected players, DB pool status, and per-check status.
async fn health_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    let agg = state.health.aggregate();
    let uptime_secs = state.uptime_started.elapsed().as_secs();
    let connected_players = state.connected_count();

    let mut body = serde_json::json!({
        "uptime_secs": uptime_secs,
        "connected_players": connected_players,
        "status": agg.status,
        "checks": agg.checks,
    });

    #[cfg(feature = "postgres")]
    if let Some(ref pool) = state.pg_pool {
        let pool_status = {
            let opts = pool.options();
            serde_json::json!({
                "connections_active": opts.min_connections,
                "connections_idle": 0,
                "connections_max": opts.max_connections,
            })
        };
        body["db"] = pool_status;
    }

    #[cfg(not(feature = "postgres"))]
    {
        body["db"] = serde_json::json!({
            "state": "in_memory",
        });
    }

    if params.get("verbose").map(|s| s == "true").unwrap_or(false) {
        let active_sessions = state.sessions.active_sessions();
        body["active_sessions"] = serde_json::json!(active_sessions);
    }

    let code = if agg.status == "ok" {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body))
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

/// `POST /seed/discover { universe, system_id, tentative_seed }` — HTTP seed
/// discovery (S57). Returns the canonical seed for the (universe, system) pair.
async fn seed_discover(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let universe_str = body["universe"].as_str().ok_or_else(|| AppError {
        status: 400,
        message: "missing universe".into(),
    })?;
    let universe: UniverseTier = universe_str.parse().map_err(|e: String| AppError {
        status: 400,
        message: format!("bad universe tier: {e}"),
    })?;
    let system_id_str = body["system_id"].as_str().ok_or_else(|| AppError {
        status: 400,
        message: "missing system_id".into(),
    })?;
    let tentative_seed = body["tentative_seed"].as_u64().ok_or_else(|| AppError {
        status: 400,
        message: "missing or invalid tentative_seed".into(),
    })?;
    let system_id = SystemId(system_id_str.to_string());
    let seed = Seed::new(tentative_seed);
    // HTTP endpoint has no player context, so discoverer is None.
    let result = state.seeds.discover(universe, &system_id, seed, None);
    Ok(Json(serde_json::json!({
        "canonical_seed": result.canonical_seed.value(),
        "diffs": result.diffs,
        "you_discovered": result.you_discovered,
        "discoverer_name": result.discoverer_name,
        "discovered_at": result.discovered_at,
    })))
}

/// `GET /content/system/{system_id}` — in-memory content overrides stub (S57).
/// Returns an empty array; real content overrides will be added later.
async fn content_system(
    Path(system_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Json<serde_json::Value> {
    // system_id and ?universe= captured for future use
    let _ = (system_id, query);
    Json(serde_json::json!([]))
}

/// `POST /content/publish` — accept an authored content override (S56).
/// Validates the JSON body as a `ContentFile`, logs the publish, and returns
/// a mock content_override_id. Real persistence will replace the mock when
/// the PgContentStore is wired.
async fn content_publish_handler(
    state: axum::extract::State<std::sync::Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(content): Json<ContentFile>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Require auth
    if resolve_bearer_token(&headers, &state).is_none() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "unauthorized"})),
        ));
    }
    tracing::info!("content published: {}", content.id);
    Ok(Json(serde_json::json!({
        "content_override_id": format!("mock-{}", content.id),
        "published": true,
    })))
}

/// `POST /auth/dev { username, universe? }` — dev-only token issuance.
/// Disabled when REACHLOCK_AUTH=1.
async fn auth_dev(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DevLoginRequest>,
) -> Result<Json<DevLoginResponse>, AppError> {
    crate::services::auth::dev_login(State(state), Json(req)).await
}

// S51 auth handler wrappers -------------------------------------------------

async fn register_handler(
    State(state): State<Arc<AppState>>,
    peer: crate::services::auth::PeerAddr,
    headers: axum::http::HeaderMap,
    Json(body): Json<crate::services::auth::RegisterRequest>,
) -> Result<Json<crate::services::auth::RegisterResponse>, AppError> {
    crate::services::auth::register(State(state), peer, headers, Json(body)).await
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    peer: crate::services::auth::PeerAddr,
    headers: axum::http::HeaderMap,
    Json(body): Json<crate::services::auth::LoginRequest>,
) -> Result<Json<crate::services::auth::LoginResponse>, AppError> {
    crate::services::auth::login(State(state), peer, headers, Json(body)).await
}

async fn logout_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::http::StatusCode, AppError> {
    crate::services::auth::logout(State(state), headers).await
}

async fn verify_email_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<crate::services::auth::VerifyEmailRequest>,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::services::auth::verify_email(State(state), Json(body)).await
}

async fn resend_verification_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::services::auth::resend_verification(State(state), headers).await
}

async fn forgot_password_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<crate::services::auth::ForgotPasswordRequest>,
) -> axum::Json<serde_json::Value> {
    crate::services::auth::forgot_password(State(state), Json(body)).await
}

async fn reset_password_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<crate::services::auth::ResetPasswordRequest>,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::services::auth::reset_password(State(state), Json(body)).await
}

async fn delete_account_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<crate::services::auth::DeleteAccountRequest>,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::services::auth::delete_account(State(state), headers, Json(body)).await
}

async fn cancel_deletion_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::services::auth::cancel_deletion(State(state), headers).await
}

async fn tfa_enable_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<crate::services::auth::TfaEnableResponse>, AppError> {
    crate::services::auth::tfa_enable(State(state), headers).await
}

async fn tfa_verify_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<crate::services::auth::TfaVerifyRequest>,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::services::auth::tfa_verify(State(state), headers, Json(body)).await
}

async fn tfa_disable_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<crate::services::auth::TfaDisableRequest>,
) -> Result<axum::Json<serde_json::Value>, AppError> {
    crate::services::auth::tfa_disable(State(state), headers, Json(body)).await
}

async fn tfa_challenge_handler(
    State(state): State<Arc<AppState>>,
    peer: crate::services::auth::PeerAddr,
    headers: axum::http::HeaderMap,
    Json(body): Json<crate::services::auth::TfaChallengeRequest>,
) -> Result<axum::Json<crate::services::auth::LoginResponse>, AppError> {
    crate::services::auth::tfa_challenge(State(state), peer, headers, Json(body)).await
}

async fn oauth_google_device_handler(
    State(state): State<Arc<AppState>>,
) -> Result<axum::Json<crate::services::auth::OAuthDeviceResponse>, AppError> {
    crate::services::auth::oauth_google_device(State(state)).await
}

async fn oauth_github_device_handler(
    State(state): State<Arc<AppState>>,
) -> Result<axum::Json<crate::services::auth::OAuthDeviceResponse>, AppError> {
    crate::services::auth::oauth_github_device(State(state)).await
}

async fn oauth_token_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<crate::services::auth::OAuthTokenRequest>,
) -> Result<axum::Json<crate::services::auth::OAuthTokenResponse>, AppError> {
    crate::services::auth::oauth_token(State(state), Json(body)).await
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
            state
                .sessions
                .resolve(token)
                .map(|info| info.player_id.clone())
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
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "bad tier"})),
        )
    })?;

    match create_checkout_session(&player_id, tier).await {
        Ok(url) => Ok(Json(serde_json::json!({"url": url}))),
        Err(e) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e})),
        )),
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
        Err(e) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": e})),
        )),
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
        Err(e) => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": e})),
            ))
        }
    };
    Ok(Json(serde_json::to_value(&token).unwrap_or_default()))
}
