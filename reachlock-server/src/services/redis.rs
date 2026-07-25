//! S50: Redis-backed stores for sessions, rate limiting, and presence.
//! Enabled behind the `redis` feature flag when `REACHLOCK_REDIS_URL` is set.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use reachlock_core::universe::UniverseTier;
use redis::aio::ConnectionManager;

use crate::services::auth::{SessionInfo, SessionStore};
use crate::services::limiter::RateLimiter;

/// Wrapper around a Redis connection manager. All Redis-backed stores share a
/// single connection manager behind a Mutex (the traits are sync, so we do
/// `block_on` with the async conn).
pub struct RedisPool {
    mgr: Mutex<ConnectionManager>,
    runtime: tokio::runtime::Handle,
}

impl RedisPool {
    pub async fn new(url: &str) -> Result<Self, String> {
        let client = redis::Client::open(url).map_err(|e| format!("redis open: {e}"))?;
        let mgr = client
            .get_connection_manager()
            .await
            .map_err(|e| format!("redis connect: {e}"))?;
        Ok(RedisPool {
            mgr: Mutex::new(mgr),
            runtime: tokio::runtime::Handle::current(),
        })
    }

    /// Obtain a cloned connection manager and run an async operation on it.
    fn block_on<Fut>(&self, f: impl FnOnce(ConnectionManager) -> Fut) -> Fut::Output
    where
        Fut: std::future::Future + Send,
        Fut::Output: Send,
    {
        let mgr = self.mgr.lock().expect("redis pool lock").clone();
        crate::services::blocking::block_on_async(&self.runtime, f(mgr))
    }
}

// ---------------------------------------------------------------------------
// Key helpers
// ---------------------------------------------------------------------------

fn session_key(token: &str) -> String {
    format!("session:{token}")
}
fn ratelimit_key(player: &str, universe: &str) -> String {
    format!("ratelimit:{player}:{universe}")
}
fn presence_key(universe: &str, system: &str) -> String {
    format!("presence:{universe}:{system}")
}
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// RedisSessionStore
// ---------------------------------------------------------------------------

pub struct RedisSessionStore {
    pool: std::sync::Arc<RedisPool>,
    ttl_secs: u64,
}

impl RedisSessionStore {
    pub fn new(pool: std::sync::Arc<RedisPool>) -> Self {
        let ttl = std::env::var("REACHLOCK_SESSION_TTL_HOURS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(24)
            * 3600;
        RedisSessionStore {
            pool,
            ttl_secs: ttl,
        }
    }
}

impl SessionStore for RedisSessionStore {
    fn issue(&self, info: SessionInfo) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let key = session_key(&token);
        let pid = info.player_id;
        let universe = info.universe.as_str().to_string();
        let ttl = self.ttl_secs;
        self.pool.block_on(move |mut mgr| async move {
            let _ = redis::cmd("HSET")
                .arg(&key)
                .arg("player_id")
                .arg(&pid)
                .arg("universe")
                .arg(&universe)
                .query_async::<()>(&mut mgr)
                .await;
            let _ = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(ttl)
                .query_async::<()>(&mut mgr)
                .await;
        });
        token
    }

    fn resolve(&self, token: &str) -> Option<SessionInfo> {
        let key = session_key(token);
        self.pool.block_on(move |mut mgr| async move {
            let exists: bool = redis::cmd("EXISTS")
                .arg(&key)
                .query_async(&mut mgr)
                .await
                .unwrap_or(false);
            if !exists {
                return None;
            }
            let result: Vec<Option<String>> = redis::cmd("HMGET")
                .arg(&key)
                .arg("player_id")
                .arg("universe")
                .query_async(&mut mgr)
                .await
                .ok()?;
            let pid = result.first()?.clone()?;
            let universe_str = result.get(1)?.clone()?;
            let universe = universe_str.parse().ok()?;
            Some(SessionInfo {
                player_id: pid,
                universe,
            })
        })
    }

    fn revoke(&self, token: &str) {
        let key = session_key(token);
        self.pool.block_on(move |mut mgr| async move {
            let _ = redis::cmd("DEL")
                .arg(&key)
                .query_async::<()>(&mut mgr)
                .await;
        });
    }

    fn active_sessions(&self) -> usize {
        self.pool.block_on(move |mut mgr| async move {
            let count: Option<u64> = redis::cmd("DBSIZE").query_async(&mut mgr).await.ok();
            count.unwrap_or(0) as usize
        })
    }

