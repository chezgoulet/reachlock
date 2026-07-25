//! Captain's log — key moment detection and session summarization (S37, spec §18).
//! Pure functions: no I/O, no LLM calls.

use serde::{Deserialize, Serialize};

/// One session's worth of captain's log data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogSession {
    pub session_id: String,
    pub start_tick: u64,
    pub end_tick: u64,
    pub raw_events: Vec<LoggableEvent>,
    pub relationship_changes: Vec<RelationshipDelta>,
    pub key_moments: Vec<LogMoment>,
    pub previous_entry_summary: Option<String>,
    pub generated_entry: Option<LogEntry>,
}

/// A game event that can be logged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggableEvent {
    pub tick: u64,
    pub kind: String,
    pub crew_involved: Vec<String>,
    pub summary: String,
}

/// A relationship change between two crew members.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipDelta {
    pub a: String,
    pub b: String,
    pub trust_delta: i64,
    pub tick: u64,
}

/// A generated log entry: the captain's narrative for one session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub session_id: String,
    pub title: String,
    pub narrative: String,
    pub narrator_voice: NarratorVoice,
    pub generated_at: u64,
    pub model_used: String,
    pub approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NarratorVoice {
    Captain,
    ShipLog,
    CrewMember(String),
    Omniscient,
}

/// One detected key moment in a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogMoment {
    pub tick: u64,
    pub moment_type: LogMomentType,
    pub summary: String,
    pub significance: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogMomentType {
    CrewDeliberation,
    DilemmaResolved,
    CombatOutcome,
    FactionMilestone,
    CrewMilestone,
    Discovery,
    Loss,
    Triumph,
    PlayerChoice,
}

/// Request payload for LLM-based log generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogGenerationRequest {
    pub session_events: Vec<LoggableEvent>,
    pub relationship_changes: Vec<RelationshipDelta>,
    pub key_moments: Vec<LogMoment>,
    pub previous_entry: Option<String>,
    pub narrator: NarratorVoice,
    pub style_hints: Vec<String>,
    pub max_words: u32,
}

// ---------------------------------------------------------------------------
// Key moment detection (pure function, no LLM)
// ---------------------------------------------------------------------------

/// Detect key moments from session events and relationship changes.
/// Rules-based: no LLM calls.
pub fn detect_key_moments(
    events: &[LoggableEvent],
    deltas: &[RelationshipDelta],
) -> Vec<LogMoment> {
    let mut moments = Vec::new();

    for event in events {
        let moment_type = match event.kind.as_str() {
            "deliberation" if event.crew_involved.len() >= 2 => {
                Some(LogMomentType::CrewDeliberation)
            }
            "dilemma" => Some(LogMomentType::DilemmaResolved),
            "combat" => Some(LogMomentType::CombatOutcome),
            "discovery" => Some(LogMomentType::Discovery),
            "triumph" => Some(LogMomentType::Triumph),
            "loss" | "death" => Some(LogMomentType::Loss),
            "choice" | "decision" => Some(LogMomentType::PlayerChoice),
            _ => None,
        };
        if let Some(mt) = moment_type {
            let sig = score_significance(event, deltas);
            moments.push(LogMoment {
                tick: event.tick,
                moment_type: mt,
                summary: event.summary.clone(),
                significance: sig,
            });
        }
    }

    // Check for recurring patterns (same pair argued 3+ times).
    let mut pair_count: std::collections::HashMap<(String, String), u32> =
        std::collections::HashMap::new();
    for delta in deltas {
        let key = if delta.a <= delta.b {
            (delta.a.clone(), delta.b.clone())
        } else {
            (delta.b.clone(), delta.a.clone())
        };
        *pair_count.entry(key).or_insert(0) += 1;
    }
    for ((a, b), count) in &pair_count {
        if *count >= 3 {
            moments.push(LogMoment {
                tick: 0,
                moment_type: LogMomentType::CrewMilestone,
                summary: format!("{a} and {b} disagreed {count} times this session"),
                significance: 4,
            });
        }
    }

    // Sort by tick.
    moments.sort_by_key(|m| m.tick);
    moments
}

