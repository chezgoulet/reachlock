//! Main menu: title card with a selectable New Game / Continue / Settings.
//! The seed IS the game — show it. Settings opens the S31 settings panel.
//!
//! Camera setup is deliberately *not* part of menu spawning. The two are
//! separate systems because the menu is spawned on every entry to
//! `AppState::MainMenu`, while the cameras are spawned once at startup and
//! must never be duplicated.

use bevy::audio::SpatialListener;
use bevy::prelude::*;
use bevy::ui::IsDefaultUiCamera;

use crate::settings::{InputAction, Settings};
use crate::states::AppState;
use crate::systems::settings_ui::{open_settings_from_menu, SettingsUiState};
use crate::systems::setup::SYSTEM_SEED;
use crate::systems::ship::SpaceCamera;
use crate::theme;

/// Which main-menu option is highlighted. ↑/↓/Tab cycles; Enter activates.
/// S78: split into New Game / Continue / Settings.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSelection {
    #[default]
    NewGame,
    Continue,
    Settings,
}

impl MenuSelection {
    const ALL: [MenuSelection; 3] = [
        MenuSelection::NewGame,
        MenuSelection::Continue,
        MenuSelection::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            MenuSelection::NewGame => "New Game",
            MenuSelection::Continue => "Continue",
            MenuSelection::Settings => "Settings",
        }
    }

    fn next(self) -> Self {
        match self {
            MenuSelection::NewGame => MenuSelection::Continue,
            MenuSelection::Continue => MenuSelection::Settings,
            MenuSelection::Settings => MenuSelection::NewGame,
        }
    }

    fn prev(self) -> Self {
        match self {
            MenuSelection::NewGame => MenuSelection::Settings,
            MenuSelection::Continue => MenuSelection::NewGame,
            MenuSelection::Settings => MenuSelection::Continue,
        }
    }
}

/// Whether a save file exists (re-checked on every entry to the menu, so
/// finishing character creation lights up Continue).
#[derive(Resource)]
pub struct SaveExists(pub bool);

#[derive(Component)]
pub struct MenuUi;

/// One selectable menu row, tagged with the option it stands for.
#[derive(Component, Clone, Copy)]
pub struct MenuItem(pub MenuSelection);

/// The two persistent cameras (spec §14): a 3D chase-cam for SpaceFlight and
/// a 2D camera for interiors + all UI. `manage_cameras` (ship.rs) toggles
/// which is active per GameMode. The 2D camera is the default UI target so
/// bevy_ui never has to guess between the two, and renders after the 3D
/// camera (order 1) so the HUD overlays the flight view.
///
/// Spawned once at startup. Menu spawning must not live here: it runs on
/// every entry to the menu, and a second pair of cameras would break UI
/// targeting for the rest of the session.
pub fn spawn_cameras(mut commands: Commands) {
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
}

/// Build the menu. Runs on every entry to `AppState::MainMenu` — including
/// the return trip from character creation, which previously left a blank
/// screen because the menu was only ever spawned once at startup.
pub fn spawn_menu(mut commands: Commands, mut selection: ResMut<MenuSelection>) {
    let has_save = crate::save_backend::read_save().is_some();
    commands.insert_resource(SaveExists(has_save));

    // Coming back to the menu should not leave the cursor on a row that is
    // now disabled.
    if *selection == MenuSelection::Continue && !has_save {
        *selection = MenuSelection::NewGame;
    }

    commands
        .spawn((
            MenuUi,
            theme::node_with(
                "screen",
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0),
                    ..default()
                },
            ),
        ))
        .with_children(|root| {
            root.spawn(theme::text("title", "REACHLOCK"));
            root.spawn(theme::text(
                "subtitle",
                format!("system seed {SYSTEM_SEED:#x}"),
            ));

            root.spawn(theme::node_with(
                "frame",
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    min_width: Val::Px(320.0),
                    margin: UiRect::top(Val::Px(32.0)),
                    padding: UiRect::axes(Val::Px(32.0), Val::Px(20.0)),
                    row_gap: Val::Px(4.0),
                    ..default()
                },
            ))
            .with_children(|panel| {
                for option in MenuSelection::ALL {
                    let enabled = option != MenuSelection::Continue || has_save;
                    panel.spawn((
                        MenuItem(option),
                        theme::text(
                            item_class(option, *selection, enabled),
                            item_label(option, *selection, enabled),
                        ),
                    ));
                }
            });

            root.spawn(theme::node_with(
                "frame.footer",
                Node {
                    margin: UiRect::top(Val::Px(28.0)),
                    padding: UiRect::ZERO,
                    border: UiRect::ZERO,
                    ..default()
                },
            ))
            .with_children(|footer| {
                for (key, desc) in [("↑↓", "select"), ("Enter", "activate"), ("F5", "reload UI")]
                {
                    footer
                        .spawn(theme::node_with(
                            "row",
                            Node {
                                column_gap: Val::Px(6.0),
                                ..default()
                            },
                        ))
                        .with_children(|row| {
                            row.spawn(theme::text("keycap", key));
                            row.spawn(theme::text("keycap.desc", desc));
                        });
                }
            });
        });
}

