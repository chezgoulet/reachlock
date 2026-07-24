//! Production-grade authentication (S51). Argon2id, email verification,
//! password reset, TOTP 2FA, OAuth2 (Google + GitHub), account deletion,
//! lockout, admin player management, runtime-editable auth config.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aes_gcm::aead::{Aead, KeyInit, AeadCore};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::password_hash::SaltString;
use argon2::Argon2;
use argon2::PasswordHasher;
use argon2::PasswordVerifier;
use reachlock_core::universe::UniverseTier;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Secret, TOTP};

use base64::Engine;
use rand::rngs::OsRng;
use subtle::ConstantTimeEq;

// ---------------------------------------------------------------------------
// Rate limiter for auth endpoints (S63)
// ---------------------------------------------------------------------------

/// Simple sliding-window rate limiter for auth endpoints.
pub struct AuthRateLimiter {
    buckets: Mutex<HashMap<String, Vec<Instant>>>,
    max_attempts: u32,
    window_secs: u64,
}

impl AuthRateLimiter {
    pub fn new(max_attempts: u32, window_secs: u64) -> Self {
        AuthRateLimiter {
            buckets: Mutex::new(HashMap::new()),
            max_attempts,
            window_secs,
        }
    }

    pub fn is_limited(&self, key: &str) -> bool {
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(self.window_secs);
        let mut buckets = self.buckets.lock().unwrap();
        let entries = buckets.entry(key.to_string()).or_default();
        entries.retain(|&t| t > cutoff);
        if entries.len() as u32 >= self.max_attempts {
            return true;
        }
        entries.push(now);
        false
    }
}

static LOGIN_LIMITER: LazyLock<AuthRateLimiter> = LazyLock::new(|| AuthRateLimiter::new(5, 60));
static REGISTER_LIMITER: LazyLock<AuthRateLimiter> = LazyLock::new(|| AuthRateLimiter::new(1, 3600));
static FORGOT_PW_LIMITER: LazyLock<AuthRateLimiter> = LazyLock::new(|| AuthRateLimiter::new(1, 600));

// ---------------------------------------------------------------------------
// AuthConfig — runtime-editable settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub min_password_length: usize,
    pub argon2_memory_kib: u32,
    pub argon2_iterations: u32,
    pub argon2_parallelism: u32,
    pub account_lockout_threshold: u32,
    pub account_lockout_duration_mins: u32,
    pub session_ttl_hours: u32,
    pub temp_token_ttl_mins: u32,
    pub password_reset_token_ttl_mins: u32,
    pub verification_token_ttl_hours: u32,
    pub deletion_grace_period_days: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig {
            min_password_length: 12,
            argon2_memory_kib: 65536,
            argon2_iterations: 3,
            argon2_parallelism: 4,
            account_lockout_threshold: 10,
            account_lockout_duration_mins: 15,
            session_ttl_hours: 24,
            temp_token_ttl_mins: 5,
            password_reset_token_ttl_mins: 60,
            verification_token_ttl_hours: 24,
            deletion_grace_period_days: 30,
        }
    }
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let mut cfg = AuthConfig::default();
        if let Ok(v) = std::env::var("REACHLOCK_AUTH_MIN_PASSWORD_LENGTH") {
            if let Ok(n) = v.parse::<usize>() {
                cfg.min_password_length = n.max(8);
            }
        }
        if let Ok(v) = std::env::var("REACHLOCK_AUTH_LOCKOUT_THRESHOLD") {
            if let Ok(n) = v.parse::<u32>() {
                cfg.account_lockout_threshold = n;
            }
        }
        if let Ok(v) = std::env::var("REACHLOCK_AUTH_LOCKOUT_DURATION_MINS") {
            if let Ok(n) = v.parse::<u32>() {
                cfg.account_lockout_duration_mins = n;
            }
        }
        if let Ok(v) = std::env::var("REACHLOCK_AUTH_SESSION_TTL_HOURS") {
            if let Ok(n) = v.parse::<u32>() {
                cfg.session_ttl_hours = n;
            }
        }
        if let Ok(v) = std::env::var("REACHLOCK_AUTH_DELETION_GRACE_DAYS") {
            if let Ok(n) = v.parse::<u32>() {
                cfg.deletion_grace_period_days = n;
            }
        }
        cfg
    }
}

// ---------------------------------------------------------------------------
// REACHLOCK_SECRET_KEY retrieval
// ---------------------------------------------------------------------------

fn secret_key() -> Option<Vec<u8>> {
    let hex_key = std::env::var("REACHLOCK_SECRET_KEY").ok()?;
    hex::decode(&hex_key).ok().filter(|b| b.len() == 32)
}

// ---------------------------------------------------------------------------
// PlayerRecord + PlayerStore
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PlayerRecord {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    pub role: String,
    pub verified_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub banned_at: Option<i64>,
    pub banned_reason: Option<String>,
    pub failed_login_attempts: u32,
    pub locked_until: Option<i64>,
    pub created_at: i64,
}

pub trait PlayerStore: Send + Sync {
    fn create(&self, username: &str, email: &str, hash: &str) -> Result<PlayerRecord, String>;
    fn by_login(&self, login: &str) -> Option<PlayerRecord>;
    fn by_id(&self, id: &str) -> Option<PlayerRecord>;
    fn by_email(&self, email: &str) -> Option<PlayerRecord>;
    fn by_provider(&self, provider: &str, provider_uid: &str) -> Option<PlayerRecord>;
    fn link_provider(&self, player_id: &str, provider: &str, provider_uid: &str, provider_email: &str);
    fn update_hash(&self, player_id: &str, hash: &str);
    fn inc_fails(&self, player_id: &str);
    fn clear_fails(&self, player_id: &str);
    fn set_verified(&self, player_id: &str);
    fn set_deleted(&self, player_id: &str);
    fn cancel_deletion(&self, player_id: &str);
    fn set_lock(&self, player_id: &str, locked_until: i64);
    fn ban(&self, player_id: &str, reason: &str);
    fn unban(&self, player_id: &str);
    fn set_role(&self, player_id: &str, role: &str);
    fn list(&self, page: u32, per_page: u32) -> Vec<PlayerRecord>;
    fn count(&self) -> usize;
}

pub struct MemoryPlayerStore {
    players: Mutex<HashMap<String, PlayerRecord>>,
    by_email: Mutex<HashMap<String, String>>,
    by_provider: Mutex<HashMap<(String, String), String>>,
    next_id: AtomicU64,
}

