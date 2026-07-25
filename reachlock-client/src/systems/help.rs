use bevy::prelude::*;

use crate::settings::{InputAction, Settings};
use crate::states::GameMode;

#[derive(Resource, Default)]
pub struct HelpMode {
    pub active: bool,
}

#[derive(Component)]
pub struct HelpLabel;

pub fn toggle_help_mode(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut help: ResMut<HelpMode>,
) {
    let help_key = settings.key(InputAction::OpenHelp);
    if keys.just_pressed(help_key) {
        help.active = !help.active;
    }
    if help.active && keys.just_pressed(KeyCode::Escape) {
        help.active = false;
    }
}

pub fn spawn_help_labels(help: Res<HelpMode>, mut commands: Commands, mode: Res<State<GameMode>>) {
    if !help.is_changed() {
        return;
    }

    if help.active {
        let labels: Vec<String> = match **mode {
            GameMode::SpaceFlight => vec!["Press F1 for help".into()],
            GameMode::Landed | GameMode::OnBoard => vec!["Press F1 for help".into()],
            _ => vec![],
        };
        for text in &labels {
            commands.spawn((
                HelpLabel,
                Text::new(text.as_str()),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.8, 1.0)),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
            ));
        }
    }
}

pub fn despawn_help_labels(mut commands: Commands, labels: Query<Entity, With<HelpLabel>>) {
    for entity in &labels {
        commands.entity(entity).despawn();
    }
}
