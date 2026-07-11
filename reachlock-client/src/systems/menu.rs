//! Main menu: title card, Enter to launch. The seed IS the game — show it.

use bevy::prelude::*;

use crate::states::AppState;
use crate::systems::setup::SYSTEM_SEED;

#[derive(Component)]
pub struct MenuUi;

pub fn spawn_menu(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        MenuUi,
        Text::new(format!(
            "R E A C H L O C K\n\nsystem seed {SYSTEM_SEED:#x}\n\npress ENTER to launch"
        )),
        TextFont {
            font_size: 28.0,
            ..default()
        },
        TextColor(Color::srgb(0.8, 0.85, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(30.0),
            left: Val::Percent(30.0),
            ..default()
        },
    ));
}

pub fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut next: ResMut<NextState<AppState>>,
    menu: Query<Entity, With<MenuUi>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Enter) {
        for entity in &menu {
            commands.entity(entity).despawn();
        }
        next.set(AppState::Playing);
    }
}
