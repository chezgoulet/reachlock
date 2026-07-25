//! S26 audit logging: immutable append-only record of admin actions.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: String,
    pub action: String,
    pub target: String,
    pub detail: String,
    /// SHA-256 of the admin key (never the raw key).
    pub admin_key_hash: String,
}

pub trait AuditLog: Send + Sync {
    fn record(&self, entry: AuditEntry);
    fn recent(&self, limit: usize) -> Vec<AuditEntry>;
}

pub struct MemoryAuditLog {
    entries: Mutex<Vec<AuditEntry>>,
}

impl MemoryAuditLog {
    pub fn new() -> Self {
        MemoryAuditLog {
            entries: Mutex::new(Vec::new()),
        }
    }
}

impl Default for MemoryAuditLog {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLog for MemoryAuditLog {
    fn record(&self, entry: AuditEntry) {
        let mut entries = self.entries.lock().unwrap();
        entries.push(entry);
    }

    fn recent(&self, limit: usize) -> Vec<AuditEntry> {
        let entries = self.entries.lock().unwrap();
        let start = entries.len().saturating_sub(limit);
        entries[start..].to_vec()
    }
}
