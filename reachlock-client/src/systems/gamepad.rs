use bevy::prelude::*;

/// Tracks whether a gamepad is connected and active.
/// Set by `track_input_source` system. Used by ship/intern control,
/// focus ring visibility, and gamepad routing (S105).
/// Was dead code before S105 — now detection, routing, and consumption
/// are all wired.
#[derive(Resource, Default)]
pub struct GamepadActive(pub bool);

pub fn track_input_source(
    gamepads: Query<&Gamepad>,
    buttons: Res<ButtonInput<GamepadButton>>,
    axes: Res<Axis<GamepadAxis>>,
    active: ResMut<GamepadActive>,
) {
    detect_gamepad(gamepads, buttons, axes, active);
}

pub fn detect_gamepad(
    gamepads: Query<&Gamepad>,
    buttons: Res<ButtonInput<GamepadButton>>,
    axes: Res<Axis<GamepadAxis>>,
    mut active: ResMut<GamepadActive>,
) {
    for _gp in gamepads.iter() {
        for btn in [
            GamepadButton::DPadUp,
            GamepadButton::DPadDown,
            GamepadButton::DPadLeft,
            GamepadButton::DPadRight,
            GamepadButton::South,
            GamepadButton::East,
            GamepadButton::West,
            GamepadButton::North,
        ] {
            if buttons.just_pressed(btn) {
                active.0 = true;
                return;
            }
        }
        for axis in [GamepadAxis::LeftStickX, GamepadAxis::LeftStickY] {
            if let Some(val) = axes.get(axis) {
                if val.abs() > 0.3 {
                    active.0 = true;
                    return;
                }
            }
        }
    }
}
