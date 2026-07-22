//! Deterministic context compression (S35). Template-based text generation
//! from [`RelationshipMemory`] — never calls an LLM (iron rule #1, spec §35
//! gotcha). Selects strategy by token budget.

use serde::{Deserialize, Serialize};

use super::memory::{
    RelationshipMemory, SignificantEventType, TrustTrend,
};

/// LLM-ready relationship summary, produced deterministically from memory.
/// The text fields are assembled from templates + event data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressedContext {
    pub relationship_summary: String,
    pub trajectory_narrative: String,
    pub key_memories: Vec<String>,
    pub compression_metadata: CompressMeta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressMeta {
    pub compressed_at_tick: u64,
    pub event_count_compressed: u64,
    pub compression_strategy: CompressionStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionStrategy {
    /// Standard: summarize all events into narrative (200+ token budget).
    Summarize,
    /// Tight context: only the 5 highest-weight events (100-200 token budget).
    TopWeight,
    /// Minimal: just the trajectory + trust level (<100 token budget).
    TrajectoryOnly,
}

/// Produce a compressed context from a relationship memory.
/// Deterministic — pure function, no I/O.
pub fn compress(memory: &RelationshipMemory, strategy: CompressionStrategy, tick: u64) -> CompressedContext {
    let event_count = memory.significant_events.len() as u64;

    // --- relationship_summary ---
    let (a_name, b_name) = (&memory.participants.0, &memory.participants.1);
    let summary = format!(
        "{} · {} interactions, {} conversations, {} crises together. Trust: {}. Trend: {}.",
        format_pair(a_name, b_name),
        memory.interaction_count,
        memory.conversation_count,
        memory.crisis_count,
        trust_label(memory.trust_trajectory.points.last().map(|(_, v)| v.0).unwrap_or(0)),
        trend_label(&memory.trust_trajectory.trend),
    );

    // --- trajectory_narrative ---
    let trajectory = match &memory.trust_trajectory.trend {
        TrustTrend::Rising { rate } => {
            format!(
                "Trust has been rising (est. {} per event).",
                rate_label(rate.0)
            )
        }
        TrustTrend::Falling { rate } => {
            format!(
                "Trust has been declining (est. {} per event).",
                rate_label(rate.0)
            )
        }
        TrustTrend::Stable => "Trust has been stable.".into(),
        TrustTrend::Volatile { amplitude } => {
            format!("Trust is volatile (amplitude ~{}).", rate_label(amplitude.0))
        }
    };

    // --- key_memories ---
    let memories: Vec<String> = match strategy {
        CompressionStrategy::Summarize => memory
            .significant_events
            .iter()
            .rev()
            .take(10)
            .map(format_event)
            .collect(),
        CompressionStrategy::TopWeight => {
            let mut sorted: Vec<_> = memory.significant_events.iter().collect();
            sorted.sort_by_key(|b| std::cmp::Reverse(b.weight));
            sorted.iter().take(5).map(|e| format_event(e)).collect()
        }
        CompressionStrategy::TrajectoryOnly => Vec::new(),
    };

    CompressedContext {
        relationship_summary: summary,
        trajectory_narrative: trajectory,
        key_memories: memories,
        compression_metadata: CompressMeta {
            compressed_at_tick: tick,
            event_count_compressed: event_count,
            compression_strategy: strategy,
        },
    }
}

/// Select compression strategy based on approximate token budget.
pub fn select_strategy(
    memory: &RelationshipMemory,
    token_budget: u32,
) -> CompressionStrategy {
    let event_count = memory.significant_events.len();
    if token_budget >= 200 || event_count <= 5 {
        CompressionStrategy::Summarize
    } else if token_budget >= 100 {
        CompressionStrategy::TopWeight
    } else {
        CompressionStrategy::TrajectoryOnly
    }
}

/// Check whether compression is needed: after every 5 events or 1000 ticks.
pub fn should_compress(memory: &RelationshipMemory, current_tick: u64) -> bool {
    if memory.compressed_context.is_none() {
        return true;
    }
    let events_since = memory
        .significant_events
        .len()
        .saturating_sub(memory.compression_metadata().event_count_compressed as usize);
    if events_since >= 5 {
        return true;
    }
    let ticks_since = current_tick.saturating_sub(memory.last_compression_tick);
    ticks_since >= 1000
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_pair(a: &str, b: &str) -> String {
    let mut v = [a.to_string(), b.to_string()];
    v.sort();
    format!("{} & {}", v[0], v[1])
}

fn trust_label(value: i64) -> &'static str {
    if value > 512 {
        "Very High"
    } else if value > 256 {
        "High"
    } else if value > 0 {
        "Moderate"
    } else if value > -256 {
        "Low"
    } else if value > -512 {
        "Distrustful"
    } else {
        "Hostile"
    }
}

fn rate_label(rate: i64) -> String {
    // rate is in Fixed units (1024 = 1.0). Show as decimal.
    let whole = rate / 1024;
    let frac = (rate.abs() % 1024) * 100 / 1024;
    format!("{}.{:02}", whole, frac)
}

fn trend_label(trend: &TrustTrend) -> &'static str {
    match trend {
        TrustTrend::Rising { .. } => "Rising",
        TrustTrend::Falling { .. } => "Falling",
        TrustTrend::Stable => "Stable",
        TrustTrend::Volatile { .. } => "Volatile",
    }
}