pub fn despawn_menu(mut commands: Commands, menu: Query<Entity, With<MenuUi>>) {
    for entity in &menu {
        commands.entity(entity).despawn();
    }
}

fn item_class(option: MenuSelection, selected: MenuSelection, enabled: bool) -> &'static str {
    if !enabled {
        "item.disabled"
    } else if option == selected {
        "item.selected"
    } else {
        "item"
    }
}

fn item_label(option: MenuSelection, selected: MenuSelection, enabled: bool) -> String {
    let marker = if option == selected { "▸ " } else { "  " };
    let suffix = if enabled { "" } else { "   (no save)" };
    format!("{marker}{}{suffix}", option.label())
}

/// Repaint the rows when the highlight moves. Restyling is a class swap; the
/// theme decides what "selected" looks like.
pub fn update_menu_selection(
    selection: Res<MenuSelection>,
    save_exists: Res<SaveExists>,
    mut items: Query<(
        &MenuItem,
        &mut theme::Styled,
        &mut Text,
        &mut theme::SourceText,
    )>,
) {
    if !selection.is_changed() && !save_exists.is_changed() {
        return;
    }
    for (item, mut styled, mut text, mut source) in items.iter_mut() {
        let enabled = item.0 != MenuSelection::Continue || save_exists.0;
        let class = item_class(item.0, *selection, enabled);
        if styled.0 != class {
            *styled = theme::Styled::new(class);
        }
        theme::set_text(
            &mut text,
            Some(&mut source),
            item_label(item.0, *selection, enabled),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut next: ResMut<NextState<AppState>>,
    mut sel: ResMut<MenuSelection>,
    mut ui: ResMut<SettingsUiState>,
    mut commands: Commands,
    save_exists: Res<SaveExists>,
) {
    // S31: don't drive the menu while the settings panel is open (it owns the
    // keyboard); closing the panel returns focus here.
    if ui.open {
        return;
    }

    if keys.just_pressed(KeyCode::Tab) || keys.just_pressed(KeyCode::ArrowDown) {
        *sel = sel.next();
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        *sel = sel.prev();
    }

    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        match *sel {
            MenuSelection::NewGame => {
                // The menu is torn down by OnExit(MainMenu), not here — a
                // manual despawn is how the menu used to disappear for good.
                //
                // `insert_resource`, not `init_resource`: init only fills a
                // *missing* resource, so a second New Game would have resumed
                // the abandoned draft from the first.
                commands.insert_resource(
                    crate::systems::character_creation::CharacterCreationState::default(),
                );
                next.set(AppState::CharacterCreation);
            }
            MenuSelection::Continue => {
                if save_exists.0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_cycles_both_ways() {
        assert_eq!(MenuSelection::NewGame.next(), MenuSelection::Continue);
        assert_eq!(MenuSelection::Settings.next(), MenuSelection::NewGame);
        assert_eq!(MenuSelection::NewGame.prev(), MenuSelection::Settings);
        for option in MenuSelection::ALL {
            assert_eq!(option.next().prev(), option, "next/prev must be inverses");
        }
    }

    #[test]
    fn continue_is_disabled_without_a_save() {
        assert_eq!(
            item_class(MenuSelection::Continue, MenuSelection::NewGame, false),
            "item.disabled"
        );
        assert!(
            item_label(MenuSelection::Continue, MenuSelection::NewGame, false)
                .contains("(no save)")
        );
        assert!(
            !item_label(MenuSelection::Continue, MenuSelection::NewGame, true)
                .contains("(no save)")
        );
    }

    #[test]
    /// Disabled beats selected: a highlighted-but-unavailable row must not
    /// look activatable.
    fn disabled_styling_wins_over_selected() {
        assert_eq!(
            item_class(MenuSelection::Continue, MenuSelection::Continue, false),
            "item.disabled"
        );
    }

    /// Drive the real state machine and count the menu entities.
    fn menu_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppState>()
            .init_resource::<MenuSelection>()
            .add_systems(OnEnter(AppState::MainMenu), spawn_menu)
            .add_systems(OnExit(AppState::MainMenu), despawn_menu);
        app
    }

    fn menu_entity_count(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<Entity, With<MenuUi>>()
            .iter(app.world())
            .count()
    }

    fn goto(app: &mut App, state: AppState) {
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(state);
        app.update();
    }

    #[test]
    /// The bug this guards: `spawn_menu` used to run only at `Startup`, and
    /// New Game despawned the menu by hand. Backing out of character creation
    /// therefore returned to `MainMenu` with nothing on screen and no way
    /// forward — a dead grey window that could only be closed.
    fn menu_comes_back_after_leaving_and_returning() {
        let mut app = menu_app();
        app.update();
        assert!(
            menu_entity_count(&mut app) > 0,
            "menu must exist on first entry"
        );

        goto(&mut app, AppState::CharacterCreation);
        assert_eq!(
            menu_entity_count(&mut app),
            0,
            "leaving the menu must tear it down"
        );

        goto(&mut app, AppState::MainMenu);
        assert!(
            menu_entity_count(&mut app) > 0,
            "returning to the menu must rebuild it — a blank screen here is \
             the regression this test exists for"
        );
    }

    #[test]
    /// Re-entering must not stack a second copy of the menu on top of the
    /// first, which is the other way this lifecycle goes wrong.
    fn menu_is_not_duplicated_across_visits() {
        let mut app = menu_app();
        app.update();
        let first = menu_entity_count(&mut app);

        for _ in 0..3 {
            goto(&mut app, AppState::CharacterCreation);
            goto(&mut app, AppState::MainMenu);
        }
        assert_eq!(
            menu_entity_count(&mut app),
            first,
            "each visit must leave exactly one menu behind"
        );
    }

    #[test]
    /// The lifecycle tests above build their own app, so they prove the
    /// systems are correct but not that `main.rs` wires them that way. This
    /// checks the actual registration, because the original bug was purely a
    /// registration mistake: the right system in the wrong schedule.
    fn main_registers_the_menu_on_state_entry_not_startup() {
        let main_rs = reachlock_core::paths::install_root().join("reachlock-client/src/main.rs");
        let source = std::fs::read_to_string(&main_rs).expect("main.rs is readable");

        let sites: Vec<usize> = source
            .match_indices("menu::spawn_menu")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            sites.len(),
            1,
            "menu::spawn_menu must be registered exactly once, found {}",
            sites.len()
        );

        let before = &source[sites[0].saturating_sub(160)..sites[0]];
        assert!(
            before.contains("OnEnter(AppState::MainMenu)"),
            "menu::spawn_menu must be registered on OnEnter(AppState::MainMenu). \
             Registering it at Startup is what left a blank screen when the \
             player returned from character creation.\nContext was:\n{before}"
        );
        assert!(
            source.contains("OnExit(AppState::MainMenu), menu::despawn_menu"),
            "the menu must be torn down on OnExit(AppState::MainMenu)"
        );
    }

    #[test]
    fn only_the_selected_row_is_marked() {
        let label = item_label(MenuSelection::NewGame, MenuSelection::NewGame, true);
        assert!(label.starts_with("▸ "), "selected row carries the marker");
        let other = item_label(MenuSelection::Settings, MenuSelection::NewGame, true);
        assert!(other.starts_with("  "), "unselected rows stay aligned");
        assert_eq!(
            label.trim_start_matches(['▸', ' ']),
            "New Game",
            "the marker must not become part of the label"
        );
    }
}
