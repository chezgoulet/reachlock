# S105 — Gamepad Input Routing

**Wave: UX-QoL · Depends on:** S31 (Settings/InputAction), S70 (UI framework, focus ring)

## Outcome

Gamepad input is actually consumed by the game. D-pad and left stick navigate UI focus rings (from S70). A button maps to confirm, B maps to cancel. Flight mode uses left stick for pitch/yaw, right stick for roll, triggers for throttle. The `GamepadActive` resource — currently set but never read — drives UI focus ring visibility (only shown when a gamepad was the last input device used).

## Context

`gamepad.rs` is 40 lines that do exactly one thing: detect a gamepad button press and set `GamepadActive(true)`. Zero systems read this resource. There is no gamepad-to-key-emulation, no UI navigation, no flight control. The game is keyboard-only despite advertising gamepad support in settings (deadzone setting exists, but no consumer).

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/gamepad.rs` | Existing detection — replace with full routing |
| `reachlock-client/src/systems/ship.rs` | Flight `control` system — add gamepad axes |
| `reachlock-client/src/systems/interior.rs` | `walk_avatar` — add gamepad left stick for walk |
| `reachlock-client/src/systems/focus_ring.rs` | Focus ring (S70) — add D-pad navigation |
| `reachlock-client/src/settings.rs` | `ControlSettings::controller_deadzone` — wire consumer |

## Freeze first

### Gamepad button → InputAction mapping

```rust
/// Default mapping: standard Xbox/PlayStation layout.
pub fn default_gamepad_mapping() -> HashMap<GamepadButton, InputAction> {
    use GamepadButton::*;
    HashMap::from([
        (South,       InputAction::Interact),       // A / Cross
        (East,        InputAction::EditorCancel),    // B / Circle = cancel
        (West,        InputAction::OpenMap),         // X / Square
        (North,       InputAction::OpenInventory),   // Y / Triangle
        (DPadUp,      InputAction::UiUp),
        (DPadDown,    InputAction::UiDown),
        (DPadLeft,    InputAction::UiLeft),
        (DPadRight,   InputAction::UiRight),
        (Start,       InputAction::Pause),
        (LeftTrigger, InputAction::CycleTargetReverse),
        (RightTrigger, InputAction::FireWeapons),
        (LeftBumper,  InputAction::Boost),
        (RightBumper, InputAction::LaunchChaff),
    ])
}
```

### Gamepad axis → flight axes

```rust
/// Default axis mapping for flight mode.
/// Left stick: pitch (Y) + yaw (X)
/// Right stick: roll (X) + throttle (Y inverted)
/// Triggers: roll left/right (alternative)
pub struct GamepadFlightAxes {
    pub pitch: f32,    // -1..1, forward stick = positive
    pub yaw: f32,      // -1..1, right stick = positive
    pub roll: f32,     // -1..1
    pub throttle: f32, // -1..1, positive = accelerate
}
```

### Gamepad active tracking

```rust
/// Tracks whether the last input was from a gamepad.
/// Used to show/hide the focus ring.
#[derive(Resource, Default)]
pub struct GamepadActive(pub bool);

