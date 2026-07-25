use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use reachlock_core::universe::tier::UniverseTier;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallRecord {
    pub timestamp: String,
    pub player_id: String,
    pub universe: UniverseTier,
    pub provider: String,
    pub model: String,
    pub contract_id: String,
    pub latency_ms: u64,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub estimated_cost_micros: u64,
    pub success: bool,
}

pub trait CostStore: Send + Sync {
    fn record(&self, record: LlmCallRecord);
    fn player_total(&self, player_id: &str, since: &str) -> u64;
    fn universe_total(&self, universe: UniverseTier, since: &str) -> u64;
    fn daily_breakdown(&self, universe: UniverseTier, days: u32) -> Vec<DayBucket>;
}

#[derive(Debug, Clone, Serialize)]
pub struct DayBucket {
    pub date: String,
    pub tokens: u64,
    pub cost_micros: u64,
}

pub struct MemoryCostStore {
    records: Mutex<Vec<LlmCallRecord>>,
    max_records: usize,
}

impl MemoryCostStore {
    pub fn new() -> Self {
        MemoryCostStore {
            records: Mutex::new(Vec::new()),
            max_records: 10_000,
        }
    }
}

impl Default for MemoryCostStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CostStore for MemoryCostStore {
    fn record(&self, record: LlmCallRecord) {
        let mut records = self.records.lock().unwrap();
        if records.len() >= self.max_records {
            records.remove(0);
        }
        records.push(record);
    }
    fn player_total(&self, player_id: &str, _since: &str) -> u64 {
        self.records
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.player_id == player_id)
            .map(|r| r.estimated_cost_micros)
            .sum()
    }
    fn universe_total(&self, universe: UniverseTier, _since: &str) -> u64 {
        self.records
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.universe == universe)
            .map(|r| r.estimated_cost_micros)
            .sum()
    }
    fn daily_breakdown(&self, universe: UniverseTier, _days: u32) -> Vec<DayBucket> {
        let records = self.records.lock().unwrap();
        let mut day_map: HashMap<String, (u64, u64)> = HashMap::new();
        for r in records.iter().filter(|r| r.universe == universe) {
            let day = &r.timestamp[..10];
            let e = day_map.entry(day.to_string()).or_insert((0, 0));
            e.0 += r.prompt_tokens as u64 + r.completion_tokens as u64;
            e.1 += r.estimated_cost_micros;
        }
        day_map
            .into_iter()
            .map(|(d, (t, c))| DayBucket {
                date: d,
                tokens: t,
                cost_micros: c,
            })
            .collect()
    }
}

pub fn estimate_token_count(text: &str) -> u32 {
    (text.len() as u32).max(1) / 4 + 1
}

pub fn estimate_cost(provider: &str, prompt_tokens: u32, completion_tokens: u32) -> u64 {
    let price_per_1k: u64 = match provider {
        "spectrum" | "Spectrum" => std::env::var("REACHLOCK_SPECTRUM_PRICE_PER_1K")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10_000),
        _ => 0,
    };
    (prompt_tokens + completion_tokens) as u64 * price_per_1k / 1000
}

pub struct ProviderHealth {
    results: VecDeque<bool>,
    max_samples: usize,
}

impl ProviderHealth {
    pub fn new() -> Self {
        ProviderHealth {
            results: VecDeque::with_capacity(100),
            max_samples: 100,
        }
    }
    pub fn record(&mut self, success: bool) {
        self.results.push_back(success);
        while self.results.len() > self.max_samples {
            self.results.pop_front();
        }
    }
    pub fn is_tripped(&self) -> bool {
        let recent: Vec<&bool> = self.results.iter().rev().take(50).collect();
        if recent.len() < 10 {
            return false;
        }
        recent.iter().filter(|&&s| !s).count() > recent.len() / 2
    }
    pub fn consecutive_successes(&self) -> u32 {
        self.results.iter().rev().take_while(|&&s| s).count() as u32
    }
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self::new()
    }
}
