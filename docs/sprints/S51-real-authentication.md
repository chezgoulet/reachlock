# S51 — Real Authentication

**Spec:** §11 (players table), §8 (auth service) · **Wave 11 (Server Infrastructure) · Depends on:** S49, S50

## Outcome

Players register with email+password, verify their email, log in with Argon2id-verified passwords (no user enumeration — same response for "wrong password" and "no such user"), and receive session tokens. Two-factor authentication via TOTP is opt-in. OAuth2 login is available via Google and GitHub using the device authorization grant (RFC 8628 — no browser redirect needed from the game client). Password reset flows via email. Account deletion is GDPR-compliant with a configurable grace period. Admins can manage players and edit auth config at runtime. Dev-only token issuance (`POST /auth/dev`) is disabled in production mode.

## Context

- `services/auth.rs` has only `POST /auth/dev` — issues a bearer token from any username with no verification. No registration, no password, no security.
- The `players` table exists in `0001_init.sql` but has no password or credential columns. No email column. No verification state.
- The spec (§24 line 2314) defers auth to Phase 3 — this sprint brings it to production grade now.
- S49 adds Postgres stores (session persistence). S50 adds Redis (rate limit counters, cross-process session sharing). This sprint sits on top of both.
- S54 will add the `audit_log` table. This sprint writes to it.
- No email backend exists yet. This sprint adds a trait with Noop/File/Smtp implementations.

## Freeze first

### AuthConfig (runtime-editable via admin API)

All defaults are env-overridable via `REACHLOCK_AUTH_*` prefix. Live-editable at `POST /admin/auth-config` — server reads `Arc<RwLock<AuthConfig>>`, admin writes without restart.

```rust
pub struct AuthConfig {
    pub min_password_length: usize,             // default 12
    pub argon2_memory_kib: u32,                 // default 65536 (64 MiB)
    pub argon2_iterations: u32,                 // default 3
    pub argon2_parallelism: u32,                // default 4
    pub account_lockout_threshold: u32,         // default 10 attempts
    pub account_lockout_duration_mins: u32,     // default 15
    pub session_ttl_hours: u32,                 // default 24
    pub temp_token_ttl_mins: u32,               // default 5 (2FA challenge)
    pub password_reset_token_ttl_mins: u32,     // default 60
    pub verification_token_ttl_hours: u32,      // default 24
    pub deletion_grace_period_days: u32,        // default 30
}
```

### REACHLOCK_SECRET_KEY

Separate from the BYOK key. 32-byte hex key used for AES-256-GCM encryption of TOTP secrets at rest. If unset: TOTP endpoints return 503.

### Split-token design for password reset & verification

```
token = selector:verifier
  selector  → 32 char hex, identifies the row (plaintext DB column, indexed)
  verifier  → 256 char hex, compared with constant-time against stored hash

The link in the email contains both. The server only stores the verifier hash.
```

## Deliverables

### 1. Database migration `0003_add_auth.sql`

```sql
CREATE TYPE auth_role AS ENUM ('player', 'moderator', 'admin');

ALTER TABLE players ADD COLUMN password_hash VARCHAR(256);
ALTER TABLE players ADD COLUMN email VARCHAR(256) UNIQUE;
ALTER TABLE players ADD COLUMN verified_at TIMESTAMPTZ;
ALTER TABLE players ADD COLUMN role auth_role NOT NULL DEFAULT 'player';
ALTER TABLE players ADD COLUMN deleted_at TIMESTAMPTZ;
ALTER TABLE players ADD COLUMN banned_at TIMESTAMPTZ;
ALTER TABLE players ADD COLUMN banned_reason TEXT;
ALTER TABLE players ADD COLUMN failed_login_attempts INT NOT NULL DEFAULT 0;
ALTER TABLE players ADD COLUMN locked_until TIMESTAMPTZ;

CREATE TABLE email_verification_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    token_hash VARCHAR(256) NOT NULL,
    selector VARCHAR(32) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE password_reset_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    token_hash VARCHAR(256) NOT NULL,
    selector VARCHAR(32) NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE oauth_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    provider VARCHAR(32) NOT NULL,           -- 'google' | 'github'
    provider_user_id VARCHAR(256) NOT NULL,
    provider_email VARCHAR(256),
    linked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(provider, provider_user_id)
);

CREATE TABLE totp_secrets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL UNIQUE REFERENCES players(id) ON DELETE CASCADE,
    secret_encrypted TEXT NOT NULL,      -- AES-256-GCM: nonce(12) || ciphertext || tag(16), hex-encoded
    enabled_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE totp_recovery_codes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id UUID NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    code_hash VARCHAR(256) NOT NULL,     -- argon2id hashed
    used BOOLEAN NOT NULL DEFAULT false
);
```

