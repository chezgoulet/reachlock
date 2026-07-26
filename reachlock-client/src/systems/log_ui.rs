use bevy::prelude::*;

use reachlock_core::agency::log::{LogEntry, NarratorVoice};

use crate::settings::{InputAction, Settings};
use crate::theme;

/// Panel visibility toggle.
#[derive(Resource, Default)]
pub struct LogViewerVisible(pub bool);

/// All log entries, newest first.
#[derive(Resource, Default)]
pub struct LogEntries(pub Vec<LogEntry>);

/// Which entry index is currently selected / displayed in detail.
#[allow(dead_code)]
#[derive(Resource, Default)]
pub struct LogSelection(pub Option<usize>);

/// Marker on the captain's log panel text entity.
#[derive(Component)]
pub struct CaptainsLogPanel;

/// Toggle on the assigned key (L by default via OpenCaptainsLog).
pub fn captains_log_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<LogViewerVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenCaptainsLog)) {
        visible.0 = !visible.0;
    }
}

/// Spawn the panel entity (hidden by default).
pub fn spawn_captains_log_panel(mut commands: Commands) {
    commands.spawn((
        CaptainsLogPanel,
        Text::new(""),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        theme::fg("text"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            width: Val::Px(520.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Render the captain's log panel when visible.
pub fn render_captains_log(
    visible: Res<LogViewerVisible>,
    entries: Res<LogEntries>,
    mut query: Query<(&mut Text, &mut Visibility), With<CaptainsLogPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── CAPTAIN'S LOG ──".to_string()];

            if entries.0.is_empty() {
                lines.push("  No log entries yet.".into());
                lines.push("  Fly, fight, and make decisions —".into());
                lines.push("  the log will fill itself.".into());
            } else {
                for (i, entry) in entries.0.iter().enumerate() {
                    let approved_mark = if entry.approved { "✓" } else { "○" };
                    let narrator_label = match &entry.narrator_voice {
                        NarratorVoice::Captain => "Capt.",
                        NarratorVoice::ShipLog => "Ship",
                        NarratorVoice::CrewMember(n) => &n[..n.len().min(8)],
                        NarratorVoice::Omniscient => "Omni",
                    };
                    let preview = if entry.narrative.len() > 80 {
                        format!("{}…", &entry.narrative[..80])
                    } else {
                        entry.narrative.clone()
                    };
                    lines.push(format!(
                        "  {} [{}] {} — {}",
                        approved_mark, narrator_label, entry.title, preview
                    ));
                    // Separator between entries.
                    if i < entries.0.len().saturating_sub(1) {
                        lines.push("  ─────────────────".into());
                    }
                }
            }

            **text = lines.join("\n");
        } else {
            *vis = Visibility::Hidden;
            **text = String::new();
        }
    }
}