impl Default for MemoryPlayerStore {
    fn default() -> Self {
        MemoryPlayerStore {
            players: Mutex::new(HashMap::new()),
            by_email: Mutex::new(HashMap::new()),
            by_provider: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

impl PlayerStore for MemoryPlayerStore {
    fn create(&self, username: &str, email: &str, hash: &str) -> Result<PlayerRecord, String> {
        let mut players = self.players.lock().unwrap();
        let mut emails = self.by_email.lock().unwrap();
        if players.values().any(|p| p.username == username) {
            return Err("username or email already taken by another player".into());
        }
        if !email.is_empty() && emails.contains_key(email) {
            return Err("username or email already taken by another player".into());
        }
        let id_num = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = format!("p{id_num:x}");
        let now = now_secs();
        let rec = PlayerRecord {
            id: id.clone(),
            username: username.to_string(),
            email: if email.is_empty() { None } else { Some(email.to_string()) },
            password_hash: Some(hash.to_string()),
            role: "player".into(),
            verified_at: None,
            deleted_at: None,
            banned_at: None,
            banned_reason: None,
            failed_login_attempts: 0,
            locked_until: None,
            created_at: now,
        };
        if !email.is_empty() {
            emails.insert(email.to_string(), id.clone());
        }
        players.insert(id.clone(), rec.clone());
        Ok(rec)
    }

    fn by_login(&self, login: &str) -> Option<PlayerRecord> {
        let players = self.players.lock().unwrap();
        if let Some(rec) = players.values().find(|p| p.username == login) {
            return Some(rec.clone());
        }
        let emails = self.by_email.lock().unwrap();
        if let Some(pid) = emails.get(login) {
            return players.get(pid).cloned();
        }
        None
    }

    fn by_id(&self, id: &str) -> Option<PlayerRecord> {
        self.players.lock().unwrap().get(id).cloned()
    }

    fn by_email(&self, email: &str) -> Option<PlayerRecord> {
        let emails = self.by_email.lock().unwrap();
        let pid = emails.get(email)?;
        self.players.lock().unwrap().get(pid).cloned()
    }

    fn by_provider(&self, provider: &str, provider_uid: &str) -> Option<PlayerRecord> {
        let providers = self.by_provider.lock().unwrap();
        let pid = providers.get(&(provider.to_string(), provider_uid.to_string()))?;
        self.players.lock().unwrap().get(pid).cloned()
    }

    fn link_provider(&self, player_id: &str, provider: &str, provider_uid: &str, _provider_email: &str) {
        self.by_provider
            .lock()
            .unwrap()
            .insert((provider.to_string(), provider_uid.to_string()), player_id.to_string());
    }

    fn update_hash(&self, player_id: &str, hash: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.password_hash = Some(hash.to_string());
        }
    }

    fn inc_fails(&self, player_id: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.failed_login_attempts += 1;
        }
    }

    fn clear_fails(&self, player_id: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.failed_login_attempts = 0;
            rec.locked_until = None;
        }
    }

    fn set_verified(&self, player_id: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.verified_at = Some(now_secs());
        }
    }

    fn set_deleted(&self, player_id: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.deleted_at = Some(now_secs());
        }
    }

    fn cancel_deletion(&self, player_id: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.deleted_at = None;
        }
    }

    fn set_lock(&self, player_id: &str, locked_until: i64) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.locked_until = Some(locked_until);
        }
    }

    fn ban(&self, player_id: &str, reason: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.banned_at = Some(now_secs());
            rec.banned_reason = Some(reason.to_string());
        }
    }

    fn unban(&self, player_id: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.banned_at = None;
            rec.banned_reason = None;
        }
    }

    fn set_role(&self, player_id: &str, role: &str) {
        let mut players = self.players.lock().unwrap();
        if let Some(rec) = players.get_mut(player_id) {
            rec.role = role.to_string();
        }
    }

    fn list(&self, page: u32, per_page: u32) -> Vec<PlayerRecord> {
        let players = self.players.lock().unwrap();
        let mut all: Vec<_> = players.values().cloned().collect();
        all.sort_by_key(|a| a.created_at);
        let start = ((page.saturating_sub(1)) * per_page) as usize;
        all.into_iter().skip(start).take(per_page as usize).collect()
    }

    fn count(&self) -> usize {
        self.players.lock().unwrap().len()
    }
}

// ---------------------------------------------------------------------------
// SessionStore with revoke support
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    pub player_id: String,
    pub universe: UniverseTier,
}

pub trait SessionStore: Send + Sync {
    fn issue(&self, info: SessionInfo) -> String;
    fn resolve(&self, token: &str) -> Option<SessionInfo>;
    fn revoke(&self, token: &str);
    /// Reject all sessions for a player (used on password reset / ban).
    fn revoke_all_for_player(&self, player_id: &str);
}

#[derive(Default)]
pub struct MemorySessionStore {
    tokens: Mutex<HashMap<String, SessionInfo>>,
    counter: AtomicU64,
}

impl SessionStore for MemorySessionStore {
    fn issue(&self, info: SessionInfo) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let token = format!("{n:x}_{nanos:x}_{}", rand_hex(16));
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

    fn revoke(&self, token: &str) {
        self.tokens.lock().expect("session store poisoned").remove(token);
    }

    fn revoke_all_for_player(&self, player_id: &str) {
        let mut tokens = self.tokens.lock().expect("session store poisoned");
        tokens.retain(|_, info| info.player_id != player_id);
    }
}

// ---------------------------------------------------------------------------
// Temp token store (for 2FA challenge)
// ---------------------------------------------------------------------------

pub struct TempTokenStore {
    tokens: Mutex<HashMap<String, TempTokenEntry>>,
}

struct TempTokenEntry {
    player_id: String,
    expires_at: i64,
}

impl Default for TempTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TempTokenStore {
    pub fn new() -> Self {
        TempTokenStore {
            tokens: Mutex::new(HashMap::new()),
        }
    }

    pub fn issue(&self, player_id: &str, ttl_mins: u32) -> String {
        let token = generate_crypto_token(32);
        let expires_at = now_secs() + (ttl_mins as i64 * 60);
        self.tokens
            .lock()
            .unwrap()
            .insert(token.clone(), TempTokenEntry {
                player_id: player_id.to_string(),
                expires_at,
            });
        token
    }

    pub fn consume(&self, token: &str) -> Option<String> {
        let mut tokens = self.tokens.lock().unwrap();
        // Clean expired tokens while we hold the lock
        tokens.retain(|_, entry| now_secs() <= entry.expires_at);
        let entry = tokens.remove(token)?;
        if now_secs() > entry.expires_at {
            return None;
        }
        Some(entry.player_id)
    }
}

// ---------------------------------------------------------------------------
// Cryptographic helpers
// ---------------------------------------------------------------------------

/// SHA-256 hash, hex-encoded.
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn rand_hex(len: usize) -> String {
    use rand::RngCore;
    let mut bytes = vec![0u8; len];
    // OsRng from rand_core (re-exported by aes-gcm and rand)
    use rand::rngs::OsRng;
    OsRng.fill_bytes(&mut bytes);
    hex::encode(&bytes)
}

fn generate_crypto_token(byte_len: usize) -> String {
    rand_hex(byte_len)
}

/// Generate a split token: (full_token, selector, verifier_hash).
/// The full token is selector:verifier. The selector is stored as plaintext
/// (it indexes the row). The verifier is compared against verifier_hash.
pub fn generate_split_token() -> (String, String, String) {
    let selector = rand_hex(16);
    let verifier = rand_hex(32);
    let verifier_hash = sha256_hex(&verifier);
    let full = format!("{selector}:{verifier}");
    (full, selector, verifier_hash)
}

/// Verify a split token against its stored hash (constant-time).
pub fn verify_split_token(full: &str, stored_hash: &str) -> bool {
    let (_selector, verifier) = full.split_once(':').unwrap_or(("", ""));
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let computed = hasher.finalize();
    let stored = hex::decode(stored_hash).unwrap_or_default();
    computed[..].ct_eq(&stored).unwrap_u8() == 1
}