### 2. Password-based auth endpoints

- [ ] **`POST /auth/register`** — `{ username, email, password }` → `{ token, player_id, requires_verification: true }`. Validate: username 3-32 chars (alphanumeric + underscore), password ≥ 12 chars (configurable min), email format. Hash with argon2id. Create player row. Issue verification email. Issue session token. Return 400 on duplicate username/email. Rate limit: 3/hour per IP.
- [ ] **`POST /auth/login`** — `{ login, password }` → `{ token, player_id }`. `login` is username OR email. Look up player by login. If `locked_until > now()` → 403 "account locked, try again in N minutes". If not verified → 403 "verify your email". Verify argon2id hash. If 2FA enabled → 200 `{ requires_2fa: true, temp_token }`. On failure: increment `failed_login_attempts`. If threshold exceeded → set `locked_until = now() + lockout_duration`. Return 401 — **identical response whether user exists or not** (no enumeration). Rate limit: 5/min per IP, 10/min per identifier.
- [ ] **`POST /auth/logout`** — Bearer auth → revoke session token. 204.
- [ ] **`POST /auth/delete-account`** — `{ password }` Bearer auth. Verify password. Set `deleted_at = now()`. 200 "account scheduled for deletion. grace period: N days".
- [ ] **`POST /auth/cancel-deletion`** — Bearer auth. Clear `deleted_at`. 200.
- [ ] **WS handshake enforcement** — in `ws/handler.rs`, when `state.auth_required` is true, reject upgrades without valid `?token=...`. Return 401 with `{ error: "authentication required" }`. (Already designed — S03 left this as `auth_required` env toggle.)

### 3. Email verification

- [ ] **`EmailBackend` trait** in `services/email.rs`:
  - `fn send(&self, to: &str, subject: &str, html_body: &str) -> Result<(), String>`
  - `NoopEmailBackend` — default, logs to tracing, no real send
  - `FileEmailBackend` — writes `.eml` files to `data/emails/` (Mailpit-compatible for local dev)
  - `SmtpEmailBackend` — `lettre` SMTP via `REACHLOCK_SMTP_URL`
- [ ] **Mailpit** in `docker-compose.yml`: `axllent/mailpit` on ports 8025 (web UI) and 1025 (SMTP).
- [ ] **`POST /auth/verify-email`** — `{ token }`. Split-token: look up `selector`, constant-time compare `verifier` against `token_hash`. Set `verified_at = now()`. Delete token row. 200.
- [ ] **`POST /auth/resend-verification`** — Bearer auth. Generate new verification token, send email. Rate limit: 3/hour per player.

### 4. Password reset

- [ ] **`POST /auth/forgot-password`** — `{ email }`. If account exists: generate split-token, store `token_hash` + `selector` in `password_reset_tokens`, send email. Always return 200 "if an account with that email exists, a reset link has been sent". Rate limit: 1/min per email.
- [ ] **`POST /auth/reset-password`** — `{ token, new_password }`. Split-token lookup. Verify not expired, not used. Validate password length. Hash with argon2id. Update `password_hash`. Mark token used. Invalidate ALL existing sessions for this player. Issue new session token. 200.

