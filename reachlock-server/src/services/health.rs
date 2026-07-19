//! S26 health checking: trait, aggregator, and per-backend implementations.

use std::sync::Arc;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
    Degraded { reason: String },
    Down { reason: String },
}

pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self) -> HealthStatus;
}

/// Aggregate health across multiple backends.
pub struct HealthAggregator {
    checks: Vec<Arc<dyn HealthCheck>>,
}

impl HealthAggregator {
    pub fn new() -> Self {
        HealthAggregator { checks: Vec::new() }
    }

    pub fn register(&mut self, check: Arc<dyn HealthCheck>) {
        self.checks.push(check);
    }

    pub fn aggregate(&self) -> AggregateHealth {
        let mut checks = Vec::new();
        for c in &self.checks {
            checks.push(CheckResult {
                name: c.name().to_string(),
                status: c.check(),
            });
        }
        let overall = if checks.iter().any(|c| !matches!(c.status, HealthStatus::Ok)) {
            if checks.iter().any(|c| matches!(c.status, HealthStatus::Down { .. })) {
                "down".to_string()
            } else {
                "degraded".to_string()
            }
        } else {
            "ok".to_string()
        };
        AggregateHealth { status: overall, checks }
    }
}

impl Default for HealthAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
pub struct AggregateHealth {
    pub status: String,
    pub checks: Vec<CheckResult>,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub name: String,
    pub status: HealthStatus,
}

/// Always-ok health check for memory backends.
pub struct MemoryHealthCheck {
    pub name: String,
}

impl HealthCheck for MemoryHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }
    fn check(&self) -> HealthStatus {
        HealthStatus::Ok
    }
}