// ---------------------------------------------------------------------------
// Argon2id password hashing
// ---------------------------------------------------------------------------

fn hash_password(password: &str, cfg: &AuthConfig) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let params = argon2::Params::new(
        cfg.argon2_memory_kib * 1024,
        cfg.argon2_iterations,
        cfg.argon2_parallelism,
        Some(32),
    )
    .map_err(|e| format!("argon2 params: {e}"))?;
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    );
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("argon2 hash: {e}"))?
        .to_string();
    Ok(hash)
}

fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = argon2::PasswordHash::new(hash);
    let Ok(parsed) = parsed else { return false };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

// ---------------------------------------------------------------------------
// TOTP helpers
// ---------------------------------------------------------------------------

fn encrypt_secret(plaintext: &str) -> Result<String, String> {
    let key_bytes = secret_key().ok_or_else(|| "REACHLOCK_SECRET_KEY not set".to_string())?;
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("encrypt: {e}"))?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(hex::encode(&out))
}

fn decrypt_secret(encrypted: &str) -> Result<String, String> {
    let key_bytes = secret_key().ok_or_else(|| "REACHLOCK_SECRET_KEY not set".to_string())?;
    let data = hex::decode(encrypted).map_err(|e| format!("hex decode: {e}"))?;
    if data.len() < 12 {
        return Err("truncated ciphertext".into());
    }
    let (nonce_bytes, ct) = data.split_at(12);
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ct)
        .map_err(|e| format!("decrypt: {e}"))?;
    String::from_utf8(plaintext).map_err(|e| format!("utf8: {e}"))
}

fn generate_totp_secret_inner() -> (String, String) {
    let secret_bytes = Secret::generate_secret()
        .to_bytes()
        .expect("TOTP secret generation");
    let secret_b32 = base64::engine::general_purpose::STANDARD_NO_PAD.encode(&secret_bytes);
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("ReachLock".to_string()),
        "player".to_string(),
    )
    .expect("TOTP params valid");
    let uri = totp.get_url();
    (secret_b32, uri)
}

fn verify_totp_code(secret_b32: &str, code: &str) -> bool {
    let secret_bytes = match base64::engine::general_purpose::STANDARD_NO_PAD.decode(secret_b32) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let totp = match TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        None::<String>,
        "reachlock".to_string(),
    ) {
        Ok(t) => t,
        Err(_) => return false,
    };
    totp.check(code.trim(), 0)
}

/// Generate QR code PNG as a data URI (base64-encoded PNG).
fn totp_qr_data_uri(otpauth_uri: &str) -> String {
    use qrcode::render::svg;
    use qrcode::QrCode;
    if let Ok(code) = QrCode::new(otpauth_uri) {
        let svg_str = code.render::<svg::Color>().build();
        let b64 = base64::engine::general_purpose::STANDARD.encode(svg_str.as_bytes());
        format!("data:image/svg+xml;base64,{b64}")
    } else {
        String::new()
    }
}

