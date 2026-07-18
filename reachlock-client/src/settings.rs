//! Player settings & keybind configuration (spec S31; docs/sprints/S31).
//!
//! A single `Settings` struct is the source of truth for every configurable
//! value — keybinds, audio volume, video mode, gameplay toggles, accessibility
//! flags, network preferences. It is a Bevy `Resource`, loaded from
//! `save/settings.ron` at startup and written back on apply / game exit.
//!
//! `InputAction` is the global registry of every game action. Systems read
//! `settings.key(InputAction::X)` instead of a hardcoded `KeyCode` literal, so
//! no future sprint ever hardcodes another key.
//!
//! **KeyCode serialization.** Bevy's `KeyCode` derives `Reflect` but not
//! standard serde, so the bind map stores [`KeyBind`] — a newtype around
//! `KeyCode` that serializes as a stable string (`"KeyW"`, `"ArrowUp"`,
//! `"MouseLeft"`, `"ShiftLeft"`, …) via a closed string table. Strings are
//! chosen over the numeric discriminant so a reordered/extended `KeyCode` enum
//! never silently mis-deserializes an old settings file.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::input::keyboard::KeyCode;
use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ---------------------------------------------------------------------------
// Colorblind mode (accessibility)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorblindMode {
    #[default]
    None,
    Protanopia,
    Deuteranopia,
    Tritanopia,
}