/// Updated by any gamepad button/axis press.
/// Cleared by any keyboard/mouse press.
pub fn track_input_source(
    mut gp_active: ResMut<GamepadActive>,
    gamepad_buttons: Res<ButtonInput<GamepadButton>>,
    gamepad_axes: Res<Axis<GamepadAxis>>,
    kb: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    // Gamepad activity check
    for btn in ALL_GAMEPAD_BUTTONS {
        if gamepad_buttons.just_pressed(*btn) { gp_active.0 = true; }
    }
    for axis in [GamepadAxis::LeftStickX, GamepadAxis::LeftStickY, ...] {
        if let Some(v) = gamepad_axes.get(*axis) { if v.abs() > 0.3 { gp_active.0 = true; } }
    }
    // Keyboard/mouse clears gamepad active
    if kb.just_pressed(KeyCode::KeyW) || mouse.just_pressed(MouseButton::Left) || ... {
        gp_active.0 = false;
    }
}
```

## Deliverables

### 1. Replace `gamepad.rs` with full routing module

- [ ] Implement `default_gamepad_mapping()` — button → InputAction
- [ ] Implement `read_flight_axes()` — reads gamepad axes, applies deadzone, returns `GamepadFlightAxes`
- [ ] Implement `track_input_source()` — detects gamepad vs keyboard/mouse
- [ ] Implement `route_gamepad_to_input_action()` — on button press, synthesizes a `KeyCode` press equivalent for systems that read `ButtonInput<KeyCode>`

### 2. Wire deadzone setting

- [ ] `controller_deadzone` (f32, default 0.2) is already in `ControlSettings`
- [ ] In `read_flight_axes()`: filter axis values where `|value| < deadzone` → set to 0.0
- [ ] Add registry entry: `"controls.controller_deadzone" => "gamepad/read_flight_axes"`

### 3. Wire flight controls

- [ ] In `ship::control` (`ship.rs`): add a code path that reads `GamepadFlightAxes` when `GamepadActive.0 == true`
- [ ] Map axes to existing flight inputs:
  - `pitch` → `InputAction::ThrustForward/Backward` equivalent
  - `yaw` → `InputAction::StrafeLeft/Right` equivalent
  - `roll` → `InputAction::RollLeft/Right` equivalent
  - `throttle` → `InputAction::Boost/Brake` equivalent
- [ ] Flight system already reads key inputs — add gamepad axis reads alongside them

### 4. Wire interior walking

- [ ] In `interior::walk_avatar` (`interior.rs`): when `GamepadActive.0 == true`, use left stick for movement
- [ ] Left stick X = strafe, Left stick Y = forward/backward
- [ ] Apply the same speed and body-kind factors as keyboard input

### 5. Wire focus ring navigation

- [ ] In `focus_ring.rs`: add system that reads gamepad D-pad presses
- [ ] D-pad up/down → `advance_focus` equivalent
- [ ] A button (South) → `UiConfirm` → triggers the focused element
- [ ] B button (East) → `UiCancel` → closes panel / goes back

### 6. Show focus ring only when gamepad active

- [ ] `GamepadActive.0` drives focus ring visibility
- [ ] When `GamepadActive.0 == false` (keyboard/mouse input), focus ring is hidden
- [ ] When `GamepadActive.0 == true`, focus ring is visible on the currently-focused element

### 7. Register systems

- [ ] Add `track_input_source` to Update schedule
- [ ] Add `route_gamepad_to_input_action` to Update schedule (runs before input- consuming systems)
- [ ] Flight gamepad reads: integrate into existing `ship::control` system

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Connect gamepad → press a button → GamepadActive sets
# 2. In flight mode: left stick pitches/yaws, right stick rolls, triggers fire
# 3. In interior mode: left stick walks the avatar
# 4. D-pad navigates menu / settings / panels (if focus ring implemented)
# 5. Press keyboard key → focus ring hides, keyboard input resumes

make check
```

## Non-goals

- Rebindable gamepad buttons (default mapping only)
- Gamepad rumble / haptic feedback
- Multiple gamepad support
- Analog trigger precision for partial inputs (digital threshold: >0.5 = on)
- Gamepad-specific settings tab (extends existing Controls tab)
- Switch Pro / Steam Deck button layout detection

## Gotchas

- **Bevy 0.18 `GamepadButton` enum.** Verify all expected variants exist. `LeftTrigger`/`RightTrigger` are buttons in Bevy (digital), not analog axes. Use `GamepadAxis::LeftZ`/`RightZ` for trigger analog values.
- **`gamepad_axes.get(axis)` returns `Option<f32>`.** The axis may not exist if the controller isn't connected. Default to `0.0` on `None`.
- **Deadzone applies to ALL axes.** The `controller_deadzone` setting is a single f32. Apply the same value to all axes — flight sticks need the same threshold as left stick.
- **Gamepad flight feels different from keyboard.** Keyboard is binary (on/off). Gamepad is analog (0..1). The flight system's `ShipCommand` struct uses `Fixed` values — convert the analog f32 to a scaled Fixed value (e.g., `wheel_force * axis_value`).
- **`track_input_source` vs `handle_shortcuts`.** Keyboard-only systems should still work — `GamepadActive` just determines which input source's visual feedback to show (focus ring vs no ring). Don't disable keyboard input when gamepad is active.
