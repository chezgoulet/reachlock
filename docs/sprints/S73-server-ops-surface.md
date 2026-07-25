# S73 — Server Ops Surface

**Spec:** New (infrastructure) · **Wave E (shared world & distribution)** · **Depends on:** S26 (admin API, metrics, health checks), S66 (auth hardening)

## Outcome

An operator can administer the server from a browser: a live dashboard shows connected players, system health, and configuration at a glance. The server shuts down gracefully under SIGTERM — flushing pending writes, draining broadcast channels, and closing connections cleanly. A Prometheus `/metrics` endpoint exports counters for monitoring infrastructure. A `/health` endpoint returns structured dependency status for load balancers and Docker orchestrators. Log level can be changed at runtime without restart — reducing the debug cycle from "restart and reproduce" to "flip a switch."

## Context

- S26 built the Admin API skeleton (auth key, player CRUD, universe listing, tick trigger, audit log) and the health check trait. S66 hardened auth with persistent stores and rate limiters. What's missing is an operator-friendly surface on top of that skeleton.
- The server currently logs at the level set by `RUST_LOG` at startup. Changing log level for a production incident investigation requires editing the env var and restarting — losing the ephemeral context that caused the issue.
- Graceful shutdown exists only as a tokio Ctrl+C handler that drops everything. When the server restarts for a deploy, in-flight seed discoveries that were mid-verify can be lost, and broadcast channel receivers get dropped without notice.
- Prometheus metrics exist as a raw `GET /metrics` endpoint from S26, but it only exports LLM latency histograms and session counts. It needs the full set of counters and gauges for production monitoring.
- S26's `GET /health` returns an aggregate status. It needs uptime, connected player count, DB pool status, and per-dependency detail out of the box — the `?verbose` flag is documented but the JSON body is sparse.

## Freeze first

### Admin dashboard route (`/admin/dashboard`)

Returns an HTML page (server-rendered) at `GET /admin/dashboard`. Authenticated with the same `Authorization: Admin <key>` header as the rest of the admin API, but served via a browser — the browser sends the key as a query param `?key=<key>` on first load, which the server sets as a session cookie for subsequent requests.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub uptime_secs: u64,
    pub connected_players: usize,
    pub players_by_universe: Vec<(String, usize)>,
    pub db_pool_status: DbPoolStatus,
    pub redis_status: Option<RedisStatus>,
    pub active_sessions: usize,
    pub active_contracts: usize,
    pub admin_key_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbPoolStatus {
    pub connections_active: u32,
    pub connections_idle: u32,
    pub connections_max: u32,
}
```

Wire tests: `GET /admin/dashboard?key=correct` returns 200 with HTML content-type; `GET /admin/dashboard` (no key) returns 401; `GET /admin/dashboard?key=wrong` returns 401.

### Graceful shutdown trait

```rust
pub trait GracefulShutdown: Send + Sync {
    fn shutdown(&self);
    fn name(&self) -> &str;
}

pub struct ShutdownCoordinator {
    stages: Vec<Box<dyn GracefulShutdown>>,
}
```

Each store backend, broadcast channel, and connection pool implements `GracefulShutdown`. The coordinator runs them in order: flush stores → drain broadcasts → close WS connections → drop DB pool. The process has 30 seconds before SIGKILL (configurable via `REACHLOCK_SHUTDOWN_TIMEOUT_SECS`).

### Metrics expansions

```rust
pub struct ServerMetrics {
    pub connections_active: Gauge,         // reachlock_connections_active
    pub connections_total: Counter,        // reachlock_connections_total
    pub messages_sent: Counter,            // reachlock_messages_sent_total{type}
    pub messages_received: Counter,        // reachlock_messages_received_total{type}
    pub db_pool_connections: Gauge,        // reachlock_db_pool_connections{state="active|idle"}
    pub tick_duration: Histogram,          // reachlock_tick_duration_seconds
    pub ws_message_size: Histogram,        // reachlock_ws_message_size_bytes
    pub uptime_seconds: Gauge,             // reachlock_uptime_seconds
}
```

### Log level endpoint

```rust
pub struct LogLevelEntry {
    pub target: Option<String>,  // None = root, Some("reachlock_server::ws") = module
    pub level: String,           // "trace", "debug", "info", "warn", "error"
}