### 5. Two-factor authentication (TOTP)

- [ ] **`POST /auth/2fa/enable`** — Bearer auth. Generate TOTP secret (32 bytes, base32). Encrypt with AES-256-GCM using `REACHLOCK_SECRET_KEY`. Store in `totp_secrets`. Generate 10 recovery codes, store as argon2id hashes. Return `{ secret_base32, qr_code_url, recovery_codes: ["code1", ..., "code10"] }`. Return 503 if `REACHLOCK_SECRET_KEY` unset.
- [ ] **`POST /auth/2fa/verify`** — Bearer auth `{ code }`. Decrypt secret from DB, verify TOTP code. Set `enabled_at`. 200.
- [ ] **`POST /auth/2fa/disable`** — Bearer auth `{ code }`. Verify TOTP code (prove you have the device). Delete `totp_secrets` row. Delete `totp_recovery_codes` rows. 200.
- [ ] **`POST /auth/2fa/challenge`** — `{ temp_token, code, recovery_code? }`. Verify `temp_token` (from login step, 5min TTL). Verify TOTP code or recovery code. If recovery code: argon2id-verify, mark used, generate replacement. Issue session token. 200.

### 6. OAuth2 — Google + GitHub (Device Authorization Grant)

- [ ] **`POST /auth/oauth/google/device`** — Requests device code from Google's OAuth2 device endpoint. Returns `{ device_code, user_code, verification_uri, expires_in }`.
- [ ] **`POST /auth/oauth/github/device`** — Same flow via GitHub's device endpoint.
- [ ] **`POST /auth/oauth/token`** — `{ device_code }`. Polls provider for authorization status. Returns `{ pending: true }` while user hasn't authorized. Returns `{ token, player_id }` when authorized. Times out after `expires_in` seconds.
- [ ] **Account linking** — On first OAuth login: look up by `(provider, provider_user_id)`. If found → issue session token. If not found → try matching by email (`provider_email`): existing account with that email → link OAuth profile and login. No match → create new player account and link. All OAuth-created accounts are pre-verified (`verified_at = now()`).
- [ ] **Config** — `REACHLOCK_OAUTH_GOOGLE_CLIENT_ID`, `REACHLOCK_OAUTH_GOOGLE_CLIENT_SECRET`, `REACHLOCK_OAUTH_GITHUB_CLIENT_ID`, `REACHLOCK_OAUTH_GITHUB_CLIENT_SECRET`. If any is unset, that provider's device endpoint returns 503.

### 7. Account lockout

- [ ] On login failure: increment `failed_login_attempts`. Check against `AuthConfig::account_lockout_threshold`. If exceeded: set `locked_until = now() + lockout_duration_mins`.
- [ ] On successful login: reset `failed_login_attempts = 0`, clear `locked_until`.
- [ ] Admin can change `account_lockout_threshold` and `account_lockout_duration_mins` at runtime via `POST /admin/auth-config`.

### 8. Admin endpoints

- [ ] **`GET /admin/auth-config`** — bearer (admin). Returns current `AuthConfig` JSON.
- [ ] **`POST /admin/auth-config`** — bearer (admin). Accept partial or full `AuthConfig` JSON. Updates `Arc<RwLock<AuthConfig>>` without restart. 200.
- [ ] **`GET /admin/players`** — bearer (admin). `?page=&per_page=&search=&role=&banned=`. Paginated player list. Returns id, username, email, role, created_at, last_login, verified_at, deleted_at, banned_at.
- [ ] **`GET /admin/players/:id`** — bearer (mod+). Full player info including action history (audit log entries).
- [ ] **`POST /admin/players/:id/ban`** — bearer (admin). `{ reason? }`. Set `banned_at`, `banned_reason`. Invalidate ALL sessions for this player. Log to audit log.
- [ ] **`POST /admin/players/:id/unban`** — bearer (admin). Clear `banned_at`, `banned_reason`. Log to audit log.

