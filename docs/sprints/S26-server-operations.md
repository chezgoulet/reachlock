# S26 — Server Operations & Observability

**Spec:** §8 (server architecture), §13 (offline first-class) · **Wave 7 (tooling) ·
Depends on:** S03 (Postgres), S23 (MMO presence)

## Outcome

The server is debuggable at prod scale: every request carries a trace ID
through the handler → LLM proxy → tick → response path; structured logs ship
to an OTLP collector; a real health check probes dependencies; per-universe
metrics tell you which tiers are populated and how fast they're growing; an
Admin API lets operators inspect, moderate, and reconfigure without shell
access; the server degrades gracefully when dependencies fail rather than
crashing. This is the sprint that turns the server from a dev-level service
into a production-grade one.

## Context

- The server already has `tracing` + `tracing-subscriber` in Cargo.toml.
  `GET /metrics` serves a Prometheus histogram for LLM latency only. The
  `health` endpoint returns a static "ok". Broadening these is additive,
  not a rewrite.
- S23 adds Redis-backed sessions and interest scoping. The observability in
  this sprint must cover those paths too.
- All stores are behind traits (`SeedStore`, `SessionStore`, `ContractStore`,
  `RateLimiter`). The health check probes the configured backend through
  those traits — no direct DB/cache access.
- The Admin API needs its own auth (admin API key, not a player token).
  S03's `SessionStore` and auth pattern are the model; admin is a parallel
  path, not a player-auth extension.

## Freeze first

### Trace ID propagation (`src/observability.rs`)

```rust
/// Unique per-request identifier carried through every log and wire hop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

impl TraceId {
    pub fn new() -> Self { /* UUIDv7 */ }
}

/// Extension trait for Spans: inject a TraceId into every log in scope.
pub trait WithTrace {
    fn with_trace(self, id: &TraceId) -> Self;
}
```

The WS handler generates a `TraceId` on connect and passes it into the `Session`;
every spawned task (LLM call, eval verification) inherits it via a `Span`.
Server→client messages include `trace_id` in the JSON envelope so a client
bug report can be correlated to server-side traces.

### Health check trait

```rust
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self) -> HealthStatus;
}

pub enum HealthStatus {
    Ok,
    Degraded { reason: String },
    Down { reason: String },
}
```

Each store backend registers a `HealthCheck` impl. `GET /health` aggregates
all checks, returns 200 only if every dependency is `Ok`. Unhealthy deps
return 503 with a JSON body listing each dep and its status.

### Admin API auth

A separate header-based auth path: `Authorization: Admin <key>` checked
against `REACHLOCK_ADMIN_KEY` (64 hex chars). No session store involvement.
`401` on mismatch; never logged. Admin endpoints live under `/admin/…`.

### Metrics registry

```rust
pub trait Metric: Send + Sync {
    fn name(&self) -> &str;
    fn render_prometheus(&self) -> String;
}

pub struct MetricsRegistry {
    counters: HashMap<String, Box<dyn Metric>>,
    gauges: HashMap<String, Box<dyn Metric>>,
    histograms: HashMap<String, Box<dyn Metric>>,
}
```

`GET /metrics` walks the registry and concatenates all renderings. New
metrics register at init; no per-metric endpoint changes needed.

Wire tests: `TraceId` round-trips through the message envelope; `HealthStatus`
serialization matches the expected 200/503 contract; `GET /metrics` output
is valid Prometheus text (parseable by `promtool check metrics`).

## Deliverables

### 1. Structured observability (`src/observability.rs`)

- [ ] `TraceId` generation on WS connect; propagated into every `tracing::Span`
      spawned from the handler (LLM calls, eval verification, tick event
      forwarding). Server→client messages include `trace_id` in the JSON
      envelope (additive — old clients ignore unknown fields).
- [ ] OpenTelemetry OTLP export behind `REACHLOCK_OTLP_ENDPOINT` env var.
      Feature-gated on `#[cfg(feature = "otlp")]`. Uses `tracing-opentelemetry`
      + `opentelemetry-otlp`. When unset, structured logs go to stdout only.
- [ ] `tracing` level controlled by `RUST_LOG` (already works); add
      `REACHLOCK_LOG_JSON=1` for JSON-structured output (machine-parseable
      in production log aggregators).
- [ ] Message throughput counters: `reachlock_messages_total{type="seed.discover|eval.submit|llm.call|..."}`.
      `reachlock_connections{universe="..."}` gauge.
- [ ] Error rate counters: `reachlock_errors_total{type="parse_error|auth_failure|verification_rejected|llm_failed"}`.
- [ ] Per-universe player count gauge (piggybacks on `session_started`/`session_ended`
      — add universe scoping).

### 2. Health check (`src/services/health.rs`)

- [ ] `HealthCheck` trait + `HealthStatus` enum. Default global health check
      aggregates all registered checks.
- [ ] Postgres health check: `SELECT 1` via sqlx (behind the `postgres` feature
      flag). Returns `Degraded` if pool is exhausted, `Down` if unreachable.
- [ ] Redis health check (behind `REACHLOCK_REDIS`): `PING`. Same Degraded/Down
      semantics.
- [ ] Seed store health check: probes the configured `SeedStore` backend
      (memory always `Ok`; Postgres delegates to the DB check).
- [ ] `GET /health` → JSON `{status: "ok"|"degraded"|"down", checks: {...}}`.
      HTTP 200 / 503 based on aggregate. Detail verbosity toggleable via
      `?verbose=true`.
