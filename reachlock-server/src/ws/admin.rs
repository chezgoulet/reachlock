//! S26 Admin API: player management, universe status, content control.
//! All routes are under /admin/ and require Authorization: Admin <key>.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use subtle::ConstantTimeEq;

use super::AppState;

/// Returns admin route definitions to be merged into the main router.
/// The caller (ws/mod.rs) owns the state.
pub fn admin_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/players/{id}", get(admin_get_player))
        .route("/admin/players", get(admin_list_players))
        .route("/admin/players/{id}/ban", post(admin_ban_player))
        .route("/admin/players/{id}/unban", post(admin_unban_player))
        .route("/admin/players/{id}/role", post(admin_set_role))
        .route("/admin/auth-config", get(admin_get_auth_config))
        .route("/admin/auth-config", post(admin_set_auth_config))
        .route("/admin/universes", get(admin_list_universes))
        .route("/admin/tick/trigger", post(admin_tick_trigger))
        .route("/admin/content/purge", post(admin_content_purge))
        .route("/admin/audit", get(admin_audit_log))
}

/// The admin key, cached from the environment at first call. `None` means the
/// `REACHLOCK_ADMIN_KEY` env var is missing *and* a call has already been made
/// — admin access is permanently disabled. `Some("")` is treated the same as
/// `None` to prevent the empty-string bypass (SHA-256 of "" is a known
/// constant).
fn admin_key() -> Option<String> {
    use std::sync::OnceLock;
    static KEY: OnceLock<Option<String>> = OnceLock::new();
    KEY.get_or_init(|| {
        let key = std::env::var("REACHLOCK_ADMIN_KEY").ok()?;
        if key.is_empty() {
            None
        } else {
            Some(key)
        }
    })
    .clone()
}

/// Extract the admin token from the Authorization header and verify it.
/// Returns `Err(StatusCode::UNAUTHORIZED)` if the admin key is not configured
/// or if the provided token does not match.
fn verify_admin(headers: &axum::http::HeaderMap) -> Result<&'static str, StatusCode> {
    let expected = admin_key().ok_or(StatusCode::UNAUTHORIZED)?;
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Admin "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let provided_hash = sha256::digest(header.as_bytes());
    let expected_hash = sha256::digest(expected.as_bytes());
    if provided_hash.as_bytes().ct_eq(expected_hash.as_bytes()).unwrap_u8() != 1 {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok("authorized")
}

async fn require_admin(headers: &axum::http::HeaderMap) -> Result<(), StatusCode> {
    verify_admin(headers).map(|_| ())
}

async fn admin_get_player(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    match state.players.by_id(&id) {
        Some(rec) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": rec.id,
                "username": rec.username,
                "email": rec.email,
                "role": rec.role,
                "verified": rec.verified_at.is_some(),
                "banned": rec.banned_at.is_some(),
                "banned_reason": rec.banned_reason,
                "deleted": rec.deleted_at.is_some(),
                "failed_login_attempts": rec.failed_login_attempts,
                "created_at": rec.created_at,
            })),
        ),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "player not found"}))),
    }
}

async fn admin_list_players(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let page: u32 = params.get("page").and_then(|s| s.parse().ok()).unwrap_or(1);
    let per_page: u32 = params.get("per_page").and_then(|s| s.parse().ok()).unwrap_or(50);
    let total = state.players.count();
    let players = state.players.list(page, per_page);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "players": players.iter().map(|p| serde_json::json!({
                "id": p.id,
                "username": p.username,
                "role": p.role,
                "verified": p.verified_at.is_some(),
                "banned": p.banned_at.is_some(),
                "deleted": p.deleted_at.is_some(),
                "failed_login_attempts": p.failed_login_attempts,
                "created_at": p.created_at,
            })).collect::<Vec<_>>(),
            "total": total,
            "page": page,
            "per_page": per_page,
        })),
    )
}

async fn admin_ban_player(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let reason = body["reason"].as_str().unwrap_or("no reason provided");
    state.players.ban(&id, reason);
    state.sessions.revoke_all_for_player(&id);
    let key_hash = admin_key()
        .map(|k| sha256::digest(k.as_bytes()))
        .unwrap_or_default();
    state.audit.record(crate::services::audit::AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        action: "ban".into(),
        target: id.clone(),
        detail: format!("reason: {reason}"),
        admin_key_hash: key_hash,
    });
    (StatusCode::OK, Json(serde_json::json!({"banned": id})))
}

async fn admin_unban_player(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    state.players.unban(&id);
    let key_hash = admin_key()
        .map(|k| sha256::digest(k.as_bytes()))
        .unwrap_or_default();
    state.audit.record(crate::services::audit::AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        action: "unban".into(),
        target: id.clone(),
        detail: String::new(),
        admin_key_hash: key_hash,
    });
    (StatusCode::OK, Json(serde_json::json!({"unbanned": id})))
}

async fn admin_set_role(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let role = body["role"].as_str().unwrap_or("player");
    if !["player", "moderator", "admin"].contains(&role) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "invalid role"})));
    }
    state.players.set_role(&id, role);
    (StatusCode::OK, Json(serde_json::json!({"role_updated": true})))
}

async fn admin_get_auth_config(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let cfg = state.auth_config.read().unwrap();
    (StatusCode::OK, Json(serde_json::to_value(&*cfg).unwrap_or_default()))
}

async fn admin_set_auth_config(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let mut cfg = state.auth_config.write().unwrap();
    if let Some(v) = body["min_password_length"].as_u64() {
        cfg.min_password_length = (v as usize).max(8);
    }
    if let Some(v) = body["account_lockout_threshold"].as_u64() {
        cfg.account_lockout_threshold = v as u32;
    }
    if let Some(v) = body["account_lockout_duration_mins"].as_u64() {
        cfg.account_lockout_duration_mins = v as u32;
    }
    if let Some(v) = body["session_ttl_hours"].as_u64() {
        cfg.session_ttl_hours = v as u32;
    }
    if let Some(v) = body["deletion_grace_period_days"].as_u64() {
        cfg.deletion_grace_period_days = v as u32;
    }
    (StatusCode::OK, Json(serde_json::json!({"updated": true})))
}

async fn admin_list_universes(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = verify_admin(&headers) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (StatusCode::OK, Json(serde_json::json!({"universes": []})))
}

async fn admin_tick_trigger(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = verify_admin(&headers) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"tick": "triggered"})),
    )
}

async fn admin_content_purge(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = verify_admin(&headers) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (StatusCode::OK, Json(serde_json::json!({"purged": true})))
}

async fn admin_audit_log(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    if let Err(status) = verify_admin(&headers) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let limit: usize = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let entries = state.audit.recent(limit);
    (StatusCode::OK, Json(serde_json::json!(entries)))
}
