//! Crew comm surfaces (S16B, closing the S16 "speech bubbles/comm lines"
//! deliverable): one [`CommFeed`] that crew speech flows through, rendered
//! two ways — a fading comm line on the flight HUD, and a speech bubble
//! above the speaker's figure when you're walking the ship. Same voice
//! pipeline as dialogue (lines arrive already shaped).
//!
//! S33 adds co-deliberation: when crew contracts hit the LLM edge together,
//! they argue it out on the comms panel instead of deciding in isolation.
//! [`CrewConference`] drives a [`reachlock_core::contract::CoDeliberation`]
//! session one turn at a time (the deliberation IS the content), the player
//! can cut it short with "my way" (ENTER), and the resulting relationship
//! deltas persist in [`CrewRelationships`] for S35's long-term memory.

use std::collections::BTreeMap;

use bevy::prelude::*;

use reachlock_core::contract::co_deliberation::{
    CoDeliberation, CoDeliberationMetrics, CrewDeliberant, CrewPosition, CrewRelationship,
    GameEvent, RelationshipState, StepOutcome,
};

use crate::settings::{InputAction, Settings};
use crate::states::GameMode;
use crate::systems::contract::{DeliberationState, ShipLog};
use crate::systems::crew::{CrewFigure, CrewRole, CrewRoster};
use crate::systems::interior::ysort;
use crate::systems::soul::SoulRegistry;

/// Seconds between co-deliberation turns. Deliberate by design — the player
/// watches the exchange unfold (spec S33).
const CREW_TURN_DELAY: f32 = 0.4;

/// Seconds a comm line stays on the HUD / over a head.
const COMM_TTL: f32 = 6.0;
/// Most comm lines held at once (older ones scroll away).
const COMM_CAP: usize = 4;

pub struct CommLine {
    pub speaker: String,
    pub line: String,
    pub age: f32,
}

/// The live comm traffic. Anything the crew says out loud goes through
/// here; the log keeps the permanent record, this is the moment.
#[derive(Resource, Default)]
pub struct CommFeed {
    pub entries: Vec<CommLine>,
}

impl CommFeed {
    pub fn say(&mut self, speaker: impl Into<String>, line: impl Into<String>) {
        self.entries.push(CommLine {
            speaker: speaker.into(),
            line: line.into(),
            age: 0.0,
        });
        if self.entries.len() > COMM_CAP {
            self.entries.remove(0);
        }
    }
}

/// Marker for the HUD comm readout (top-center).
#[derive(Component)]
pub struct CommHud;

/// Marker for an in-world speech bubble; carries its speaker so it follows
/// (and dies with) the right figure.
#[derive(Component)]
pub struct CommBubble {
    pub speaker: String,
}

/// Map a crew relationship delta to a significant event type for S35 memory.
fn relationship_event_for_delta(
    rel: &reachlock_core::contract::co_deliberation::CrewRelationship,
) -> reachlock_core::soul::memory::SignificantEventType {
    use reachlock_core::soul::memory::SignificantEventType;
    if rel.trust.0 > 256 {
        SignificantEventType::ShowedTrust
    } else if rel.trust.0 < -256 {
        SignificantEventType::BrokeTrust
    } else if rel.respect.0 > 256 {
        SignificantEventType::DefendedMe
    } else if rel.tension.0 > 512 {
        SignificantEventType::OverruledMe
    } else {
        SignificantEventType::SharedSilence
    }
}

pub fn spawn_comm_hud(mut commands: Commands) {
    commands.spawn((
        CommHud,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.95, 0.9)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(56.0),
            left: Val::Percent(30.0),
            ..default()
        },
    ));
}

/// Age the feed and render the HUD comm lines (all modes — comms carry
/// through the hull).
pub fn tick_comms(
    time: Res<Time>,
    mut feed: ResMut<CommFeed>,
    mut hud: Query<&mut Text, With<CommHud>>,
) {
    let dt = time.delta_secs();
    for entry in feed.entries.iter_mut() {
        entry.age += dt;
    }
    feed.entries.retain(|e| e.age < COMM_TTL);
    if let Ok(mut text) = hud.single_mut() {
        **text = feed
            .entries
            .iter()
            .map(|e| format!("{} » {}", e.speaker, e.line))
            .collect::<Vec<_>>()
            .join("\n");
    }
}

