//! Session tokens and dev authentication (spec §8).
//!
//! Token issuance is HTTP and dev-only: `POST /auth/dev { username }` mints a
//! bearer token and echoes the resolved `player_id`. The WebSocket then
//! connects with `?token=…`. Tokens live behind the `SessionStore` trait —
//! in-memory today, Redis-backed later (S23). We design the trait now; we do
//! NOT build the Redis (sprint non-goal).
//!
//! Auth is OFF by default so S02 clients and local play keep working with the
//! legacy `?player=` handshake. Set `REACHLOCK_AUTH=1` to require tokens.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use reachlock_core::universe::UniverseTier;
use serde::{Deserialize, Serialize};

/// What a valid token resolves to: the identity a connection carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub player_id: String,
    pub universe: UniverseTier,
}

/// Session-token vault. Memory now; the trait is the seam for Redis later so
/// tokens can outlive a single server process and be shared across a fleet.
pub trait SessionStore: Send + Sync {
    /// Mint a fresh opaque token for this identity and remember it.
    fn issue(&self, info: SessionInfo) -> String;

    /// Resolve a token to its identity, or `None` if unknown/expired.
    fn resolve(&self, token: &str) -> Option<SessionInfo>;
}

/// Process-local token store. Tokens vanish on restart — acceptable for dev
/// auth; Redis is the durable/shared implementation (non-goal here).
#[derive(Default)]
pub struct MemorySessionStore {
    tokens: Mutex<HashMap<String, SessionInfo>>,
    counter: AtomicU64,
}

impl SessionStore for MemorySessionStore {
    fn issue(&self, info: SessionInfo) -> String {
        // Dev-grade opacity: a monotonic counter mixed with wall-clock nanos.
        // Not a security boundary (sprint non-goal: real account security).
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let token = format!("dev_{n:x}_{nanos:x}");
        self.tokens
            .lock()
            .expect("session store poisoned")
            .insert(token.clone(), info);
        token
    }

    fn resolve(&self, token: &str) -> Option<SessionInfo> {
        self.tokens
            .lock()
            .expect("session store poisoned")
            .get(token)
            .cloned()
    }
}

/// `POST /auth/dev` request body.
#[derive(Debug, Clone, Deserialize)]
pub struct DevLoginRequest {
    pub username: String,
    /// Which universe this token plays in. Defaults to Classic.
    #[serde(default = "default_universe")]
    pub universe: UniverseTier,
}

fn default_universe() -> UniverseTier {
    UniverseTier::Classic
}

/// `POST /auth/dev` response body.
#[derive(Debug, Clone, Serialize)]
pub struct DevLoginResponse {
    pub token: String,
    pub player_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_resolves_to_identity() {
        let store = MemorySessionStore::default();
        let token = store.issue(SessionInfo {
            player_id: "boris".into(),
            universe: UniverseTier::FairPlay,
        });
        let info = store.resolve(&token).expect("token resolves");
        assert_eq!(info.player_id, "boris");
        assert_eq!(info.universe, UniverseTier::FairPlay);
    }

    #[test]
    fn unknown_token_resolves_to_none() {
        let store = MemorySessionStore::default();
        assert!(store.resolve("dev_deadbeef_0").is_none());
    }

    #[test]
    fn tokens_are_unique_per_issue() {
        let store = MemorySessionStore::default();
        let a = store.issue(SessionInfo {
            player_id: "a".into(),
            universe: UniverseTier::Classic,
        });
        let b = store.issue(SessionInfo {
            player_id: "a".into(),
            universe: UniverseTier::Classic,
        });
        assert_ne!(a, b, "each issuance is a distinct token");
    }

    #[test]
    fn dev_login_defaults_universe_to_classic() {
        let req: DevLoginRequest = serde_json::from_str(r#"{"username":"tib"}"#).unwrap();
        assert_eq!(req.universe, UniverseTier::Classic);
    }
}
