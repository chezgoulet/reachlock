//! Main menu: title card, Enter to launch. The seed IS the game — show it.

use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use crate::states::AppState;
use crate::systems::setup::SYSTEM_SEED;
use crate::systems::ship::SpaceCamera;

#[derive(Component)]
pub struct MenuUi;

pub fn spawn_menu(mut commands: Commands) {
    // Two persistent cameras (spec §14): a 3D chase-cam for SpaceFlight and a
    // 2D camera for interiors + all UI. `manage_cameras` (ship.rs) toggles
    // which is active per GameMode. The 2D camera is the default UI target so
    // bevy_ui never has to guess between the two, and renders after the 3D
    // camera (order 1) so the HUD overlays the flight view.
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.0, 0.5, 0.8)),
            ..default()
        },
        SpaceCamera,
        Transform::from_xyz(0.0, 60.0, 160.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            ..default()
        },
        IsDefaultUiCamera,
    ));
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
        next.set(AppState::InGame);
    }
}
