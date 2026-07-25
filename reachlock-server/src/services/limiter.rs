//! Per-player LLM rate limiting (S14): a token bucket per
//! `(player, universe)`. In-memory behind a SessionStore-style trait so a
//! Redis-backed implementation can slot in later without touching the
//! proxy. Exceeding the bucket is `llm.failed { reason: "rate_limited" }`
//! — "the crew is overwhelmed" is a valid fiction, not an HTTP apology.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use reachlock_core::universe::UniverseTier;

/// Bucket size: how many calls a player can burst.
pub const BUCKET_CAPACITY: f64 = 5.0;
/// Refill rate: one token every this many seconds.
pub const SECONDS_PER_TOKEN: f64 = 10.0;

/// The seam a Redis limiter would implement later.
pub trait RateLimiter: Send + Sync {
    /// Try to take one token for this player/universe. `false` = limited.
    fn try_acquire(&self, player_id: &str, universe: UniverseTier) -> bool;
}

/// Token bucket in a mutex-guarded map — the same zero-infra pattern as
/// `MemorySeedStore`.
pub struct MemoryRateLimiter {
    buckets: Mutex<HashMap<(String, UniverseTier), Bucket>>,
    capacity: f64,
    seconds_per_token: f64,
}

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl Default for MemoryRateLimiter {
    fn default() -> Self {
        Self::new(BUCKET_CAPACITY, SECONDS_PER_TOKEN)
    }
}

impl MemoryRateLimiter {
    pub fn new(capacity: f64, seconds_per_token: f64) -> Self {
        MemoryRateLimiter {
            buckets: Mutex::new(HashMap::new()),
            capacity,
            seconds_per_token,
        }
    }
}

impl RateLimiter for MemoryRateLimiter {
    fn try_acquire(&self, player_id: &str, universe: UniverseTier) -> bool {
        let mut buckets = self.buckets.lock().expect("limiter lock");
        let now = Instant::now();
        let bucket = buckets
            .entry((player_id.to_string(), universe))
            .or_insert(Bucket {
                tokens: self.capacity,
                last_refill: now,
            });
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed / self.seconds_per_token).min(self.capacity);
        bucket.last_refill = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_then_limited_then_refilled() {
        // 2-token bucket refilling instantly-ish for the test.
        let limiter = MemoryRateLimiter::new(2.0, 0.01);
        assert!(limiter.try_acquire("tib", UniverseTier::FairPlay));
        assert!(limiter.try_acquire("tib", UniverseTier::FairPlay));
        assert!(
            !limiter.try_acquire("tib", UniverseTier::FairPlay),
            "third call in a burst is limited"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
        assert!(
            limiter.try_acquire("tib", UniverseTier::FairPlay),
            "tokens refill with time"
        );
    }

    #[test]
    fn buckets_are_per_player_and_per_universe() {
        let limiter = MemoryRateLimiter::new(1.0, 1000.0);
        assert!(limiter.try_acquire("tib", UniverseTier::FairPlay));
        assert!(!limiter.try_acquire("tib", UniverseTier::FairPlay));
        // A different player, and the same player in another universe, both
        // have their own buckets.
        assert!(limiter.try_acquire("tove", UniverseTier::FairPlay));
        assert!(limiter.try_acquire("tib", UniverseTier::Spectrum));
    }
}
