//! Main menu: title card with a selectable Launch / Settings option. The seed
//! IS the game — show it. Settings opens the S31 settings panel.

use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use crate::settings::{InputAction, Settings};
use crate::states::AppState;
use crate::systems::settings_ui::{open_settings_from_menu, SettingsUiState};
use crate::systems::setup::SYSTEM_SEED;
use crate::systems::ship::SpaceCamera;

/// Which main-menu option is highlighted. Tab / ↓ cycles; Enter activates.
#[derive(Resource, Default, PartialEq, Eq)]
pub enum MenuSelection {
    #[default]
    Launch,
    Settings,
}

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
            clear_color: ClearColorConfig::Custom(Color::srgb(0.0, 0.0, 0.02)),
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
        Text::new(menu_text(&MenuSelection::default())),
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

fn menu_text(sel: &MenuSelection) -> String {
    let launch = if *sel == MenuSelection::Launch {
        "> "
    } else {
        "  "
    };
    let settings = if *sel == MenuSelection::Settings {
        "> "
    } else {
        "  "
    };
    format!(
        "R E A C H L O C K\n\nsystem seed {SYSTEM_SEED:#x}\n\n\
         {launch}Launch\n\
         {settings}Settings\n\n\
         Tab/↓ select · Enter activate"
    )
}

#[allow(clippy::too_many_arguments)]
pub fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut next: ResMut<NextState<AppState>>,
    mut sel: ResMut<MenuSelection>,
    mut ui: ResMut<SettingsUiState>,
    menu: Query<Entity, With<MenuUi>>,
    mut texts: Query<&mut Text, With<MenuUi>>,
    mut commands: Commands,
) {
    // S31: don't drive the menu while the settings panel is open (it owns the
    // keyboard); closing the panel returns focus here.
    if ui.open {
        return;
    }
    let cycle = keys.just_pressed(KeyCode::Tab)
        || keys.just_pressed(KeyCode::ArrowDown)
        || keys.just_pressed(KeyCode::ArrowUp);
    if cycle {
        *sel = match *sel {
            MenuSelection::Launch => MenuSelection::Settings,
            MenuSelection::Settings => MenuSelection::Launch,
        };
        if let Ok(mut t) = texts.single_mut() {
            **t = menu_text(&sel);
        }
    }
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        match *sel {
            MenuSelection::Launch => {
                for entity in &menu {
                    commands.entity(entity).despawn();
                }
                next.set(AppState::InGame);
            }
            MenuSelection::Settings => {
                open_settings_from_menu(ui.as_mut(), &settings);
            }
        }
    }
}
