use std::collections::HashMap;
use std::sync::Mutex;

use reachlock_core::universe::tier::UniverseTier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaStatus {
    Normal,
    Fatigued { message: String },
    Exhausted,
}

#[derive(Debug, Clone)]
pub struct QuotaTierConfig {
    pub universe: UniverseTier,
    pub monthly_call_budget: u64,
    pub soft_limit: f64,
    pub hard_limit: f64,
    pub fatigue_message: String,
}

impl QuotaTierConfig {
    pub fn for_tier(universe: UniverseTier) -> Self {
        match universe {
            UniverseTier::Classic => QuotaTierConfig {
                universe, monthly_call_budget: 0, soft_limit: 0.8, hard_limit: 1.0,
                fatigue_message: String::new(),
            },
            UniverseTier::FairPlay => QuotaTierConfig {
                universe, monthly_call_budget: 1000, soft_limit: 0.8, hard_limit: 1.0,
                fatigue_message: "Your crew is fatigued from intensive deliberation.".into(),
            },
            UniverseTier::Spectrum => QuotaTierConfig {
                universe, monthly_call_budget: 5000, soft_limit: 0.8, hard_limit: 1.0,
                fatigue_message: "Your crew is fatigued from intensive deliberation.".into(),
            },
            UniverseTier::Byok => QuotaTierConfig {
                universe, monthly_call_budget: u64::MAX, soft_limit: 1.0, hard_limit: 1.0,
                fatigue_message: String::new(),
            },
        }
    }
}

pub trait QuotaManager: Send + Sync {
    fn check(&self, player_id: &str, universe: UniverseTier) -> QuotaStatus;
    fn record_call(&self, player_id: &str, universe: UniverseTier);
    fn monthly_usage(&self, player_id: &str, universe: UniverseTier) -> u64;
}

pub struct MemoryQuotaManager {
    counters: Mutex<HashMap<String, u64>>,
    configs: HashMap<UniverseTier, QuotaTierConfig>,
}

impl MemoryQuotaManager {
    pub fn new() -> Self {
        use UniverseTier::*;
        let configs = vec![Classic, FairPlay, Spectrum, Byok]
            .into_iter().map(|t| (t, QuotaTierConfig::for_tier(t))).collect();
        MemoryQuotaManager { counters: Mutex::new(HashMap::new()), configs }
    }
}

impl Default for MemoryQuotaManager { fn default() -> Self { Self::new() } }

impl QuotaManager for MemoryQuotaManager {
    fn check(&self, player_id: &str, universe: UniverseTier) -> QuotaStatus {
        let cfg = match self.configs.get(&universe) { Some(c) => c, None => return QuotaStatus::Normal };
        if cfg.monthly_call_budget == 0 { return QuotaStatus::Exhausted; }
        if cfg.monthly_call_budget == u64::MAX { return QuotaStatus::Normal; }
        let usage = self.counters.lock().unwrap()
            .get(&format!("{player_id}_{universe:?}")).copied().unwrap_or(0);
        let pct = usage as f64 / cfg.monthly_call_budget as f64;
        if pct >= cfg.hard_limit { QuotaStatus::Exhausted }
        else if pct >= cfg.soft_limit { QuotaStatus::Fatigued { message: cfg.fatigue_message.clone() } }
        else { QuotaStatus::Normal }
    }
    fn record_call(&self, player_id: &str, universe: UniverseTier) {
        *self.counters.lock().unwrap()
            .entry(format!("{player_id}_{universe:?}")).or_insert(0) += 1;
    }
    fn monthly_usage(&self, player_id: &str, universe: UniverseTier) -> u64 {
        self.counters.lock().unwrap().get(&format!("{player_id}_{universe:?}")).copied().unwrap_or(0)
    }
}