/// Hash a recovery code with argon2id (one-way, reuses password hasher).
fn hash_recovery_code(code: &str) -> Result<String, String> {
    hash_password(code, &AuthConfig::default())
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegisterResponse {
    pub token: String,
    pub player_id: String,
    pub requires_verification: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_2fa: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TfaEnableRequest {}

#[derive(Debug, Clone, Serialize)]
pub struct TfaEnableResponse {
    pub secret: String,
    pub qr_code_url: String,
    pub recovery_codes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TfaVerifyRequest {
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TfaDisableRequest {
    pub code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TfaChallengeRequest {
    pub temp_token: String,
    pub code: Option<String>,
    pub recovery_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OAuthDeviceResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthTokenRequest {
    pub device_code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OAuthTokenResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<bool>,
}

/// Dev auth — preserved from S03.
#[derive(Debug, Clone, Deserialize)]
pub struct DevLoginRequest {
    pub username: String,
    #[serde(default = "default_universe")]
    pub universe: UniverseTier,
}

fn default_universe() -> UniverseTier {
    UniverseTier::Classic
}

#[derive(Debug, Clone, Serialize)]
pub struct DevLoginResponse {
    pub token: String,
    pub player_id: String,
}

// ---------------------------------------------------------------------------
// OAuth2 device flow helpers
// ---------------------------------------------------------------------------

/// Google's OAuth2 device authorization endpoint.
const GOOGLE_DEVICE_AUTH: &str = "https://accounts.google.com/o/oauth2/device/code";
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USER_INFO: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

/// GitHub's OAuth2 device authorization endpoint.
const GITHUB_DEVICE_AUTH: &str = "https://github.com/login/device/code";
const GITHUB_TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_INFO: &str = "https://api.github.com/user";

fn oauth_client_config(provider: &str) -> Option<(String, String)> {
    match provider {
        "google" => {
            let id = std::env::var("REACHLOCK_OAUTH_GOOGLE_CLIENT_ID").ok()?;
            let secret = std::env::var("REACHLOCK_OAUTH_GOOGLE_CLIENT_SECRET").ok()?;
            Some((id, secret))
        }
        "github" => {
            let id = std::env::var("REACHLOCK_OAUTH_GITHUB_CLIENT_ID").ok()?;
            let secret = std::env::var("REACHLOCK_OAUTH_GITHUB_CLIENT_SECRET").ok()?;
            Some((id, secret))
        }
        _ => None,
    }
}

fn device_auth_url(provider: &str) -> &'static str {
    match provider {
        "google" => GOOGLE_DEVICE_AUTH,
        "github" => GITHUB_DEVICE_AUTH,
        _ => "",
    }
}

fn token_endpoint(provider: &str) -> &'static str {
    match provider {
        "google" => GOOGLE_TOKEN_ENDPOINT,
        "github" => GITHUB_TOKEN_ENDPOINT,
        _ => "",
    }
}

async fn oauth_device_code(provider: &str) -> Result<OAuthDeviceResponse, String> {
    let (client_id, _client_secret) =
        oauth_client_config(provider).ok_or_else(|| format!("{provider} OAuth not configured"))?;

    let client = reqwest::Client::new();
    let params = [("client_id", client_id.as_str()), ("scope", "openid email profile")];
    let resp: serde_json::Value = client
        .post(device_auth_url(provider))
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("OAuth device req: {e}"))?
        .json()
        .await
        .map_err(|e| format!("OAuth device resp: {e}"))?;

    Ok(OAuthDeviceResponse {
        device_code: resp["device_code"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        user_code: resp["user_code"].as_str().unwrap_or("").to_string(),
        verification_uri: resp["verification_uri"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        expires_in: resp["expires_in"].as_u64().unwrap_or(300),
    })
}

struct OAuthUser {
    sub: String,
    email: Option<String>,
}

async fn oauth_poll_token(
    provider: &str,
    device_code: &str,
) -> Result<Option<OAuthUser>, String> {
    let (client_id, client_secret) =
        oauth_client_config(provider).ok_or_else(|| format!("{provider} OAuth not configured"))?;

    let client = reqwest::Client::new();
    let params = [
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("device_code", device_code),
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
    ];

    let resp: serde_json::Value = client
        .post(token_endpoint(provider))
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("OAuth poll: {e}"))?
        .json()
        .await
        .map_err(|e| format!("OAuth poll resp: {e}"))?;

    if resp.get("access_token").is_none() {
        let error = resp["error"].as_str().unwrap_or("authorization_pending");
        if error == "authorization_pending" || error == "slow_down" {
            return Ok(None);
        }
        return Err(format!("OAuth error: {error}"));
    }

    let access_token = resp["access_token"].as_str().unwrap_or("");
    // Fetch user info from the provider
    let user_info_endpoint = match provider {
        "google" => GOOGLE_USER_INFO,
        "github" => GITHUB_USER_INFO,
        _ => return Err("unknown provider".into()),
    };
    let user_resp: serde_json::Value = client
        .get(user_info_endpoint)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .map_err(|e| format!("OAuth userinfo: {e}"))?
        .json()
        .await
        .map_err(|e| format!("OAuth userinfo resp: {e}"))?;

    Ok(Some(OAuthUser {
        sub: user_resp["id"]
            .as_str()
            .or_else(|| user_resp["sub"].as_str())
            .unwrap_or("")
            .to_string(),
        email: user_resp["email"].as_str().map(|s| s.to_string()),
    }))
}

// ---------------------------------------------------------------------------
// Auth handler helper: extract bearer player_id
// ---------------------------------------------------------------------------

pub fn resolve_player_id(
    headers: &axum::http::HeaderMap,
    sessions: &dyn SessionStore,
) -> Option<String> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?
        .to_string();
    sessions.resolve(&token).map(|info| info.player_id)
}

// ---------------------------------------------------------------------------
// Auth endpoints
// ---------------------------------------------------------------------------

pub async fn register(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<RegisterRequest>,
) -> Result<axum::Json<RegisterResponse>, crate::ws::AppError> {
    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    if REGISTER_LIMITER.is_limited(&format!("register:{}", client_ip)) {
        return Err(app_err(429, "too many requests"));
    }
    let cfg = state.auth_config.read().unwrap();

    // Validate input
    if body.username.len() < 3 || body.username.len() > 32 {
        return Err(app_err(400, "username must be 3-32 characters"));
    }
    if !body
        .username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_')
    {
        return Err(app_err(400, "username may only contain letters, digits, and underscores"));
    }
    if body.password.len() < cfg.min_password_length {
        return Err(app_err(400, &format!("password must be at least {} characters", cfg.min_password_length)));
    }
    if !body.email.contains('@') || !body.email.contains('.') {
        return Err(app_err(400, "invalid email address"));
    }

    let hash = hash_password(&body.password, &cfg).map_err(|e| app_err(500, &e))?;
    let record = state.players.create(&body.username, &body.email, &hash).map_err(|e| app_err(409, &e))?;

    let token = state.sessions.issue(SessionInfo {
        player_id: record.id.clone(),
        universe: UniverseTier::Classic,
    });

    // Send verification email
    let (full, _selector, _vhash) = generate_split_token();
    // Store in memory for now — in production this goes to email_verification_tokens table
    // via PgPlayerStore. For the memory store, we hold it alongside the player record
    // in a separate map. For simplicity in the in-memory path, we'll use a shared map.
    state.verification_tokens.lock().unwrap().insert(full.clone(), (record.id.clone(), now_secs() + 24 * 3600));
    let token_escaped = full; // hex tokens are HTML-safe
    let body_html = format!(
        "<h2>Welcome to ReachLock!</h2><p>Verify your email: <a href='https://reachlock.example/auth/verify?token={token_escaped}'>click here</a></p><p>Your verification code: <code>{token_escaped}</code></p>"
    );
    let _ = state.email.send(&body.email, "Verify your ReachLock account", &body_html);

    state.audit.record(crate::services::audit::AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        action: "register".into(),
        target: record.id.clone(),
        detail: format!("username={}", body.username),
        admin_key_hash: String::new(),
    });

    Ok(axum::Json(RegisterResponse {
        token,
        player_id: record.id,
        requires_verification: true,
    }))
}

pub async fn login(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<LoginRequest>,
) -> Result<axum::Json<LoginResponse>, crate::ws::AppError> {
    if LOGIN_LIMITER.is_limited(&body.login) {
        return Err(app_err(429, "too many requests"));
    }
    let cfg = state.auth_config.read().unwrap();
    let Some(record) = state.players.by_login(&body.login) else {
        // Prevent timing enumeration: run dummy verify against a known hash
        let _ = verify_password(&body.password, "$argon2id$v=19$m=65536,t=3,p=4$c2FsdHlzYWx0Zm9yZHVtbXk$c2FsdHlzYWx0Zm9yZHVtbXlzYWx0");
        return Err(app_err(401, "invalid login credentials"));
    };

    // Check lockout
    if let Some(locked_until) = record.locked_until {
        if now_secs() < locked_until {
            let remaining = locked_until - now_secs();
            return Err(app_err(403, &format!("account locked, try again in {remaining}s")));
        }
    }

    // Check banned
    if record.banned_at.is_some() {
        return Err(app_err(403, "account is banned"));
    }

    // Check deleted
    if record.deleted_at.is_some() {
        return Err(app_err(403, "account is scheduled for deletion"));
    }

    // Verify password
    let Some(ref pw_hash) = record.password_hash else {
        return Err(app_err(401, "invalid login credentials"));
    };
    if !verify_password(&body.password, pw_hash) {
        state.players.inc_fails(&record.id);
        if let Some(updated) = state.players.by_id(&record.id) {
            if updated.failed_login_attempts >= cfg.account_lockout_threshold
                && cfg.account_lockout_threshold > 0
            {
                let lock_until = now_secs() + (cfg.account_lockout_duration_mins as i64 * 60);
                state.players.set_lock(&record.id, lock_until);
            }
        }
        return Err(app_err(401, "invalid login credentials"));
    }

    // Check verification
    if record.verified_at.is_none() {
        return Err(app_err(403, "verify your email before logging in"));
    }

    // Clear failed attempts
    state.players.clear_fails(&record.id);

    // Check 2FA
    // For now, TOTP secrets are in-memory map on state
    if state.totp_secrets.lock().unwrap().contains_key(&record.id) {
        let temp = state.temp_tokens.issue(&record.id, cfg.temp_token_ttl_mins);
        return Ok(axum::Json(LoginResponse {
            token: None,
            player_id: Some(record.id),
            requires_2fa: Some(true),
            temp_token: Some(temp),
        }));
    }

    let token = state.sessions.issue(SessionInfo {
        player_id: record.id.clone(),
        universe: UniverseTier::Classic,
    });

    state.audit.record(crate::services::audit::AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        action: "login".into(),
        target: record.id,
        detail: String::new(),
        admin_key_hash: String::new(),
    });

    Ok(axum::Json(LoginResponse {
        token: Some(token),
        player_id: None,
        requires_2fa: None,
        temp_token: None,
    }))
}

pub async fn logout(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::http::StatusCode, crate::ws::AppError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| app_err(401, "missing bearer token"))?;
    state.sessions.revoke(&token);
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn verify_email(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<VerifyEmailRequest>,
) -> Result<axum::Json<serde_json::Value>, crate::ws::AppError> {
    let mut tokens = state.verification_tokens.lock().unwrap();
    let &(ref player_id, expires_at) = tokens.get(&body.token).ok_or_else(|| app_err(400, "invalid or expired token"))?;
    if now_secs() > expires_at {
        tokens.remove(&body.token);
        return Err(app_err(400, "invalid or expired token"));
    }
    let player_id = player_id.clone();
    drop(tokens);
    state.players.set_verified(&player_id);
    state.verification_tokens.lock().unwrap().remove(&body.token);
    Ok(axum::Json(serde_json::json!({"verified": true})))
}

pub async fn resend_verification(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<serde_json::Value>, crate::ws::AppError> {
    let player_id =
        resolve_player_id(&headers, &*state.sessions).ok_or_else(|| app_err(401, "unauthorized"))?;
    let record = state.players.by_id(&player_id).ok_or_else(|| app_err(404, "player not found"))?;
    if record.verified_at.is_some() {
        return Ok(axum::Json(serde_json::json!({"verified": true})));
    }
    let (full, _selector, _vhash) = generate_split_token();
    let email = record.email.as_deref().unwrap_or("unknown");
    state.verification_tokens.lock().unwrap().insert(full.clone(), (player_id, now_secs() + 24 * 3600));
    let token_escaped = full; // hex tokens are HTML-safe
    let body_html = format!(
        "<h2>ReachLock Email Verification</h2><p>Verify: <a href='https://reachlock.example/auth/verify?token={token_escaped}'>click here</a></p><p>Code: <code>{token_escaped}</code></p>"
    );
    let _ = state.email.send(email, "Verify your ReachLock account", &body_html);
    Ok(axum::Json(serde_json::json!({"sent": true})))
}

pub async fn forgot_password(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<ForgotPasswordRequest>,
) -> axum::Json<serde_json::Value> {
    if FORGOT_PW_LIMITER.is_limited(&body.email) {
        let always = "if an account with that email exists, a reset link has been sent";
        return axum::Json(serde_json::json!({"message": always}));
    }
    let always = "if an account with that email exists, a reset link has been sent";
    let Some(record) = state.players.by_email(&body.email) else {
        return axum::Json(serde_json::json!({"message": always}));
    };
    let (full, _selector, _vhash) = generate_split_token();
    state.reset_tokens.lock().unwrap().insert(full.clone(), (record.id.clone(), now_secs() + 24 * 3600));
    let token_escaped = full; // hex tokens are HTML-safe
    let body_html = format!(
        "<h2>ReachLock Password Reset</h2><p><a href='https://reachlock.example/auth/reset?token={token_escaped}'>Reset password</a></p><p>Code: <code>{token_escaped}</code></p>"
    );
    let _ = state.email.send(&body.email, "Reset your ReachLock password", &body_html);
    axum::Json(serde_json::json!({"message": always}))
}

pub async fn reset_password(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<ResetPasswordRequest>,
) -> Result<axum::Json<serde_json::Value>, crate::ws::AppError> {
    let cfg = state.auth_config.read().unwrap();
    if body.new_password.len() < cfg.min_password_length {
        return Err(app_err(400, &format!("password must be at least {} characters", cfg.min_password_length)));
    }
    let mut tokens = state.reset_tokens.lock().unwrap();
    let (player_id, expires_at) = tokens.remove(&body.token).ok_or_else(|| app_err(400, "invalid or expired token"))?;
    if now_secs() > expires_at {
        return Err(app_err(400, "invalid or expired token"));
    }
    drop(tokens);
    let hash = hash_password(&body.new_password, &cfg).map_err(|e| app_err(500, &e))?;
    state.players.update_hash(&player_id, &hash);
    state.sessions.revoke_all_for_player(&player_id);
    Ok(axum::Json(serde_json::json!({"reset": true})))
}

pub async fn delete_account(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<DeleteAccountRequest>,
) -> Result<axum::Json<serde_json::Value>, crate::ws::AppError> {
    let player_id =
        resolve_player_id(&headers, &*state.sessions).ok_or_else(|| app_err(401, "unauthorized"))?;
    let record = state.players.by_id(&player_id).ok_or_else(|| app_err(404, "player not found"))?;
    let Some(ref pw_hash) = record.password_hash else {
        return Err(app_err(400, "password login not configured for this account"));
    };
    if !verify_password(&body.password, pw_hash) {
        return Err(app_err(401, "invalid password"));
    }
    let grace_days = state.auth_config.read().unwrap().deletion_grace_period_days;
    state.players.set_deleted(&player_id);
    state.sessions.revoke_all_for_player(&player_id);

    state.audit.record(crate::services::audit::AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        action: "delete_account".into(),
        target: player_id,
        detail: format!("grace_period_days={grace_days}"),
        admin_key_hash: String::new(),
    });

    Ok(axum::Json(serde_json::json!({
        "deleted": true,
        "grace_period_days": grace_days,
        "message": format!("account scheduled for deletion. grace period: {grace_days} days")
    })))
}

pub async fn cancel_deletion(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<serde_json::Value>, crate::ws::AppError> {
    let player_id =
        resolve_player_id(&headers, &*state.sessions).ok_or_else(|| app_err(401, "unauthorized"))?;
    state.players.cancel_deletion(&player_id);

    state.audit.record(crate::services::audit::AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        action: "cancel_deletion".into(),
        target: player_id,
        detail: String::new(),
        admin_key_hash: String::new(),
    });

    Ok(axum::Json(serde_json::json!({"cancelled": true})))
}

// ---------------------------------------------------------------------------
// TOTP 2FA endpoints
// ---------------------------------------------------------------------------

pub async fn tfa_enable(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
) -> Result<axum::Json<TfaEnableResponse>, crate::ws::AppError> {
    if secret_key().is_none() {
        return Err(app_err(503, "2FA disabled — set REACHLOCK_SECRET_KEY"));
    }
    let player_id =
        resolve_player_id(&headers, &*state.sessions).ok_or_else(|| app_err(401, "unauthorized"))?;

    let (secret_b32, otpauth_uri) = generate_totp_secret_inner();
    let encrypted = encrypt_secret(&secret_b32).map_err(|e| app_err(500, &e))?;

    // Store encrypted secret
    state.totp_secrets.lock().unwrap().insert(player_id.clone(), encrypted);

    // Generate recovery codes
    let mut recovery_codes = Vec::new();
    let mut code_strings = Vec::new();
    for _ in 0..10 {
        let code = generate_crypto_token(4).to_uppercase();
        let code_str = format!("{}-{}", &code[..4], &code[4..8]);
        let hashed = hash_recovery_code(&code_str).unwrap_or_default();
        state.totp_recovery_codes.lock().unwrap().push((player_id.clone(), hashed));
        recovery_codes.push(code_str.clone());
        code_strings.push(code_str);
    }

    // Also store the secret for 2FA challenge (we already did above via totp_secrets map)

    let qr_url = totp_qr_data_uri(&otpauth_uri);

    Ok(axum::Json(TfaEnableResponse {
        secret: secret_b32,
        qr_code_url: qr_url,
        recovery_codes: code_strings,
    }))
}

pub async fn tfa_verify(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<TfaVerifyRequest>,
) -> Result<axum::Json<serde_json::Value>, crate::ws::AppError> {
    let player_id =
        resolve_player_id(&headers, &*state.sessions).ok_or_else(|| app_err(401, "unauthorized"))?;

    let secrets = state.totp_secrets.lock().unwrap();
    let encrypted = secrets.get(&player_id).ok_or_else(|| app_err(400, "2FA not enabled"))?;
    let secret_b32 = decrypt_secret(encrypted).map_err(|e| app_err(500, &e))?;
    drop(secrets);

    if verify_totp_code(&secret_b32, &body.code) {
        // Mark as enabled — in the in-memory store, it's already stored.
        Ok(axum::Json(serde_json::json!({"enabled": true})))
    } else {
        Err(app_err(400, "invalid TOTP code"))
    }
}

pub async fn tfa_disable(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<TfaDisableRequest>,
) -> Result<axum::Json<serde_json::Value>, crate::ws::AppError> {
    let player_id =
        resolve_player_id(&headers, &*state.sessions).ok_or_else(|| app_err(401, "unauthorized"))?;

    let secrets = state.totp_secrets.lock().unwrap();
    let encrypted = secrets.get(&player_id).ok_or_else(|| app_err(400, "2FA not enabled"))?;
    let secret_b32 = decrypt_secret(encrypted).map_err(|e| app_err(500, &e))?;
    drop(secrets);

    if !verify_totp_code(&secret_b32, &body.code) {
        return Err(app_err(400, "invalid TOTP code"));
    }

    state.totp_secrets.lock().unwrap().remove(&player_id);
    state.totp_recovery_codes.lock().unwrap().retain(|(pid, _)| pid != &player_id);

    Ok(axum::Json(serde_json::json!({"disabled": true})))
}

pub async fn tfa_challenge(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<TfaChallengeRequest>,
) -> Result<axum::Json<LoginResponse>, crate::ws::AppError> {
    let player_id = state
        .temp_tokens
        .consume(&body.temp_token)
        .ok_or_else(|| app_err(400, "invalid or expired temp token"))?;

    let secrets = state.totp_secrets.lock().unwrap();
    let encrypted = secrets.get(&player_id).ok_or_else(|| app_err(400, "2FA not enabled"))?;
    let secret_b32 = decrypt_secret(encrypted).map_err(|e| app_err(500, &e))?;
    drop(secrets);

    if let Some(code) = &body.code {
        if verify_totp_code(&secret_b32, code) {
            let token = state.sessions.issue(SessionInfo {
                player_id: player_id.clone(),
                universe: UniverseTier::Classic,
            });
            return Ok(axum::Json(LoginResponse {
                token: Some(token),
                player_id: Some(player_id),
                requires_2fa: None,
                temp_token: None,
            }));
        }
    }

    if let Some(rc) = &body.recovery_code {
        let codes = state.totp_recovery_codes.lock().unwrap();
        // Find a matching recovery code (argon2id verify each)
        let mut found = false;
        for (pid, hash) in codes.iter() {
            if pid == &player_id {
                let parsed = argon2::PasswordHash::new(hash);
                if let Ok(parsed) = parsed {
                    if Argon2::default().verify_password(rc.as_bytes(), &parsed).is_ok() {
                        found = true;
                        break;
                    }
                }
            }
        }
        drop(codes);
        if found {
            // Burn the code and generate a replacement
            let new_code = generate_crypto_token(4).to_uppercase();
            let new_code_str = format!("{}-{}", &new_code[..4], &new_code[4..8]);
            let new_hash = hash_recovery_code(&new_code_str).unwrap_or_default();
            // Remove old, add new
            state.totp_recovery_codes.lock().unwrap().retain(|(pid, _)| pid != &player_id);
            state.totp_recovery_codes.lock().unwrap().push((player_id.clone(), new_hash));

            let token = state.sessions.issue(SessionInfo {
                player_id: player_id.clone(),
                universe: UniverseTier::Classic,
            });
            return Ok(axum::Json(LoginResponse {
                token: Some(token),
                player_id: Some(player_id),
                requires_2fa: None,
                temp_token: None,
            }));
        }
    }

    Err(app_err(400, "invalid 2FA code"))
}

// ---------------------------------------------------------------------------
// OAuth endpoints
// ---------------------------------------------------------------------------

pub async fn oauth_google_device(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
) -> Result<axum::Json<OAuthDeviceResponse>, crate::ws::AppError> {
    let resp = oauth_device_code("google").await.map_err(|e| app_err(503, &e))?;
    state.oauth_flows.lock().unwrap().insert(resp.device_code.clone(), "google".into());
    Ok(axum::Json(resp))
}

pub async fn oauth_github_device(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
) -> Result<axum::Json<OAuthDeviceResponse>, crate::ws::AppError> {
    let resp = oauth_device_code("github").await.map_err(|e| app_err(503, &e))?;
    state.oauth_flows.lock().unwrap().insert(resp.device_code.clone(), "github".into());
    Ok(axum::Json(resp))
}

pub async fn oauth_token(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<OAuthTokenRequest>,
) -> Result<axum::Json<OAuthTokenResponse>, crate::ws::AppError> {
    let provider = state
        .oauth_flows
        .lock()
        .unwrap()
        .get(&body.device_code)
        .cloned()
        .ok_or_else(|| app_err(400, "invalid device code"))?;

    match oauth_poll_token(&provider, &body.device_code).await {
        Ok(Some(user)) => {
            state.oauth_flows.lock().unwrap().remove(&body.device_code);

            // Find or create player
            let player_id =
                if let Some(rec) = state.players.by_provider(&provider, &user.sub) {
                    rec.id
                } else if let Some(ref user_email) = user.email {
                    if let Some(rec) = state.players.by_email(user_email) {
                        state.players.link_provider(&rec.id, &provider, &user.sub, user_email);
                        rec.id
                    } else {
                        create_oauth_player(&state, &provider, &user.sub, user_email).await?
                    }
                } else {
                    create_oauth_player(&state, &provider, &user.sub, "").await?
                };

            let token = state.sessions.issue(SessionInfo {
                player_id: player_id.clone(),
                universe: UniverseTier::Classic,
            });

            Ok(axum::Json(OAuthTokenResponse {
                token: Some(token),
                player_id: Some(player_id),
                pending: None,
            }))
        }
        Ok(None) => Ok(axum::Json(OAuthTokenResponse {
            token: None,
            player_id: None,
            pending: Some(true),
        })),
        Err(e) => Err(app_err(502, &e)),
    }
}

// ---------------------------------------------------------------------------
// Dev auth (preserved from S03)
// ---------------------------------------------------------------------------

pub async fn dev_login(
    state: axum::extract::State<std::sync::Arc<crate::ws::AppState>>,
    axum::Json(body): axum::Json<DevLoginRequest>,
) -> Result<axum::Json<DevLoginResponse>, crate::ws::AppError> {
    if state.auth_required.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(app_err(403, "dev auth disabled in production mode"));
    }
    // Ensure player exists in the store
    let record = state
        .players
        .by_login(&body.username)
        .unwrap_or_else(|| {
            state
                .players
                .create(&body.username, "dev@local", "")
                .unwrap_or_else(|_| state.players.by_login(&body.username).unwrap())
        });
    let token = state.sessions.issue(SessionInfo {
        player_id: record.id.clone(),
        universe: body.universe,
    });
    Ok(axum::Json(DevLoginResponse {
        token,
        player_id: record.id,
    }))
}

