# S50 — Redis Integration

**Spec:** §2 (cache/pubsub), §8 (auth service) · **Wave 11 (Server Infrastructure) · Depends on:** S49

## Outcome

Session tokens survive server restart. Rate-limit counters are cross-process. Presence tracking survives restart. The Redis trait implementations slot into AppState when `REACHLOCK_REDIS_URL` is set, falling back to memory stores when unset.

## Context

- Every store that evaluates to Redis (`SessionStore`, rate-limit counters, presence) currently lives only in memory. Server restart loses all sessions, force-disconnecting every player.
- The `SessionStore` trait is designed as a seam — memory now, Redis later. This sprint fills in the later.
- Rate limiting (`services/limiter.rs`) uses in-memory counters. On a multi-process server (horizontal scaling), each process has its own counters — a player can exceed the rate limit by connecting to multiple server instances.
- Presence (`ws/mod.rs::PresenceManager`) lives in `AppState` memory. Server restart loses all system assignments.

## Freeze first

1. `redis` crate behind `redis` feature flag. No change to `reachlock-core` or `reachlock-client`.
2. Trait implementations live in `services/redis.rs`. Wire from `AppState::new()` when `REACHLOCK_REDIS_URL` is set.

## Deliverables

- [ ] **Add `redis` crate** to `reachlock-server/Cargo.toml` behind a `redis` feature flag. Use `deadpool-redis` for connection pooling (same ergonomics as `sqlx::PgPool`).
- [ ] **`services/redis.rs`** — `RedisPool` wrapper type holding a `deadpool::managed::Pool` handle. Module contains all Redis-backed trait implementations.
- [ ] `RedisSessionStore` in `services/redis.rs` — implement `SessionStore`. Store sessions as Redis hashes with TTL (default: 24h). `issue()` writes with TTL; `resolve()` reads; `revoke()` deletes. On restart, existing tokens survive (TTL starts at issue time, not at process start).
- [ ] `RedisRateLimiter` in `services/redis.rs` — implement the rate-limit interface from `services/limiter.rs`. Use Redis sorted sets with sliding-window Lua scripts. Atomic check-and-increment. Default window: 10 requests/second.
- [ ] `RedisPresenceTracker` in `services/redis.rs` — track which players are in which `(universe, system_id)`. Redis sets keyed by system. Rebuild `PresenceManager::by_system` from Redis on restart.
- [ ] **Wire in AppState::new()** — when `REACHLOCK_REDIS_URL` is set, use `RedisSessionStore`, `RedisRateLimiter`, and `RedisPresenceTracker`. Otherwise fall back to the existing in-memory implementations.
- [ ] **Lua scripts** for rate limiting to ensure atomicity. Scripts as constants in the `redis.rs` module, loaded with `SCRIPT LOAD` on startup.
- [ ] **Graceful degradation** — if Redis is unreachable at startup, log a warning and fall back to memory stores for session and presence. Rate limiting falls back to in-memory (per-process) counters.
- [ ] **Tests** — integration test that stores a session, kills the app (simulated), restarts, and resolves the same token successfully. Gated on `REACHLOCK_REDIS_URL`.

## Acceptance gates

```
REACHLOCK_REDIS_URL=redis://127.0.0.1:6379 cargo test --features redis -p reachlock-server
# Session token survives app restart:
#   issue token → record token value → simulate restart → resolve token → succeeds
# Rate limiting is cross-process:
#   two concurrent requests from different sources → counted together
make check  # (tests without --features redis — memory stores)
```

## Non-goals

Redis pub/sub for cross-process event broadcast (Phase 3). Redis cache for content overrides (S57). Redis-backed queue for LLM calls (Phase 3).

## Gotchas

- The `redis` feature must be additive — `make check` (no features) must pass. Cargo features are additive by design, but verify that `#[cfg(feature = "redis")]` guards don't leak into reachlock-core.
- Lua scripts for rate limiting must be idempotent — a crashed request that succeeded on Redis but failed to return must not double-count.
- Redis connection pool size: default to 4 connections. Configurable via `REACHLOCK_REDIS_POOL_SIZE`. Too small = contention under load; too large = unnecessary connections.
- TTL on session tokens: 24h by default. Configurable via `REACHLOCK_SESSION_TTL_HOURS`. Players who return within 24h maintain their session without re-login.