/// On board, the newest comm line also appears as a bubble above the
/// speaker's figure (matched by roster display name via the figure's id).
#[allow(clippy::type_complexity)]
pub fn comm_bubbles(
    mode: Option<Res<State<GameMode>>>,
    feed: Res<CommFeed>,
    roster: Res<crate::systems::crew::CrewRoster>,
    figures: Query<(&CrewFigure, &Transform), Without<CommBubble>>,
    mut bubbles: Query<(Entity, &CommBubble, &mut Transform, &mut Text2d)>,
    mut commands: Commands,
) {
    let on_board = mode.is_some_and(|m| **m == GameMode::OnBoard);
    // Latest fresh line per speaker.
    let mut wanted: Vec<(&str, &str)> = Vec::new();
    if on_board {
        for entry in feed.entries.iter().rev() {
            if entry.age < COMM_TTL * 0.7 && !wanted.iter().any(|(s, _)| *s == entry.speaker) {
                wanted.push((&entry.speaker, &entry.line));
            }
        }
    }
    let figure_pos = |speaker: &str| -> Option<Vec2> {
        let member = roster.members.iter().find(|m| m.name == speaker)?;
        figures
            .iter()
            .find_map(|(fig, t)| (fig.0 == member.id).then(|| t.translation.truncate()))
    };
    // Update or despawn existing bubbles.
    let mut covered: Vec<String> = Vec::new();
    for (entity, bubble, mut transform, mut text) in &mut bubbles {
        match (
            wanted.iter().find(|(s, _)| *s == bubble.speaker),
            figure_pos(&bubble.speaker),
        ) {
            (Some((_, line)), Some(pos)) => {
                **text = (*line).to_string();
                transform.translation = Vec3::new(pos.x, pos.y + 34.0, ysort(pos.y) + 0.05);
                covered.push(bubble.speaker.clone());
            }
            _ => commands.entity(entity).despawn(),
        }
    }
    // Spawn missing bubbles.
    for (speaker, line) in wanted {
        if covered.iter().any(|c| c == speaker) {
            continue;
        }
        let Some(pos) = figure_pos(speaker) else {
            continue; // speaker is on the other deck — HUD line only
        };
        commands.spawn((
            CommBubble {
                speaker: speaker.to_string(),
            },
            Text2d::new(line.to_string()),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(Color::srgb(0.95, 0.97, 0.9)),
            Transform::from_xyz(pos.x, pos.y + 34.0, ysort(pos.y) + 0.05),
            crate::states::ModeScope(GameMode::OnBoard),
        ));
    }
}

// ===========================================================================
// S33 — co-deliberation
// ===========================================================================

/// The active crew conference, if any. Drives a [`CoDeliberation`] session
/// one turn at a time via [`tick_crew_conference`].
#[derive(Resource, Default)]
pub struct CrewConference {
    pub session: Option<CoDeliberation>,
    /// Counts up to [`CREW_TURN_DELAY`] between turns.
    pub timer: f32,
    /// The trigger event type, for the ship-log line.
    pub last_trigger: String,
}

/// Live crew-to-crew relationship state, keyed by `(speaker_id, other_id)`.
/// S33 writes here; S35 (persistent relationship memory) will extend and
/// persist it into the soul save.
#[derive(Resource, Default)]
pub struct CrewRelationships {
    pub map: BTreeMap<(String, String), CrewRelationship>,
}

/// The S33 research/metrics table: one entry per co-deliberation session
/// (structural data only, no PII).
#[derive(Resource, Default)]
pub struct CoDeliberationLog {
    pub events: Vec<CoDeliberationMetrics>,
}

/// What a crew role proposes when a conference opens. The real system would
/// derive this from each crew member's contract evaluation (S06/S16); this
/// is the offline-first default so co-deliberation runs without a server.
fn role_action(role: CrewRole) -> &'static str {
    match role {
        CrewRole::Pilot => "hold_course",
        CrewRole::Engineer => "repair_systems",
        CrewRole::Navigator => "plot_jump",
        CrewRole::Medic => "tend_medbay",
        CrewRole::Gunner => "man_battle_stations",
    }
}

