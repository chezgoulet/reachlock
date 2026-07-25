# S28 — Payments & Subscriptions (Stripe)

**Spec:** §7 (multi-universe tiers), §24 Phase 3 (persistence) · **Wave 7 (tooling) ·
Depends on:** S23 (MMO presence, player profiles), S26 (admin API)

## Outcome

Players subscribe to universe tiers through Stripe Checkout. The server
handles Stripe webhooks to maintain entitlement state. Subscription status
gates universe access — a Classic player connecting to a Spectrum system gets
a clear "requires Spectrum subscription" message, not a cryptic auth error.
Paid content drops are gated by subscription tier + release date. Players
manage their subscription through Stripe's hosted Customer Portal. Offline
players with an active subscription get a cached entitlement token that the
server verifies on reconnect. Stripe does the PCI compliance; the server
does the entitlement mapping.

## Context

- The `universe_tier` enum and per-universe seed partitions were designed
  with billing hooks from day one (spec §7: "The universe_tier enum and
  per-universe seed partitions are designed so that future tier enforcement
  requires...").
- Stripe Checkout (hosted payment page) handles card collection, 3D Secure,
  invoicing, and recurring billing. The server never sees card numbers —
  Stripe sends webhooks with event types; the server maps those to
  entitlements. PCI compliance is Stripe's problem.
- S23 adds player profiles (`GET /player/{id}`) and session auth. This
  sprint extends the profile with subscription state.
- S26 adds the Admin API. This sprint adds subscription-management endpoints
  to it (view/override subscription for support purposes).
- Offline-first still holds: a player with an active subscription can play
  offline with a cached entitlement token that the server verifies on next
  online session. Token expires after 30 days offline, requiring re-auth.
- BYOK is intentionally NOT gated by subscription — it's free by design
  (player brings own key). The universe's existence is the gate, not
  payment.

## Freeze first

### Stripe webhook event mapping (`src/services/billing.rs`)

```rust
pub struct StripeWebhook {
    pub event_type: String,
    pub customer_id: String,
    pub subscription_id: String,
    pub status: SubscriptionStatus,
    pub current_period_end: chrono::DateTime<chrono::Utc>,
}

pub enum SubscriptionStatus {
    Active,
    PastDue,        // payment failed, grace period
    Canceled,       // canceled but period hasn't ended
    Incomplete,     // payment pending
    IncompleteExpired,
    Trialing,
    Unpaid,         // final failed — tier access revoked
}

pub trait SubscriptionStore: Send + Sync {
    fn upsert(&self, player_id: &str, sub: PlayerSubscription);
    fn get(&self, player_id: &str) -> Option<PlayerSubscription>;
    fn entitlement(&self, player_id: &str) -> TierEntitlement;
}

pub struct PlayerSubscription {
    pub player_id: String,
    pub stripe_customer_id: String,
    pub tier: UniverseTier,
    pub status: SubscriptionStatus,
    pub current_period_end: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub enum TierEntitlement {
    Granted { tier: UniverseTier, expires: chrono::DateTime<chrono::Utc> },
    GracePeriod { tier: UniverseTier, expires: chrono::DateTime<chrono::Utc>, message: String },
    Denied { reason: String },
}
```

Memory impl for dev; Postgres `player_subscriptions` table behind the
`postgres` feature.

### Offline entitlement token

```rust
pub struct EntitlementToken {
    pub player_id: String,
    pub tier: UniverseTier,
    pub issued: chrono::DateTime<chrono::Utc>,
    pub expires: chrono::DateTime<chrono::Utc>,
    pub signature: String,   // HMAC-SHA256(player_id|tier|expires, server_secret)
}
```

Wire tests: webhook JSON → `StripeWebhook` deserialization for the event types
we handle; `TierEntitlement` discriminant ordering; offline token round-trip
with valid and expired signatures.

## Deliverables

### 1. Stripe webhook handler (`POST /stripe/webhook`)

- [ ] Stripe signature verification: every webhook is signed with a webhook
      signing secret (`REACHLOCK_STRIPE_WEBHOOK_SECRET`). Verify the
      `stripe-signature` header before processing. Reject with 400 if
      signature is missing or invalid. This is Stripe's documented
      verification pattern — implement exactly to their spec.
- [ ] Event routing: handle `checkout.session.completed` (new subscription),
      `customer.subscription.updated` (plan change, renewal, pause),
      `customer.subscription.deleted` (cancellation), `invoice.paid`,
      `invoice.payment_failed`.
- [ ] Event → entitlement mapping:
      - `checkout.session.completed` → `upsert(player_id, Active, tier_from_metadata)`
      - `invoice.payment_failed` → `upsert(player_id, PastDue)`
      - `customer.subscription.deleted` → `upsert(player_id, Canceled)`
- [ ] Metadata passthrough: Stripe Checkout session includes `player_id` and
      `universe_tier` in `metadata`. The webhook reads these to map customer →
      player without a separate customer lookup table (metadata is Stripe's
      canonical coupling mechanism).
- [ ] Idempotency: webhook events carry an `idempotency_key`. Deduplicate by
      event ID in a `processed_webhooks` set (memory: `Mutex<HashSet<String>>`;
      Postgres: `stripe_webhook_events` table with a UNIQUE constraint).
- [ ] Test: send a simulated signed webhook → verify entitlement updated;
      replay the same webhook → idempotent (entitlement unchanged, no error).

### 2. Subscription gating on WS connect

- [ ] Middleware in `Session::authenticate()`: after resolving the player's
      identity, check `SubscriptionStore::entitlement(player_id)`.
- [ ] `TierEntitlement::Granted` → proceed normally, universe from the
      subscription (overrides any query-string universe).
- [ ] `TierEntitlement::GracePeriod` → proceed normally, but inject
      `ServerMessage::SystemNotice { message: "Your subscription payment is
      past due. Crew morale unaffected — for now." }` as the first message
      on the socket. The HUD displays it.
- [ ] `TierEntitlement::Denied` → reject WS upgrade with
      `ServerMessage::Error { message: "Subscription required: {reason}" }`.
      Client shows a "Subscribe" button linking to the Stripe Checkout URL.
- [ ] Test: connect with expired subscription → denied with a reason string
      that matches `"subscription_expired"`.

### 3. Stripe Checkout session creation (`POST /billing/checkout`)

- [ ] Authenticated endpoint: player bearer token required (same `Authorization:
      Bearer <token>` pattern as `/byok`).
- [ ] Creates a Stripe Checkout Session via Stripe's API (server-side HTTP to
      `api.stripe.com/v1/checkout/sessions`). The session includes:
      - `mode: "subscription"`
      - `line_items` → Stripe Price ID from `REACHLOCK_STRIPE_PRICE_{TIER}` env vars
      - `metadata: { player_id, universe_tier }`
      - `success_url` → game's "subscription confirmed" page
      - `cancel_url` → game's "subscription canceled" page
