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

impl ByokService {
    pub fn register(&self, player_id: &str, reg: &ByokRegistration) -> Result<(), ByokError> {
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
