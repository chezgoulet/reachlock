//! Pause (spec §14 deliverable): Esc toggles a `Paused` overlay that stops
//! the sim clock. Implemented by pausing the virtual `Time` — every
//! gameplay system reads `Time::delta()`, so fuel burn, contract timers, and
//! the rapier step all freeze. `Paused` is a `GameMode` variant, but the
//! generic teardown early-outs on it, so the underlying scene is preserved
//! across the round-trip and no scene is rebuilt on resume.

use bevy::prelude::*;

use crate::states::GameMode;
use crate::systems::interaction::ActivePanel;

/// Remembers which mode we paused from, so Esc can return to it.
#[derive(Resource, Default, Clone, Debug)]
pub struct PausedFrom(pub GameMode);

/// Marker for the pause-overlay text entity (toggled by `hud::update_hud`).
#[derive(Component, Default)]
pub struct PauseOverlay;

/// `Esc` is overloaded: if an interaction panel is open it closes that panel
/// (dialogue/market/console); only when nothing is open does it toggle pause.
/// This keeps the one key the rest of the game uses for pause available to
/// close panels too (S07/S08 gotcha).
pub fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<State<GameMode>>,
    mut next: ResMut<NextState<GameMode>>,
    mut from: ResMut<PausedFrom>,
    mut panel: ResMut<ActivePanel>,
    mut time: ResMut<Time<Virtual>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if *panel != ActivePanel::None {
        *panel = ActivePanel::None;
        return;
    }
    if *mode == GameMode::Paused {
        next.set(from.0);
        from.0 = GameMode::SpaceFlight;
        time.unpause();
    } else {
        from.0 = **mode;
        next.set(GameMode::Paused);
        time.pause();
    }
}
