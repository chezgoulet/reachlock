//! S26 structured observability: TraceId generation, Prometheus metrics.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique per-request identifier carried through every log and wire hop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

impl TraceId {
    pub fn new() -> Self {
        TraceId(Uuid::now_v7().to_string())
    }
}

impl Default for TraceId {
    fn default() -> Self {
        TraceId::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Initialize a Prometheus registry with standard metrics and return it.
/// Call once at startup; hand the registry to the metrics endpoint.
pub fn init_prometheus() -> prometheus::Registry {
    prometheus::Registry::new()
}
