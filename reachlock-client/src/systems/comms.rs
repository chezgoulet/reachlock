//! Crew comm surfaces (S16B, closing the S16 "speech bubbles/comm lines"
//! deliverable): one [`CommFeed`] that crew speech flows through, rendered
//! two ways — a fading comm line on the flight HUD, and a speech bubble
//! above the speaker's figure when you're walking the ship. Same voice
//! pipeline as dialogue (lines arrive already shaped).

use bevy::prelude::*;

use crate::states::GameMode;
use crate::systems::crew::CrewFigure;
use crate::systems::interior::ysort;

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
