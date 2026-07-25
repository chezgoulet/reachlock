# S66 — Server Auth Hardening

**Spec:** New (auth security fixes) · **Wave A (stop the bleeding)** · **Depends on:** — (standalone, different crate from S65)

## Outcome

All 15 auth security findings (S1–S15 from MASTER-PLAN.md) are closed. Auth state is durable behind `TokenStore`, `TotpStore`, and `OAuthFlowStore` traits with Postgres impls matching the existing `0002_add_auth.sql` schema. Rate limiters key on peer socket addr (with trusted-proxy XFF), expire old entries, and bound the bucket map. AuthConfig TTLs are honoured everywhere. Sessions expire. 2FA enrollment is two-phase (scan QR → verify code → enabled). Recovery codes are ≥128-bit, burn exactly one per use, and the challenge endpoint is rate-limited. The LLM `system_prompt` field is template-gated so the client cannot route arbitrary prompts through the server.

## Context

- MASTER-PLAN.md findings S1–S15 cover severities C (3), H (10), and M (2). S1 is critical: enabling 2FA then restarting the server destroys every TOTP enrollment — the secrets live in an in-memory HashMap that has no Postgres path, even when `REACHLOCK_DB` is set. The Postgres tables exist (`migrations/0002_add_auth.sql:46-57`) but no code reads or writes them.
- The pattern is proven: `PlayerStore` + `SessionStore` already have `Memory*` and `Pg*` impls, selected by AppState based on `REACHLOCK_DB`. But verification tokens, reset tokens, TOTP secrets, recovery codes, and OAuth flow state all use raw `Arc<Mutex<HashMap>>` on AppState regardless of config. They need the same trait → Memory → Pg pipeline.
- Rate limiter keys on `X-Forwarded-For` with `unwrap_or("unknown")` (S2) — meaning every request without XFF shares the "unknown" bucket, making registration globally 1/hour. Login limiter keys on the attacker's username (S3) — a dictionary attack against 1,000 usernames creates 1,000 buckets and each bucket gets 5 attempts. The bucket map (`HashMap<String, Vec<Instant>>`) grows unbounded (S4).
- Session store has no expiry — `MemorySessionStore:issue` stores forever; `PgSessionStore` has the SQL column but no cleanup mechanism for expired rows.

## Freeze first

### `TokenStore` trait

Replaces `verification_tokens` and `reset_tokens` on AppState. Covers both kinds, discriminated by `TokenKind`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    EmailVerification,
    PasswordReset,
}

pub struct TokenEntry {
    pub player_id: String,
    pub expires_at: i64,
}

pub trait TokenStore: Send + Sync {
    fn insert(&self, kind: TokenKind, token: &str, player_id: &str, expires_at: i64);
    fn consume(&self, kind: TokenKind, token: &str) -> Option<TokenEntry>;
    fn cleanup_expired(&self);
}

pub struct MemoryTokenStore {
    tokens: Mutex<HashMap<(TokenKind, String), TokenEntry>>,
}
```

### `TotpStore` trait

Replaces `totp_secrets` and `totp_recovery_codes` on AppState.

```rust
pub struct TotpSecret {
    pub player_id: String,
    pub secret_encrypted: String,
}

pub struct RecoveryCode {
    pub player_id: String,
    pub code_hash: String,
    pub used: bool,
}

pub trait TotpStore: Send + Sync {
    fn set_secret(&self, player_id: &str, encrypted: &str);
    fn get_secret(&self, player_id: &str) -> Option<String>;
    fn has_secret(&self, player_id: &str) -> bool;
    fn remove_secret(&self, player_id: &str);
    fn add_recovery_code(&self, player_id: &str, code_hash: &str);
    fn use_recovery_code(&self, player_id: &str, code_hash: &str) -> bool;
    fn list_recovery_codes(&self, player_id: &str) -> Vec<RecoveryCode>;
}

pub struct MemoryTotpStore {
    secrets: Mutex<HashMap<String, String>>,
    codes: Mutex<Vec<(String, String, bool)>>, // (player_id, code_hash, used)
}
```

### `OAuthFlowStore` trait

Replaces `oauth_flows` on AppState.

```rust
pub trait OAuthFlowStore: Send + Sync {
    fn insert(&self, device_code: &str, provider: &str, expires_at: i64);
    fn consume(&self, device_code: &str) -> Option<String>;
    fn cleanup_expired(&self);
}