    fn revoke_all_for_player(&self, player_id: &str) {
        let pid = player_id.to_string();
        self.pool.block_on(move |mut mgr| async move {
            let mut cursor = 0u64;
            loop {
                let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                    .arg(cursor)
                    .arg("MATCH")
                    .arg("session:*")
                    .arg("COUNT")
                    .arg(100)
                    .query_async(&mut mgr)
                    .await
                    .unwrap_or((0, vec![]));
                for key in &keys {
                    let pid_check: Option<String> = redis::cmd("HGET")
                        .arg(key)
                        .arg("player_id")
                        .query_async(&mut mgr)
                        .await
                        .ok();
                    if pid_check.as_deref() == Some(&pid) {
                        let _ = redis::cmd("DEL").arg(key).query_async::<()>(&mut mgr).await;
                    }
                }
                cursor = next_cursor;
                if cursor == 0 {
                    break;
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// RedisRateLimiter — sliding window via Redis sorted sets
// ---------------------------------------------------------------------------

const RATE_LIMIT_WINDOW_SECS: u64 = 10;
const RATE_LIMIT_MAX_REQUESTS: u64 = 10;

/// Lua script for atomic rate-limit check-and-increment.
const RATE_LIMIT_SCRIPT: &str = r#"
    local key = KEYS[1]
    local now = tonumber(ARGV[1])
    local window = tonumber(ARGV[2])
    local max_req = tonumber(ARGV[3])
    local cutoff = now - window
    redis.call('ZREMRANGEBYSCORE', key, 0, cutoff)
    local count = redis.call('ZCARD', key)
    if count >= max_req then
        return 0
    end
    redis.call('ZADD', key, now, now)
    redis.call('EXPIRE', key, window)
    return 1
"#;

pub struct RedisRateLimiter {
    pool: std::sync::Arc<RedisPool>,
    window_secs: u64,
    max_requests: u64,
}

impl RedisRateLimiter {
    pub fn new(pool: std::sync::Arc<RedisPool>) -> Self {
        RedisRateLimiter {
            pool,
            window_secs: RATE_LIMIT_WINDOW_SECS,
            max_requests: RATE_LIMIT_MAX_REQUESTS,
        }
    }
}

impl RateLimiter for RedisRateLimiter {
    fn try_acquire(&self, player_id: &str, universe: UniverseTier) -> bool {
        let key = ratelimit_key(player_id, universe.as_str());
        let now = now_secs();
        let window = self.window_secs;
        let max_req = self.max_requests;
        self.pool.block_on(move |mut mgr| async move {
            let script = redis::Script::new(RATE_LIMIT_SCRIPT);
            let result: Option<i64> = script
                .key(&key)
                .arg(now)
                .arg(window)
                .arg(max_req)
                .invoke_async(&mut mgr)
                .await
                .ok();
            result.unwrap_or(1) == 1
        })
    }
}

// ---------------------------------------------------------------------------
// RedisPresenceStore — persists (player_id) in (universe, system) sets
// ---------------------------------------------------------------------------

pub trait PresenceStore: Send + Sync {
    fn set(&self, universe: UniverseTier, system: &str, player_id: &str);
    fn remove(&self, universe: UniverseTier, system: &str, player_id: &str);
}

pub struct RedisPresenceStore {
    pool: std::sync::Arc<RedisPool>,
}

impl RedisPresenceStore {
    pub fn new(pool: std::sync::Arc<RedisPool>) -> Self {
        RedisPresenceStore { pool }
    }
}

impl PresenceStore for RedisPresenceStore {
    fn set(&self, universe: UniverseTier, system: &str, player_id: &str) {
        let key = presence_key(universe.as_str(), system);
        let pid = player_id.to_string();
        self.pool.block_on(move |mut mgr| async move {
            let _ = redis::cmd("SADD")
                .arg(&key)
                .arg(&pid)
                .query_async::<()>(&mut mgr)
                .await;
            // TTL: 1 hour. Players who disconnect without leaving
            // will be cleaned up automatically.
            let _ = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(3600u64)
                .query_async::<()>(&mut mgr)
                .await;
        });
    }

    fn remove(&self, universe: UniverseTier, system: &str, player_id: &str) {
        let key = presence_key(universe.as_str(), system);
        let pid = player_id.to_string();
        self.pool.block_on(move |mut mgr| async move {
            let _ = redis::cmd("SREM")
                .arg(&key)
                .arg(&pid)
                .query_async::<()>(&mut mgr)
                .await;
        });
    }
}