// POST /admin/log-level — set a specific target's log level
// Body: { "target": null, "level": "debug" } — sets root to debug
// GET /admin/log-level — returns all override targets and their levels
```

Uses `tracing_subscriber::reload::Handle` to change filter levels at runtime. Only works with the `fmt` subscriber — the OTLP exporter's filter is independent.

## Deliverables

### 1. Admin HTML dashboard (`ws/admin.rs` or `services/dashboard.rs`)

- [ ] `GET /admin/dashboard` — server-rendered HTML page. Browser auth via `?key=` query param on first load → server sets a signed cookie (HMAC with admin key) for subsequent requests. No persistent sessions — the cookie expires in 5 minutes and is re-signed on each dashboard page load.
- [ ] Dashboard layout: top row of stat cards (connected players, uptime, active sessions, active contracts). Middle section: per-universe player count table. Bottom section: per-dependency health status (colored dot: green/yellow/red), DB pool active/idle/max, Redis PING latency.
- [ ] Dashboard auto-refreshes via a `refresh=N` meta tag or a simple `setInterval(fetch, 15000)` JS snippet — no JS framework, just inline `<script>` in the HTML.
- [ ] Styling: inline CSS in the HTML response. No external assets. Dark theme with green/amber/red status indicators.
- [ ] Navigation links in dashboard header: "/dashboard", "/admin/players", "/admin/universes", "/admin/audit" — the existing admin API routes become links from the dashboard.

### 2. Graceful shutdown (`services/shutdown.rs`)

- [ ] `ShutdownCoordinator` — ordered list of `GracefulShutdown` implementations. Signal handler (tokio::signal::unix::signal(SIGTERM) + SIGINT) triggers the coordinator.
- [ ] `GracefulShutdown` impl for each store that has flushable state: `MemorySeedStore` (flush pending discoveries — if any exist, wait up to 2 seconds for the DB write), `MemoryContractStore` (same), `MemoryContractLibrary` (same). Stores with no mutable state (read-only caches) are no-ops.
- [ ] Broadcast channel drain: for every `broadcast::Sender` on `AppState`, call `receiver_count()` and log the count, then drop the sender (waiting receivers get `RecvError::Closed`). Active clients get a `ServerMessage::Shutdown` notification before the connection drops — the client shows "Server is restarting — reconnecting..." instead of a silent disconnect.
- [ ] Connection close: iterate `player_senders` and send `ServerMessage::Shutdown`, then close each WebSocket with a 1001 (going away) close code. Wait up to `REACHLOCK_SHUTDOWN_TIMEOUT_SECS` for outstanding eval verifications to complete (check `verify_in_flight` counter — from S66's `PendingEval` tracking — poll every 100ms).
- [ ] Log a structured event at each stage: `shutdown.stage={name} status=ok duration_ms=N`. If a stage times out, log at WARN with the timeout value.
- [ ] Test: send SIGTERM to server process → response is 200 + the server process exits within 10 seconds with exit code 0. (Integration test using `std::process::Command` and `kill`.)

### 3. Metrics expansion (`services/metrics.rs`)

- [ ] Add `ServerMetrics` struct with proper metrics crate types (or the existing `prometheus` crate's `Gauge`, `Counter`, `Histogram`). Register all metrics at server startup.
- [ ] Gauge updates on each connect/disconnect: `connections_active` increments/decrements. `connections_total` increments on connect only.
- [ ] Message counters: increment `reachlock_messages_sent_total{type}` and `reachlock_messages_received_total{type}` in the WS handler's message dispatch loop. Type is the `ServerMessage`/`ClientMessage` discriminant name (e.g., `seed.chart`, `eval.submit`, `llm.call`).
- [ ] `db_pool_connections` gauge: poll the sqlx pool state every 30 seconds and set active/idle/max gauges.
- [ ] Tick duration histogram: the universe tick system records elapsed time per tick cycle. Send to the histogram.
- [ ] WS message size: in the WS handler, record the byte length of each outgoing and incoming message frame.
- [ ] Uptime gauge: set to `UNIX_EPOCH.elapsed().as_secs()` on server start — Prometheus calculates uptime from the time series, but an explicit gauge is more portable.
- [ ] Test: connect a test client → verify `connections_active` increments in the `/metrics` output. Disconnect → verify it decrements. Send a message → verify `messages_received_total` incremented.

### 4. Health check expansion (`services/health.rs`)

- [ ] `GET /health` response includes: `uptime_secs`, `connected_players` count, `db` status object (active/idle/max connections), `redis` status if configured, and the existing `checks` map.
- [ ] `GET /health?verbose=true` returns the same data plus per-universe player counts and the last 5 error timestamps (scraped from the error counters).
- [ ] Wire between `AppState` and the health endpoint: the health handler already receives `Arc<AppState>`. Add `uptime_started: Instant` to `AppState`. Add `connected_players` count from the session store's `active_sessions()` method (S66 added `SessionStore::cleanup_expired` — add `active_sessions: &self -> usize` to the trait).

### 5. Runtime log level (`ws/admin.rs` or `services/log_level.rs`)

- [ ] `GET /admin/log-level` — returns JSON list of current log level overrides: root level + any module-targeted overrides. Format: `{ root: "info", overrides: [{"target": "reachlock_server::ws::handler", "level": "debug"}] }`.
- [ ] `POST /admin/log-level` — accepts `{ target: null, level: "debug" }`. Sets the root log level. Accepts `{ target: "reachlock_server::ws", level: "warn" }` to set a module-specific level. Uses `tracing_subscriber::reload::Handle` to swap the filter.
- [ ] Level validation: only accept "trace", "debug", "info", "warn", "error". Return 400 with valid options on invalid input.
- [ ] The reload handle is stored on AppState as `Option<ReloadHandle>` and wired at startup when the `fmt` subscriber is initialized. If the server uses OTLP-only (no fmt subscriber), log level control is a no-op (returns 501 Not Implemented).
- [ ] Test: set root level to "warn" → a debug log from any module is suppressed → set module-level override to "debug" → that module's debug logs appear.

## Acceptance gates

```
cargo test -p reachlock-server admin::dashboard:: html_returns_200
cargo test -p reachlock-server shutdown:: stages_run_in_order
cargo test -p reachlock-server metrics:: counters_increment
cargo test -p reachlock-server health:: verbose_includes_uptime
cargo test -p reachlock-server log_level:: set_and_verify