// ---------------------------------------------------------------------------
// Settings sub-structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    #[serde(default = "default_master")]
    pub master_volume: f32,
    #[serde(default = "default_one")]
    pub music_volume: f32,
    #[serde(default = "default_one")]
    pub sfx_volume: f32,
    #[serde(default = "default_one")]
    pub voice_volume: f32,
    #[serde(default = "default_true")]
    pub mute_when_unfocused: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            master_volume: default_master(),
            music_volume: default_one(),
            sfx_volume: default_one(),
            voice_volume: default_one(),
            mute_when_unfocused: default_true(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSettings {
    #[serde(default)]
    pub fullscreen: bool,
    /// Pixel dimensions. `(0, 0)` means "use the native display resolution".
    #[serde(default)]
    pub resolution: (u32, u32),
    #[serde(default = "default_true")]
    pub vsync: bool,
    #[serde(default = "default_one")]
    pub render_scale: f32,
    #[serde(default = "default_one")]
    pub ui_scale: f32,
    #[serde(default = "default_true")]
    pub show_fps: bool,
}

impl Default for VideoSettings {
    fn default() -> Self {
        VideoSettings {
            fullscreen: false,
            resolution: (0, 0),
            vsync: default_true(),
            render_scale: default_one(),
            ui_scale: default_one(),
            show_fps: default_true(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlSettings {
    #[serde(default)]
    pub keybinds: HashMap<InputAction, KeyBind>,
    #[serde(default = "default_one")]
    pub mouse_sensitivity: f32,
    #[serde(default)]
    pub invert_y: bool,
    #[serde(default = "default_deadzone")]
    pub controller_deadzone: f32,
}

impl Default for ControlSettings {
    fn default() -> Self {
        ControlSettings {
            keybinds: InputAction::default_keybinds(),
            mouse_sensitivity: default_one(),
            invert_y: false,
            controller_deadzone: default_deadzone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameplaySettings {
    #[serde(default = "default_true")]
    pub aim_assist: bool,
    #[serde(default = "default_true")]
    pub auto_dock: bool,
    #[serde(default = "default_true")]
    pub show_tutorial_hints: bool,
    #[serde(default = "default_verbosity")]
    pub combat_log_verbosity: u8,
    #[serde(default = "default_autosave")]
    pub auto_save_interval_secs: u32,
}

impl Default for GameplaySettings {
    fn default() -> Self {
        GameplaySettings {
            aim_assist: default_true(),
            auto_dock: default_true(),
            show_tutorial_hints: default_true(),
            combat_log_verbosity: default_verbosity(),
            auto_save_interval_secs: default_autosave(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilitySettings {
    #[serde(default)]
    pub colorblind_mode: ColorblindMode,
    #[serde(default = "default_one")]
    pub text_scale: f32,
    #[serde(default = "default_true")]
    pub high_contrast_ui: bool,
    #[serde(default = "default_one")]
    pub screen_shake: f32,
    #[serde(default = "default_true")]
    pub subtitles: bool,
    #[serde(default = "default_one")]
    pub subtitle_size: f32,
    #[serde(default)]
    pub hold_for_interact: bool,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        AccessibilitySettings {
            colorblind_mode: ColorblindMode::None,
            text_scale: default_one(),
            high_contrast_ui: default_true(),
            screen_shake: default_one(),
            subtitles: default_true(),
            subtitle_size: default_one(),
            hold_for_interact: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSettings {
    #[serde(default = "default_server")]
    pub server_url: String,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default = "default_true")]
    pub show_latency: bool,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        NetworkSettings {
            server_url: default_server(),
            auto_connect: false,
            show_latency: default_true(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Resource)]
pub struct Settings {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default)]
    pub video: VideoSettings,
    #[serde(default)]
    pub controls: ControlSettings,
    #[serde(default)]
    pub gameplay: GameplaySettings,
    #[serde(default)]
    pub accessibility: AccessibilitySettings,
    #[serde(default)]
    pub network: NetworkSettings,
}

// ---------------------------------------------------------------------------
// Field defaults (used by `#[serde(default = "…")]` so old saves with a
// missing/new field load cleanly instead of erroring).
// ---------------------------------------------------------------------------

fn default_version() -> u32 {
    1
}
fn default_master() -> f32 {
    0.8
}
fn default_one() -> f32 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_deadzone() -> f32 {
    0.2
}
fn default_verbosity() -> u8 {
    2
}
fn default_autosave() -> u32 {
    5
}
fn default_server() -> String {
    "127.0.0.1:40711".to_string()
}

impl Settings {
    /// Sensible defaults. Audio master at 80%, everything else at 1.0, video
    /// windowed at desktop resolution with vsync on, schema version 1.
    pub fn with_defaults() -> Self {
        Settings {
            version: default_version(),
            audio: AudioSettings::default(),
            video: VideoSettings::default(),
            controls: ControlSettings::default(),
            gameplay: GameplaySettings::default(),
            accessibility: AccessibilitySettings::default(),
            network: NetworkSettings::default(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings::with_defaults()
    }
}

/// Lazily-computed default keybind map, built once and reused across all
/// `Settings::key()` lookups that miss the user's bindings.
static DEFAULT_KEYBINDS: OnceLock<HashMap<InputAction, KeyBind>> = OnceLock::new();

impl Settings {
    /// Look up the currently-bound `KeyCode` for an action, falling back to the
    /// registry default if the settings file somehow omitted it (never panics).
    pub fn key(&self, action: InputAction) -> KeyCode {
        self.controls
            .keybinds
            .get(&action)
            .map(|b| b.0)
            .or_else(|| {
                DEFAULT_KEYBINDS
                    .get_or_init(InputAction::default_keybinds)
                    .get(&action)
                    .map(|b| b.0)
            })
            .unwrap_or(KeyCode::KeyF)
    }

    /// Human-readable label for the bound key of an action, for help strings.
    pub fn key_display(&self, action: InputAction) -> String {
        let kc = self.key(action);
        KeyBind::display(kc)
    }

    /// Returns `true` if `key` is currently bound to some *other* action than
    /// `except`. Used by the rebind UI to warn about conflicts.
    pub fn conflict_for(&self, key: KeyCode, except: InputAction) -> Option<InputAction> {
        self.controls
            .keybinds
            .iter()
            .find(|(a, b)| **a != except && b.0 == key)
            .map(|(a, _)| *a)
    }
}

// ---------------------------------------------------------------------------
// InputAction registry — one variant per game action.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputAction {
    // Movement
    ThrustForward,
    ThrustBackward,
    StrafeLeft,
    StrafeRight,
    RollLeft,
    RollRight,
    Boost,
    Brake,

    // Combat
    FireWeapons,
    FireMissile,
    CycleTarget,
    CycleTargetReverse,
    PowerSelectUp,
    PowerSelectDown,
    PowerAdjustLeft,
    PowerAdjustRight,
    LaunchChaff,

    // Landed combat (S20)
    LockOnCycleNext,
    LockOnCyclePrev,
    AttackLight,
    AttackHeavy,
    Dodge,
    Block,

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
    EditorConfirm,
    /// Reserved — exit an editor operation without saving.
    EditorCancel,
    EditorCursorUp,
    EditorCursorDown,
    EditorCursorLeft,
    EditorCursorRight,
    /// Reserved — cycle to the next item in an editor palette.
    EditorCycleNext,
    /// Reserved — cycle to the previous item in an editor palette.
    EditorCyclePrev,
    EditorTabNext,
    EditorRotate,
    EditorDelete,

    // OnBoard consoles
    ConsoleDigit1,
    ConsoleDigit2,
    ConsoleDigit3,
    ConsoleDigit4,

    // Reserved (do not assign defaults that collide; variant exists for S29 /
    // save-management so future sprints don't hardcode literals).
    VoicePushToTalk,
    QuickSave,
    QuickLoad,
}

impl InputAction {
    /// The canonical default keybind map. Every variant must appear exactly
    /// once (see `default_keybinds_unique` test). Reserved actions get a
    /// placeholder default so the map stays total, but they're overridable.
    pub fn default_keybinds() -> HashMap<InputAction, KeyBind> {
        use InputAction::*;
        use KeyCode::*;
        HashMap::from([
            // Movement
            (ThrustForward, KeyBind(KeyW)),
            (ThrustBackward, KeyBind(KeyS)),
            (StrafeLeft, KeyBind(KeyA)),
            (StrafeRight, KeyBind(KeyD)),
            (RollLeft, KeyBind(KeyQ)),
            (RollRight, KeyBind(KeyE)),
            (Boost, KeyBind(ShiftLeft)),
            (Brake, KeyBind(Space)),
            // Combat
            (FireWeapons, KeyBind(KeyF)),
            (FireMissile, KeyBind(KeyG)),
            (CycleTarget, KeyBind(KeyR)),
            (CycleTargetReverse, KeyBind(KeyF)),
            (PowerSelectUp, KeyBind(ArrowUp)),
            (PowerSelectDown, KeyBind(ArrowDown)),
            (PowerAdjustLeft, KeyBind(ArrowLeft)),
            (PowerAdjustRight, KeyBind(ArrowRight)),
            (LaunchChaff, KeyBind(KeyC)),
            // Landed combat (S20). Keys deliberately overlap non-combat
            // actions (J/K/Tab/Space/Q): landed combat and, say, the mission
            // board are never both live, and duplicate keys across actions are
            // allowed (only per-action uniqueness is enforced).
            (LockOnCycleNext, KeyBind(Tab)),
            (LockOnCyclePrev, KeyBind(ShiftLeft)),
            (AttackLight, KeyBind(KeyJ)),
            (AttackHeavy, KeyBind(KeyK)),
            (Dodge, KeyBind(Space)),
            (Block, KeyBind(KeyQ)),
            // Interaction
            (Interact, KeyBind(KeyE)),
            (Self::Pause, KeyBind(Escape)),
            (OpenComms, KeyBind(KeyT)),
            (OpenMap, KeyBind(KeyM)),
            (OpenInventory, KeyBind(KeyI)),
            (OpenCrewRoster, KeyBind(KeyU)),
            (OpenShipLog, KeyBind(KeyL)),
            (OpenMissionBoard, KeyBind(KeyJ)),
            // Editor
            (EditorConfirm, KeyBind(Enter)),
            (EditorCancel, KeyBind(Escape)),
            (EditorCursorUp, KeyBind(ArrowUp)),
            (EditorCursorDown, KeyBind(ArrowDown)),
            (EditorCursorLeft, KeyBind(ArrowLeft)),
            (EditorCursorRight, KeyBind(ArrowRight)),
            (EditorCycleNext, KeyBind(KeyD)),
            (EditorCyclePrev, KeyBind(KeyA)),
            (EditorTabNext, KeyBind(Tab)),
            (EditorRotate, KeyBind(KeyR)),
            (EditorDelete, KeyBind(Backspace)),
            // OnBoard consoles
            (ConsoleDigit1, KeyBind(Digit1)),
            (ConsoleDigit2, KeyBind(Digit2)),
            (ConsoleDigit3, KeyBind(Digit3)),
            (ConsoleDigit4, KeyBind(Digit4)),
            // Reserved
            (VoicePushToTalk, KeyBind(KeyV)),
            (QuickSave, KeyBind(F5)),
            (QuickLoad, KeyBind(F9)),
        ])
    }

    /// All variants, in declaration order — used by the settings UI to render
    /// every rebindable action.
    pub fn all() -> &'static [InputAction] {
        use InputAction::*;
        &[
            ThrustForward,
            ThrustBackward,
            StrafeLeft,
            StrafeRight,
            RollLeft,
            RollRight,
            Boost,
            Brake,
            FireWeapons,
            FireMissile,
            CycleTarget,
            CycleTargetReverse,
            PowerSelectUp,
            PowerSelectDown,
            PowerAdjustLeft,
            PowerAdjustRight,
            LaunchChaff,
            LockOnCycleNext,
            LockOnCyclePrev,
            AttackLight,
            AttackHeavy,
            Dodge,
            Block,
            Interact,
            Pause,
            OpenComms,
            OpenMap,
            OpenInventory,
            OpenCrewRoster,
            OpenShipLog,
            OpenMissionBoard,
            EditorConfirm,
            EditorCancel,
            EditorCursorUp,
            EditorCursorDown,
            EditorCursorLeft,
            EditorCursorRight,
            EditorCycleNext,
            EditorCyclePrev,
            EditorTabNext,
            EditorRotate,
            EditorDelete,
            ConsoleDigit1,
            ConsoleDigit2,
            ConsoleDigit3,
            ConsoleDigit4,
            VoicePushToTalk,
            QuickSave,
            QuickLoad,
        ]
    }

    /// Short label for the settings UI tab/row.
    pub fn label(&self) -> &'static str {
        use InputAction::*;
        match self {
            ThrustForward => "Thrust forward",
            ThrustBackward => "Thrust backward",
            StrafeLeft => "Strafe left",
            StrafeRight => "Strafe right",
            RollLeft => "Roll left",
            RollRight => "Roll right",
            Boost => "Boost",
            Brake => "Brake",
            FireWeapons => "Fire weapons",
            FireMissile => "Fire missile",
            CycleTarget => "Cycle target",
            CycleTargetReverse => "Cycle target (reverse)",
            PowerSelectUp => "Power select up",
            PowerSelectDown => "Power select down",
            PowerAdjustLeft => "Power adjust left",
            PowerAdjustRight => "Power adjust right",
            LaunchChaff => "Launch chaff",
            LockOnCycleNext => "Lock-on next",
            LockOnCyclePrev => "Lock-on previous",
            AttackLight => "Light attack",
            AttackHeavy => "Heavy attack",
            Dodge => "Dodge roll",
            Block => "Block",
            Interact => "Interact",
            Pause => "Pause",
            OpenComms => "Open comms",
            OpenMap => "Open map",
            OpenInventory => "Open inventory",
            OpenCrewRoster => "Open crew roster",
            OpenShipLog => "Open ship log",
            OpenMissionBoard => "Open mission board",
            EditorConfirm => "Editor confirm",
            EditorCancel => "Editor cancel",
            EditorCursorUp => "Editor cursor up",
            EditorCursorDown => "Editor cursor down",
            EditorCursorLeft => "Editor cursor left",
            EditorCursorRight => "Editor cursor right",
            EditorCycleNext => "Editor cycle next",
            EditorCyclePrev => "Editor cycle prev",
            EditorTabNext => "Editor tab next",
            EditorRotate => "Editor rotate",
            EditorDelete => "Editor delete",
            ConsoleDigit1 => "Console 1",
            ConsoleDigit2 => "Console 2",
            ConsoleDigit3 => "Console 3",
            ConsoleDigit4 => "Console 4",
            VoicePushToTalk => "Voice push-to-talk",
            QuickSave => "Quick save",
            QuickLoad => "Quick load",
        }
    }
}

// ---------------------------------------------------------------------------
// KeyBind — a `KeyCode` that serializes as a stable string.
// ---------------------------------------------------------------------------

/// Wrapper so `KeyCode` (which lacks standard serde) round-trips through RON
/// as a human-readable string. Unknown strings deserialize to `KeyF` (a safe,
/// always-present fallback) so a malformed value never corrupts the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBind(pub KeyCode);

impl Serialize for KeyBind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(KeyBind::name(self.0))
    }
}

impl<'de> Deserialize<'de> for KeyBind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Ok(KeyBind(KeyBind::from_name(&raw)))
    }
}

impl Default for KeyBind {
    fn default() -> Self {
        KeyBind(KeyCode::KeyF)
    }
}

impl KeyBind {
    /// Stable string name for a `KeyCode`. Covers every variant the game can
    /// bind (plus the common letters/digits/mouse/modifiers). Unknown variants
    /// fall back to `"KeyF"`.
    pub fn name(kc: KeyCode) -> &'static str {
        use KeyCode::*;
        match kc {
            KeyW => "KeyW",
            KeyA => "KeyA",
            KeyS => "KeyS",
            KeyD => "KeyD",
            KeyQ => "KeyQ",
            KeyE => "KeyE",
            KeyF => "KeyF",
            KeyG => "KeyG",
            KeyR => "KeyR",
            KeyT => "KeyT",
            KeyV => "KeyV",
            KeyX => "KeyX",
            KeyB => "KeyB",
            KeyN => "KeyN",
            KeyM => "KeyM",
            KeyI => "KeyI",
            KeyJ => "KeyJ",
            KeyK => "KeyK",
            KeyL => "KeyL",
            KeyU => "KeyU",
            KeyP => "KeyP",
            KeyC => "KeyC",
            Space => "Space",
            Enter => "Enter",
            Escape => "Escape",
            Backspace => "Backspace",
            Tab => "Tab",
            ShiftLeft => "ShiftLeft",
            ShiftRight => "ShiftRight",
            ControlLeft => "ControlLeft",
            ControlRight => "ControlRight",
            ArrowUp => "ArrowUp",
            ArrowDown => "ArrowDown",
            ArrowLeft => "ArrowLeft",
            ArrowRight => "ArrowRight",
            Digit0 => "Digit0",
            Digit1 => "Digit1",
            Digit2 => "Digit2",
            Digit3 => "Digit3",
            Digit4 => "Digit4",
            Digit5 => "Digit5",
            Digit6 => "Digit6",
            Digit7 => "Digit7",
            Digit8 => "Digit8",
            Digit9 => "Digit9",
            F1 => "F1",
            F2 => "F2",
            F3 => "F3",
            F4 => "F4",
            F5 => "F5",
            F6 => "F6",
            F7 => "F7",
            F8 => "F8",
            F9 => "F9",
            F10 => "F10",
            F11 => "F11",
            F12 => "F12",
            AltLeft => "AltLeft",
            AltRight => "AltRight",
            SuperLeft => "SuperLeft",
            SuperRight => "SuperRight",
            CapsLock => "CapsLock",
            ContextMenu => "ContextMenu",
            Delete => "Delete",
            End => "End",
            Home => "Home",
            Insert => "Insert",
            PageDown => "PageDown",
            PageUp => "PageUp",
            NumLock => "NumLock",
            ScrollLock => "ScrollLock",
            Pause => "Pause",
            PrintScreen => "PrintScreen",
            Fn => "Fn",
            Backquote => "Backquote",
            BracketLeft => "BracketLeft",
            BracketRight => "BracketRight",
            Comma => "Comma",
            Equal => "Equal",
            Minus => "Minus",
            Period => "Period",
            Quote => "Quote",
            Semicolon => "Semicolon",
            Slash => "Slash",
            Numpad0 => "Numpad0",
            Numpad1 => "Numpad1",
            Numpad2 => "Numpad2",
            Numpad3 => "Numpad3",
            Numpad4 => "Numpad4",
            Numpad5 => "Numpad5",
            Numpad6 => "Numpad6",
            Numpad7 => "Numpad7",
            Numpad8 => "Numpad8",
            Numpad9 => "Numpad9",
            NumpadAdd => "NumpadAdd",
            NumpadSubtract => "NumpadSubtract",
            NumpadMultiply => "NumpadMultiply",
            NumpadDivide => "NumpadDivide",
            NumpadDecimal => "NumpadDecimal",
            NumpadEnter => "NumpadEnter",
            NumpadComma => "NumpadComma",
            NumpadEqual => "NumpadEqual",
            kc => {
                warn!("KeyBind::name: unknown KeyCode variant {kc:?}, serializing as KeyF");
                "KeyF"
            }
        }
    }

    /// Reverse of [`name`]. Unknown / unsupported strings map to `KeyF`.
    pub fn from_name(s: &str) -> KeyCode {
        use KeyCode::*;
        match s {
            "KeyW" => KeyW,
            "KeyA" => KeyA,
            "KeyS" => KeyS,
            "KeyD" => KeyD,
            "KeyQ" => KeyQ,
            "KeyE" => KeyE,
            "KeyF" => KeyF,
            "KeyG" => KeyG,
            "KeyR" => KeyR,
            "KeyT" => KeyT,
            "KeyV" => KeyV,
            "KeyX" => KeyX,
            "KeyB" => KeyB,
            "KeyN" => KeyN,
            "KeyM" => KeyM,
            "KeyI" => KeyI,
            "KeyJ" => KeyJ,
            "KeyK" => KeyK,
            "KeyL" => KeyL,
            "KeyU" => KeyU,
            "KeyP" => KeyP,
            "KeyC" => KeyC,
            "Space" => Space,
            "Enter" => Enter,
            "Escape" => Escape,
            "Backspace" => Backspace,
            "Tab" => Tab,
            "ShiftLeft" => ShiftLeft,
            "ShiftRight" => ShiftRight,
            "ControlLeft" => ControlLeft,
            "ControlRight" => ControlRight,
            "ArrowUp" => ArrowUp,
            "ArrowDown" => ArrowDown,
            "ArrowLeft" => ArrowLeft,
            "ArrowRight" => ArrowRight,
            "Digit0" => Digit0,
            "Digit1" => Digit1,
            "Digit2" => Digit2,
            "Digit3" => Digit3,
            "Digit4" => Digit4,
            "Digit5" => Digit5,
            "Digit6" => Digit6,
            "Digit7" => Digit7,
            "Digit8" => Digit8,
            "Digit9" => Digit9,
            "F1" => F1,
            "F2" => F2,
            "F3" => F3,
            "F4" => F4,
            "F5" => F5,
            "F6" => F6,
            "F7" => F7,
            "F8" => F8,
            "F9" => F9,
            "F10" => F10,
            "F11" => F11,
            "F12" => F12,
            "AltLeft" => AltLeft,
            "AltRight" => AltRight,
            "SuperLeft" => SuperLeft,
            "SuperRight" => SuperRight,
            "CapsLock" => CapsLock,
            "ContextMenu" => ContextMenu,
            "Delete" => Delete,
            "End" => End,
            "Home" => Home,
            "Insert" => Insert,
            "PageDown" => PageDown,
            "PageUp" => PageUp,
            "NumLock" => NumLock,
            "ScrollLock" => ScrollLock,
            "Pause" => Pause,
            "PrintScreen" => PrintScreen,
            "Fn" => Fn,
            "Backquote" => Backquote,
            "BracketLeft" => BracketLeft,
            "BracketRight" => BracketRight,
            "Comma" => Comma,
            "Equal" => Equal,
            "Minus" => Minus,
            "Period" => Period,
            "Quote" => Quote,
            "Semicolon" => Semicolon,
            "Slash" => Slash,
            "Numpad0" => Numpad0,
            "Numpad1" => Numpad1,
            "Numpad2" => Numpad2,
            "Numpad3" => Numpad3,
            "Numpad4" => Numpad4,
            "Numpad5" => Numpad5,
            "Numpad6" => Numpad6,
            "Numpad7" => Numpad7,
            "Numpad8" => Numpad8,
            "Numpad9" => Numpad9,
            "NumpadAdd" => NumpadAdd,
            "NumpadSubtract" => NumpadSubtract,
            "NumpadMultiply" => NumpadMultiply,
            "NumpadDivide" => NumpadDivide,
            "NumpadDecimal" => NumpadDecimal,
            "NumpadEnter" => NumpadEnter,
            "NumpadComma" => NumpadComma,
            "NumpadEqual" => NumpadEqual,
            s => {
                warn!("KeyBind::from_name: unknown key string \"{s}\", falling back to KeyF");
                KeyF
            }
        }
    }

    /// Short UI label: `KeyW`→"W", `ArrowUp`→"↑", `ShiftLeft`→"LShift",
    /// `MouseLeft`→"M1", `Space`→"Space", etc.
    pub fn display(kc: KeyCode) -> String {
        use KeyCode::*;
        let s = match kc {
            KeyW => "W",
            KeyA => "A",
            KeyS => "S",
            KeyD => "D",
            KeyQ => "Q",
            KeyE => "E",
            KeyF => "F",
            KeyG => "G",
            KeyR => "R",
            KeyT => "T",
            KeyV => "V",
            KeyX => "X",
            KeyB => "B",
            KeyN => "N",
            KeyM => "M",
            KeyI => "I",
            KeyJ => "J",
            KeyK => "K",
            KeyL => "L",
            KeyU => "U",
            KeyP => "P",
            KeyC => "C",
            Space => "Space",
            Enter => "Enter",
            Escape => "Esc",
            Backspace => "Bksp",
            Tab => "Tab",
            ShiftLeft => "LShift",
            ShiftRight => "RShift",
            ControlLeft => "LCtrl",
            ControlRight => "RCtrl",
            ArrowUp => "↑",
            ArrowDown => "↓",
            ArrowLeft => "←",
            ArrowRight => "→",
            Digit0 => "0",
            Digit1 => "1",
            Digit2 => "2",
            Digit3 => "3",
            Digit4 => "4",
            Digit5 => "5",
            Digit6 => "6",
            Digit7 => "7",
            Digit8 => "8",
            Digit9 => "9",
            F1 => "F1",
            F2 => "F2",
            F3 => "F3",
            F4 => "F4",
            F5 => "F5",
            F6 => "F6",
            F7 => "F7",
            F8 => "F8",
            F9 => "F9",
            F10 => "F10",
            F11 => "F11",
            F12 => "F12",
            AltLeft => "LAlt",
            AltRight => "RAlt",
            SuperLeft => "LSuper",
            SuperRight => "RSuper",
            CapsLock => "Caps",
            ContextMenu => "Menu",
            Delete => "Del",
            End => "End",
            Home => "Home",
            Insert => "Ins",
            PageDown => "PgDn",
            PageUp => "PgUp",
            NumLock => "NumLk",
            ScrollLock => "Scrlk",
            Pause => "Pause",
            PrintScreen => "PrtSc",
            Fn => "Fn",
            Backquote => "`",
            BracketLeft => "[",
            BracketRight => "]",
            Comma => ",",
            Equal => "=",
            Minus => "-",
            Period => ".",
            Quote => "'",
            Semicolon => ";",
            Slash => "/",
            Numpad0 => "Num0",
            Numpad1 => "Num1",
            Numpad2 => "Num2",
            Numpad3 => "Num3",
            Numpad4 => "Num4",
            Numpad5 => "Num5",
            Numpad6 => "Num6",
            Numpad7 => "Num7",
            Numpad8 => "Num8",
            Numpad9 => "Num9",
            NumpadAdd => "Num+",
            NumpadSubtract => "Num−",
            NumpadMultiply => "Num×",
            NumpadDivide => "Num÷",
            NumpadDecimal => "Num.",
            NumpadEnter => "NumEnt",
            NumpadComma => "Num,",
            NumpadEqual => "Num=",
            _ => "?",
        };
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub const SETTINGS_PATH: &str = "save/settings.ron";

/// Load settings from `save/settings.ron`. Missing or corrupt file → defaults
/// (best-effort: offline-first, never fatal).
pub fn load_settings() -> Settings {
    match std::fs::read_to_string(SETTINGS_PATH) {
        Ok(text) => match ron::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                warn!("settings.ron corrupt, using defaults: {e}");
                Settings::default()
            }
        },
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                warn!("settings load failed ({e}); using defaults");
            }
            Settings::default()
        }
    }
}

/// Write settings to `save/settings.ron`. Best-effort: a failed write is
/// logged, never fatal.
pub fn save_settings(settings: &Settings) {
    let text = match ron::ser::to_string_pretty(settings, ron::ser::PrettyConfig::default()) {
        Ok(t) => t,
        Err(e) => {
            error!("settings serialize failed: {e}");
            return;
        }
    };
    if let Some(parent) = std::path::Path::new(SETTINGS_PATH).parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            error!("settings mkdir failed: {e}");
        }
    }
    if let Err(e) = std::fs::write(SETTINGS_PATH, text) {
        error!("settings write failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// Help text cache — HUD help strings rebuilt from settings, invalidated on
// settings change (spec S31 §6).
// ---------------------------------------------------------------------------

/// Cached, settings-derived HUD help strings. Rebuilt whenever keybinds
/// change; the HUD reads from here so it never hardcodes a binding.
#[derive(Resource, Default, Clone, Debug)]
pub struct HelpTextCache {
    pub flight: String,
    pub interior: String,
}

impl HelpTextCache {
    /// Rebuild both help strings from the current settings.
    pub fn rebuild(settings: &Settings) -> Self {
        let d = |a: InputAction| settings.key_display(a);
        // `X` (anomaly) has no input-action registry entry — surface it by the
        // raw key label so the help line still reflects the binding intent.
        let anomaly = KeyBind::display(KeyCode::KeyX);
        HelpTextCache {
            flight: format!(
                "{} pitch · {} yaw · {} roll (double-tap: barrel roll) · {} thrust · \
                 {} boost · brake · {} fire · {} target subsystem · {} chaff · arrows power · \
                 {} mine · {} scan · {} map · {} dock/jump · {} self-jump · \
                 {} stand up · {} anomaly · {} pause",
                d(InputAction::ThrustForward),
                d(InputAction::StrafeLeft),
                d(InputAction::RollLeft),
                d(InputAction::Brake),
                d(InputAction::Boost),
                d(InputAction::FireWeapons),
                d(InputAction::CycleTarget),
                d(InputAction::LaunchChaff),
                d(InputAction::FireMissile),
                d(InputAction::OpenComms),
                d(InputAction::OpenMap),
                d(InputAction::EditorConfirm),
                d(InputAction::OpenMissionBoard),
                d(InputAction::Interact),
                anomaly,
                d(InputAction::Pause),
            ),
            interior: format!(
                "WASD walk · {} interact (board at the ship, disembark at the airlock, \
                 fly from the pilot seat; in flight the gunner/scanner/miner consoles go live) · \
                 {} launch · {} refuel (docked) · {} pause",
                d(InputAction::Interact),
                d(InputAction::OpenShipLog),
                d(InputAction::OpenCrewRoster),
                d(InputAction::Pause),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_ron() {
        let s = Settings::default();
        let text = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: Settings = ron::from_str(&text).unwrap();
        assert_eq!(s.audio.master_volume, back.audio.master_volume);
        assert_eq!(s.controls.keybinds, back.controls.keybinds);
        assert_eq!(s.version, back.version);
    }

    #[test]
    fn modified_round_trip_ron() {
        let mut s = Settings::default();
        s.audio.master_volume = 0.3;
        s.video.fullscreen = true;
        s.controls
            .keybinds
            .insert(InputAction::Interact, KeyBind(KeyCode::KeyF));
        let text = ron::ser::to_string_pretty(&s, ron::ser::PrettyConfig::default()).unwrap();
        let back: Settings = ron::from_str(&text).unwrap();
        assert_eq!(back.audio.master_volume, 0.3);
        assert!(back.video.fullscreen);
        assert_eq!(
            back.controls.keybinds.get(&InputAction::Interact),
            Some(&KeyBind(KeyCode::KeyF))
        );
    }

    #[test]
    fn corrupt_settings_returns_defaults() {
        std::fs::write(SETTINGS_PATH, "this is not ron {").ok();
        let s = load_settings();
        assert_eq!(s.audio.master_volume, default_master());
        let _ = std::fs::remove_file(SETTINGS_PATH);
    }

    #[test]
    fn missing_settings_returns_defaults() {
        let _ = std::fs::remove_file(SETTINGS_PATH);
        let s = load_settings();
        assert_eq!(s.version, 1);
    }

    #[test]
    fn new_field_defaults_from_old_file() {
        // An old file without the `accessibility` block must load with the
        // accessibility defaults (high_contrast_ui = true).
        let old = "(version:1,audio:(master_volume:0.8,music_volume:1, \
            sfx_volume:1,voice_volume:1,mute_when_unfocused:true), \
            video:(fullscreen:false,resolution:(0,0),vsync:true,render_scale:1, \
            ui_scale:1,show_fps:true), \
            controls:(keybinds:{},mouse_sensitivity:1,invert_y:false, \
            controller_deadzone:0.2), \
            gameplay:(aim_assist:true,auto_dock:true,show_tutorial_hints:true, \
            combat_log_verbosity:2,auto_save_interval_secs:5), \
            network:(server_url:\"127.0.0.1:40711\",auto_connect:false, \
            show_latency:true))";
        let s: Settings = ron::from_str(old).unwrap();
        assert!(s.accessibility.high_contrast_ui);
        assert_eq!(s.accessibility.screen_shake, 1.0);
    }

    #[test]
    fn default_keybinds_unique_and_total() {
        let binds = InputAction::default_keybinds();
        assert_eq!(
            binds.len(),
            InputAction::all().len(),
            "every action has a bind"
        );
        let mut seen = std::collections::HashSet::new();
        for a in InputAction::all() {
            assert!(seen.insert(*a), "duplicate entry for {a:?}");
        }
    }

    #[test]
    fn keybind_round_trips_as_string() {
        for kc in [
            KeyCode::KeyW,
            KeyCode::ArrowUp,
            KeyCode::ShiftLeft,
            KeyCode::Space,
            KeyCode::Escape,
        ] {
            let name = KeyBind::name(kc);
            assert_eq!(KeyBind::from_name(name), kc);
        }
    }

    #[test]
    fn key_display_known() {
        assert_eq!(KeyBind::display(KeyCode::KeyW), "W");
        assert_eq!(KeyBind::display(KeyCode::ArrowUp), "↑");
        assert_eq!(KeyBind::display(KeyCode::ShiftLeft), "LShift");
    }
}