### 9. Deletion cron

- [ ] **Scheduled task** in `main.rs` alongside the universe tick. Runs every hour.
- [ ] Purges accounts where `deleted_at IS NOT NULL` AND `deleted_at + deletion_grace_period_days < now()`:
  - `username = 'deleted_' || substr(uuid::gen_random_uuid()::text, 1, 12)`
  - `password_hash = NULL`
  - `email = NULL`
  - `display_name = NULL`
  - Delete `email_verification_tokens`, `password_reset_tokens`, `oauth_accounts`, `totp_secrets`, `totp_recovery_codes` for that player
  - Keep `id`, `created_at`, `deleted_at` — FK integrity with seeds, contracts, eval_signatures

### 10. Dev auth

- [ ] `POST /auth/dev { username }` preserved — returns dev token when `REACHLOCK_AUTH` is NOT set.
- [ ] When `REACHLOCK_AUTH=1`: `/auth/dev` returns 403 "dev auth disabled in production mode".
- [ ] Dev tokens are still in-memory (the dev endpoint is for development, not persistence).

### 11. Audit logging

- [ ] All auth events write to the audit log (S54 will add the table; write to `services/audit.rs` trait):
  - Registration success/failure
  - Login success/failure (with failed_attempts count)
  - Verification email sent/resend
  - Password reset requested/completed
  - 2FA enabled/disabled
  - OAuth link created
  - Account deletion scheduled/cancelled/purged
  - Admin config changes
  - Player ban/unban

### 12. Rate limiting (builds on S50 Redis)

- [ ] `POST /auth/login`: 5/min per IP, 10/min per identifier (username/email)
- [ ] `POST /auth/register`: 3/hour per IP
- [ ] `POST /auth/forgot-password`, `POST /auth/resend-verification`: 1/min per email
- [ ] `POST /auth/2fa/challenge`: 10/min per temp_token
- [ ] Fall back to in-memory counters when Redis is unavailable

### 13. New crate dependencies

- [ ] `argon2` — password hashing (replaces bcrypt from original design)
- [ ] `totp-rs` — TOTP code generation and verification
- [ ] `aes-gcm` — encrypt TOTP secrets at rest with REACHLOCK_SECRET_KEY
- [ ] `lettre` — SMTP email sending
- [ ] `qrcode` or `image` + `base64` — generate QR code for TOTP setup (inline PNG in the response, or use a `otpauth://` URI that authenticator apps scan)

## Acceptance gates

