//! Pause (spec §14 deliverable): Esc toggles a `Paused` overlay that stops
//! the sim clock. Implemented by pausing the virtual `Time` — every
//! gameplay system reads `Time::delta()`, so fuel burn, contract timers, and
//! the rapier step all freeze. `Paused` is a `GameMode` variant, but the
//! generic teardown early-outs on it, so the underlying scene is preserved
//! across the round-trip and no scene is rebuilt on resume.
//!
//! S31: while paused, Tab cycles [Resume, Settings] and Enter opens Settings.

use bevy::prelude::*;

use crate::settings::{InputAction, Settings};
use crate::states::GameMode;
use crate::systems::interaction::ActivePanel;
use crate::systems::settings_ui::{open_settings_from_pause, SettingsUiState};

/// Remembers which mode we paused from, so Esc can return to it.
#[derive(Resource, Default, Clone, Debug)]
pub struct PausedFrom(pub GameMode);

/// Marker for the pause-overlay text entity (toggled by `hud::update_hud`).
#[derive(Component, Default)]
pub struct PauseOverlay;

/// Which pause-menu option is highlighted (only meaningful while paused).
#[derive(Resource, Default, PartialEq, Eq)]
pub enum PauseSelection {
    #[default]
    Resume,
    Settings,
}

/// `Esc` is overloaded: if an interaction panel is open it closes that panel
/// (dialogue/market/console); only when nothing is open does it toggle pause.
/// This keeps the one key the rest of the game uses for pause available to
/// close panels too (S07/S08 gotcha).
#[allow(clippy::too_many_arguments)]
pub fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut ui: ResMut<SettingsUiState>,
    mode: Res<State<GameMode>>,
    mut next: ResMut<NextState<GameMode>>,
    mut from: ResMut<PausedFrom>,
    mut sel: ResMut<PauseSelection>,
    mut panel: ResMut<ActivePanel>,
    mut time: ResMut<Time<Virtual>>,
) {
    // While the settings panel is open it owns the keyboard; pause ignores
    // input so the two don't fight. Closing the panel returns here (still
    // paused, because GameMode::Paused is untouched).
    if ui.open {
        return;
    }
    let pause_key = keys.just_pressed(settings.key(InputAction::Pause));

    // Paused: drive the Resume/Settings menu.
    if *mode == GameMode::Paused {
        if keys.just_pressed(KeyCode::Tab) {
            *sel = match *sel {
                PauseSelection::Resume => PauseSelection::Settings,
                PauseSelection::Settings => PauseSelection::Resume,
            };
            return;
        }
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
            match *sel {
                PauseSelection::Resume => resume(&mut next, &mut from, &mut time),
                PauseSelection::Settings => {
                    open_settings_from_pause(ui.as_mut(), &settings);
                }
            }
            return;
        }
        // Esc resumes (default behaviour preserved).
        if pause_key {
            resume(&mut next, &mut from, &mut time);
        }
        return;
    }

    // Not paused: Esc only pauses when no interaction panel is open.
    if !pause_key {
        return;
    }
    if *panel != ActivePanel::None {
        *panel = ActivePanel::None;
        return;
    }
    from.0 = **mode;
    next.set(GameMode::Paused);
    time.pause();
}

fn resume(
    next: &mut ResMut<NextState<GameMode>>,
    from: &mut ResMut<PausedFrom>,
    time: &mut ResMut<Time<Virtual>>,
) {
    next.set(from.0);
    from.0 = GameMode::SpaceFlight;
    time.unpause();
}
