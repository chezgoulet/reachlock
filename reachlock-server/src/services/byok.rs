//! BYOK key storage (S14, spec §7): players in the Byok tier register their
//! own provider endpoint + API key via `POST /byok`. Keys are encrypted at
//! rest with ChaCha20-Poly1305 under a server key from
//! `REACHLOCK_BYOK_KEY` (64 hex chars = 32 bytes) — real crypto, not
//! base64 — and are never logged. The store trait mirrors `SeedStore`:
//! in-memory now, Postgres (`byok_keys.api_key_encrypted`) later.

use std::collections::HashMap;
use std::sync::Mutex;

use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use serde::{Deserialize, Serialize};

/// What a player registers: where to call, what model, and their key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ByokRegistration {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

/// What the router needs back at call time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByokCredentials {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByokError {
    /// The server has no `REACHLOCK_BYOK_KEY`; BYOK is disabled.
    NotConfigured,
    /// Stored blob failed to decrypt (key rotation, corruption).
    DecryptFailed,
    /// The submitted endpoint URL is not one the server may be made to call.
    InvalidUrl(String),
    NoKeyRegistered,
}

/// Encrypts/decrypts with the server key. Cheap to clone (the cipher key is
/// 32 bytes).
#[derive(Clone)]
pub struct ByokCrypto {
    key: Key,
}

impl ByokCrypto {
    /// Parse `REACHLOCK_BYOK_KEY` (64 hex chars). `None` disables BYOK.
    pub fn from_env() -> Option<Self> {
        let hex = std::env::var("REACHLOCK_BYOK_KEY").ok()?;
        Self::from_hex(&hex)
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim();
        if hex.len() != 64 {
            return None;
        }
        let mut bytes = [0u8; 32];
        for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
            let hi = (chunk[0] as char).to_digit(16)?;
            let lo = (chunk[1] as char).to_digit(16)?;
            bytes[i] = (hi * 16 + lo) as u8;
        }
        Some(ByokCrypto {
            key: Key::from(bytes),
        })
    }

    /// Nonce-prefixed ChaCha20-Poly1305 ciphertext.
    pub fn encrypt(&self, plaintext: &str) -> Vec<u8> {
        let cipher = ChaCha20Poly1305::new(&self.key);
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let mut out = nonce.to_vec();
        let ct = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .expect("chacha20poly1305 encrypt is infallible for in-memory data");
        out.extend(ct);
        out
    }

    pub fn decrypt(&self, blob: &[u8]) -> Result<String, ByokError> {
        if blob.len() < 12 {
            return Err(ByokError::DecryptFailed);
        }
        let (nonce, ct) = blob.split_at(12);
        let cipher = ChaCha20Poly1305::new(&self.key);
        let pt = cipher
            .decrypt(Nonce::from_slice(nonce), ct)
            .map_err(|_| ByokError::DecryptFailed)?;
        String::from_utf8(pt).map_err(|_| ByokError::DecryptFailed)
    }
}

/// One stored row: endpoint, model, and the encrypted key blob.
type ByokRow = (String, String, Vec<u8>);

/// Storage seam (Postgres `byok_keys` implements this later).
pub trait ByokStore: Send + Sync {
    fn put(&self, player_id: &str, base_url: &str, model: &str, key_encrypted: Vec<u8>);
    fn get(&self, player_id: &str) -> Option<ByokRow>;
}

#[derive(Default)]
pub struct MemoryByokStore {
    rows: Mutex<HashMap<String, ByokRow>>,
}

impl ByokStore for MemoryByokStore {
    fn put(&self, player_id: &str, base_url: &str, model: &str, key_encrypted: Vec<u8>) {
        self.rows.lock().expect("byok lock").insert(
            player_id.to_string(),
            (base_url.to_string(), model.to_string(), key_encrypted),
        );
    }

    fn get(&self, player_id: &str) -> Option<ByokRow> {
        self.rows.lock().expect("byok lock").get(player_id).cloned()
    }
}

/// The full service: crypto (if configured) + store.
pub struct ByokService {
    pub crypto: Option<ByokCrypto>,
    pub store: Box<dyn ByokStore>,
}

impl Default for ByokService {
    fn default() -> Self {
        ByokService {
            crypto: ByokCrypto::from_env(),
            store: Box::new(MemoryByokStore::default()),
        }
    }
}