```bash
# Register
curl -X POST http://localhost:40711/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"captain_a","email":"a@test.net","password":"twelvecharsOK"}' \
  # → 200, token

# Unverified login blocked
curl -X POST http://localhost:40711/auth/login \
  -H "Content-Type: application/json" \
  -d '{"login":"captain_a","password":"twelvecharsOK"}' \
  # → 403 "verify your email"

# Verify email (grab token from data/emails/ or Mailpit)
curl -X POST http://localhost:40711/auth/verify-email \
  -H "Content-Type: application/json" \
  -d '{"token":"..."}' \
  # → 200

# Login after verification
curl -X POST http://localhost:40711/auth/login \
  -H "Content-Type: application/json" \
  -d '{"login":"captain_a","password":"twelvecharsOK"}' \
  # → 200, token

# No enumeration: wrong password
curl -X POST http://localhost:40711/auth/login \
  -H "Content-Type: application/json" \
  -d '{"login":"doesnotexist","password":"twelvecharsOK"}' \
  # → 401 (same as "captain_a" + "wrong")

# Password reset flow
curl -X POST http://localhost:40711/auth/forgot-password \
  -H "Content-Type: application/json" \
  -d '{"email":"a@test.net"}' \
  # → 200 "if account exists, email sent"

# 2FA
TOKEN=$(curl -sb -H "..." POST /auth/login -d ... | jq -r .token)
curl -X POST http://localhost:40711/auth/2fa/enable \
  -H "Authorization: Bearer $TOKEN" \
  # → { secret, qr_code_url, recovery_codes: [...] }

# OAuth — Google
curl -X POST http://localhost:40711/auth/oauth/google/device \
  # → { device_code, user_code, verification_uri, expires_in }

# Account lockout
# 10 failed logins → curl POST /auth/login returns 403 "locked"
# Admin changes threshold:
ADMIN_TOKEN=$(curl -sb -H "..." POST /auth/login -d '{"login":"admin","password":"..."}' | jq -r .token)
curl -X POST http://localhost:40711/admin/auth-config \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -d '{"account_lockout_threshold":20}' \
  # → 200

# Account deletion + grace period
curl -X POST http://localhost:40711/auth/delete-account \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"password":"twelvecharsOK"}' \
  # → 200
curl -X POST http://localhost:40711/auth/cancel-deletion \
  -H "Authorization: Bearer $TOKEN" \
  # → 200

# WS handshake
# websocat "ws://localhost:40711/ws?token=ISSUED_TOKEN" → 200
# REACHLOCK_AUTH=1
# websocat "ws://localhost:40711/ws?player=captain_a" → 401

cargo test -p reachlock-server auth::
# full integration battery: register → login → verify → 2fa → oauth →
# lockout → reset → delete → purge

make check
```

## Non-goals

WebAuthn / passkeys (future). Push notifications for 2FA (TOTP only). OAuth auto-linking with login hints (nice-to-have, not blocking). Account merging (take users through support). Discord OAuth (we chose Google + GitHub). Captcha / hCaptcha (add when bot registration becomes a problem).

## Gotchas

- **Argon2id memory cost**: 64 MiB per hash. On a 2 GiB server handling 10 concurrent logins, that's 640 MiB of peak memory during authentication. Acceptable for a game server with fewer than 100 concurrent logins. Monitor and reduce to 32 MiB if needed.
- **TOTP secret encryption key**: `REACHLOCK_SECRET_KEY` must be 32 bytes (64 hex chars). If the key is lost, all TOTP secrets are unrecoverable — players must re-enroll. Document this in the server setup guide.
- **OAuth device flow**: The client polls `POST /auth/oauth/token` every 5 seconds. After `expires_in` (300s default), the server returns 410 Gone. The client must show a "expired, try again" flow.
- **Split-token for email links**: The link in the email is `https://server/auth/verify?token={selector}:{verifier}`. The `selector` is also stored in the `tokens` column (not hashed) for quick lookup. The `verifier` is only stored as hash. Both `selector` and `token_hash` columns are needed.
- **Account lockout as DoS vector**: An attacker can lock a legitimate player out by spamming `POST /auth/login` with wrong passwords for their known username. Mitigation: rate-limit per IP (5/min), which caps the attacker's ability to generate failed attempts. A determined attacker with a botnet can still do it. The admin can disable lockout by setting threshold to 0 or a very high number. This is the accepted trade-off.
- **Admin routes**: The existing `/admin/*` routes at `ws/admin.rs` need role checks added. Every handler checks `state.sessions.resolve(token)` → lookup player → check `role >= moderator`. The `role` column is on the `players` table, not in the session token — resolved via the session store's player_id lookup.
- **Email sending in dev**: Without SMTP config, the Noop backend logs to tracing and the File backend writes to `data/emails/`. Add a `make mailpit` target that starts the Mailpit service and sets `REACHLOCK_SMTP_URL = smtp://127.0.0.1:1025`. The Mailpit web UI is at `http://127.0.0.1:8025`.
- **Password minimum**: The default is 12. The admin can lower it in the config, but never below 8 (enforce with `min()` inside the setter). The config runtime check clamps the value.
