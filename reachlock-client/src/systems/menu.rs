//! Main menu: title card with a selectable Launch / Settings option. The seed
//! IS the game — show it. Settings opens the S31 settings panel.

use bevy::audio::SpatialListener;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use crate::settings::{InputAction, Settings};
use crate::states::AppState;
use crate::systems::settings_ui::{open_settings_from_menu, SettingsUiState};
use crate::systems::setup::SYSTEM_SEED;
use crate::systems::ship::SpaceCamera;

/// Which main-menu option is highlighted. Tab / ↓ cycles; Enter activates.
/// S78: split into New Game / Continue / Settings.
#[derive(Resource, Default, PartialEq, Eq)]
pub enum MenuSelection {
    #[default]
    NewGame,
    Continue,
    Settings,
}

/// Whether a save file exists (checked at menu open).
#[derive(Resource)]
pub struct SaveExists(pub bool);

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
        SpatialListener::default(),
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

    // Check if a save file exists for the Continue button.
    let has_save = crate::save_backend::read_save().is_some();
    commands.insert_resource(SaveExists(has_save));

    commands.spawn((
        MenuUi,
        Text::new(menu_text(&MenuSelection::default(), has_save)),
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

fn menu_text(sel: &MenuSelection, has_save: bool) -> String {
    let new_game = if *sel == MenuSelection::NewGame {
        "> "
    } else {
        "  "
    };
    let continue_str = if *sel == MenuSelection::Continue {
        "> "
    } else {
        "  "
    };
    let settings = if *sel == MenuSelection::Settings {
        "> "
    } else {
        "  "
    };
    let continue_disabled = if has_save { "" } else { " (no save)" };
    format!(
        "R E A C H L O C K\n\nsystem seed {SYSTEM_SEED:#x}\n\n\
         {new_game}New Game\n\
         {continue_str}Continue{continue_disabled}\n\
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
    save_exists: Res<SaveExists>,
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
            MenuSelection::NewGame => MenuSelection::Continue,
            MenuSelection::Continue => MenuSelection::Settings,
            MenuSelection::Settings => MenuSelection::NewGame,
        };
        if let Ok(mut t) = texts.single_mut() {
            **t = menu_text(&sel, save_exists.0);
        }
    }
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        match *sel {
            MenuSelection::NewGame => {
                for entity in &menu {
                    commands.entity(entity).despawn();
                }
                // Insert fresh CharacterCreationState and transition.
                commands
                    .init_resource::<crate::systems::character_creation::CharacterCreationState>();
                next.set(AppState::CharacterCreation);
            }
            MenuSelection::Continue => {
                if save_exists.0 {
                    for entity in &menu {
                        commands.entity(entity).despawn();
                    }
                    // load_save runs on OnEnter(InGame) — finds existing save.
                    next.set(AppState::InGame);
                }
            }
            MenuSelection::Settings => {
                open_settings_from_menu(ui.as_mut(), &settings);
            }
        }
    }
}
