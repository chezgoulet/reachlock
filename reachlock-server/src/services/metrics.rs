//! Telemetry and Prometheus metrics (S14, S73): LLM latency histograms,
//! server ops counters and gauges.

use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Opts, Registry};

use std::sync::Arc;

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

/// S73: production server metrics. Registered on the prometheus registry at
/// startup. All metric names are prefixed with `reachlock_`.
pub struct ServerMetrics {
    pub connections_active: Gauge,
    pub connections_total: Counter,
    pub messages_sent: Counter,
    pub messages_received: Counter,
    pub db_pool_connections: Gauge,
    pub tick_duration: Histogram,
    pub ws_message_size: Histogram,
    pub uptime_seconds: Gauge,
}

impl ServerMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        let connections_active = Gauge::with_opts(Opts::new(
            "reachlock_connections_active",
            "Current number of active WebSocket connections",
        ))
        .unwrap();
        let connections_total = Counter::with_opts(Opts::new(
            "reachlock_connections_total",
            "Total number of WebSocket connections opened",
        ))
        .unwrap();
        let messages_sent = Counter::with_opts(Opts::new(
            "reachlock_messages_sent_total",
            "Total messages sent to clients",
        ))
        .unwrap();
        let messages_received = Counter::with_opts(Opts::new(
            "reachlock_messages_received_total",
            "Total messages received from clients",
        ))
        .unwrap();
        let db_pool_connections = Gauge::with_opts(Opts::new(
            "reachlock_db_pool_connections",
            "Database pool connection counts by state",
        ))
        .unwrap();
        let tick_duration = Histogram::with_opts(HistogramOpts::new(
            "reachlock_tick_duration_seconds",
            "Universe tick duration in seconds",
        ))
        .unwrap();
        let ws_message_size = Histogram::with_opts(HistogramOpts::new(
            "reachlock_ws_message_size_bytes",
            "WebSocket message size in bytes",
        ))
        .unwrap();
        let uptime_seconds = Gauge::with_opts(Opts::new(
            "reachlock_uptime_seconds",
            "Server uptime in seconds",
        ))
        .unwrap();

        let metrics = Arc::new(ServerMetrics {
            connections_active,
            connections_total,
            messages_sent,
            messages_received,
            db_pool_connections,
            tick_duration,
            ws_message_size,
            uptime_seconds,
        });

        registry
            .register(Box::new(metrics.connections_active.clone()))
            .unwrap();
        registry
            .register(Box::new(metrics.connections_total.clone()))
            .unwrap();
        registry
            .register(Box::new(metrics.messages_sent.clone()))
            .unwrap();
        registry
            .register(Box::new(metrics.messages_received.clone()))
            .unwrap();
        registry
            .register(Box::new(metrics.db_pool_connections.clone()))
            .unwrap();
        registry
            .register(Box::new(metrics.tick_duration.clone()))
            .unwrap();
        registry
            .register(Box::new(metrics.ws_message_size.clone()))
            .unwrap();
        registry
            .register(Box::new(metrics.uptime_seconds.clone()))
            .unwrap();

        metrics
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
