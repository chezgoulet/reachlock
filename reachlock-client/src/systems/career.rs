//! Career panel (S42): shows the player's active career paths, ranks,
//! progress, and perks. Lines of text on a toggleable panel.

use bevy::prelude::*;

use reachlock_core::career::piracy::PiracyState;
use reachlock_core::career::PlayerCareer;

use crate::settings::{InputAction, Settings};

/// Career panel visibility toggle.
#[derive(Resource, Default)]
pub struct CareerPanelVisible(pub bool);

/// Marker on the career panel text entity.
#[derive(Component)]
pub struct CareerPanel;

/// The player's career state.
#[derive(Resource, Default)]
pub struct CareerResource(pub Option<PlayerCareer>);

/// The player's piracy state (notoriety, bounties, contraband knowledge).
#[derive(Resource, Default)]
pub struct PiracyResource(pub Option<PiracyState>);

/// Toggle on the assigned key.
pub fn career_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<CareerPanelVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenCrewRoster)) {
        visible.0 = !visible.0;
    }
}

/// Spawn the panel entity (hidden by default).
pub fn spawn_career_panel(mut commands: Commands) {
    commands.spawn((
        CareerPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.7, 0.95, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Render the panel text when visible.
pub fn render_career_panel(
    visible: Res<CareerPanelVisible>,
    career: Res<CareerResource>,
    piracy: Res<PiracyResource>,
    mut query: Query<(&mut Text, &mut Visibility), With<CareerPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── CAREERS ──".to_string()];

            // Career section.
            match &career.0 {
                None => lines.push("  No career data loaded.".into()),
                Some(pc) => {
                    if pc.active_paths.is_empty() && pc.completed_paths.is_empty() {
                        lines.push("  No career paths joined yet.".into());
                    }
                    for ap in &pc.active_paths {
                        lines.push(format!("  {} — rank {}  prestige {}", ap.path_id, ap.current_rank, pc.total_prestige));
                        for (action, count) in &ap.progress {
                            lines.push(format!("    {:?}: {}", action, count));
                        }
                    }
                    for cp in &pc.completed_paths {
                        lines.push(format!("  [done] {} — final rank {} ({:?})", cp.path_id, cp.final_rank, cp.reason));
                    }
                }
            }

            // Piracy section.
            lines.push("".into());
            lines.push("── NOTORIETY ──".into());
            match &piracy.0 {
                None => lines.push("  No piracy data loaded.".into()),
                Some(ps) => {
                    lines.push(format!("  Level: {:?} (value: {})", ps.notoriety.level, ps.notoriety.value));
                    if !ps.active_bounties.is_empty() {
                        lines.push(format!("  Bounties: {}", ps.active_bounties.len()));
                        for b in &ps.active_bounties {
                            lines.push(format!("    {} {}cr ({})", b.issuer_faction, b.amount, b.crime));
                        }
                    }
                    if !ps.current_havens_known.is_empty() {
                        lines.push(format!("  Pirate havens known: {}", ps.current_havens_known.len()));
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
