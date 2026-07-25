//! Mission board (S46): shows generated missions for the current station/system.

use bevy::prelude::*;

use crate::settings::{InputAction, Settings};
use crate::systems::jump::MissionBoardResource;

/// Panel visibility toggle.
#[derive(Resource, Default)]
pub struct MissionBoardVisible(pub bool);

/// Marker on the mission board panel text entity.
#[derive(Component)]
pub struct MissionBoard;

/// Toggle on assigned key.
pub fn mission_board_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<MissionBoardVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenMissionBoard)) {
        visible.0 = !visible.0;
    }
}

/// Spawn the mission board panel entity.
pub fn spawn_mission_board(mut commands: Commands) {
    commands.spawn((
        MissionBoard,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.8, 0.9, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Render the mission board when visible.
pub fn render_mission_board(
    visible: Res<MissionBoardVisible>,
    missions: Res<MissionBoardResource>,
    mut query: Query<(&mut Text, &mut Visibility), With<MissionBoard>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── MISSION BOARD ──".to_string()];
            let ms = &missions.0;
            if ms.is_empty() {
                lines.push("  No missions available at this station.".into());
            } else {
                for (i, m) in ms.iter().enumerate() {
                    lines.push(format!("  {}. {} ({:?})", i + 1, m.title, m.mission_type));
                    if let Some(ref chain) = m.chain {
                        lines.push(format!(
                            "     Chain: {} ({}/{})",
                            chain.chain_title,
                            chain.position + 1,
                            chain.total_missions
                        ));
                    }
                    lines.push(format!("     Rewards: {}cr", m.rewards.credits));
                }
            }
            **text = lines.join("\n");
        } else {
            *vis = Visibility::Hidden;
            **text = String::new();
        }
    }
}
