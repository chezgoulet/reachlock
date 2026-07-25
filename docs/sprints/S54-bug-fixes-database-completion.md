# S54 — Bug Fixes & Database Completion

**Spec:** §18 (LLM proxy), §8 (voice), §11 (schema), various · **Wave 13 (Bug Fixes) · Depends on:** S52, S53

## Outcome

Three known bugs are fixed: the LLM success metric is no longer inverted, voice signaling targets the correct peer instead of broadcasting, and the metric recording sends the right values. The database schema matches the code — `audit_log`, LLM cost records, health history, careers, reputation, and criminal records all have tables. The `faction/` directory is cleaned up — no orphaned files.

## Context

- `services/llm_proxy.rs:235`: `let success = result.is_err()` — this is inverted. It should be `result.is_ok()`. Every successful LLM call is recorded as a failure, and every failure as a success.
- `ws/handler.rs:265-268`: voice signaling broadcasts to all players in the system via `presence.broadcast()` instead of routing to the specific peer. The comment acknowledges the gap: "Send VoiceSignal directly to the target's out_tx isn't possible here — we only have the current session's out_tx."
- One migration exists (`0001_init.sql`). Code references tables that don't exist in the schema.
- The `faction/` directory exists but is empty. All faction code lives in `faction.rs` at the core root.

## Freeze first

1. Bug fixes must not change the public API of any store trait or message type.
2. Migrations are additive — do NOT alter existing columns in `0001_init.sql`. Add new tables only.

## Deliverables

### Bug fixes

- [ ] **Fix LLM success metric** — `services/llm_proxy.rs:235`: change `let success = result.is_err()` to `let success = result.is_ok()`. The `success` boolean feeds into `self.metrics.record(success, ...)` at line 240. Only the boolean is wrong; the latency and token-count recording are correct.
- [ ] **Fix voice signaling broadcast** — `ws/handler.rs:265-268`: store a `HashMap<String, tokio::sync::mpsc::Sender<ServerMessage>>` in the session state mapping target player_id → out_tx. When `VoiceSignal` arrives, look up the target by player_id and send directly. Fall back to broadcast if target not found (player may have disconnected). Add a `PeerNotFound` response.
- [ ] **Add metric for voice delivery** — track successful targeted delivery vs fallback broadcast in Prometheus counters.

### Database completion (migration `0003_add_tracking.sql`)

- [ ] **`audit_log` table** — `id UUID`, `admin_player_id UUID NOT NULL REFERENCES players(id)`, `action VARCHAR(256)`, `target_type VARCHAR(64)`, `target_id VARCHAR(128)`, `details JSONB`, `occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`. Index on `occurred_at`.
- [ ] **`llm_calls` table** — `id UUID`, `provider VARCHAR(64)`, `model VARCHAR(128)`, `player_id UUID REFERENCES players(id)`, `contract_id VARCHAR(128)`, `tokens_input INTEGER`, `tokens_output INTEGER`, `latency_ms INTEGER`, `success BOOLEAN`, `failure_reason VARCHAR(256)`, `cost_micro_credits INTEGER`, `occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`. Index on `occurred_at`, `player_id`.
- [ ] **`health_checks` table** — `id UUID`, `service VARCHAR(64)`, `status VARCHAR(32)`, `detail TEXT`, `latency_ms INTEGER`, `checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`. Index on `checked_at`.
- [ ] **`offline_entitlements` table** — `id UUID`, `player_id UUID NOT NULL REFERENCES players(id)`, `token VARCHAR(256) UNIQUE NOT NULL`, `tier universe_tier NOT NULL`, `issued_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`, `expires_at TIMESTAMPTZ NOT NULL`.

### Database completion (migration `0004_add_gameplay.sql`)

- [ ] **`careers` table** — `id UUID`, `player_id UUID NOT NULL REFERENCES players(id)`, `path_id VARCHAR(128)`, `current_rank INTEGER NOT NULL DEFAULT 1`, `total_prestige BIGINT NOT NULL DEFAULT 0`, `progress JSONB DEFAULT '{}'`, `created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`, `updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`, `UNIQUE(player_id, path_id)`.
- [ ] **`reputation` table** — `id UUID`, `player_id UUID NOT NULL REFERENCES players(id)`, `faction_id VARCHAR(128) NOT NULL`, `universe universe_tier NOT NULL`, `standing INTEGER NOT NULL DEFAULT 0`, `created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`, `updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`, `UNIQUE(player_id, faction_id, universe)`.
- [ ] **`criminal_records` table** — `id UUID`, `player_id UUID NOT NULL REFERENCES players(id)`, `universe universe_tier NOT NULL`, `crime_type VARCHAR(128)`, `description TEXT`, `bounty_amount BIGINT`, `issuer_faction VARCHAR(128)`, `recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`, `cleared_at TIMESTAMPTZ`.

### Structural cleanup

- [ ] **Move `faction.rs` to `faction/mod.rs`** — create `faction/mod.rs` containing the contents of the 1188-line `faction.rs`. Move into the existing `faction/` directory. Delete the top-level `faction.rs`. Update all `use crate::faction::*` imports in core to `use crate::faction::*` (they should still resolve — `faction/` is a module directory). Update `lib.rs` export.
- [ ] **Verify no orphaned imports** — after the faction move, `cargo check -p reachlock-core` must pass with zero errors.
- [ ] **Remove no-op placeholder `voice_native_placeholder_handle()`** at `voice/mod.rs:346` — this is replaced in S62 with a real TTS thread, but remove the no-op now to make the gap explicit. If the compiler complains about an unused import or function, acknowledge with `#[allow(dead_code)]` or a simple stub comment.

## Acceptance gates

```
cargo test -p reachlock-server metrics::
# LLM metric test: submit a known-successful call → metric records success = true
# Voice test: send VoiceSignal → target receives it (not broadcast)

cargo test -p reachlock-core::faction
# faction module tests still pass after the directory move

cargo test -p reachlock-server pg::
# migrations 0003 and 0004 run clean against fresh Postgres
make check
```

## Non-goals

Rewriting the LLM proxy failure model (spec §18 baseline probabilities remain at 70/10/10/5/3/2). Adding Postgres or Redis backends for the gameplay tables — this sprint creates the tables but does not wire them into stores (that's S49's pattern, applied to gameplay tables in a follow-up).

## Gotchas

- The voice signaling fix requires adding per-session state to `ws/session.rs`. Add a `target_map: Arc<RwLock<HashMap<String, Sender<ServerMessage>>>>` to `AppState` that sessions register into on connect. This is the same pattern as `PresenceManager` but keyed by player_id.
- The faction directory move touches every file that imports from `crate::faction`. Use `rg "use crate::faction" reachlock-core/src/` to find all imports. Most will work with no change if `faction/mod.rs` exports the same names `faction.rs` did.
- The two new migrations (`0003` and `0004`) must be run separately so the version table tracks them independently. Use `sqlx migrate add` to generate the file names.
