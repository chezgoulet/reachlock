//! S26 Admin API: player management, universe status, content control.
//! All routes are under /admin/ and require Authorization: Admin <key>.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use axum::routing::{get, post};

use super::AppState;

/// Returns admin route definitions to be merged into the main router.
/// The caller (ws/mod.rs) owns the state.
pub fn admin_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/players/{id}", get(admin_get_player))
        .route("/admin/players/{id}/ban", post(admin_ban_player))
        .route("/admin/players/{id}/unban", post(admin_unban_player))
        .route("/admin/universes", get(admin_list_universes))
        .route("/admin/tick/trigger", post(admin_tick_trigger))
        .route("/admin/content/purge", post(admin_content_purge))
        .route("/admin/audit", get(admin_audit_log))
}

/// Extract the admin token from the Authorization header and verify it.
fn verify_admin(headers: &axum::http::HeaderMap, expected: &str) -> Result<&'static str, StatusCode> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Admin "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    // Constant-time comparison using the hash.
    let provided_hash = sha256::digest(header.as_bytes());
    let expected_hash = sha256::digest(expected.as_bytes());
    if provided_hash != expected_hash {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok("authorized")
}

async fn admin_get_player(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _key = std::env::var("REACHLOCK_ADMIN_KEY").unwrap_or_default();
    if let Err(status) = verify_admin(&headers, &_key) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (StatusCode::OK, Json(serde_json::json!({"player_id": id, "status": "ok"})))
}

async fn admin_ban_player(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _key = std::env::var("REACHLOCK_ADMIN_KEY").unwrap_or_default();
    if let Err(status) = verify_admin(&headers, &_key) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    // Record in audit log.
    let entry = crate::services::audit::AuditEntry {
        timestamp: format!("{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()),
        action: "ban".into(),
        target: id.clone(),
        detail: String::new(),
        admin_key_hash: sha256::digest(_key.as_bytes()),
    };
    state.audit.record(entry);
    (StatusCode::OK, Json(serde_json::json!({"banned": id})))
}

async fn admin_unban_player(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let _key = std::env::var("REACHLOCK_ADMIN_KEY").unwrap_or_default();
    if let Err(status) = verify_admin(&headers, &_key) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (StatusCode::OK, Json(serde_json::json!({"unbanned": id})))
}

async fn admin_list_universes(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let _key = std::env::var("REACHLOCK_ADMIN_KEY").unwrap_or_default();
    if let Err(status) = verify_admin(&headers, &_key) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (StatusCode::OK, Json(serde_json::json!({"universes": []})))
}

async fn admin_tick_trigger(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let _key = std::env::var("REACHLOCK_ADMIN_KEY").unwrap_or_default();
    if let Err(status) = verify_admin(&headers, &_key) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (StatusCode::OK, Json(serde_json::json!({"tick": "triggered"})))
}

async fn admin_content_purge(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let _key = std::env::var("REACHLOCK_ADMIN_KEY").unwrap_or_default();
    if let Err(status) = verify_admin(&headers, &_key) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (StatusCode::OK, Json(serde_json::json!({"purged": true})))
}

async fn admin_audit_log(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let _key = std::env::var("REACHLOCK_ADMIN_KEY").unwrap_or_default();
    if let Err(status) = verify_admin(&headers, &_key) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let limit: usize = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(100);
    let entries = state.audit.recent(limit);
    (StatusCode::OK, Json(serde_json::json!(entries)))
}