# Manual: open http://localhost:40711/admin/dashboard?key=... → see dashboard
# Manual: curl localhost:40711/health → see uptime_secs, connected_players
# Manual: curl -X POST /admin/log-level -H "Authorization: Admin ..." -d '{"level":"debug"}' → logs go debug
# Manual: kill -TERM <pid> → server logs shutdown stages, exits cleanly within 10s
make check
```

## Non-goals

- Grafana dashboards (S26 already states this — the server emits Prometheus metrics; operators configure collection and dashboards)
- Alertmanager rules or PagerDuty integration
- Admin dashboard WebSocket (live-updating player list would be nice but adds complexity — the 15s auto-refresh is sufficient for an ops dashboard)
- Multi-instance admin dashboard (one dashboard per server instance; no aggregation)
- Log tailing / streaming logs from the dashboard (use `kubectl logs` or systemd journal)
- Admin dashboard editing config (read-only view; editing remains via the admin API / CLI)
- Rate limiting on the dashboard page itself (it's admin-only behind the key; rate limiting the admin API from S26 covers it)

## Gotchas

- The admin dashboard HTML must be a single HTTP response with no external CSS/JS/asset dependencies. Inline everything. The operator's browser must work air-gapped — the server is the origin, not a CDN.
- `tracing_subscriber::reload::Handle` requires the subscriber to be a `Layered` with a `Filter` that implements `reload::Interface`. The `fmt` subscriber's filter (env filter) supports reload out of the box. The OTLP layer's filter is separate and does NOT support runtime reload — log level control only affects the fmt subscriber. Document this in the endpoint's response: the level returned is the fmt-subscriber level.
- Graceful shutdown timeout: a single long-running eval verification that takes >30 seconds can exceed the shutdown window. The ShutdownCoordinator logs a warning and moves on — it does not wait indefinitely. The verification in flight continues in the background (the tokio task is not cancelled). Its result is silently dropped.
- Broadcast channel drain: calling `sender.receiver_count()` gives the count at that instant. Between that call and dropping the sender, a new receiver could subscribe. In practice, the server stops accepting new connections before draining broadcasts — the signal handler sets a `shutting_down` flag checked by the WS accept loop. Document that the signal handler must first stop the accept loop, then drain.
- The `DashboardData::connected_players` count comes from `SessionStore::active_sessions()`. Add this method to the trait (S66 didn't add it — it added `cleanup_expired`). The memory impl counts entries where `expires_at > now`. The Pg impl runs `SELECT COUNT(*) FROM sessions WHERE expires_at > NOW()`.
- HMAC-signed cookie for dashboard auth: use a simple HMAC-SHA256 of the admin key + expiry timestamp. The key is the `REACHLOCK_ADMIN_KEY` env var. Expiry is embedded in the cookie value, not the cookie's Max-Age — the server re-signs on each request so the cookie is single-use. This prevents replay if the cookie is intercepted. Simpler approach: skip the cookie entirely and require `?key=` on every dashboard request. The auto-refresh JS appends the key to the URL. Either approach works — pick the simplest.
