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
        // S73: admin dashboard and log level.
        .route("/admin/dashboard", get(admin_dashboard))
        .route("/admin/log-level", get(admin_get_log_level))
        .route("/admin/log-level", post(admin_set_log_level))
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
    if provided_hash
        .as_bytes()
        .ct_eq(expected_hash.as_bytes())
        .unwrap_u8()
        != 1
    {
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
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "player not found"})),
        ),
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
    let per_page: u32 = params
        .get("per_page")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
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
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid role"})),
        );
    }
    state.players.set_role(&id, role);
    (
        StatusCode::OK,
        Json(serde_json::json!({"role_updated": true})),
    )
}

async fn admin_get_auth_config(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = require_admin(&headers).await {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let cfg = state.auth_config.read().unwrap();
    (
        StatusCode::OK,
        Json(serde_json::to_value(&*cfg).unwrap_or_default()),
    )
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
        Json(serde_json::json!({"tick": "triggered", "implemented": false})),
    )
}

async fn admin_content_purge(
    State(_state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    if let Err(status) = verify_admin(&headers) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"purged": true, "implemented": false})),
    )
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

// S73: Admin dashboard -------------------------------------------------------

fn render_dashboard_html(state: &AppState) -> String {
    let uptime = state.uptime_started.elapsed().as_secs();
    let connected = state.connected_count();
    let active_sessions = state.sessions.active_sessions();
    let uptime_hms = format!(
        "{:02}:{:02}:{:02}",
        uptime / 3600,
        (uptime % 3600) / 60,
        uptime % 60
    );

    let health = state.health.aggregate();
    let health_color = |s: &str| -> &str {
        match s {
            "ok" => "#40c040",
            "degraded" => "#c0a000",
            _ => "#c04040",
        }
    };
    let health_rows: String = health
        .checks
        .iter()
        .map(|c| {
            let color = health_color(&format!("{:?}", c.status));
            let status_text = match &c.status {
                crate::services::health::HealthStatus::Ok => "ok".into(),
                crate::services::health::HealthStatus::Degraded { reason } => {
                    format!("degraded: {reason}")
                }
                crate::services::health::HealthStatus::Down { reason } => format!("down: {reason}"),
            };
            format!(
                "<tr><td>{}</td><td style=\"color:{}\">{}</td></tr>",
                c.name, color, status_text
            )
        })
        .collect::<Vec<_>>()
        .join("");

    let db_status = "memory (no pool info)";
    let admin_key_configured = admin_key().is_some();

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><meta http-equiv="refresh" content="15">
<title>ReachLock Admin</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ background: #0d1117; color: #c9d1d9; font-family: -apple-system,BlinkMacSystemFont,sans-serif; padding: 2rem; }}
  h1 {{ color: #58a6ff; margin-bottom: 1.5rem; font-size: 1.5rem; }}
  nav {{ margin-bottom: 2rem; }}
  nav a {{ color: #58a6ff; text-decoration: none; margin-right: 1rem; font-size: 0.9rem; }}
  nav a:hover {{ text-decoration: underline; }}
  .cards {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 1rem; margin-bottom: 2rem; }}
  .card {{ background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 1rem; }}
  .card h3 {{ color: #8b949e; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.5rem; }}
  .card .value {{ font-size: 1.8rem; font-weight: 600; color: #f0f6fc; }}
  table {{ width: 100%; border-collapse: collapse; margin-bottom: 2rem; }}
  th, td {{ text-align: left; padding: 0.5rem; border-bottom: 1px solid #30363d; }}
  th {{ color: #8b949e; font-size: 0.8rem; text-transform: uppercase; }}
  .health-dot {{ display: inline-block; width: 10px; height: 10px; border-radius: 50%; margin-right: 0.5rem; }}
  section {{ margin-bottom: 2rem; }}
  section h2 {{ color: #58a6ff; font-size: 1.1rem; margin-bottom: 0.75rem; }}
</style>
</head>
<body>
<h1>ReachLock Dashboard</h1>
<nav>
  <a href="/admin/dashboard">Dashboard</a>
  <a href="/admin/players">Players</a>
  <a href="/admin/universes">Universes</a>
  <a href="/admin/audit">Audit Log</a>
</nav>
<div class="cards">
  <div class="card"><h3>Connected Players</h3><div class="value">{connected}</div></div>
  <div class="card"><h3>Active Sessions</h3><div class="value">{active_sessions}</div></div>
  <div class="card"><h3>Uptime</h3><div class="value">{uptime_hms}</div></div>
  <div class="card"><h3>Admin Key</h3><div class="value">{admin_status}</div></div>
</div>
<section>
  <h2>Database</h2>
  <p>{db_status}</p>
</section>
<section>
  <h2>Health Checks</h2>
  <table><thead><tr><th>Check</th><th>Status</th></tr></thead><tbody>{health_rows}</tbody></table>
</section>
<script>
  setTimeout(function(){{ window.location.reload(); }}, 15000);
</script>
</body>
</html>"#,
        connected = connected,
        active_sessions = active_sessions,
        uptime_hms = uptime_hms,
        admin_status = if admin_key_configured {
            "configured"
        } else {
            "not set"
        },
        db_status = db_status,
        health_rows = health_rows,
    )
}

/// The dashboard authenticates by header only.
///
/// It used to accept `?key=<admin key>` "for browser convenience". A secret in
/// a query string leaks into server access logs, proxy logs, browser history,
/// and the `Referer` header of every outbound link on the page — so a single
/// dashboard visit could scatter the key across several systems that were
/// never meant to hold it. Browser access is still possible; it just needs a
/// header (an extension, `curl`, or an authenticating reverse proxy).
async fn admin_dashboard(
    headers: axum::http::HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if params.contains_key("key") {
        tracing::warn!(
            "admin dashboard called with a ?key= query parameter — refused. \
             Query strings are logged; send `Authorization: Admin <key>`. \
             Treat the key used here as compromised and rotate it."
        );
        return (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            serde_json::json!({
                "error": "admin key must be sent in the Authorization header, \
                          not a query parameter",
                "hint": "Authorization: Admin <key>"
            })
            .to_string(),
        );
    }

    let authorized = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Admin "))
        .is_some_and(check_admin_key);

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            serde_json::json!({"error": "unauthorized"}).to_string(),
        );
    }

    let html = render_dashboard_html(&state);
    (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
        html,
    )
}

/// Check admin key (not timing-safe — acceptable for dashboard page).
fn check_admin_key(provided: &str) -> bool {
    let Some(expected) = admin_key() else {
        return false;
    };
    let provided_hash = sha256::digest(provided.as_bytes());
    let expected_hash = sha256::digest(expected.as_bytes());
    provided_hash
        .as_bytes()
        .ct_eq(expected_hash.as_bytes())
        .unwrap_u8()
        == 1
}

// S73: Log level endpoints ---------------------------------------------------

use std::sync::OnceLock;

use tracing_subscriber::reload;
use tracing_subscriber::EnvFilter;

static RELOAD_HANDLE: OnceLock<reload::Handle<EnvFilter, tracing_subscriber::Registry>> =
    OnceLock::new();

/// Register the reload handle at startup. Called from main().
pub fn init_reload_handle(handle: reload::Handle<EnvFilter, tracing_subscriber::Registry>) {
    let _ = RELOAD_HANDLE.set(handle);
}

fn get_reload_handle() -> Option<&'static reload::Handle<EnvFilter, tracing_subscriber::Registry>> {
    RELOAD_HANDLE.get()
}

#[derive(serde::Deserialize)]
struct LogLevelBody {
    target: Option<String>,
    level: String,
}

fn valid_level(s: &str) -> bool {
    matches!(s, "trace" | "debug" | "info" | "warn" | "error")
}

async fn admin_set_log_level(
    headers: axum::http::HeaderMap,
    Json(body): Json<LogLevelBody>,
) -> impl IntoResponse {
    if let Err(status) = verify_admin(&headers) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    if !valid_level(&body.level) {
        return (
            StatusCode::BAD_REQUEST,
            Json(
                serde_json::json!({"error": "invalid level, use one of: trace, debug, info, warn, error"}),
            ),
        );
    }
    let Some(handle) = get_reload_handle() else {
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(
                serde_json::json!({"error": "log level reload not available (no fmt subscriber)"}),
            ),
        );
    };
    let filter = match &body.target {
        Some(target) => EnvFilter::new(format!("{target}={}", body.level)),
        None => EnvFilter::new(body.level.clone()),
    };
    if handle.modify(|f| *f = filter).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "reload failed"})),
        );
    }
    tracing::info!(target = ?body.target, level = %body.level, "log level changed");
    (StatusCode::OK, Json(serde_json::json!({"updated": true})))
}

async fn admin_get_log_level(headers: axum::http::HeaderMap) -> impl IntoResponse {
    if let Err(status) = verify_admin(&headers) {
        return (status, Json(serde_json::json!({"error": "unauthorized"})));
    }
    let current = get_reload_handle()
        .map(|_| serde_json::json!({"root": "unknown", "overrides": []}))
        .unwrap_or_else(
            || serde_json::json!({"root": "unknown", "note": "log level reload not available"}),
        );
    (StatusCode::OK, Json(current))
}

#[cfg(test)]
mod admin_key_transport_tests {
    /// The dashboard must not accept the admin key in the query string.
    ///
    /// Query strings land in server access logs, proxy logs, browser history,
    /// and the `Referer` header of outbound links — so one dashboard visit
    /// could scatter the key across systems that were never meant to hold it.
    /// This asserts the source stays header-only; the handler returns 401 with
    /// a pointer to the header for any request carrying `?key=`.
    #[test]
    fn dashboard_does_not_read_the_key_from_a_query_parameter() {
        let src = include_str!("admin.rs");
        // Find the handler body and confirm it never pulls "key" from params.
        let start = src
            .find("async fn admin_dashboard(")
            .expect("admin_dashboard exists");
        let body = &src[start..];
        let end = body.find("\nasync fn ").unwrap_or(body.len());
        let body = &body[..end];
        assert!(
            !body.contains(r#"params.get("key").cloned()"#),
            "admin_dashboard reads the admin key from a query parameter again"
        );
        assert!(
            body.contains("Authorization"),
            "admin_dashboard should authenticate from the Authorization header"
        );
    }
}