pub struct MemoryOAuthFlowStore {
    flows: Mutex<HashMap<String, (String, i64)>>, // device_code -> (provider, expires_at)
}
```

### Updated `AppState` in `ws/mod.rs`

```rust
// Replace these raw fields:
pub verification_tokens: Arc<Mutex<HashMap<String, (String, i64)>>>,
pub reset_tokens: Arc<Mutex<HashMap<String, (String, i64)>>>,
pub totp_secrets: Arc<Mutex<HashMap<String, String>>>,
pub totp_recovery_codes: Arc<Mutex<Vec<(String, String)>>>,
pub oauth_flows: Arc<Mutex<HashMap<String, String>>>,

// With these trait-backed fields:
pub tokens: Box<dyn TokenStore>,
pub totp: Box<dyn TotpStore>,
pub oauth_flows: Box<dyn OAuthFlowStore>,
```

`TempTokenStore` stays — it's a local concern for the 2FA challenge handshake. But it gains TTL cleanup.

### Rate limiter changes

```rust
pub struct AuthRateLimiter {
    buckets: Mutex<BTreeMap<String, VecDeque<Instant>>>,  // BTreeMap for bounded iteration
    max_attempts: u32,
    window_secs: u64,
    max_buckets: usize,  // hard cap — evict oldest when exceeded
}

// Key derivation:
// 1. Try X-Forwarded-For first (trusted-proxy check via config)
// 2. Fall back to peer socket addr
// 3. Never key on user-supplied data (login text, username)
fn rate_limit_key_for(headers: &HeaderMap, remote_addr: &SocketAddr) -> String {
    // config.trusted_proxies controls which XFF values are accepted
}

