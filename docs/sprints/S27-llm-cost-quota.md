# S27 — LLM Cost & Quota Management

**Spec:** §7 (multi-universe tiers), §8 (LLM proxy) · **Wave 7 (tooling) ·
Depends on:** S14 (LLM providers), S26 (observability)

## Outcome

The server tracks every LLM call's token usage and estimated cost per player
and per universe, enforces soft quotas with in-fiction feedback ("your crew
is fatigued" rather than "rate limited"), monitors provider health so a
degrading upstream is caught before players notice, and exposes cost reports
so operators can see which tiers and players drive the LLM bill. This is the
sprint that makes the LLM proxy operationally viable at scale.

## Context

- S14's `LlmService::route()` already records latency in a histogram and
  returns structured `LlmError` variants. This sprint adds token counting
  and cost estimation alongside that path — additive, not a rewrite.
- S14's `RateLimiter` trait currently enforces a hard cap (burst then deny).
  This sprint adds a `QuotaManager` layer above it that soft-throttles
  before the hard cap: reduced response quality, longer deliberation windows,
  "crew fatigue" narrative injected into the system prompt.
- S26's `MetricsRegistry` gives us the counters and histograms to publish
  cost metrics. S26's Admin API gives us the endpoint to query cost reports.
- Provider costs vary: BYOK costs $0 (player pays), FairPlay is a local
  model (server electricity, not API cost), Spectrum is a cloud API (real
  money per token). Tracking must distinguish.
- All cost tracking is additive to the existing `LlmService` path. Zero
  changes to the WS handler or client protocol — this is server-internal.

## Freeze first

### Token usage record (`src/services/cost.rs`)

```rust
pub struct LlmCallRecord {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub player_id: String,
    pub universe: UniverseTier,
    pub provider: ProviderKind,     // FairPlay, Spectrum, Byok
    pub model: String,
    pub contract_id: String,
    pub latency_ms: u64,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub estimated_cost_micros: u64, // 1/1,000,000 of a cent
    pub success: bool,
}

pub trait CostStore: Send + Sync {
    fn record(&self, record: LlmCallRecord);
    fn player_total(&self, player_id: &str, since: chrono::DateTime<chrono::Utc>) -> PlayerCostSummary;
    fn universe_total(&self, universe: UniverseTier, since: chrono::DateTime<chrono::Utc>) -> UniverseCostSummary;
    fn daily_breakdown(&self, universe: UniverseTier, days: u32) -> Vec<DailyCostBucket>;
}
```

Memory impl for dev; Postgres impl behind the `postgres` feature (table:
`llm_call_records`).

### Quota tiers (`src/services/quota.rs`)

```rust
pub struct QuotaTier {
    pub monthly_token_budget: u64,
    pub soft_limit_pct: f64,        // e.g. 0.8 → fatigue kicks in at 80%
    pub hard_limit_pct: f64,        // e.g. 1.0 → calls blocked at 100%
    pub fatigue_message: String,    // injected into system prompt at soft limit
}

pub trait QuotaManager: Send + Sync {
    fn check(&self, player_id: &str, universe: UniverseTier) -> QuotaStatus;
}

pub enum QuotaStatus {
    Normal,
    Fatigued { message: String, remaining_pct: f64 },
    Exhausted,
}
```

Default quotas: Classic = no budget (no inference), FairPlay = 1000 calls/month,
Spectrum = 5000 calls/month, BYOK = unlimited.

Wire tests: `LlmCallRecord` serializes round-trip; `QuotaStatus` discriminant
ordering ensures `Normal < Fatigued < Exhausted` (ordinal comparison in code).

## Deliverables

### 1. Token counting & cost recording (`src/services/cost.rs`)

- [ ] Token estimation: providers that return `usage.prompt_tokens` /
      `usage.completion_tokens` in their response populate the record directly.
      Providers that don't (stub, Ollama without token counting) estimate:
      1 token ≈ 4 characters (conservative over-estimate for English text).
      Document the estimation formula in the module docs.
- [ ] `CostStore` trait with `MemoryCostStore` (rolling in-memory buffer,
      last 10,000 records) + `PgCostStore` (insert into `llm_call_records`).
- [ ] Per-call cost estimation: `estimated_cost_micros` computed from the
      provider's known pricing (Spectrum model pricing from env var
      `REACHLOCK_SPECTRUM_PRICE_PER_1K`; BYOK = 0; FairPlay = 0).
- [ ] Recording happens in `LlmService::route()` — one-line addition after
      the latency recording that's already there.
- [ ] Metrics: `reachlock_llm_tokens_total{universe,provider}` counter,
      `reachlock_llm_cost_micros_total{universe,provider}` counter.
- [ ] Test: route a stub call → verify the record is written with correct
      player/universe/token estimates.