- [ ] Test: bring up server with no DB → `/health` shows seed store as
      `degraded` with the Postgres check reason.

### 3. Metrics dashboard (`src/services/metrics.rs` expansion)

- [ ] `MetricsRegistry` — replaces the standalone `LatencyHistogram` resource.
      The histogram registers itself; new metrics add to the registry, not
      to manual wire-up in `router()`.
- [ ] `GET /metrics` renders every registered metric. Format: Prometheus
      text exposition, same as the existing histogram output.
- [ ] New histograms: `reachlock_ws_message_bytes` (with size buckets),
      `reachlock_tick_duration_seconds`.
- [ ] Counters: `reachlock_sessions_total`, `reachlock_sessions_active`,
      `reachlock_llm_calls_total{universe,tier}`, `reachlock_content_publishes_total`.
- [ ] Test: round-trip the metrics output through `promtool check metrics`;
      assert every counter increments under load in an integration test.

### 4. Admin API (`src/ws/admin.rs` or separate router)

- [ ] Admin auth: `Authorization: Admin <key>` header, checked against
      `REACHLOCK_ADMIN_KEY`. `401` on missing/wrong key. Key is never logged.
- [ ] `GET /admin/players/:id` — player info: session status, universe, last seen,
      reputation summary, active contracts count.
- [ ] `POST /admin/players/:id/ban` + `POST /admin/players/:id/unban` —
      adds/removes player from an in-memory ban list. Banned players get
      `403` on WS connect with a ban reason. Persisted to a `bans` table
      behind `postgres` feature.
- [ ] `GET /admin/universes` — lists active universes with player counts and
      tick status.
- [ ] `POST /admin/tick/trigger` — forces one tick immediately (bypasses the
      interval timer). Returns the events produced.
- [ ] `POST /admin/content/purge` — removes a content override by system_id +
      object_id. Admin-only; logged immutably (audit log).
- [ ] Rate limiting on admin endpoints: 10 req/s per admin key (token bucket,
      same pattern as S14's player limiter).

### 5. Audit logging (`src/services/audit.rs`)

- [ ] `AuditLog` trait + `MemoryAuditLog` impl. Every admin action appends a
      record: `{timestamp, admin_key_hash, action, target, detail}`. Immutable
      append-only (Vec push, never remove).
- [ ] Postgres `audit_log` table behind the `postgres` feature.
- [ ] `GET /admin/audit?limit=100` — returns recent audit entries (admin-only).
- [ ] Test: perform a ban → query audit log → ban record present with correct
      target and admin key hash (not the raw key).

### 6. Graceful degradation (`src/services/degradation.rs`)

- [ ] On startup: probe each configured backend. Build `AppState` with the
      backends that responded. Log a warning for each missing backend and
      substitute the memory implementation. Never crash on missing Postgres.
- [ ] Runtime reconnection: a background task pings downed backends every 30s.
      When a backend comes back, log at info and switch the store trait's
      active backend. Sessions use the degraded store until the switch.
- [ ] `/health` reflects current degradation state.
- [ ] Test: start server with unreachable Postgres URL → server starts with
      memory stores and `degraded` health; fix the URL → health becomes `ok`
      within 30s.

### 7. WS message rate limiting + size caps

- [ ] Per-connection message rate limit: 60 msg/s burst, 20 msg/s sustained
      (token bucket, same pattern as `MemoryRateLimiter`). Exceeding → close
      the connection with a `rate_limited` close reason.
- [ ] Max message size: 64 KB per frame. Larger frames → close with
      `message_too_large`. Configurable via `REACHLOCK_MAX_MESSAGE_BYTES`.
- [ ] Test: flood a test client → connection dropped with the expected reason.

## Acceptance gates

```
cargo test -p reachlock-server observability:: health:: metrics:: admin:: audit:: degradation::
# Admin API: curl -H "Authorization: Admin $KEY" localhost:40711/admin/universes → JSON
# Health: curl localhost:40711/health → 200 when DB is up, 503 when DB is down
# Metrics: curl localhost:40711/metrics | promtool check metrics
make check
```

## Non-goals

- Grafana/Prometheus deployment (infra, not code — the server emits the format;
  operators configure collection)
- Real account security (S28 handles Stripe auth; admin keys are a boot-level
  env var)
- Automated alerting rules (operators define those against the metrics)
- Dashboard UI (metrics are for machines; a separate Grafana config is the
  human-readable layer)
- Distributed tracing across server instances (single-instance scoped; the
  OTLP export is the foundation for multi-instance later)
- Redis cluster health (single Redis instance; Sentinel/Cluster is infra)

## Gotchas

- The trace ID must be injected into `ServerMessage` JSON without breaking
  existing clients. Use an `Option<String>` field (`trace_id`) that old
  clients ignore — the envelope is additive.
- `tracing-opentelemetry` pulls in a non-trivial dep tree. Feature-gate it
  behind `otlp` so the default build stays lean and the WASM build doesn't
  see it.
- The health check aggregator must be `Send + Sync` for `Arc<AppState>`.
  Trait objects in the registry use `Arc<dyn HealthCheck>`.
- Admin key validation must be constant-time (use `subtle` or compare hashes,
  not raw strings) to avoid timing side-channels. The key itself is never
  logged — log a truncated hash.
- `GET /admin/audit` is paginated by `limit` only — no offset-based pagination
  on an append-only log (offset drifts as entries are added). Cursor-based
  pagination is the correct pattern but adds complexity; document that
  `limit=100` is an admin convenience, not a pagination API.
