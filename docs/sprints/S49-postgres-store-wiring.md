# S49 — Postgres Store Wiring

**Spec:** §8 (services), §11 (schema) · **Wave 11 (Server Infrastructure) · Depends on:** S03

## Outcome

The server's AppState uses real Postgres stores when `REACHLOCK_DB` is set. The tick loop persists events to `universe_events`. Every store (Seed, Session, Contract, Library, Billing, Audit) has a `Pg*` implementation behind the `postgres` feature flag, and each passes the same contract test battery as its in-memory counterpart. The in-memory stores remain the zero-infra default.

## Context

- `AppState::new()` at `ws/mod.rs:141-163` hardwires all 10 stores to memory regardless of `config.db_url`. The `config.db_url` is parsed but never consumed.
- `services/seed.rs` has a working `PgSeedStore` behind `#[cfg(feature = "postgres")]` — the reference pattern.
- `services/tick.rs` has `pg::append_events()` at lines 102-167 — never called from the tick loop.
- `migrations/0001_init.sql` exists with the full spec §11 schema. It has never run against a real Postgres.
- `docker-compose.yml` exists (from our earlier work) with Postgres 16.
- No CI job exercises the `postgres` feature.

## Freeze first

1. Store-selection API: `AppState::new()` branches on `config.db_url` — `Some(url)` → Pg stores, `None` → memory stores. One branching point, no scattered `#[cfg(feature)]` guards in AppState construction.
2. Every store trait that already exists (`SeedStore`, `SessionStore`, `ContractStore`, `SubscriptionStore`, `AuditLog`, `ContractLibrary`) needs its Pg impl in the same file/module pattern as `seed.rs::pg::PgSeedStore`.

## Deliverables

- [ ] **Store selection in AppState::new** — branch on `config.db_url`. When set, construct Pg stores; when unset, construct memory stores. Pass a `PgPool` to all Pg constructors from a single pool initialization.
- [ ] `PgSessionStore` in `services/auth.rs` — session tokens in a `sessions` table with TTL column. Implement `SessionStore` trait. Same pattern as `PgSeedStore`.
- [ ] `PgContractStore` in `services/contracts.rs` — contracts and evaluations persisted to `contracts` and `eval_signatures` tables. Implement `ContractStore` trait.
- [ ] `PgContractLibrary` in `services/library.rs` — published contracts and stories persisted to a `contract_library` table. Implement `ContractLibrary` trait.
- [ ] `PgSubscriptionStore` in `services/billing.rs` — subscriptions persisted to a `player_subscriptions` table. Implement `SubscriptionStore` trait.
- [ ] `PgAuditLog` in `services/audit.rs` — admin actions persisted to `audit_log` table. Implement `AuditLog` trait.
- [ ] **Wire pg::append_events** — call `pg::append_events(&pool, tier, &events)` in the tick loop (`tick.rs:56-58`) after broadcasting. Gate behind `#[cfg(feature = "postgres")]`.
- [ ] **Migration** — `sqlx migrate run` on startup when Postgres is configured. Fix any issues in `0001_init.sql` that real Postgres rejects (the file has never executed). Add a `sessions` table migration (`0002_add_sessions.sql`) if needed.
- [ ] **Test battery per Pg store** — each `Pg*` store runs the same contract tests as the memory store, gated on `REACHLOCK_TEST_DB`. Follow the pattern in `seed.rs::pg_tests::pg_store_obeys_the_contract`.
- [ ] **`make db-test` target** — starts docker-compose Postgres, runs `cargo test --features postgres -p reachlock-server` with `REACHLOCK_TEST_DB` set.
- [ ] **CI job** — `postgres` job with a GitHub Actions service container. Sets `REACHLOCK_TEST_DB` and `REACHLOCK_DB`, runs the Pg-gated tests.

## Acceptance gates

```
make db && REACHLOCK_DB=postgres://reachlock:reachlock@127.0.0.1/reachlock \
  cargo test --features postgres -p reachlock-server
# server restart mid-chain:
#   submit evals → restart server → next eval still verifies (chain heads reloaded)
make check
```

## Non-goals

Redis integration (S50). Real authentication (S51). Content override distribution (S57).

## Gotchas

- `PgSeedStore` uses `runtime.block_on` inside a sync trait. Keep this pattern for all Pg stores — it's the established convention. Each store gets its own `tokio::runtime::Handle` from construction.
- `gen_random_uuid()` needs Postgres 13+ (docker-compose pins 16, CI service container pins 16).
- Seeds are stored as BIGINT with a 2^53 CHECK constraint. Keep u64→i64 casts masked through `Seed::new`.
- The `postgres` feature must remain additive — the server MUST compile and pass all tests without it. `make check` tests the memory stores only.
