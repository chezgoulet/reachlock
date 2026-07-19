//! S28 Stripe subscription management: types, trait, memory store, offline tokens.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use reachlock_core::universe::tier::UniverseTier;

// ---------------------------------------------------------------------------
// Webhook types
// ---------------------------------------------------------------------------

/// Parsed Stripe webhook event (the subset we handle).
#[derive(Debug, Clone, Deserialize)]
pub struct StripeWebhook {
    #[serde(rename = "type")]
    pub event_type: String,
    pub id: String,
    pub data: StripeWebhookData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StripeWebhookData {
    pub object: StripeSubscriptionObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StripeSubscriptionObject {
    pub id: String,
    pub customer: String,
    pub status: Option<String>,
    pub current_period_end: Option<i64>,
    pub metadata: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Subscription state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionStatus {
    Active,
    PastDue,
    Canceled,
    Incomplete,
    IncompleteExpired,
    Trialing,
    Unpaid,
}

impl SubscriptionStatus {
    /// Map a Stripe subscription status string to our enum.
    pub fn from_stripe(s: &str) -> SubscriptionStatus {
        match s {
            "active" => SubscriptionStatus::Active,
            "past_due" => SubscriptionStatus::PastDue,
            "canceled" => SubscriptionStatus::Canceled,
            "incomplete" => SubscriptionStatus::Incomplete,
            "incomplete_expired" => SubscriptionStatus::IncompleteExpired,
            "trialing" => SubscriptionStatus::Trialing,
            "unpaid" => SubscriptionStatus::Unpaid,
            _ => SubscriptionStatus::Incomplete,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSubscription {
    pub player_id: String,
    pub stripe_customer_id: String,
    pub tier: UniverseTier,
    pub status: SubscriptionStatus,
    pub current_period_end: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Entitlement
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum TierEntitlement {
    Granted { tier: UniverseTier, expires: DateTime<Utc> },
    GracePeriod { tier: UniverseTier, expires: DateTime<Utc>, message: String },
    Denied { reason: String },
}

// ---------------------------------------------------------------------------
// Store trait
// ---------------------------------------------------------------------------

pub trait SubscriptionStore: Send + Sync {
    fn upsert(&self, sub: PlayerSubscription);
    fn get(&self, player_id: &str) -> Option<PlayerSubscription>;
    fn entitlement(&self, player_id: &str) -> TierEntitlement;
    fn is_webhook_processed(&self, event_id: &str) -> bool;
    fn mark_webhook_processed(&self, event_id: &str);
}

// ---------------------------------------------------------------------------
// Memory store
// ---------------------------------------------------------------------------

pub struct MemorySubscriptionStore {
    subs: Mutex<HashMap<String, PlayerSubscription>>,
    pub processed_webhooks: Mutex<HashSet<String>>,
}

impl MemorySubscriptionStore {
    pub fn new() -> Self {
        MemorySubscriptionStore {
            subs: Mutex::new(HashMap::new()),
            processed_webhooks: Mutex::new(HashSet::new()),
        }
    }
}

impl Default for MemorySubscriptionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscriptionStore for MemorySubscriptionStore {
    fn upsert(&self, sub: PlayerSubscription) {
        self.subs.lock().unwrap().insert(sub.player_id.clone(), sub);
    }

    fn get(&self, player_id: &str) -> Option<PlayerSubscription> {
        self.subs.lock().unwrap().get(player_id).cloned()
    }

    fn is_webhook_processed(&self, event_id: &str) -> bool {
        self.processed_webhooks.lock().unwrap().contains(event_id)
    }

    fn mark_webhook_processed(&self, event_id: &str) {
        self.processed_webhooks.lock().unwrap().insert(event_id.to_owned());
    }

    fn entitlement(&self, player_id: &str) -> TierEntitlement {
        let subs = self.subs.lock().unwrap();
        match subs.get(player_id) {
            Some(sub) => {
                let now = Utc::now();
                if now > sub.current_period_end {
                    return TierEntitlement::Denied {
                        reason: "subscription_expired".into(),
                    };
                }
                match sub.status {
                    SubscriptionStatus::Active | SubscriptionStatus::Trialing => {
                        TierEntitlement::Granted {
                            tier: sub.tier,
                            expires: sub.current_period_end,
                        }
                    }
                    SubscriptionStatus::PastDue => TierEntitlement::GracePeriod {
                        tier: sub.tier,
                        expires: sub.current_period_end,
                        message: "Your subscription payment is past due. \
                                   Crew morale unaffected — for now.".into(),
                    },
                    _ => TierEntitlement::Denied {
                        reason: "subscription_inactive".into(),
                    },
                }
            }
            None => TierEntitlement::Denied {
                reason: "no_subscription".into(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Offline entitlement token
// ---------------------------------------------------------------------------

/// Signed offline token for subscription-free play.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitlementToken {
    pub player_id: String,
    pub tier: UniverseTier,
    pub issued: DateTime<Utc>,
    pub expires: DateTime<Utc>,
    pub signature: String,
}

/// Signature format: HMAC-SHA256(key, "player_id|tier|expires_timestamp")
fn sign_entitlement(player_id: &str, tier: &UniverseTier, expires: &DateTime<Utc>) -> String {
    let key = std::env::var("REACHLOCK_SERVER_SECRET")
        .unwrap_or_default();
    let msg = format!("{player_id}|{tier:?}|{}", expires.timestamp());
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
        .expect("HMAC key");
    mac.update(msg.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Mint a 30-day offline token.
pub fn mint_offline_token(player_id: &str, tier: UniverseTier) -> EntitlementToken {
    let now = Utc::now();
    let expires = now + chrono::TimeDelta::days(30);
    let signature = sign_entitlement(player_id, &tier, &expires);
    EntitlementToken {
        player_id: player_id.to_owned(),
        tier,
        issued: now,
        expires,
        signature,
    }
}

/// Verify an offline token. Returns `Ok(entitlement)` on success or `Err(reason)`.
pub fn verify_offline_token(token: &EntitlementToken) -> Result<TierEntitlement, String> {
    let now = Utc::now();
    if now > token.expires {
        return Err("token_expired".into());
    }
    let expected = sign_entitlement(&token.player_id, &token.tier, &token.expires);
    if token.signature != expected {
        return Err("invalid_signature".into());
    }
    Ok(TierEntitlement::Granted {
        tier: token.tier,
        expires: token.expires,
    })
}

// ---------------------------------------------------------------------------
// Stripe API helpers
// ---------------------------------------------------------------------------

static STRIPE_API_BASE: &str = "https://api.stripe.com/v1";

/// Parse the Stripe webhook signature header and verify the body.
/// Returns the Stripe event ID on success.
pub fn verify_stripe_webhook(
    payload: &[u8],
    sig_header: &str,
    webhook_secret: &str,
) -> Result<String, &'static str> {
    // Stripe-sig format: t=timestamp,v1=signature
    let parts: Vec<&str> = sig_header.split(',').collect();
    let mut timestamp = "";
    let mut sig = "";
    for p in &parts {
        if let Some(val) = p.strip_prefix("t=") { timestamp = val; }
        if let Some(val) = p.strip_prefix("v1=") { sig = val; }
    }
    if timestamp.is_empty() || sig.is_empty() {
        return Err("bad_stripe_signature_format");
    }

    let signed_payload = format!("{timestamp}.{}", std::str::from_utf8(payload).unwrap_or(""));
    let mut mac = Hmac::<Sha256>::new_from_slice(webhook_secret.as_bytes())
        .map_err(|_| "bad_webhook_secret")?;
    mac.update(signed_payload.as_bytes());
    let expected = hex::encode(mac.finalize().into_bytes());

    if expected != sig {
        return Err("invalid_stripe_signature");
    }

    // Parse the event ID from the payload
    let event: StripeWebhook = serde_json::from_slice(payload).map_err(|_| "unparseable_webhook")?;
    Ok(event.id)
}

/// Create a Stripe Checkout Session URL.
pub async fn create_checkout_session(
    player_id: &str,
    tier: UniverseTier,
) -> Result<String, &'static str> {
    let secret_key = std::env::var("REACHLOCK_STRIPE_SECRET_KEY")
        .map_err(|_| "stripe_not_configured")?;
    let tier_name = format!("{tier:?}").to_lowercase();
    let price_env = format!("REACHLOCK_STRIPE_PRICE_{}", tier_name.to_uppercase());
    let price_id = std::env::var(&price_env)
        .map_err(|_| "stripe_price_not_configured")?;

    let client = reqwest::Client::new();
    let params = [
        ("mode", "subscription"),
        ("line_items[0][price]", &price_id),
        ("line_items[0][quantity]", "1"),
        ("metadata[player_id]", player_id),
        ("metadata[universe_tier]", &tier_name),
        ("success_url", "https://reachlock.app/subscription/confirmed"),
        ("cancel_url", "https://reachlock.app/subscription/canceled"),
    ];
    let resp = client
        .post(format!("{STRIPE_API_BASE}/checkout/sessions"))
        .header("Authorization", format!("Bearer {secret_key}"))
        .form(&params)
        .send()
        .await
        .map_err(|_| "stripe_request_failed")?;

    let body: serde_json::Value = resp.json().await.map_err(|_| "stripe_parse_failed")?;
    body["url"]
        .as_str()
        .map(|s| s.to_owned())
        .ok_or("stripe_no_url")
}

/// Create a Stripe Customer Portal session.
pub async fn create_portal_session(customer_id: &str) -> Result<String, &'static str> {
    let secret_key = std::env::var("REACHLOCK_STRIPE_SECRET_KEY")
        .map_err(|_| "stripe_not_configured")?;

    let client = reqwest::Client::new();
    let params = [
        ("customer", customer_id),
        ("return_url", "https://reachlock.app/subscription/manage"),
    ];
    let resp = client
        .post(format!("{STRIPE_API_BASE}/billing_portal/sessions"))
        .header("Authorization", format!("Bearer {secret_key}"))
        .form(&params)
        .send()
        .await
        .map_err(|_| "stripe_request_failed")?;

    let body: serde_json::Value = resp.json().await.map_err(|_| "stripe_parse_failed")?;
    body["url"]
        .as_str()
        .map(|s| s.to_owned())
        .ok_or("stripe_no_url")
}
