# S51 — Real Authentication

**Spec:** §11 (players table), §8 (auth service) · **Wave 11 (Server Infrastructure) · Depends on:** S49, S50

## Outcome

Players can register accounts with username+password, log in with bcrypt verification, and receive session tokens. The WebSocket handshake rejects anonymous connections when `auth_required` is enabled. Dev-only token issuance (`POST /auth/dev`) remains as a convenience for development.

## Context

- `services/auth.rs` has only `POST /auth/dev` — issues a bearer token from any username with no verification. No registration, no password, no security.
- The `players` table exists in `0001_init.sql` but has no password or credential columns.
- The spec (§8, §11) defines `players` (username, display_name, created_at, last_login) but leaves auth to Phase 3 (spec §24 line 2314). This sprint implements it to a production-suitable level.

## Freeze first

1. Password is stored as bcrypt hash (not plaintext, not SHA, not argon2 — bcrypt is the Rust ecosystem standard and computationally bounded enough for a game server).
2. Session tokens are UUIDs stored in the session store (Redis from S50, memory from S49).

## Deliverables

- [ ] **Migration `0003_add_auth.sql`** — add `password_hash VARCHAR(128)` column to `players` table, add `UNIQUE(username)` index if not already present, add `last_login TIMESTAMPTZ` column with `DEFAULT NOW()`.
- [ ] **`POST /auth/register`** — `{ username, password }` → `{ player_id, token }`. Validate username (3-32 chars, alphanumeric + underscore). Validate password (8+ chars). Hash with `bcrypt::hash()` (cost 12). Create player row. Issue session token. Return 400 on duplicate username.
- [ ] **`POST /auth/login`** — `{ username, password }` → `{ token, player_id }`. Look up player by username. Verify password with `bcrypt::verify()`. Update `last_login`. Issue session token. Return 401 on wrong password, 404 on unknown user.
- [ ] **`POST /auth/logout`** — Bearer auth → revoke session token. Return 204 on success, 401 on invalid token.
- [ ] **Dev auth preserved** — `POST /auth/dev { username }` still works when `REACHLOCK_AUTH` is not set (dev mode). When auth is enabled, `/auth/dev` returns 403 "dev auth disabled in production mode".
- [ ] **WS handshake enforcement** — in `ws/handler.rs`, when `state.auth_required` is true, reject WebSocket upgrades that lack a valid `?token=...` parameter. Return 401 with `{ error: "authentication required" }`.
- [ ] **Rate limiting on auth endpoints** — `POST /auth/login` and `POST /auth/register` are rate-limited (5 attempts/minute per IP). Prevent brute-force attacks. Use Redis rate limiter when available, in-memory otherwise.
- [ ] **Tests** — full integration test battery: register → login → use token → logout → token rejected. Wrong password rejected. Duplicate username rejected. WS with bad token rejected. Gated on `REACHLOCK_TEST_DB`.

## Acceptance gates

```bash
# Register, login, use token
curl -X POST http://localhost:3333/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"captain_a","password":"hunter2"}'  # → 200, token

curl -X POST http://localhost:3333/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"captain_a","password":"hunter2"}'  # → 200, token

# Wrong password
curl -X POST http://localhost:3333/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"captain_a","password":"wrong"}'  # → 401

# Duplicate username
curl -X POST http://localhost:3333/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"captain_a","password":"hunter2"}'  # → 400

# Token used in WS handshake
# websocat "ws://localhost:3333/ws?token=ISSUED_TOKEN" → connected
# websocat "ws://localhost:3333/ws?token=BAD_TOKEN" → rejected (401)
make check
```

## Non-goals

OAuth / social login (post-launch). Email verification, password reset flow (post-launch). 2FA (post-launch). WebAuthn / passkeys (future).

## Gotchas

- Bcrypt cost 12 takes ~250ms per hash on modern hardware — acceptable for a login endpoint. Cost 10 (~80ms) is the minimum acceptable. Do not go below cost 10.
- The `password_hash` column is `VARCHAR(128)` — bcrypt hashes are 60 characters, but leave room for future algorithm upgrades (argon2 produces ~100 chars).
- `POST /auth/dev` must be disabled when `REACHLOCK_AUTH=1`. The check is: `if state.auth_required { return 403 }`. This prevents accidental dev-auth exposure in production.
- Rate limiting on login: use exponential backoff after 5 failures per username, not per IP — this prevents attackers from cycling IPs to brute-force a single account.