/// Build a co-deliberation session from the current crew: each member opens
/// with a `Propose` stance from their role, and starts neutral toward every
/// other member.
pub fn start_conference(roster: &CrewRoster, trigger: GameEvent) -> CoDeliberation {
    let ids: Vec<String> = roster.members.iter().map(|m| m.id.clone()).collect();
    let participants = roster
        .members
        .iter()
        .map(|m| {
            let mut relationship_state: RelationshipState = BTreeMap::new();
            for other in ids.iter().filter(|o| **o != m.id) {
                relationship_state.insert(other.clone(), CrewRelationship::default());
            }
            let action = role_action(m.role);
            CrewDeliberant {
                crew_id: m.id.clone(),
                relationship_state,
                initial_position: CrewPosition::Propose {
                    action: action.to_string(),
                    reasoning: format!("{} recommends {}", m.name, action),
                },
                current_position: CrewPosition::Propose {
                    action: action.to_string(),
                    reasoning: format!("{} recommends {}", m.name, action),
                },
            }
        })
        .collect();
    CoDeliberation::new(participants, trigger)
}

/// Player-initiated crew conference (the "hold a crew meeting" mechanic).
/// Press the conference key (default Y) to open one with the current crew.
pub fn crew_conference_hotkey(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    roster: Res<CrewRoster>,
    mut conf: ResMut<CrewConference>,
) {
    if conf.session.is_some() {
        return;
    }
    if keys.just_pressed(settings.key(InputAction::OpenCrewConference)) {
        let trigger = GameEvent {
            event_type: "crew_conference".into(),
            summary: "Captain called a crew conference.".into(),
            fields: BTreeMap::new(),
        };
        conf.session = Some(start_conference(&roster, trigger.clone()));
        conf.timer = 0.0;
        conf.last_trigger = trigger.event_type;
    }
}

/// Advance the active conference one turn at a time, render each spoken turn
/// to the comms feed, and persist the outcome. Never blocks the game loop:
/// turns are paced by [`CREW_TURN_DELAY`] and the game keeps running between
/// them (spec S33 gotcha).
#[allow(clippy::too_many_arguments)]
pub fn tick_crew_conference(
    time: Res<Time>,
    mut conf: ResMut<CrewConference>,
    mut feed: ResMut<CommFeed>,
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut rels: ResMut<CrewRelationships>,
    mut log: ResMut<CoDeliberationLog>,
    mut ship_log: ResMut<ShipLog>,
    mut deliberation: ResMut<DeliberationState>,
    mut souls: ResMut<SoulRegistry>,
) {
    let Some(mut session) = conf.session.take() else {
        return;
    };

    // Player override: ENTER ("my way") short-circuits the remaining
    // deliberation. ESC is a no-op — let them work it out.
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        let action = session
            .leading_action()
            .unwrap_or_else(|| "hold_course".to_string());
        session.player_override(action);
    }

    conf.timer += time.delta_secs();
    if conf.timer < CREW_TURN_DELAY {
        conf.session = Some(session);
        return;
    }
    conf.timer = 0.0;

    match session.step() {
        StepOutcome::Turn(t) => {
            feed.say(t.speaker, t.visible_to_player);
            conf.session = Some(session);
        }
        StepOutcome::Resolved(resolution) => {
            feed.say("crew", format!("Crew reached a decision: {resolution:?}"));
            ship_log.log(format!(
                "Crew conference ({}) resolved: {resolution:?}",
                conf.last_trigger
            ));
            log.events.push(session.metrics());
            deliberation.just_completed = Some("crew".into());
            // S35: record co-deliberation relationship events into soul states.
            for p in &session.participants {
                for (other, rel) in &p.relationship_state {
                    let event = reachlock_core::soul::memory::SignificantEvent {
                        tick: 0,
                        event_type: relationship_event_for_delta(rel),
                        summary: format!("co-deliberation: {} with {}", p.crew_id, rel.trust.0),
                        weight: reachlock_core::util::rng::Fixed(rel.trust.0.abs().min(1024)),
                        fading: false,
                    };
                    souls.record_interaction(&p.crew_id, other, &event);
                }
            }
            // Persist relationship deltas (feeds S35 persistent memory).
            for p in &session.participants {
                for (other, rel) in &p.relationship_state {
                    rels.map
                        .insert((p.crew_id.clone(), other.clone()), rel.clone());
                }
            }
            // Leave `conf.session` as None — the conference is over.
        }
    }
}