- [ ] Returns `{ url: "https://checkout.stripe.com/..." }`. The client opens
      this URL (native: system browser; WASM: `window.open()`).
- [ ] Environment: `REACHLOCK_STRIPE_SECRET_KEY` + per-tier
      `REACHLOCK_STRIPE_PRICE_CLASSIC`, `REACHLOCK_STRIPE_PRICE_FAIRPLAY`,
      `REACHLOCK_STRIPE_PRICE_SPECTRUM`. BYOK has no price (free).
- [ ] Test: the endpoint returns a URL when env vars are set; returns 503
      "Stripe not configured" when the secret key is missing.

### 4. Stripe Customer Portal (`POST /billing/portal`)

- [ ] Authenticated endpoint. Creates a Stripe Customer Portal session for
      the player's `stripe_customer_id`. Returns `{ url }`. Client opens it.
- [ ] The portal is Stripe-hosted: players manage their plan, payment method,
      billing history, cancellation. Zero UI to build — Stripe provides it.
- [ ] Test: endpoint returns URL or 404 if player has no subscription yet.

### 5. Content drop gating

- [ ] Content entitlement check: when a client requests `GET /content/system/{id}`,
      the server filters `content_overrides` rows by the player's subscription
      tier AND the content's `available_at` date relative to the player's
      subscription start date.
- [ ] "Season 1 content" requires a subscription that was active during the
      season window. Players who subscribe in Season 2 get Season 2 content
      but not Season 1's exclusive drops (unless the operator makes them
      evergreen — configurable per content item).