// ---------------------------------------------------------------------------
// Helper: create a player from OAuth data
// ---------------------------------------------------------------------------

async fn create_oauth_player(
    state: &std::sync::Arc<crate::ws::AppState>,
    provider: &str,
    sub: &str,
    email: &str,
) -> Result<String, crate::ws::AppError> {
    let username = format!("oauth_{}_{}", provider, &sub[..8.min(sub.len())]);
    match state.players.create(&username, email, "") {
        Ok(rec) => {
            state.players.link_provider(&rec.id, provider, sub, email);
            state.players.set_verified(&rec.id);
            Ok(rec.id)
        }
        Err(_) => {
            let username2 = format!("oauth_{}_{}", provider, &sub[..12.min(sub.len())]);
            let rec = state.players.create(&username2, email, "").map_err(|e| app_err(500, &e))?;
            state.players.link_provider(&rec.id, provider, sub, email);
            state.players.set_verified(&rec.id);
            Ok(rec.id)
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: build an error response for Axum
// ---------------------------------------------------------------------------

pub fn app_err(status: u16, msg: &str) -> crate::ws::AppError {
    crate::ws::AppError {
        status,
        message: msg.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Postgres-backed PlayerStore + SessionStore (behind `postgres` feature)
// ---------------------------------------------------------------------------

#[cfg(feature = "postgres")]
pub mod pg {
    use super::*;
    use sqlx::PgPool;

    pub struct PgPlayerStore {
        pool: PgPool,
        runtime: tokio::runtime::Handle,
    }

    impl PgPlayerStore {
        pub fn new(pool: PgPool) -> Self {
            PgPlayerStore {
                pool,
                runtime: tokio::runtime::Handle::current(),
            }
        }
    }

    impl PlayerStore for PgPlayerStore {
        fn create(&self, username: &str, email: &str, hash: &str) -> Result<PlayerRecord, String> {
            let pool = self.pool.clone();
            let username = username.to_string();
            let email = email.to_string();
            let hash = hash.to_string();
            self.runtime.block_on(async move {
                let row: Result<(String,), sqlx::Error> = sqlx::query_as(
                    "INSERT INTO players (username, email, password_hash)
                     VALUES ($1, $2, $3) RETURNING id",
                )
                .bind(&username)
                .bind(&email)
                .bind(&hash)
                .fetch_one(&pool)
                .await;
                match row {
                    Ok((id,)) => Ok(PlayerRecord {
                        id,
                        username,
                        email: Some(email),
                        password_hash: Some(hash),
                        role: "player".into(),
                        verified_at: None,
                        deleted_at: None,
                        banned_at: None,
                        banned_reason: None,
                        failed_login_attempts: 0,
                        locked_until: None,
                        created_at: now_secs(),
                    }),
                    Err(e) => {
                        if let Some(db_err) = e.as_database_error() {
                            if db_err.code().as_deref() == Some("23505") {
                                return Err("username or email already taken".into());
                            }
                        }
                        Err(format!("db: {e}"))
                    }
                }
            })
        }

        fn by_login(&self, login: &str) -> Option<PlayerRecord> {
            let pool = self.pool.clone();
            let login = login.to_string();
            self.runtime.block_on(async move {
                sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, Option<i64>, Option<i64>, Option<i64>, Option<String>, i32, Option<i64>, i64)>(
                    "SELECT id, username, email, password_hash, role,
                            EXTRACT(EPOCH FROM verified_at)::bigint,
                            EXTRACT(EPOCH FROM deleted_at)::bigint,
                            EXTRACT(EPOCH FROM banned_at)::bigint,
                            banned_reason,
                            failed_login_attempts,
                            EXTRACT(EPOCH FROM locked_until)::bigint,
                            EXTRACT(EPOCH FROM created_at)::bigint
                     FROM players
                     WHERE username = $1 OR email = $1",
                )
                .bind(&login)
                .fetch_optional(&pool)
                .await
                .ok()?
                .map(|(id, uname, email, pw_hash, role, verified, deleted, banned, reason, fails, locked, created)| {
                    PlayerRecord {
                        id, username: uname, email, password_hash: pw_hash,
                        role, verified_at: verified.filter(|&v| v > 0),
                        deleted_at: deleted.filter(|&v| v > 0),
                        banned_at: banned.filter(|&v| v > 0),
                        banned_reason: reason,
                        failed_login_attempts: fails as u32,
                        locked_until: locked.filter(|&v| v > 0),
                        created_at: created,
                    }
                })
            })
        }

        fn by_id(&self, id: &str) -> Option<PlayerRecord> {
            let pool = self.pool.clone();
            let id = id.to_string();
            self.runtime.block_on(async move {
                fetch_player_by_id(&pool, &id).await
            })
        }

        fn by_email(&self, email: &str) -> Option<PlayerRecord> {
            let pool = self.pool.clone();
            let email = email.to_string();
            self.runtime.block_on(async move {
                sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, Option<i64>, Option<i64>, Option<i64>, Option<String>, i32, Option<i64>, i64)>(
                    "SELECT id, username, email, password_hash, role,
                            EXTRACT(EPOCH FROM verified_at)::bigint,
                            EXTRACT(EPOCH FROM deleted_at)::bigint,
                            EXTRACT(EPOCH FROM banned_at)::bigint,
                            banned_reason,
                            failed_login_attempts,
                            EXTRACT(EPOCH FROM locked_until)::bigint,
                            EXTRACT(EPOCH FROM created_at)::bigint
                     FROM players WHERE email = $1",
                )
                .bind(&email)
                .fetch_optional(&pool)
                .await
                .ok()?
                .map(|(id, uname, email, pw_hash, role, verified, deleted, banned, reason, fails, locked, created)| {
                    PlayerRecord {
                        id, username: uname, email, password_hash: pw_hash,
                        role, verified_at: verified.filter(|&v| v > 0),
                        deleted_at: deleted.filter(|&v| v > 0),
                        banned_at: banned.filter(|&v| v > 0),
                        banned_reason: reason,
                        failed_login_attempts: fails as u32,
                        locked_until: locked.filter(|&v| v > 0),
                        created_at: created,
                    }
                })
            })
        }

        fn by_provider(&self, provider: &str, provider_uid: &str) -> Option<PlayerRecord> {
            let pool = self.pool.clone();
            let provider = provider.to_string();
            let provider_uid = provider_uid.to_string();
            self.runtime.block_on(async move {
                let pid: Option<(String,)> = sqlx::query_as(
                    "SELECT player_id FROM oauth_accounts WHERE provider = $1 AND provider_user_id = $2",
                )
                .bind(&provider)
                .bind(&provider_uid)
                .fetch_optional(&pool)
                .await
                .ok()?;
                let (pid,) = pid?;
                fetch_player_by_id(&pool, &pid).await
            })
        }

        fn link_provider(&self, player_id: &str, provider: &str, provider_uid: &str, provider_email: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            let prov = provider.to_string();
            let uid = provider_uid.to_string();
            let email = provider_email.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "INSERT INTO oauth_accounts (player_id, provider, provider_user_id, provider_email)
                     VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
                )
                .bind(&pid)
                .bind(&prov)
                .bind(&uid)
                .bind(&email)
                .execute(&pool)
                .await;
            });
        }

        fn update_hash(&self, player_id: &str, hash: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            let hash = hash.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query("UPDATE players SET password_hash = $1 WHERE id = $2")
                    .bind(&hash)
                    .bind(&pid)
                    .execute(&pool)
                    .await;
            });
        }

        fn inc_fails(&self, player_id: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET failed_login_attempts = failed_login_attempts + 1 WHERE id = $1",
                )
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn clear_fails(&self, player_id: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET failed_login_attempts = 0, locked_until = NULL WHERE id = $1",
                )
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn set_verified(&self, player_id: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET verified_at = NOW() WHERE id = $1",
                )
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn set_deleted(&self, player_id: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET deleted_at = NOW() WHERE id = $1",
                )
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn cancel_deletion(&self, player_id: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET deleted_at = NULL WHERE id = $1",
                )
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn set_lock(&self, player_id: &str, locked_until: i64) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET locked_until = to_timestamp($1::double precision) WHERE id = $2",
                )
                .bind(locked_until as f64)
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn ban(&self, player_id: &str, reason: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            let reason = reason.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET banned_at = NOW(), banned_reason = $1 WHERE id = $2",
                )
                .bind(&reason)
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn unban(&self, player_id: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET banned_at = NULL, banned_reason = NULL WHERE id = $1",
                )
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn set_role(&self, player_id: &str, role: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            let role = role.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "UPDATE players SET role = $1::auth_role WHERE id = $2",
                )
                .bind(&role)
                .bind(&pid)
                .execute(&pool)
                .await;
            });
        }

        fn list(&self, page: u32, per_page: u32) -> Vec<PlayerRecord> {
            let pool = self.pool.clone();
            let offset = ((page.saturating_sub(1)) * per_page) as i64;
            let limit = per_page as i64;
            self.runtime.block_on(async move {
                sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, Option<i64>, Option<i64>, Option<i64>, Option<String>, i32, Option<i64>, i64)>(
                    "SELECT id, username, email, password_hash, role,
                            EXTRACT(EPOCH FROM verified_at)::bigint,
                            EXTRACT(EPOCH FROM deleted_at)::bigint,
                            EXTRACT(EPOCH FROM banned_at)::bigint,
                            banned_reason,
                            failed_login_attempts,
                            EXTRACT(EPOCH FROM locked_until)::bigint,
                            EXTRACT(EPOCH FROM created_at)::bigint
                     FROM players ORDER BY created_at LIMIT $1 OFFSET $2",
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(&pool)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(id, uname, email, pw_hash, role, verified, deleted, banned, reason, fails, locked, created)| {
                    PlayerRecord {
                        id, username: uname, email, password_hash: pw_hash,
                        role, verified_at: verified.filter(|&v| v > 0),
                        deleted_at: deleted.filter(|&v| v > 0),
                        banned_at: banned.filter(|&v| v > 0),
                        banned_reason: reason,
                        failed_login_attempts: fails as u32,
                        locked_until: locked.filter(|&v| v > 0),
                        created_at: created,
                    }
                })
                .collect()
            })
        }

        fn count(&self) -> usize {
            let pool = self.pool.clone();
            self.runtime.block_on(async move {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM players")
                    .fetch_one(&pool)
                    .await
                    .unwrap_or(0) as usize
            })
        }
    }

    /// Helper: fetch a player row by id.
    async fn fetch_player_by_id(pool: &PgPool, id: &str) -> Option<PlayerRecord> {
        sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String, Option<i64>, Option<i64>, Option<i64>, Option<String>, i32, Option<i64>, i64)>(
            "SELECT id, username, email, password_hash, role,
                    EXTRACT(EPOCH FROM verified_at)::bigint,
                    EXTRACT(EPOCH FROM deleted_at)::bigint,
                    EXTRACT(EPOCH FROM banned_at)::bigint,
                    banned_reason,
                    failed_login_attempts,
                    EXTRACT(EPOCH FROM locked_until)::bigint,
                    EXTRACT(EPOCH FROM created_at)::bigint
             FROM players WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()?
        .map(|(id, uname, email, pw_hash, role, verified, deleted, banned, reason, fails, locked, created)| {
            PlayerRecord {
                id, username: uname, email, password_hash: pw_hash,
                role, verified_at: verified.filter(|&v| v > 0),
                deleted_at: deleted.filter(|&v| v > 0),
                banned_at: banned.filter(|&v| v > 0),
                banned_reason: reason,
                failed_login_attempts: fails as u32,
                locked_until: locked.filter(|&v| v > 0),
                created_at: created,
            }
        })
    }

    // ------------------------------------------------------------------
    // PgSessionStore
    // ------------------------------------------------------------------

    pub struct PgSessionStore {
        pool: PgPool,
        runtime: tokio::runtime::Handle,
    }

    impl PgSessionStore {
        pub fn new(pool: PgPool) -> Self {
            PgSessionStore {
                pool,
                runtime: tokio::runtime::Handle::current(),
            }
        }
    }

    impl SessionStore for PgSessionStore {
        fn issue(&self, info: SessionInfo) -> String {
            let pool = self.pool.clone();
            let token = uuid::Uuid::new_v4().to_string();
            let token2 = token.clone();
            let pid = info.player_id;
            let universe = info.universe.as_str().to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query(
                    "INSERT INTO sessions (token, player_id, universe) VALUES ($1, $2, $3::universe_tier)",
                )
                .bind(&token2)
                .bind(&pid)
                .bind(&universe)
                .execute(&pool)
                .await;
            });
            token
        }

        fn resolve(&self, token: &str) -> Option<SessionInfo> {
            let pool = self.pool.clone();
            let t = token.to_string();
            self.runtime.block_on(async move {
                let row: Option<(String, String)> = sqlx::query_as(
                    "SELECT player_id, universe::text FROM sessions
                     WHERE token = $1 AND expires_at > NOW()",
                )
                .bind(&t)
                .fetch_optional(&pool)
                .await
                .ok()?;
                let (player_id, universe_str) = row?;
                let universe = universe_str.parse().ok()?;
                Some(SessionInfo {
                    player_id,
                    universe,
                })
            })
        }

        fn revoke(&self, token: &str) {
            let pool = self.pool.clone();
            let t = token.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query("DELETE FROM sessions WHERE token = $1")
                    .bind(&t)
                    .execute(&pool)
                    .await;
            });
        }

        fn revoke_all_for_player(&self, player_id: &str) {
            let pool = self.pool.clone();
            let pid = player_id.to_string();
            self.runtime.block_on(async move {
                let _ = sqlx::query("DELETE FROM sessions WHERE player_id = $1")
                    .bind(&pid)
                    .execute(&pool)
                    .await;
            });
        }
    }
}