/// Reject a BYOK endpoint the server must not be made to call.
///
/// The registered `base_url` becomes a URL the *server* fetches on the
/// player's behalf, so an unvalidated value is a server-side request forgery
/// primitive: a player could point it at `http://169.254.169.254/` (cloud
/// instance metadata), at `http://localhost:40711/admin/...`, or at any host
/// inside the deployment's private network, and read the response back
/// through the LLM proxy.
///
/// Policy: HTTPS only, no credentials in the URL, and the host must not be a
/// loopback, link-local, or private address. `REACHLOCK_BYOK_ALLOWED_HOSTS`
/// (comma-separated) narrows it further to an explicit allowlist, which is
/// what a real deployment should set.
pub fn validate_byok_url(raw: &str) -> Result<(), ByokError> {
    let url = url::Url::parse(raw).map_err(|_| ByokError::InvalidUrl("not a URL".into()))?;

    if url.scheme() != "https" {
        return Err(ByokError::InvalidUrl(
            "endpoint must use https (plaintext would also expose the API key)".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ByokError::InvalidUrl(
            "credentials must not be embedded in the endpoint URL".into(),
        ));
    }
    let Some(host) = url.host_str() else {
        return Err(ByokError::InvalidUrl("endpoint has no host".into()));
    };

    // Explicit allowlist wins when configured.
    if let Ok(list) = std::env::var("REACHLOCK_BYOK_ALLOWED_HOSTS") {
        let allowed: Vec<&str> = list
            .split(',')
            .map(|h| h.trim())
            .filter(|h| !h.is_empty())
            .collect();
        if !allowed.is_empty() {
            return if allowed.iter().any(|h| h.eq_ignore_ascii_case(host)) {
                Ok(())
            } else {
                Err(ByokError::InvalidUrl(format!(
                    "host {host} is not in REACHLOCK_BYOK_ALLOWED_HOSTS"
                )))
            };
        }
    }

    // No allowlist: block the addresses that make SSRF worth attempting.
    let lowered = host.to_ascii_lowercase();
    if lowered == "localhost" || lowered.ends_with(".localhost") || lowered.ends_with(".internal") {
        return Err(ByokError::InvalidUrl(
            "endpoint must not resolve to the server itself".into(),
        ));
    }
    // `host_str()` renders an IPv6 literal with brackets (`[::1]`), which does
    // not parse as an IpAddr — so trim them before checking, or ::1 walks
    // straight through the loopback guard.
    let host_ip = lowered.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host_ip.parse::<std::net::IpAddr>() {
        let blocked = match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_broadcast()
                    || v4.is_unspecified()
                    || v4.octets()[0] == 0
                    // 100.64.0.0/10 carrier-grade NAT
                    || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    // unique-local fc00::/7 and link-local fe80::/10
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        };
        if blocked {
            return Err(ByokError::InvalidUrl(format!(
                "endpoint address {ip} is loopback, private, or link-local"
            )));
        }
    }
    Ok(())
}

impl ByokService {
    pub fn register(&self, player_id: &str, reg: &ByokRegistration) -> Result<(), ByokError> {
        validate_byok_url(&reg.base_url)?;
        let crypto = self.crypto.as_ref().ok_or(ByokError::NotConfigured)?;
        let blob = crypto.encrypt(&reg.api_key);
        self.store.put(player_id, &reg.base_url, &reg.model, blob);
        // Deliberately no key material in this log line.
        tracing::info!(player = %player_id, url = %reg.base_url, model = %reg.model, "byok registered");
        Ok(())
    }

    pub fn credentials(&self, player_id: &str) -> Result<ByokCredentials, ByokError> {
        let crypto = self.crypto.as_ref().ok_or(ByokError::NotConfigured)?;
        let (base_url, model, blob) = self
            .store
            .get(player_id)
            .ok_or(ByokError::NoKeyRegistered)?;
        Ok(ByokCredentials {
            base_url,
            model,
            api_key: crypto.decrypt(&blob)?,
        })
    }
}

#[cfg(feature = "postgres")]
pub mod pg {
    //! Postgres-backed BYOK storage against the existing `byok_keys`
    //! migration. The row's `api_key_encrypted` column carries a JSON
    //! envelope `{url, model, key_b64}` (the encrypted blob base64-inside)
    //! so the S03 schema needs no new migration; `provider` records the
    //! dialect ("openai_compat"). Same blocking contract as
    //! `PgContractStore`: invoke from `spawn_blocking`, never an async
    //! worker.

    use super::*;
    use sqlx::types::Uuid;
    use sqlx::PgPool;

    pub struct PgByokStore {
        pool: PgPool,
        runtime: tokio::runtime::Handle,
    }

    impl PgByokStore {
        pub fn new(pool: PgPool) -> Self {
            PgByokStore {
                pool,
                runtime: tokio::runtime::Handle::current(),
            }
        }
    }

