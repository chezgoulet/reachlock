# S03 — Server Persistence & Auth (Postgres Proven)

**Spec:** §8 (services), §11 (schema) · **Wave 1 · Depends on:** nothing

## Outcome

The server's Postgres path is real, not just compiled: migrations run, the
`PgSeedStore` passes the same test battery as the in-memory store, contracts
and evaluation signatures persist across restarts, and connections carry a
session token instead of a bare `?player=` query. In-memory remains the
zero-infra default.

## Context

- `reachlock-server/src/services/seed.rs` has the `SeedStore` trait, the
  in-memory impl (with the full test battery), and an UNVERIFIED sqlx impl
  behind the `postgres` cargo feature. CI only checks it compiles.
- `migrations/0001_init.sql` exists — spec §11 schema with the
  UNIQUE-over-COALESCE fix (generated `object_key` column).
- Sessions parse from the query string in `ws/session.rs`. `AppState::new`
  hardwires `MemorySeedStore`.

## Freeze first

1. `Store` selection: `REACHLOCK_DB=postgres://…` env → Pg stores; unset →
   memory. One constructor, no scattered cfgs.
2. Auth message shape: token issuance is HTTP (`POST /auth/dev { username }`
   → `{ token, player_id }`, dev-mode only), WS connects with `?token=…`.
   Session tokens live in a `SessionStore` trait (memory now, Redis later —
   design the trait, not the Redis).

## Deliverables

- [ ] `docker-compose.dev.yml` (postgres:17 + volume) and a `make db` target.
      Document in the README dev section.
- [ ] Migration runner on startup (sqlx migrate). Fix anything in
      `0001_init.sql` that real Postgres rejects — the file has never run.
- [ ] `PgSeedStore` passes a shared test battery: extract the in-memory tests
      into a generic `fn store_contract_tests(store: &dyn SeedStore)` and run
      it against both impls (Pg tests gated on `REACHLOCK_TEST_DB` being set).
      Include the 32-way concurrent discovery race against real Postgres.
- [ ] Persist `contract.sync` payloads (contracts table) and accepted
      evaluations (`eval_signatures` table, `verified = true`); VerifyService
      reloads chain heads from the DB on boot so restarts don't break chains.
- [ ] Auth service: dev token issuance + validation, WS handshake rejects
      bad/missing tokens when auth is enabled (`REACHLOCK_AUTH=1`), stays
      permissive by default so S02 and local play don't break.
- [ ] CI: a `postgres` job using a GitHub Actions service container that runs
      the Pg-gated tests.

## Acceptance gates

```
make db && REACHLOCK_DB=postgres://… cargo test -p reachlock-server --features postgres
# server restart mid-chain:
#   submit evals → restart server → next eval in the chain still verifies
make check
```

## Non-goals

Redis (design the SessionStore trait only). Real account security/passwords.
Content override distribution (S23). Rate limiting (S14).

## Gotchas

- `PgSeedStore` currently uses `runtime.block_on` inside a sync trait —
  acceptable for now, but confirm it's never called from inside the WS
  handler's async context without `spawn_blocking`, or make the trait async
  (breaking S02? no — the trait is server-internal; do it if it's cleaner).
- `gen_random_uuid()` needs Postgres 13+; compose file pins 17.
- Seeds are stored as BIGINT with a 2^53 CHECK — keep u64→i64 casts masked
  through `Seed::new`.