// Remove the per-endpoint static LazyLock limiters; use one shared instance
// per endpoint with config-driven parameters, or keep static but fix keying.
```

### `SessionStore` changes

Add `expires_at` to `issue()`:

```rust
pub trait SessionStore: Send + Sync {
    fn issue(&self, info: SessionInfo, ttl_secs: u64) -> String;  // new: ttl_secs
    fn resolve(&self, token: &str) -> Option<SessionInfo>;
    fn revoke(&self, token: &str);
    fn revoke_all_for_player(&self, player_id: &str);
    fn cleanup_expired(&self);  // new: remove expired sessions
}
```

`MemorySessionStore` records `expires_at` and filters on resolve. `PgSessionStore` already uses `expires_at > NOW()` in the SQL query (handled at the DB level), but needs `cleanup_expired` to `DELETE FROM sessions WHERE expires_at < NOW()` (optional, the DB check handles correctness already).

## Deliverables

### 1. Auth state traits + memory impls (`services/auth.rs`)

- [ ] Define `TokenStore` trait and `MemoryTokenStore` impl covering both `EmailVerification` and `PasswordReset` kinds.
- [ ] Define `TotpStore` trait and `MemoryTotpStore` impl covering secrets and recovery codes.
- [ ] Define `OAuthFlowStore` trait and `MemoryOAuthFlowStore` impl with expiry.
- [ ] Add `cleanup_expired` to `SessionStore` trait with default no-op; implement for `MemorySessionStore` (remove expired entries on an interval or on resolve).

**Gate:** All three new traits compile and export from `services/auth.rs`. Memory impls pass basic round-trip tests: insert → consume → Some; double-consume → None.

### 2. Postgres impls + AppState wiring (`services/auth.rs`, `ws/mod.rs`)

- [ ] Implement `TokenStore` for `PgTokenStore` using `email_verification_tokens` and `password_reset_tokens` tables.
- [ ] Implement `TotpStore` for `PgTotpStore` using `totp_secrets` and `totp_recovery_codes` tables. TOTP secrets stored encrypted (already done via `encrypt_secret` before calling `set_secret`).
- [ ] Implement `OAuthFlowStore` for `PgOAuthFlowStore` (new table `oauth_flows` — device_code, provider, expires_at) or store in-memory only (OAuth device codes are inherently short-lived and provider-scoped).
- [ ] Wire into `AppState::new` and `AppState::new_pg`: when postgres feature + pool present, use Pg stores; otherwise fall back to memory stores.
- [ ] Remove `verification_tokens`, `reset_tokens`, `totp_secrets`, `totp_recovery_codes` raw fields from `AppState`.
- [ ] Update every auth handler (`register`, `verify_email`, `forgot_password`, `reset_password`, `tfa_enable`, `tfa_verify`, `tfa_disable`, `tfa_challenge`, `oauth_*`) to use the new trait methods.

**Gate:** Enable 2FA (tfa_enable) → restart the server → tfa_challenge still accepts the same TOTP code (secret persisted in Postgres). Recovery codes survive restart. Verification tokens survive restart.

### 3. Rate limiter fix (`services/auth.rs`)

- [ ] `AuthRateLimiter::is_limited` takes `(&HeaderMap, &SocketAddr)` and derives the key from peer addr, falling back to XFF only when a trusted-proxy list is configured.
- [ ] `AuthRateLimiter` uses `BTreeMap` with a `max_buckets` cap (default 10_000). When exceeded, evict the oldest bucket.
- [ ] Login rate limiter keys on `ip(hash(login))` — bind the bucket to the IP, not the login string. Use a fixed salt per process to prevent precomputation.
- [ ] Reset password limiter keys on IP, not email.
- [ ] Add rate limit to `tfa_challenge` endpoint (5 attempts per temp_token lifetime).

**Gate:** Two requests from different IPs with no XFF header get different buckets. 10,001 unique IPs do not cause OOM — the 10,001st evicts the oldest. Login with username "admin" and username "nobody" from the same IP counts against the same bucket.

### 4. Session expiry (`services/auth.rs`)

- [ ] `AuthConfig::session_ttl_hours` is read by `login` and `oauth_token` handlers and passed as `ttl_secs` to `SessionStore::issue`.
- [ ] `MemorySessionStore::issue` records `expires_at`. `MemorySessionStore::resolve` filters out expired entries.
- [ ] `MemorySessionStore::cleanup_expired` runs every 60s via a spawned tokio task (or inline on resolve). Remove purged entries.

**Gate:** Set `REACHLOCK_AUTH_SESSION_TTL_HOURS=0` (or 1 second via env override) → login → wait 2 seconds → session resolves to `None`.

### 5. Honour every AuthConfig TTL

- [ ] `session_ttl_hours` — see deliverable 4.
- [ ] `temp_token_ttl_mins` — already consumed by `TempTokenStore::issue` (lines 439-449). Verify it's not hardcoded. Done.
- [ ] `password_reset_token_ttl_mins` — consumed by `forgot_password`/`reset_password` via `TokenStore::insert`.
- [ ] `verification_token_ttl_hours` — consumed by `register`/`resend_verification` via `TokenStore::insert`.
- [ ] `account_lockout_threshold` and `account_lockout_duration_mins` — already consumed by `login` (lines 1033-1038). Verify the `locked_until` check at line 1010 reads lockout_duration from config (it currently multiplies by 60 — confirm the config field stores minutes). Done.

**Gate:** `REACHLOCK_AUTH_VERIFICATION_TOKEN_TTL_HOURS=0` → send verification → token expires immediately → verify returns "invalid or expired token".

### 6. Two-phase 2FA enrollment

- [ ] `tfa_enable` generates secret + QR + recovery codes and stores them (in the TotpStore), but does NOT mark the player as "2FA enrolled" with a flag.
- [ ] `tfa_verify` (new step) — the player enters a TOTP code. If it matches the stored secret, the player is marked `2fa_enabled` (new field on `PlayerRecord` or a separate `player_2fa` table).
- [ ] `login` checks `player_2fa_enabled` flag instead of `totp_secrets.contains_key()`. This prevents the lockout scenario: enabling without verifying.
- [ ] `tfa_disable` requires a valid TOTP code (existing behaviour) AND the player's `2fa_enabled` flag is cleared.

**Gate:** Enable 2FA → server returns QR + codes → close the tab → reopen → secret persists in TotpStore → enter a wrong TOTP code → "invalid code", still not enrolled → enter correct code → enrolled → login now requires 2FA.

### 7. Recovery code fix (`services/auth.rs`)

- [ ] `generate_crypto_token(4)` → `generate_crypto_token(16)` produces 32 hex chars (128 bits). Current `generate_crypto_token(4)` produces 8 hex chars (32 bits) — brute-forceable.
- [ ] Recovery codes stored as argon2id hashes (existing behaviour — good).
- [ ] `tfa_challenge` burns exactly ONE recovery code: `UPDATE totp_recovery_codes SET used = true WHERE player_id = $1 AND code_hash = $2 AND used = false LIMIT 1`. The old code that removes all codes for the player and re-inserts (line 1369-1370) must be replaced.
- [ ] `tfa_challenge` is rate-limited: 5 attempts per temp_token TTL.

**Gate:** Login → 2FA challenge → enter recovery code → accepted, other 9 still valid → enter same recovery code again → rejected (already used). Enter a wrong recovery code 6 times → temp token consumed, must re-login.

### 8. Ban/delete checks on reset + consistent login response

- [ ] `reset_password` checks `banned_at` and `deleted_at` after consuming the token (or before issuing the token in `forgot_password`). Banned/deleted accounts cannot reset.
- [ ] `login` response shape is consistent: when 2FA is required, `player_id` is included (so the client can display "verify 2FA for {username}"). When 2FA is not required, `player_id` is absent (existing behaviour, S11). Either both return `player_id` or neither does — pick one.
- [ ] Recommendation: always return `player_id` in `LoginResponse`. The client already has it or can derive it from the session. Consistency prevents client-side branches.

**Gate:** Ban a player → `forgot_password` with their email → still returns "if an account exists…" (no leak) → token on DB but `reset_password` with that token returns "account is banned". Login response always includes `player_id`.

### 9. Email template URL fix + CORS + body limit + LLM gate

- [ ] All email templates (register, resend_verification, forgot_password) use a configurable `public_url` instead of hardcoded `https://reachlock.example`. Read from `REACHLOCK_PUBLIC_URL` env var.
- [ ] CORS layer on the router: `tower_http::cors::CorsLayer::permissive()` for dev, configurable origins for production. Without this the WASM client cannot connect from a different origin (S14).
- [ ] Body limit: `tower_http::limit::RequestBodyLimitLayer::new(1 << 20)` (1 MB) on the router.
- [ ] Trace layer: `tower_http::trace::TraceLayer` on the router for request logging.
- [ ] LLM `system_prompt` is template-gated: the client can inject variables (contract id, context) but cannot replace the base prompt. The `CallOverrides::system_prompt` on the wire (handler.rs:356) is validated before reaching `llm_proxy.rs:182` — if it contains phrases not in the allowlist, it's rejected. Minimum: verify it starts with `"You are the deliberation engine for ship contract '"` or similar fixed prefix. Cleaner: strip the client-supplied system_prompt entirely and reconstruct from server-side data.

