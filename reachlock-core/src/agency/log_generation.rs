//! Log entry generation (S37). LLM-based with template fallback for offline.
//! Pure data types — the actual LLM call is the caller's responsibility.

use super::log::{LogEntry, LogMoment, LoggableEvent, NarratorVoice};

/// Error type for log generation failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogGenError {
    NoEvents,
    QuotaExhausted,
    OfflineFallback,
}

/// Generate a log entry from a request.
/// Returns the generated entry or an error.
/// The caller dispatches the LLM call; this function builds the prompt
/// context and parses the response.
pub fn generate_log_entry(
    events: &[LoggableEvent],
    moments: &[LogMoment],
    _previous_entry: Option<&str>,
    narrator: &NarratorVoice,
    style_hints: &[String],
    max_words: u32,
    session_id: &str,
) -> Result<LogEntry, LogGenError> {
    if events.is_empty() {
        return Err(LogGenError::NoEvents);
    }

    let title = build_title(moments, narrator);
    let narrative = build_narrative(events, moments, narrator, style_hints, max_words);

    let model = "core-template";

    Ok(LogEntry {
        session_id: session_id.into(),
        title,
        narrative,
        narrator_voice: narrator.clone(),
        generated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        model_used: model.into(),
        approved: false,
    })
}

/// Build the prompt context for an LLM log generation call.
/// The caller sends this as the `context` field in an `LlmCall`.
pub fn build_prompt_context(
    events: &[LoggableEvent],
    moments: &[LogMoment],
    previous_entry: Option<&str>,
    narrator: &NarratorVoice,
    style_hints: &[String],
    max_words: u32,
) -> serde_json::Value {
    let moment_summaries: Vec<String> = moments
        .iter()
        .map(|m| format!("[sig={}] {}", m.significance, m.summary))
        .collect();

    serde_json::json!({
        "event_count": events.len(),
        "moment_count": moments.len(),
        "moments": moment_summaries,
        "previous_entry": previous_entry,
        "narrator": format!("{narrator:?}"),
        "style_hints": style_hints,
        "max_words": max_words,
        "instruction": "Write the captain's log entry for this session. \
                        Narrative, not bullet points. Focus on the moments\
                        that mattered. Use the narrator voice specified."
    })
}

/// Build a template-based narrative if LLM is unavailable.
/// Deterministic from event data — no I/O.
pub fn template_narrative(events: &[LoggableEvent], moments: &[LogMoment]) -> String {
    use super::log::LogMomentType;

    let total = events.len();
    let deliberation_count = moments
        .iter()
        .filter(|m| m.moment_type == LogMomentType::CrewDeliberation)
        .count();
    let combat_count = moments
        .iter()
        .filter(|m| m.moment_type == LogMomentType::CombatOutcome)
        .count();
    let dilemma_count = moments
        .iter()
        .filter(|m| m.moment_type == LogMomentType::DilemmaResolved)
        .count();

    let mut lines = vec![
        format!(
            "Session log. {total} events logged. \
             {deliberation_count} crew deliberations, \
             {combat_count} combat encounters, \
             {dilemma_count} dilemmas."
        ),
        String::new(),
    ];

    for m in moments.iter().filter(|m| m.significance >= 6).take(5) {
        lines.push(format!("  · {}", m.summary));
    }

    if !moments.is_empty() {
        lines.push(String::new());
        lines.push("Notable moments recorded.".into());
    }

    lines.join("\n")
}

fn build_title(moments: &[LogMoment], narrator: &NarratorVoice) -> String {
    let top = moments.iter().max_by_key(|m| m.significance);
    match top {
        Some(m) => match narrator {
            NarratorVoice::Captain => format!("Day —: {}", m.summary),
            NarratorVoice::ShipLog => format!("Log entry: {}", m.summary),
            NarratorVoice::CrewMember(name) => {
                format!("{name}'s account: {}", m.summary)
            }
            NarratorVoice::Omniscient => format!("The story of {}", m.summary),
        },
        None => "Quiet session".into(),
    }
}

fn build_narrative(
    events: &[LoggableEvent],
    moments: &[LogMoment],
    narrator: &NarratorVoice,
    _style_hints: &[String],
    _max_words: u32,
) -> String {
    if moments.is_empty() {
        return format!(
            "A quiet session with {} events. Nothing remarkable to report.",
            events.len()
        );
    }

    let prelude = match narrator {
        NarratorVoice::Captain => "Here is what happened today:".into(),
        NarratorVoice::ShipLog => "The ship's log records:".into(),
        NarratorVoice::CrewMember(name) => format!("{name} recalls:"),
        NarratorVoice::Omniscient => "The tale unfolds:".into(),
    };

    let body: Vec<String> = moments
        .iter()
        .filter(|m| m.significance >= 5)
        .take(10)
        .map(|m| format!("- {}", m.summary))
        .collect();

    if body.is_empty() {
        return format!("{prelude}\nNothing significant to note.");
    }

    format!("{prelude}\n{}", body.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agency::log::LogMomentType;

    fn sample_request() -> (Vec<LoggableEvent>, Vec<LogMoment>) {
        let events = vec![
            LoggableEvent {
                tick: 100,
                kind: "deliberation".into(),
                crew_involved: vec!["boris".into(), "tove".into()],
                summary: "Crew debated repair priorities.".into(),
            },
            LoggableEvent {
                tick: 200,
                kind: "dilemma".into(),
                crew_involved: vec!["boris".into()],
                summary: "Dilemma resolved: saved crew.".into(),
            },
        ];
        let moments = vec![
            LogMoment {
                tick: 100,
                moment_type: LogMomentType::CrewDeliberation,
                summary: "Crew debated repair priorities.".into(),
                significance: 7,
            },
            LogMoment {
                tick: 200,
                moment_type: LogMomentType::DilemmaResolved,
                summary: "Dilemma resolved: saved crew.".into(),
                significance: 8,
            },
        ];
        (events, moments)
    }

    #[test]
    fn generate_with_events_produces_entry() {
        let (events, moments) = sample_request();
        let entry = generate_log_entry(
            &events,
            &moments,
            None,
            &NarratorVoice::Captain,
            &[],
            200,
            "session-1",
        )
        .unwrap();
        assert!(!entry.title.is_empty());
        assert!(!entry.narrative.is_empty());
        assert_eq!(entry.narrator_voice, NarratorVoice::Captain);
    }

    #[test]
    fn empty_events_returns_error() {
        let result = generate_log_entry(
            &[],
            &[],
            None,
            &NarratorVoice::ShipLog,
            &[],
            200,
            "session-empty",
        );
        assert_eq!(result, Err(LogGenError::NoEvents));
    }

    #[test]
    fn prompt_context_includes_moments() {
        let (events, moments) = sample_request();
        let ctx = build_prompt_context(
            &events,
            &moments,
            None,
            &NarratorVoice::Omniscient,
            &[],
            200,
        );
        assert!(ctx.get("moments").is_some());
        assert_eq!(ctx["event_count"], 2);
    }

    #[test]
    fn template_narrative_is_deterministic() {
        let (events, moments1) = sample_request();
        let (_, moments2) = sample_request();
        let a = template_narrative(&events, &moments1);
        let b = template_narrative(&events, &moments2);
        assert_eq!(a, b);
    }
}
