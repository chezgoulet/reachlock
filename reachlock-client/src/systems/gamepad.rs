use bevy::prelude::*;

#[derive(Resource, Default)]
pub struct GamepadActive(pub bool);

#[derive(Component)]
pub struct FocusRing;

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