- [ ] `content_overrides` table gains `required_tier` column (nullable, NULL =
      all tiers) and `season_window` (daterange, nullable).
- [ ] Test: create content with `required_tier: Spectrum` → Classic player
      doesn't see it in the content response; Spectrum player does.

### 6. Offline entitlement token

- [ ] `POST /billing/entitlement-token` — authenticated. Returns a signed
      offline entitlement token valid for 30 days. The token contains
      `player_id`, `tier`, `expires`, and a HMAC-SHA256 signature using
      `REACHLOCK_SERVER_SECRET` (same key as admin).
- [ ] The offline client stores this token locally. On offline launch, it
      reads the token and grants the tier without a server connection.
      The deliberateness-panel shows "Offline — entitlement valid until {date}."
- [ ] On online reconnect, the server verifies the token's signature and
      expiry. If valid and newer than the server-side entitlement state,
      the server updates its subscription store (trust-on-first-use pattern —
      the token was minted from verified Stripe state).
- [ ] Expired token + offline: tier downgrades to Classic with a notice.
- [ ] Test: mint token → verify offline for 1 day → token still valid;
      advance clock past 30 days → token rejected, Classic tier applied.

### 7. Admin subscription management (S26 extension)

- [ ] `GET /admin/players/:id/subscription` — current subscription state,
      stripe customer ID, payment history summary.
- [ ] `POST /admin/players/:id/subscription/grant` — support override:
      manually grant a tier for N days. Logged in the audit log.
- [ ] `POST /admin/players/:id/subscription/revoke` — support override:
      revoke a subscription. Logged in the audit log.
- [ ] Test: admin grants a trial → player connects with that tier; revoke →
      player downgraded.

## Acceptance gates

```
cargo test -p reachlock-server billing:: entitlement:: webhook:: offline_token::
# Stripe webhook: simulate checkout.session.completed → subscription active
# Entitlement gating: expired sub → WS rejected with clear error
# Offline token: mint → verify → expire → rejected
make check
```

Manual: configure Stripe test keys → `POST /billing/checkout` → follow the
URL → complete test payment (Stripe test card `4242...`) → Stripe sends
webhook → server updates entitlement → player connects at the paid tier.
Cancel subscription → webhook fires → player downgraded.

## Non-goals

- Stripe Connect / marketplace payments (single-vendor subscriptions only)
- Per-token metered billing (subscription tiers, not usage billing)
- Free trial flows (Stripe handles this; the server doesn't need custom trial
  logic — use Stripe's trial period on the Price object)
- Promo codes / coupons (Stripe handles this; pass-through to Checkout)
- In-game purchase UI (the "Subscribe" button opens a browser; no native
  payment UI in Bevy)
- Tax handling (Stripe Tax; server just passes the customer's address if
  available)
- Multi-currency pricing (Stripe handles this; prices in USD, Stripe
  converts at checkout)

## Gotchas

- Stripe webhook signature verification uses the raw request body. Axum
  buffers the body by default — the `Bytes` extractor must get the raw bytes
  before any JSON deserialization. The webhook handler extracts `Bytes` +
  headers, verifies signature, THEN deserializes.
- Stripe metadata values are strings. `player_id` and `universe_tier` are
  ASCII-safe but must be URL-safe in the Checkout session metadata (no
  unicode, max 500 chars per key/value, max 50 keys). Validate on session
  creation.
- The offline entitlement token is a trust bridge — the server signs it from
  verified Stripe state. The HMAC key (`REACHLOCK_SERVER_SECRET`) must be
  the same across all server instances. If the key rotates, all offline
  tokens become invalid. Document this; provide a key rotation strategy
  (dual-accept old+new keys for 30 days during rotation).
- `POST /stripe/webhook` must respond with 200 within Stripe's timeout
  (typically 20 seconds). The handler does entitlement DB writes inline
  (fast). Any slow work (content re-indexing after a tier change) is
  spawned onto a Tokio task. The 200 is sent immediately after the DB write.
- Content gating by season requires knowing the player's subscription start
  date. This is stored in `PlayerSubscription.created_at`. If a player
  upgrades from Classic to Spectrum mid-season, they get Spectrum content
  from that point forward — NOT retroactive access to earlier drops. This
  is the intended design (spec §7: universes don't transfer between tiers).