**Gate:** `REACHLOCK_PUBLIC_URL=http://localhost:40711` → register email contains `http://localhost:40711/auth/verify?token=…`. WASM client on `http://localhost:5173` can connect. LLM call with a custom system_prompt is rejected.

### 10. Registration does not issue a session

- [ ] `register` returns `RegisterResponse` with `token: None` (no session until email verified). Remove the `state.sessions.issue()` call at line 963.
- [ ] After email verification, the client logs in normally (login endpoint), which returns a session.
- [ ] Login still returns 403 "verify your email before logging in" (existing behaviour, correct).

**Gate:** Register → response has no token → `/auth/verify?token=…` → then login → session returned.

### 11. Migrations

- [ ] New migration `0006_add_auth_stores.sql`:
  - `ALTER TABLE players ADD COLUMN IF NOT EXISTS totp_enrolled BOOLEAN NOT NULL DEFAULT false`
  - `CREATE TABLE IF NOT EXISTS oauth_flows (device_code VARCHAR(256) PRIMARY KEY, provider VARCHAR(32) NOT NULL, expires_at TIMESTAMPTZ NOT NULL)`
  - `ALTER TABLE sessions ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '24 hours'`
  - Add index on `sessions.expires_at`

**Gate:** Migration runs without error on a fresh Postgres. Migration is idempotent (existing tables not modified destructively).

