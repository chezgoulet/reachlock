use bevy::prelude::*;

/// Tracks whether a gamepad is connected and active.
/// Set by `track_input_source` system. Used by ship/intern control,
/// focus ring visibility, and gamepad routing (S105).
#[derive(Resource, Default)]
pub struct GamepadActive(pub bool);

/// Buttons that count as "the player picked up a controller".
const WAKE_BUTTONS: [GamepadButton; 8] = [
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::West,
    GamepadButton::North,
];

/// Stick deflection past which we treat the pad as in use, rather than a
/// resting stick drifting off centre.
const STICK_DEADZONE: f32 = 0.3;

pub fn track_input_source(gamepads: Query<&Gamepad>, active: ResMut<GamepadActive>) {
    detect_gamepad(gamepads, active);
}

/// Notice the first sign of controller input.
///
/// Reads state off the [`Gamepad`] component. Bevy moved gamepads from global
/// `Res<ButtonInput<GamepadButton>>` / `Res<Axis<GamepadAxis>>` resources to
/// per-entity components in 0.15, and those resources no longer exist — asking
/// for them compiled fine and then failed parameter validation at runtime, so
/// this system panicked the moment it first ran.
pub fn detect_gamepad(gamepads: Query<&Gamepad>, mut active: ResMut<GamepadActive>) {
    for gamepad in gamepads.iter() {
        if WAKE_BUTTONS.iter().any(|b| gamepad.just_pressed(*b)) {
            active.0 = true;
            return;
        }
        for axis in [GamepadAxis::LeftStickX, GamepadAxis::LeftStickY] {
            if gamepad.get(axis).is_some_and(|v| v.abs() > STICK_DEADZONE) {
                active.0 = true;
                return;
            }
        }
    }
}
