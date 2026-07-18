# S31 — Game Settings & Preferences

**Spec:** New (player settings, accessibility, keybind configuration) ·
**Wave 7 (tooling) · Depends on:** — (standalone infrastructure, blocks nothing)

## Outcome

Every configurable value in the game — keybindings, audio volume, video mode,
gameplay toggles, accessibility flags, network preferences — lives in a single
`Settings` struct persisted to disk. A tabbed settings UI accessible from the
main menu and pause menu lets players remap keys, adjust audio, and configure
the game to their hardware and preferences. Every hardcoded key literal in the
codebase is replaced with a `Settings.key()` lookup. Future sprints add new
keys via an `InputAction` enum variant — they never hardcode another literal.
This is infrastructure. It must exist before any system that needs config.

## Context

- **No settings system exists.** The main menu is a text-only "Press Enter to
  launch" screen. All inputs are hardcoded `KeyCode` literals scattered across
  10+ system files. Audio has no volume controls. Video uses Bevy defaults.
- S19's targeting key (`R`) collided with S09d's scanner key (`T` was taken
  by the scanner pulse) because there was no registry to check. S29 will add
  voice push-to-talk. Every future sprint adds more keys. The collision
  problem compounds.
- This sprint serves DOUBLE DUTY: it creates the settings framework AND
  retrofits all existing systems (S01-S19) to use it. The retrofitting is
  the bulk of the work — ~30-40 keybinding sites across 10+ files.
- The settings system uses the same serialization/persistence pattern as the
  save system (`save/player.ron` → `save/settings.ron`). The pattern is
  proven; this just parallel-tracks it.
- Settings are a `Resource` in Bevy, accessed via `Res<Settings>` in any
  system. The resource is initialized at startup from disk (or defaults) and
  written back to disk on any change.
- Offline-first: settings work identically with no server. Online servers
  never read or write client settings. Settings are local.

## Freeze first

### Settings struct (`client/src/settings.rs` or `core/src/settings.rs`)

