//! S26 graceful degradation: startup probing, runtime reconnection.

use std::sync::Arc;
use std::time::Duration;

use crate::services::health::{HealthAggregator, HealthStatus};

/// Reconnection task: pings downed backends every 30s.
pub fn spawn_reconnection_task(health: Arc<HealthAggregator>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let agg = health.aggregate();
            for check in &agg.checks {
                if !matches!(check.status, HealthStatus::Ok) {
                    // Trigger a fresh probe of each degraded backend.
                    // In a full implementation, this would re-attempt a DB
                    // connection or Redis PING. For now, we re-check the
                    // health aggregator which delegates to registered checks.
                    tracing::info!(
                        "degradation check: {} is {:?}",
                        check.name,
                        check.status
                    );
                }
            }
        }
    });
}