/// Score a moment's significance (0-10).
/// Weight: crew involvement, consequence persistence, event rarity,
/// relationship change magnitude.
pub fn score_significance(event: &LoggableEvent, deltas: &[RelationshipDelta]) -> u8 {
    let mut score: u8 = 5; // baseline

    // Crew involvement boosts significance.
    if event.crew_involved.len() >= 3 {
        score = score.saturating_add(2);
    } else if event.crew_involved.len() >= 2 {
        score = score.saturating_add(1);
    }

    // Relationship changes boost significance.
    let trust_delta_sum: i64 = deltas.iter().map(|d| d.trust_delta.abs()).sum();
    if trust_delta_sum > 1024 {
        score = score.saturating_add(3);
    } else if trust_delta_sum > 512 {
        score = score.saturating_add(2);
    } else if trust_delta_sum > 256 {
        score = score.saturating_add(1);
    }

    // Event type weighting.
    match event.kind.as_str() {
        "loss" | "death" => score = score.saturating_add(3),
        "dilemma" => score = score.saturating_add(2),
        "triumph" => score = score.saturating_add(1),
        _ => {}
    }

    score.min(10)
}

/// Build a template-based log summary for offline/non-LLM mode.
pub fn template_summary(events: &[LoggableEvent], moments: &[LogMoment]) -> String {
    let event_count = events.len();
    let moment_count = moments.len();
    let significant: Vec<&LogMoment> = moments.iter().filter(|m| m.significance >= 7).collect();

    let mut summary =
        format!("Session summary: {event_count} events, {moment_count} notable moments.");
    if !significant.is_empty() {
        summary.push_str(&format!(" {} significant moments.", significant.len()));
        for m in significant.iter().take(3) {
            summary.push_str(&format!(
                "\n- [{}] {}",
                moment_label(&m.moment_type),
                m.summary
            ));
        }
    }
    summary
}

fn moment_label(mt: &LogMomentType) -> &'static str {
    match mt {
        LogMomentType::CrewDeliberation => "deliberation",
        LogMomentType::DilemmaResolved => "dilemma",
        LogMomentType::CombatOutcome => "combat",
        LogMomentType::FactionMilestone => "faction",
        LogMomentType::CrewMilestone => "crew event",
        LogMomentType::Discovery => "discovery",
        LogMomentType::Loss => "loss",
        LogMomentType::Triumph => "triumph",
        LogMomentType::PlayerChoice => "choice",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(tick: u64, kind: &str, crew: &[&str]) -> LoggableEvent {
        LoggableEvent {
            tick,
            kind: kind.into(),
            crew_involved: crew.iter().map(|s| s.to_string()).collect(),
            summary: format!("{kind} at tick {tick}"),
        }
    }

    #[test]
    fn detect_crew_deliberation() {
        let e = vec![event(100, "deliberation", &["boris", "tove"])];
        let moments = detect_key_moments(&e, &[]);
        assert!(moments
            .iter()
            .any(|m| m.moment_type == LogMomentType::CrewDeliberation));
    }

    #[test]
    fn detect_dilemma() {
        let e = vec![event(100, "dilemma", &["boris"])];
        let moments = detect_key_moments(&e, &[]);
        assert!(moments
            .iter()
            .any(|m| m.moment_type == LogMomentType::DilemmaResolved));
    }

    #[test]
    fn recurring_arguments_detected() {
        let deltas = vec![
            RelationshipDelta {
                a: "boris".into(),
                b: "tove".into(),
                trust_delta: -50,
                tick: 1,
            },
            RelationshipDelta {
                a: "boris".into(),
                b: "tove".into(),
                trust_delta: -30,
                tick: 2,
            },
            RelationshipDelta {
                a: "boris".into(),
                b: "tove".into(),
                trust_delta: -20,
                tick: 3,
            },
        ];
        let moments = detect_key_moments(&[], &deltas);
        assert!(moments
            .iter()
            .any(|m| m.moment_type == LogMomentType::CrewMilestone));
    }

    #[test]
    fn significance_scales_with_crew_count() {
        let solo = event(100, "combat", &["boris"]);
        let crew = event(100, "combat", &["boris", "tove", "risc"]);
        assert!(score_significance(&crew, &[]) > score_significance(&solo, &[]));
    }

    #[test]
    fn template_summary_non_empty() {
        let e = vec![event(100, "dilemma", &["boris"])];
        let moments = detect_key_moments(&e, &[]);
        let summary = template_summary(&e, &moments);
        assert!(!summary.is_empty());
        assert!(summary.contains("dilemma"));
    }
}
