use bevy::prelude::*;

use reachlock_core::agency::log::{
    detect_key_moments, LogEntry, LogMoment, LogSession, LoggableEvent, NarratorVoice,
    RelationshipDelta,
};
use reachlock_core::agency::log_generation::{generate_log_entry, LogGenError};

use crate::settings::Settings;
use crate::systems::dilemma::DilemmaChoiceSelected;
use crate::systems::log_ui::LogEntries;
use crate::systems::ship::ShipSystems;
use crate::systems::soul::SoulRegistry;

/// The active log capture session. Initialized on `OnEnter(AppState::InGame)`,
/// flushed on `OnExit(AppState::InGame)`.
#[allow(dead_code)]
#[derive(Resource)]
pub struct LogCapture {
    pub session_id: String,
    pub start_tick: u64,
    pub raw_events: Vec<LoggableEvent>,
    pub relationship_changes: Vec<RelationshipDelta>,
    #[allow(dead_code)]
    pub key_moments: Vec<LogMoment>,
}

impl Default for LogCapture {
    fn default() -> Self {
        LogCapture {
            session_id: format!("session-{}", epoch_secs()),
            start_tick: 0,
            raw_events: Vec::new(),
            relationship_changes: Vec::new(),
            key_moments: Vec::new(),
        }
    }
}

impl LogCapture {
    pub fn push_event(&mut self, event: LoggableEvent) {
        self.raw_events.push(event);
    }

    pub fn push_relationship_delta(&mut self, delta: RelationshipDelta) {
        self.relationship_changes.push(delta);
    }

    /// Flush the buffer: detect key moments, generate a log entry, and
    /// return the session. Clears the internal buffers.
    #[allow(dead_code)]
    pub fn flush(&mut self) -> LogSession {
        let moments = detect_key_moments(&self.raw_events, &self.relationship_changes);
        self.key_moments = moments.clone();

        let end_tick = self
            .raw_events
            .last()
            .map(|e| e.tick)
            .unwrap_or(self.start_tick);

        LogSession {
            session_id: self.session_id.clone(),
            start_tick: self.start_tick,
            end_tick,
            raw_events: std::mem::take(&mut self.raw_events),
            relationship_changes: std::mem::take(&mut self.relationship_changes),
            key_moments: moments,
            previous_entry_summary: None,
            generated_entry: None,
        }
    }

    fn generated_entry(&self) -> Option<LogEntry> {
        if self.raw_events.is_empty() {
            return None;
        }
        let moments = detect_key_moments(&self.raw_events, &self.relationship_changes);
        match generate_log_entry(
            &self.raw_events,
            &moments,
            None,
            &NarratorVoice::Captain,
            &[],
            200,
            &self.session_id,
        ) {
            Ok(entry) => Some(entry),
            Err(LogGenError::NoEvents) => None,
            Err(_) => {
                warn!("log capture: generation failed unexpectedly");
                None
            }
        }
    }
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Initialize the log capture on game start.
#[allow(dead_code)]
pub fn init_log_capture(mut commands: Commands) {
    commands.insert_resource(LogCapture::default());
}

/// Capture combat damage events: when the player hull takes a hit, push
/// a combat loggable event.
pub fn capture_combat_damage(
    systems: Res<ShipSystems>,
    settings: Res<Settings>,
    mut capture: ResMut<LogCapture>,
    mut prev_hull: Local<i64>,
) {
    let hull = systems.hull_hp.0;
    if hull < *prev_hull {
        let damage = *prev_hull - hull;
        // S71: combat_log_verbosity filters log detail (0 = none, 3 = verbose).
        if settings.gameplay.combat_log_verbosity >= 1 {
            capture.push_event(LoggableEvent {
                tick: 0,
                kind: "combat".into(),
                crew_involved: vec![],
                summary: format!("Hull damaged: -{damage} HP"),
            });
        }
    }
    *prev_hull = hull;
}

/// Capture dilemma resolution.
pub fn capture_dilemma_resolution(
    mut capture: ResMut<LogCapture>,
    choice: Res<DilemmaChoiceSelected>,
    mut prev: Local<Option<usize>>,
) {
    if choice.0.is_some() && choice.0 != *prev {
        capture.push_event(LoggableEvent {
            tick: 0,
            kind: "dilemma".into(),
            crew_involved: vec![],
            summary: "Captain resolved a dilemma.".into(),
        });
    }
    *prev = choice.0;
}

/// Capture crew relationship changes from the soul registry.
/// Tracks trust deltas by comparing current trust against a cached snapshot.
pub fn capture_crew_relationship_changes(
    souls: Res<SoulRegistry>,
    mut capture: ResMut<LogCapture>,
    mut snapshot: Local<Option<std::collections::HashMap<(String, String), i64>>>,
) {
    let snap = snapshot.get_or_insert_with(std::collections::HashMap::new);
    for state in souls.states.values() {
        for rel in &state.relationships {
            let key = (state.soul_id.clone(), rel.target_id.clone());
            let prev = snap.get(&key).copied().unwrap_or(rel.trust);
            let delta = rel.trust.saturating_sub(prev);
            if delta.abs() >= 50 {
                capture.push_relationship_delta(RelationshipDelta {
                    a: state.soul_id.clone(),
                    b: rel.target_id.clone(),
                    trust_delta: delta,
                    tick: 0,
                });
            }
            snap.insert(key, rel.trust);
        }
    }
}

/// Flush the log capture on game exit and push a generated entry.
pub fn flush_log_capture(mut capture: ResMut<LogCapture>, mut entries: ResMut<LogEntries>) {
    if capture.raw_events.is_empty() {
        info!("log capture: no events this session, nothing to generate");
        return;
    }
    if let Some(entry) = capture.generated_entry() {
        info!(
            "log capture: generated entry '{}' from {} events",
            entry.title,
            capture.raw_events.len()
        );
        entries.0.insert(0, entry);
        if entries.0.len() > 100 {
            let dropped = entries.0.split_off(100);
            info!(
                "log capture: dropped {} old entries (cap 100)",
                dropped.len()
            );
        }
    }
    // Reset capture for next session.
    *capture = LogCapture::default();
}