Settings live in the client crate — they're render-layer configuration, not
gameplay state. They contain no fixed-point values (iron rule #2 — floats for
render/audio only).

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
pub struct Settings {
    pub version: u32,                    // schema version for migration
    pub audio: AudioSettings,
    pub video: VideoSettings,
    pub controls: ControlSettings,
    pub gameplay: GameplaySettings,
    pub accessibility: AccessibilitySettings,
    pub network: NetworkSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub master_volume: f32,              // 0.0 - 1.0
    pub music_volume: f32,
    pub sfx_volume: f32,
    pub voice_volume: f32,               // for S29 voice chat
    pub mute_when_unfocused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    pub fullscreen: bool,
    pub resolution: (u32, u32),          // 0,0 = use desktop native
    pub vsync: bool,
    pub render_scale: f32,               // 0.5 - 2.0
    pub ui_scale: f32,                   // 0.5 - 2.0
    pub show_fps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlSettings {
    pub keybinds: HashMap<InputAction, KeyCode>,
    pub mouse_sensitivity: f32,          // 0.1 - 5.0
    pub invert_y: bool,
    pub controller_deadzone: f32,        // 0.0 - 0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameplaySettings {
    pub aim_assist: bool,
    pub auto_dock: bool,
    pub show_tutorial_hints: bool,
    pub combat_log_verbosity: u8,        // 0 = minimal, 3 = everything
    pub auto_save_interval_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    pub colorblind_mode: ColorblindMode,
    pub text_scale: f32,                 // 0.5 - 3.0
    pub high_contrast_ui: bool,
    pub screen_shake: f32,               // 0.0 - 1.0 (0 = disabled)
    pub subtitles: bool,
    pub subtitle_size: f32,              // 0.5 - 2.0
    pub hold_for_interact: bool,         // false = tap, true = hold
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ColorblindMode {
    None,
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    pub server_url: String,              // default: "127.0.0.1:40711"
    pub auto_connect: bool,
    pub show_latency: bool,
}
```

### Input action registry (`InputAction` enum)

Every game action gets exactly one entry. Default keybindings are defined in
a single function. Systems read from settings, not hardcoded keys.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputAction {
    // Movement
    ThrustForward, ThrustBackward,
    StrafeLeft, StrafeRight,
    RollLeft, RollRight,
    Boost, Brake,

    // Combat
    FireWeapons, FireMissile,
    CycleTarget, CycleTargetReverse,
    PowerSelectUp, PowerSelectDown,
    PowerAdjustLeft, PowerAdjustRight,
    LaunchChaff,

    // Interaction
    Interact,
    Pause,
    OpenComms,
    OpenMap,
    OpenInventory,
    OpenCrewRoster,
    OpenShipLog,
    OpenMissionBoard,

    // Editor (S17/S18)
    EditorConfirm, EditorCancel,
    EditorCursorUp, EditorCursorDown,
    EditorCursorLeft, EditorCursorRight,
    EditorCycleNext, EditorCyclePrev,
    EditorTabNext, EditorRotate,
    EditorDelete,

    // OnBoard consoles
    ConsoleDigit1, ConsoleDigit2, ConsoleDigit3, ConsoleDigit4,

    // Future (reserved — don't assign defaults yet, but the variant exists)
    VoicePushToTalk,     // S29
    QuickSave, QuickLoad, // save management
}

impl InputAction {
    pub fn default_keybinds() -> HashMap<InputAction, KeyCode> {
        use InputAction::*;
        use KeyCode::*;
        HashMap::from([
            // Movement
            (ThrustForward, KeyW), (ThrustBackward, KeyS),
            (StrafeLeft, KeyA), (StrafeRight, KeyD),
            (RollLeft, KeyQ), (RollRight, KeyE),
            (Boost, ShiftLeft), (Brake, Space),

            // Combat
            (FireWeapons, MouseLeft), (FireMissile, MouseRight),
            (CycleTarget, KeyR), (CycleTargetReverse, KeyF),
            (PowerSelectUp, ArrowUp), (PowerSelectDown, ArrowDown),
            (PowerAdjustLeft, ArrowLeft), (PowerAdjustRight, ArrowRight),
            (LaunchChaff, KeyC),

            // Interaction
            (Interact, KeyE),
            (Pause, Escape),
            (OpenComms, KeyT), (OpenMap, KeyM),
            (OpenInventory, KeyI), (OpenCrewRoster, KeyU),
            (OpenShipLog, KeyL), (OpenMissionBoard, KeyJ),

            // Editor
            (EditorConfirm, Enter),
            (EditorCancel, Escape),
            (EditorCursorUp, ArrowUp), (EditorCursorDown, ArrowDown),
            (EditorCursorLeft, ArrowLeft), (EditorCursorRight, ArrowRight),
            (EditorCycleNext, KeyD), (EditorCyclePrev, KeyA),
            (EditorTabNext, Tab), (EditorRotate, KeyR),
            (EditorDelete, Backspace),

            // OnBoard consoles
            (ConsoleDigit1, Digit1), (ConsoleDigit2, Digit2),
            (ConsoleDigit3, Digit3), (ConsoleDigit4, Digit4),

            // Reserved
            (VoicePushToTalk, KeyV),
            (QuickSave, F5), (QuickLoad, F9),
        ])
    }
}
```

### Settings store trait

```rust
const SETTINGS_PATH: &str = "save/settings.ron";

pub fn load_settings() -> Settings {
    match std::fs::read_to_string(SETTINGS_PATH) {
        Ok(text) => ron::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!("settings corrupt, using defaults: {e}");
            Settings::default()
        }),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) {
    let text = match ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::default()) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("settings serialize failed: {e}");
            return;
        }
    };
    if let Err(e) = std::fs::write(SETTINGS_PATH, &text) {
        tracing::error!("settings write failed: {e}");
    }
}
```

Wire tests: `Settings` serializes round-trip through RON. Missing field on
deserialization = `serde(default)` → field default. Unknown field = ignored.
`default_keybinds()` has no duplicate action entries. Every `InputAction`
variant has exactly one entry in `default_keybinds()`.

## Deliverables

### 1. Settings types + persistence (`client/src/settings.rs`)

- [ ] `Settings` struct and all sub-structs as defined above. Derive `Resource`,
      `Serialize`, `Deserialize`, `Default`, `Clone`. Every field has
      `#[serde(default)]` so old saves with missing fields load cleanly.
- [ ] `Settings::default()` — sensible defaults for every field. Audio at
      80% master. Video at desktop resolution, windowed, vsync on. Settings
      version field = 1.
- [ ] `load_settings()` / `save_settings()` — read/write `save/settings.ron`.
      Corrupt file → defaults. Missing file → defaults. Write failure →
      logged, not fatal.
- [ ] Integration with `main.rs`: load settings at startup, insert as a
      resource BEFORE any system that reads them. Save settings to disk on
      any change (in the settings UI) and on game exit.
- [ ] Test: RON round-trip with all default values; round-trip with modified
      values; corrupt file → defaults; missing file → defaults; new field
      in struct (add test-only field) → old file loads with default for
      new field.

### 2. Settings UI (`client/src/systems/settings_ui.rs`)

- [ ] Accessible from main menu: when `GameMode` is `MainMenu`, show a
      "Settings" option alongside the existing Enter-to-launch prompt.
      Tab to select; Enter to open.
- [ ] Accessible from pause menu: when `GameMode::Paused`, show a "Settings"
      option alongside the existing "Resume" prompt. Same Tab/Enter.
- [ ] Tabbed panel: Audio | Video | Controls | Gameplay | Accessibility |
      Network. Tabs cycled with `Tab` / `Shift+Tab`. Each tab is a vertical
      list of settings with their current values.
- [ ] Widget pattern (keyboard-driven, same ActivePanel pattern as market):
      - Toggle (bool): Enter to flip
      - Slider (f32): A/D to adjust by 0.1 or 1 step
      - Dropdown (enum): A/D to cycle options
      - Key rebind: Enter → "Press new key..." capture mode → press any key
        → validate (no conflict with existing binds for the same action) →
        accept or warn. Esc to cancel capture.
      - Text field (server URL): type with keyboard (basic text input)
- [ ] Volume preview: when adjusting audio sliders, play a test tone at the
      new volume level so the player can hear the change immediately.
- [ ] Apply/Cancel: Enter to apply (writes to disk, immediately active).
      Esc to cancel (reverts to last-saved settings). Changes are live-
      previewed during editing (volume, video mode, etc.) but only persisted
      on apply.
- [ ] Reset to defaults: a "Reset to Defaults" button in each tab + a
      "Reset All" at the bottom. Requires confirmation (Enter again).

### 3. Retrofit all existing hardcoded keys

Every `KeyCode` literal in `reachlock-client/src/systems/` is replaced with
`settings.controls.keybinds.get(&InputAction::*).copied().unwrap_or(default)`.
Files to touch:

| File | Keys to replace |
|---|---|
| `ship.rs` | W/A/S/D/Q/E, Shift/Space (flight), MouseLeft/MouseRight (weapons), R (cycle target), Arrow keys (power), C (chaff) |
| `interaction.rs` | E (interact) |
| `pause.rs` | Escape (pause) |
| `menu.rs` | Enter (launch) |
| `combat.rs` | R (target), Arrow keys (power), C (chaff) |
| `hud.rs` | All help string references — rebuild strings from settings |
| `shipeditor.rs` | Enter/Esc/Tab/WASD/Arrow keys/Backspace (editor controls) |
| `onboard.rs` | Digit1-Digit4 (console keys) |
| `jump.rs` | Any jump/refuel keys |
| `comms.rs` | Any comm panel keys |
| `cryojump.rs` | Any cryo jump keys |
| `market.rs` | Any market UI keys |
| `inventory.rs` | Any inventory UI keys |

Each file gets `settings: Res<Settings>` added to its system params and
`let k = settings.key(InputAction::Whatever);` replaces the key literal.

### 4. Settings consumers

- [ ] Audio: the audio playback system reads `settings.audio.master_volume`
      and `music_volume`/`sfx_volume`. Multiply gain on every `AudioSource`
      play call. Wire `mute_when_unfocused` via Bevy's window focus event.
- [ ] Video: on startup, apply `settings.video` to Bevy's `Window` resource.
      `fullscreen` → `WindowMode::BorderlessFullscreen`. `resolution` →
      set window size. `vsync` → `PresentMode::AutoVsync`. `render_scale` →
      applied to the camera or render target.
- [ ] Controls: `settings.controls.mouse_sensitivity` feeds into the camera
      and ship control systems. `invert_y` inverts mouse Y input.
- [ ] Accessibility: `colorblind_mode` → palette remap for critical UI
      elements (health bars, faction colors, target markers). `text_scale` →
      multiplies all UI text font sizes. `screen_shake` → multiplier on
      camera shake effects (0 = disabled). `subtitles` → show crew comm
      lines as text in combat (S29 voice).
- [ ] Network: `settings.network.server_url` → the WebSocket connect target.
      `auto_connect` → connect on game start. `show_latency` → HUD latency
      indicator.

### 5. Key conflict detection

- [ ] During key rebind, check the new key against all OTHER `InputAction`
      entries. If the new key is already bound to a different action, show a
      warning: "This key is already bound to '{action}'. Rebind anyway?"
      The player can proceed (steal the key — the old action becomes unbound)
      or cancel.
- [ ] Unbound actions show a warning in the settings UI: a yellow highlight
      with "No key bound." The game still functions — the action just can't
      be triggered until rebound.
- [ ] Default keybinds are stored separately from current keybinds. "Reset to
      Defaults" restores from the default map without affecting other settings.

### 6. Help text generation

- [ ] The HUD help strings (`HELP_FLIGHT`, `HELP_INTERIOR`, etc.) are rebuilt
      from the settings at startup and whenever keybinds change. Instead of
      hardcoded strings like `"R cycle target · C chaff"`, they become
      format strings: `"{cycle_target} cycle target · {launch_chaff} chaff"`
      with `{cycle_target}` replaced by `settings.key_display(InputAction::CycleTarget)`.
- [ ] `KeyCode` → display string: `KeyW` → "W", `ArrowUp` → "↑", `ShiftLeft` →
      "LShift", `MouseLeft` → "M1". A lookup table in settings.

## Acceptance gates

```
cargo test -p reachlock-client settings::   # round-trip, defaults, migration
cargo run -p reachlock-client               # menu shows "Settings" option
# Key rebind works: rebind Interact to F → press F to interact in-game
# Settings persist: change audio → quit → relaunch → audio still changed
# Corrupt settings: trash settings.ron → game starts with defaults
# All existing keybinds still work at their defaults
make check
```

Manual: open settings from main menu → change master volume → hear test tone
→ rebind FireWeapons to Space → enter game → Space fires weapons → pause →
open settings → reset Controls to defaults → Escape → Enter fires weapons
(mouse) and Space is brake (default restored) → quit → relaunch → settings
still show defaults.

## Non-goals

- Controller/gamepad support (the `deadzone` field is a placeholder; full
  controller input mapping is a separate sprint)
- Cloud-synced settings (local only — Phase 4)
- Per-save settings (settings are global, not per save file)
- Theme/skin support (colorblind mode remaps UI colors but doesn't support
  custom themes)
- Language/locale (the enum field exists as a comment placeholder; full
  localization is a separate infrastructure sprint)
- Save slot management UI (the save system exists but save slots are a
  separate sprint)

## Gotchas

- When retrofitting keys, some systems use the same `KeyCode` for different
  actions depending on context (e.g., `E` = interact in interior, `E` = roll
  in space flight). These are DIFFERENT `InputAction` variants
  (`InputAction::Interact` vs `InputAction::RollRight`) — the context
  determines which action is checked. The system checks the action, not
  the key. Two actions CAN be bound to the same key if they're contextually
  separated, but the binding UI warns nonetheless.
- The HUD help strings must be rebuilt every time keybinds change. This means
  the help text generation is a function of `&Settings`, not a static string.
  Cache the generated strings in a `HelpTextCache` resource, invalidated when
  settings change.
- Bevy's `KeyCode` enum serialization: `KeyCode` derives `Reflect` but not
  standard `Serialize`/`Deserialize`. The keybind map stores keycodes as
  strings (Bevy's `KeyCode` → `&str` via the `bevy::input::keyboard::Key`
  conversion) and deserializes via a lookup. Or use a wrapper type that
  implements serde via the `KeyCode` discriminant. Pick the simplest path.
- Settings writes happen on apply (in the settings UI) and on game exit.
  There's no autosave for settings — the player explicitly applies changes.
  This prevents partial settings corruption from a crash mid-edit.
- The `InputAction` enum is a protocol — every variant is a contract. Removing
  a variant would break existing settings files (the enum value in the RON
  file wouldn't deserialize). Never remove variants — deprecate them by
  leaving the variant but removing it from `default_keybinds()`. Renaming
  is safe (serde deserializes by name).