    fn b64(bytes: &[u8]) -> String {
        // Tiny local base64 (standard alphabet, padded) — not worth a dep.
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(A[(n >> 18) as usize & 63] as char);
            out.push(A[(n >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 {
                A[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                A[n as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    fn unb64(text: &str) -> Option<Vec<u8>> {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let val = |c: u8| A.iter().position(|a| *a == c).map(|p| p as u32);
        let clean: Vec<u8> = text.bytes().filter(|c| *c != b'=').collect();
        let mut out = Vec::new();
        for chunk in clean.chunks(4) {
            let mut n = 0u32;
            for (i, c) in chunk.iter().enumerate() {
                n |= val(*c)? << (18 - 6 * i);
            }
            out.push((n >> 16) as u8);
            if chunk.len() > 2 {
                out.push((n >> 8) as u8);
            }
            if chunk.len() > 3 {
                out.push(n as u8);
            }
        }
        Some(out)
    }

    impl ByokStore for PgByokStore {
        fn put(&self, player_id: &str, base_url: &str, model: &str, key_encrypted: Vec<u8>) {
            let pool = self.pool.clone();
            let username = player_id.to_string();
            let envelope = serde_json::json!({
                "url": base_url,
                "model": model,
                "key_b64": b64(&key_encrypted),
            })
            .to_string();
            crate::services::blocking::block_on_async(&self.runtime, async move {
                let mut tx = pool.begin().await.expect("begin byok tx");
                let (pid,): (Uuid,) = sqlx::query_as(
                    "INSERT INTO players (username) VALUES ($1)
                     ON CONFLICT (username) DO UPDATE SET last_login = NOW()
                     RETURNING id",
                )
                .bind(&username)
                .fetch_one(&mut *tx)
                .await
                .expect("upsert player");
                // One active key per player: retire the old ones.
                sqlx::query("UPDATE byok_keys SET is_active = false WHERE player_id = $1")
                    .bind(pid)
                    .execute(&mut *tx)
                    .await
                    .expect("retire old keys");
                sqlx::query(
                    "INSERT INTO byok_keys (player_id, provider, api_key_encrypted)
                     VALUES ($1, 'openai_compat', $2)",
                )
                .bind(pid)
                .bind(&envelope)
                .execute(&mut *tx)
                .await
                .expect("insert byok key");
                tx.commit().await.expect("commit byok tx");
            });
        }

        fn get(&self, player_id: &str) -> Option<ByokRow> {
            let pool = self.pool.clone();
            let username = player_id.to_string();
            let envelope: Option<(String,)> =
                crate::services::blocking::block_on_async(&self.runtime, async move {
                    sqlx::query_as(
                        "SELECT k.api_key_encrypted FROM byok_keys k
                     JOIN players p ON p.id = k.player_id
                     WHERE p.username = $1 AND k.is_active
                     ORDER BY k.created_at DESC LIMIT 1",
                    )
                    .bind(&username)
                    .fetch_optional(&pool)
                    .await
                    .expect("select byok key")
                });
            let (text,) = envelope?;
            let value: serde_json::Value = serde_json::from_str(&text).ok()?;
            Some((
                value.get("url")?.as_str()?.to_string(),
                value.get("model")?.as_str()?.to_string(),
                unb64(value.get("key_b64")?.as_str()?)?,
            ))
        }
    }

    #[cfg(test)]
    mod pg_tests {
        use super::*;

        /// Runs only where `DATABASE_URL` points at a reachable Postgres —
        /// CI's postgres job sets it (same convention as the seed store).
        #[tokio::test(flavor = "multi_thread")]
        async fn put_get_round_trips_when_db_available() {
            let Ok(url) = std::env::var("DATABASE_URL") else {
                return;
            };
            let pool = PgPool::connect(&url).await.expect("connect");
            sqlx::migrate!("./migrations").run(&pool).await.ok();
            let store = PgByokStore::new(pool);
            let blob = vec![1u8, 2, 3, 254, 255];
            let (store, row) = tokio::task::spawn_blocking(move || {
                store.put(
                    "byok-test-player",
                    "https://api.example",
                    "m1",
                    blob.clone(),
                );
                let row = store.get("byok-test-player");
                (store, row)
            })
            .await
            .expect("blocking task");
            drop(store);
            let (url_, model, key) = row.expect("row present");
            assert_eq!(url_, "https://api.example");
            assert_eq!(model, "m1");
            assert_eq!(key, vec![1u8, 2, 3, 254, 255]);
        }

        #[test]
        fn base64_round_trips() {
            for input in [
                vec![],
                vec![1u8],
                vec![1, 2],
                vec![1, 2, 3],
                vec![0, 255, 128, 7],
            ] {
                assert_eq!(unb64(&b64(&input)).unwrap(), input);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    #[test]
    fn round_trip_encrypt_decrypt() {
        let crypto = ByokCrypto::from_hex(TEST_KEY).unwrap();
        let blob = crypto.encrypt("sk-super-secret");
        assert!(
            !blob.windows(15).any(|w| w == b"sk-super-secret"),
            "ciphertext hides the key"
        );
        assert_eq!(crypto.decrypt(&blob).unwrap(), "sk-super-secret");
    }

    #[test]
    fn nonces_differ_between_encryptions() {
        let crypto = ByokCrypto::from_hex(TEST_KEY).unwrap();
        assert_ne!(crypto.encrypt("same"), crypto.encrypt("same"));
    }

    #[test]
    fn bad_hex_is_rejected() {
        assert!(ByokCrypto::from_hex("short").is_none());
        assert!(ByokCrypto::from_hex(&"zz".repeat(32)).is_none());
    }

    #[test]
    fn service_round_trip_and_unconfigured() {
        let svc = ByokService {
            crypto: ByokCrypto::from_hex(TEST_KEY),
            store: Box::new(MemoryByokStore::default()),
        };
        let reg = ByokRegistration {
            base_url: "https://api.example".into(),
            model: "some-model".into(),
            api_key: "sk-abc".into(),
        };
        svc.register("tib", &reg).unwrap();
        let creds = svc.credentials("tib").unwrap();
        assert_eq!(creds.api_key, "sk-abc");
        assert_eq!(creds.model, "some-model");
        assert_eq!(svc.credentials("nobody"), Err(ByokError::NoKeyRegistered));

        let disabled = ByokService {
            crypto: None,
            store: Box::new(MemoryByokStore::default()),
        };
        assert_eq!(
            disabled.register("tib", &reg),
            Err(ByokError::NotConfigured)
        );
    }

    #[test]
    fn tampered_blob_fails_closed() {
        let crypto = ByokCrypto::from_hex(TEST_KEY).unwrap();
        let mut blob = crypto.encrypt("sk-abc");
        let last = blob.len() - 1;
        blob[last] ^= 0xFF;
        assert_eq!(crypto.decrypt(&blob), Err(ByokError::DecryptFailed));
    }
}

#[cfg(test)]
mod ssrf_tests {
    use super::*;

    /// The registered endpoint becomes a URL the *server* fetches. Anything
    /// pointing back inside the deployment is an SSRF primitive.
    #[test]
    fn rejects_addresses_that_reach_the_deployment() {
        for bad in [
            "https://127.0.0.1/v1",
            "https://localhost/v1",
            "https://[::1]/v1",
            "https://10.0.0.5/v1",
            "https://192.168.1.10/v1",
            "https://172.16.0.1/v1",
            // Cloud instance metadata — the classic SSRF target.
            "https://169.254.169.254/latest/meta-data/",
            "https://100.64.0.1/v1",
            "https://metadata.internal/v1",
        ] {
            assert!(
                validate_byok_url(bad).is_err(),
                "{bad} should have been refused"
            );
        }
    }

    #[test]
    fn requires_https_and_no_embedded_credentials() {
        // Plaintext would also put the player's API key on the wire.
        assert!(validate_byok_url("http://api.example.com/v1").is_err());
        assert!(validate_byok_url("file:///etc/passwd").is_err());
        assert!(validate_byok_url("https://user:pw@api.example.com/v1").is_err());
        assert!(validate_byok_url("not a url").is_err());
    }

    #[test]
    fn accepts_an_ordinary_public_endpoint() {
        assert!(validate_byok_url("https://api.openai.com/v1").is_ok());
        assert!(validate_byok_url("https://llm.example.co.uk/v1/chat").is_ok());
    }

    /// Registration must refuse before anything is stored — a rejected URL
    /// that still landed in the store would be called on the next request.
    #[test]
    fn register_refuses_a_private_endpoint() {
        let svc = ByokService {
            crypto: ByokCrypto::from_hex(&"ab".repeat(32)),
            store: Box::new(MemoryByokStore::default()),
        };
        let reg = ByokRegistration {
            base_url: "https://169.254.169.254/v1".into(),
            model: "m".into(),
            api_key: "sk-test".into(),
        };
        assert!(svc.register("p1", &reg).is_err());
        assert!(
            svc.credentials("p1").is_err(),
            "nothing may be stored for a refused endpoint"
        );
    }
}