fn format_event(event: &super::memory::SignificantEvent) -> String {
    let tag = match &event.event_type {
        SignificantEventType::SavedMyLife => "saved life",
        SignificantEventType::FollowedMyAdvice => "followed advice",
        SignificantEventType::ShowedTrust => "showed trust",
        SignificantEventType::DefendedMe => "defended",
        SignificantEventType::SharedSuccess => "shared success",
        SignificantEventType::OverruledMe => "overruled",
        SignificantEventType::EndangeredMe => "endangered",
        SignificantEventType::BrokeTrust => "broke trust",
        SignificantEventType::AbandonedMission => "abandoned mission",
        SignificantEventType::WasWrong { .. } => "was wrong",
        SignificantEventType::FirstMet => "first met",
        SignificantEventType::Reunited => "reunited",
        SignificantEventType::SharedSilence => "shared silence",
        SignificantEventType::ObservedFromAfar => "observed",
        SignificantEventType::PlayerNoted { .. } => "player noted",
    };
    if event.weight.0 <= 0 && event.fading {
        return String::new(); // skip faded events
    }
    format!("[{tag}] {}", event.summary)
}

// Need a method on RelationshipMemory to get compression metadata count.
// We store compression_version as the count; this is fine for the initial impl.
impl RelationshipMemory {
    pub fn compression_metadata(&self) -> CompressMeta {
        CompressMeta {
            compressed_at_tick: self.last_compression_tick,
            event_count_compressed: self.compression_version as u64,
            compression_strategy: CompressionStrategy::Summarize,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soul::memory::{RelationshipMemory, SignificantEvent, SignificantEventType};
    use crate::util::rng::Fixed;

    fn sample_memory() -> RelationshipMemory {
        let mut m = RelationshipMemory::new("boris".into(), "tove".into(), 100);
        m.record_event(
            SignificantEvent {
                tick: 150,
                event_type: SignificantEventType::SavedMyLife,
                summary: "Boris pulled Tove from a fire.".into(),
                weight: Fixed(800),
                fading: false,
            },
            150,
        );
        m.record_event(
            SignificantEvent {
                tick: 300,
                event_type: SignificantEventType::OverruledMe,
                summary: "Captain overruled Tove's repair call.".into(),
                weight: Fixed(400),
                fading: false,
            },
            300,
        );
        m.record_trust(150, Fixed(300));
        m.record_trust(300, Fixed(200));
        m
    }

    #[test]
    fn compress_summarize_produces_text() {
        let m = sample_memory();
        let ctx = compress(&m, CompressionStrategy::Summarize, 500);
        assert!(ctx.relationship_summary.contains("boris"));
        assert!(ctx.trajectory_narrative.contains("Trust"));
        assert_eq!(ctx.key_memories.len(), 2);
    }

    #[test]
    fn compress_trajectory_only_omits_events() {
        let m = sample_memory();
        let ctx = compress(&m, CompressionStrategy::TrajectoryOnly, 500);
        assert!(ctx.key_memories.is_empty());
    }

    #[test]
    fn select_strategy_chooses_correctly() {
        let m = sample_memory();
        // 2 events → always Summarize (low event count overrides budget).
        assert_eq!(select_strategy(&m, 250), CompressionStrategy::Summarize);
        assert_eq!(select_strategy(&m, 50), CompressionStrategy::Summarize);

        // Many events with limited budget → TopWeight or TrajectoryOnly.
        let mut big = sample_memory();
        for i in 0..20 {
            big.record_event(
                SignificantEvent {
                    tick: 500 + i as u64,
                    event_type: SignificantEventType::ObservedFromAfar,
                    summary: "event".into(),
                    weight: crate::util::rng::Fixed(100),
                    fading: false,
                },
                500 + i as u64,
            );
        }
        assert_eq!(select_strategy(&big, 150), CompressionStrategy::TopWeight);
        assert_eq!(select_strategy(&big, 50), CompressionStrategy::TrajectoryOnly);
    }

    #[test]
    fn should_compress_on_first_call() {
        let m = sample_memory();
        assert!(should_compress(&m, 500));
    }
}
