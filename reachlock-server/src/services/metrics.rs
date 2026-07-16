//! Deliberation timing telemetry (S14): per-call latency into an in-memory
//! histogram, rendered as Prometheus text at `GET /metrics`. Lock-free
//! atomics — the proxy must never contend on telemetry.

use std::sync::atomic::{AtomicU64, Ordering};

/// Upper bounds (ms) of the histogram buckets; the last bucket is +Inf.
pub const BUCKET_BOUNDS_MS: [u64; 7] = [100, 250, 500, 1000, 2500, 5000, 15000];

#[derive(Default)]
pub struct LatencyHistogram {
    buckets: [AtomicU64; 7],
    inf: AtomicU64,
    count: AtomicU64,
    sum_ms: AtomicU64,
    failures: AtomicU64,
}

impl LatencyHistogram {
    pub fn record(&self, latency_ms: u64, failed: bool) {
        for (i, bound) in BUCKET_BOUNDS_MS.iter().enumerate() {
            if latency_ms <= *bound {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
        if latency_ms > BUCKET_BOUNDS_MS[BUCKET_BOUNDS_MS.len() - 1] {
            self.inf.fetch_add(1, Ordering::Relaxed);
        }
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_ms.fetch_add(latency_ms, Ordering::Relaxed);
        if failed {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Prometheus text exposition (cumulative buckets, as the format wants).
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("# TYPE reachlock_llm_latency_ms histogram\n");
        let mut cumulative = 0u64;
        for (i, bound) in BUCKET_BOUNDS_MS.iter().enumerate() {
            cumulative += self.buckets[i].load(Ordering::Relaxed);
            out.push_str(&format!(
                "reachlock_llm_latency_ms_bucket{{le=\"{bound}\"}} {cumulative}\n"
            ));
        }
        cumulative += self.inf.load(Ordering::Relaxed);
        out.push_str(&format!(
            "reachlock_llm_latency_ms_bucket{{le=\"+Inf\"}} {cumulative}\n"
        ));
        out.push_str(&format!(
            "reachlock_llm_latency_ms_sum {}\n",
            self.sum_ms.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "reachlock_llm_latency_ms_count {}\n",
            self.count.load(Ordering::Relaxed)
        ));
        out.push_str("# TYPE reachlock_llm_failures_total counter\n");
        out.push_str(&format!(
            "reachlock_llm_failures_total {}\n",
            self.failures.load(Ordering::Relaxed)
        ));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_renders_cumulative_buckets() {
        let h = LatencyHistogram::default();
        h.record(50, false); // le=100
        h.record(400, false); // le=500
        h.record(20_000, true); // +Inf, failed
        let text = h.render();
        assert!(text.contains("le=\"100\"} 1"));
        assert!(text.contains("le=\"500\"} 2"));
        assert!(text.contains("le=\"+Inf\"} 3"));
        assert!(text.contains("reachlock_llm_latency_ms_count 3"));
        assert!(text.contains("reachlock_llm_failures_total 1"));
    }
}