## Acceptance gates

```
cargo test -p reachlock-server auth::stores::memory::round_trip
cargo test -p reachlock-server auth::totp::survives_restart
cargo test -p reachlock-server auth::recovery_code::burn_one

# Manual 2FA persistence
REACHLOCK_DB=postgres://… cargo run -p reachlock-server
curl -X POST /auth/register …  # create account
curl -X POST /auth/tfa_enable -H "Authorization: Bearer …"
# ^ returns QR + codes
# Kill server, restart
curl -X POST /auth/tfa_challenge -d '{"code":"…","temp_token":"…"}'
# ^ still accepts the TOTP code

# Rate limiter test
for i in $(seq 1 10); do curl -X POST /auth/login -d '{"login":"wrong","password":"wrong"}'; done
# ^ attempt 6+ from same IP returns 429

# Session expiry
REACHLOCK_AUTH_SESSION_TTL_HOURS=0 cargo run -p reachlock-server
curl -X POST /auth/login …  # returns token
sleep 2
curl -H "Authorization: Bearer $TOKEN" /auth/me
# ^ 401

# Email URL fix
REACHLOCK_PUBLIC_URL=http://myhost:40711 cargo run -p reachlock-server
curl -X POST /auth/register … > /dev/null
# ^ check data/emails/ for .eml file containing "myhost:40711"

make check
```

## Non-goals

- Passwordless/WebAuthn auth — S51 already ships email+password, OAuth, and TOTP. Adding WebAuthn is a separate sprint.
- Session refresh without re-login (token rotation). Sessions expire and the client re-logins. Refresh tokens are a separate improvement.
- Postgres connection pool tuning or migration framework changes. Use sqlx as currently configured.
- Admin API hardening — the admin API (`ws/admin.rs`) is not in scope. Its auth model (admin key hash) is separate.
- OAuth PKCE flow — device code grant is what exists. PKCE is a separate improvement.
- The `player_senders` map on AppState (voice-chat session routing) is not in scope. It has a different expiry model.

## Gotchas

- The `totp_secrets` map currently stores encrypted secrets (`encrypt_secret` is called before insert). The Pg impl should also encrypt before storing — use the existing `encrypt_secret`/`decrypt_secret` functions with `REACHLOCK_SECRET_KEY`.
- `TempTokenStore` is a stand-alone struct, not in AppState as `Arc<Mutex<HashMap>>`. It already has a dedicated struct. It does not need a trait yet — it's only used for the 2FA challenge handshake and is inherently ephemeral. If it needs persistence later, promote it to a trait then.
- The `totp_recovery_codes` field is `Vec<(String, String)>` not `HashMap<String, Vec<String>>`. Each entry is `(player_id, code_hash)` — so a single player's recovery codes are interleaved with other players' codes. The new `TotpStore` trait uses `use_recovery_code(player_id, code_hash) -> bool` which atomically marks one code as used for the given player.
- Rate limiter `max_buckets` eviction: evict the bucket with the oldest activity timestamp. If all buckets are active (attacker rotating IPs), still bounded by `max_buckets`. The eviction policy is LRU-adjacent: when inserting a new bucket at capacity, find the bucket whose last access was farthest in the past and remove it.
- CORS in dev: `permissive()` is fine for local testing but must be configurable for production. Use `REACHLOCK_CORS_ORIGINS` env var (comma-separated). Default to `http://localhost:5173,http://localhost:1420` (Vite dev, Tauri dev).
- The LLM system_prompt gate: the simplest correct approach is to remove `system_prompt` from `CallOverrides` entirely and reconstruct it server-side from `contract_id` and the contract engine's known prompt template. The wire shape (`ClientMessage::LlmCall`) has `system_prompt: Option<String>` — keep it on the wire for schema compatibility but ignore it server-side. The contract's own `LlmConfig.system_prompt` (stored in the `Contract` struct) is the source of truth. Document this in the PR that changes `llm_proxy.rs`.