### 2. Quota management (`src/services/quota.rs`)

- [ ] `QuotaManager` trait with `MemoryQuotaManager` impl. Quota tracking
      reads from the `CostStore` (cumulative monthly usage).
- [ ] `QuotaStatus::Fatigued` at 80% of monthly budget: the next LLM call
      prepends `"Your crew is fatigued from intensive deliberation. Responses
      will be shorter and more direct."` to the system prompt. `max_tokens`
      is halved.
- [ ] `QuotaStatus::Exhausted` at 100%: calls are rejected with
      `LlmError::Failed("quota_exhausted")` — the existing contract fallback
      path handles this (deliberation → timeout → fallback action → crew log
      entry "LLM unavailable: crew is resting").
- [ ] Monthly reset: quota periods align to calendar months. A player's
      first call of a new month resets their counter.
- [ ] `GET /admin/players/:id/llm-quota` → current quota status, monthly
      usage, remaining budget (admin-only, S26).
- [ ] Test: issue 999 calls under a 1000-call budget → next call is
      `Fatigued`; issue 1 more → `Exhausted`; advance clock to next month →
      `Normal` again.

### 3. Provider health monitoring (`src/services/providers.rs` expansion)

- [ ] Per-provider health tracking: rolling success/failure window (last
      100 calls). `ProviderHealth { success_rate: f64, avg_latency_ms: f64 }`.
- [ ] Circuit breaker: if a provider's success rate drops below 50% over
      the last 50 calls, route Spectrum → FairPlay (stub) and log a warning.
      The fallback stub returns deterministic fallback actions — players see
      slower, simpler crews, not errors.
- [ ] Auto-recovery: after 20 consecutive successful calls through the
      degraded path, the circuit breaker resets and Spectrum calls resume.
- [ ] Metrics: `reachlock_llm_provider_health{provider}` gauge (0=unhealthy,
      1=healthy).
- [ ] Test: inject N failures into a test provider → circuit breaker trips;
      inject success → breaker resets.

### 4. Admin cost reports (`src/services/cost.rs` expansion)

- [ ] `GET /admin/costs/daily?universe=spectrum&days=30` — per-day token
      counts and estimated cost, broken down by provider. JSON array.
- [ ] `GET /admin/costs/players?universe=spectrum&limit=20` — top N players
      by LLM usage this month. Admin audit trail.
- [ ] `GET /admin/costs/summary` — aggregate costs YTD per universe,
      estimated monthly burn rate. The operations dashboard query.
- [ ] Test: record 10 calls across 2 universes → summary endpoint returns
      correct per-universe totals.

## Acceptance gates

```
cargo test -p reachlock-server cost:: quota:: provider_health::
# Admin: curl -H "Authorization: Admin $KEY" localhost:40711/admin/costs/summary
# Quota: exhaust a player's budget → next call returns quota_exhausted
# Circuit breaker: trip a provider → calls auto-fallback → breaker resets
make check
```

## Non-goals

- Real-time per-token billing (that's S28 — Stripe integration charges by
  subscription tier, not per-token microtransactions)
- BYOK cost passthrough (player's key, player's bill — we just count tokens
  for our own curiosity)
- Token optimization / prompt caching (prompt caching is a Claude Code
  feature; the game's LLM surface is small enough that prompt engineering
  matters more than cache policy)
- Automated provider switching based on cost (FairPlay → Spectrum upgrade is
  a player choice at universe selection, not an automated optimization)
- GPU utilization metrics for local models (infra concern, not app concern)

## Gotchas

- Token estimation for providers that don't return counts (Ollama without
  verbose, stub) is an approximation. The 4-char = 1-token heuristic is
  conservative (~25% over-estimate for English). The over-estimate is safer
  than under-estimating: quota exhaustion arrives a bit early, not a bit
  late. Document the formula and the over-estimation bias.
- Monthly quota reset must be UTC midnight on the 1st. `chrono::Utc::now()`
  handles this. Test with fixed timestamps — inject a `TimeSource` trait
  rather than calling `Utc::now()` in the quota check (same pattern as
  S14's rate limiter `Instant`).
- The circuit breaker path must NOT silently drop calls — every `LlmError`
  gets recorded in the cost store as a failure. The `LlmService::route()`
  already records failures; the breaker adds a path, not a bypass.
- `CostStore` write must be non-blocking in the handler path. The memory
  impl is a `Mutex<VecDeque>` (fast). The Postgres impl uses `spawn_blocking`
  (same pattern as every other pg store in the codebase).
- Quota exhausts at the soft limit for FairPlay (cheapest tier) but at the
  hard limit for Spectrum (paying tier). Don't hardcode this — put it in
  the `QuotaTier` struct and test both paths.
