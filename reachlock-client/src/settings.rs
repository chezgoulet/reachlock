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
// AnimationSpeedMultiplier — scales timed animations when reduce_motion is on
// ---------------------------------------------------------------------------

/// Resource that all animation systems read. Derived from
/// `accessibility.reduce_motion` on settings change.
#[derive(Resource, Clone, Copy, Debug)]
pub struct AnimationSpeedMultiplier(pub f32);

impl Default for AnimationSpeedMultiplier {
    fn default() -> Self {
        AnimationSpeedMultiplier(1.0)
    }
}

impl AnimationSpeedMultiplier {
    pub fn from_settings(settings: &Settings) -> Self {
        if settings.accessibility.reduce_motion {
            AnimationSpeedMultiplier(0.25)
        } else {
            AnimationSpeedMultiplier(1.0)
        }
    }
}

// ---------------------------------------------------------------------------
// SemanticGlyph — glyph/icon companion for colour-coded game states
// ---------------------------------------------------------------------------

/// A glyph that accompanies every colour-coded game state indicator so
/// meaning is never conveyed by hue alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticGlyph {
    // Faction standing
    Ally,
    Neutral,
    Hostile,
    // Health / hull
    HullFull,
    HullDamaged,
    HullCritical,
    HullDestroyed,
    // Threat level
    Passive,
    Alert,
    Engaged,
    CriticalThreat,
    // Reputation tier
    Tier1,
    Tier2,
    Tier3,
    Tier4,
    // Online/offline
    Online,
    Offline,
    // Fuel
    FuelFull,
    FuelMid,
    FuelLow,
    FuelEmpty,
    // Target lock
    TargetLocked,
    // Cargo
    CargoEmpty,
    CargoPartial,
    CargoFull,
    // Scanner contact
    ContactFriendly,
    ContactHostile,
    // Mission difficulty
    DifficultyEasy,
    DifficultyMedium,
    DifficultyHard,
    DifficultyExtreme,
}

impl SemanticGlyph {
    /// The Unicode glyph character.
    pub fn glyph(&self) -> &'static str {
        use SemanticGlyph::*;
        match self {
            Ally => "\u{2605}",                           // ★
            Neutral => "\u{25C6}",                        // ◆
            Hostile => "\u{26A0}",                        // ⚠
            HullFull => "\u{2588}",                       // █
            HullDamaged => "\u{2586}",                    // ▆
            HullCritical => "\u{2584}",                   // ▄
            HullDestroyed => "\u{2205}",                  // ∅
            Passive => "\u{25C7}",                        // ◇
            Alert => "\u{25C8}",                          // ◈
            Engaged => "\u{25C6}",                        // ◆
            CriticalThreat => "\u{25C6}\u{25C6}",         // ◆◆
            Tier1 => "\u{2160}",                          // Ⅰ
            Tier2 => "\u{2161}",                          // Ⅱ
            Tier3 => "\u{2162}",                          // Ⅲ
            Tier4 => "\u{2163}",                          // Ⅳ
            Online => "\u{2713}",                         // ✓
            Offline => "\u{26D4}",                        // ⛔
            FuelFull => "\u{26FD}\u{2588}",               // ⛽█
            FuelMid => "\u{26FD}\u{2586}",                // ⛽▆
            FuelLow => "\u{26FD}\u{2584}",                // ⛽▄
            FuelEmpty => "\u{26FD}\u{2582}",              // ⛽▂
            TargetLocked => "\u{25C6}",                   // ◆
            CargoEmpty => "\u{25A1}",                     // □
            CargoPartial => "\u{2588}",                   // ■█
            CargoFull => "\u{2588}\u{2588}\u{2588}",      // ■■■
            ContactFriendly => "\u{25C6}",                // ◆
            ContactHostile => "\u{25C6}",                 // ◆
            DifficultyEasy => "\u{2605}",                 // ★
            DifficultyMedium => "\u{2605}\u{2605}",       // ★★
            DifficultyHard => "\u{2605}\u{2605}\u{2605}", // ★★★
            DifficultyExtreme => "\u{2620}",              // ☠
        }
    }
}

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
    /// Name of the preferred voice input device. `None` = OS default.
    #[serde(default)]
    pub voice_input_device: Option<String>,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings {
            master_volume: default_master(),
            music_volume: default_one(),
            sfx_volume: default_one(),
            voice_volume: default_one(),
            mute_when_unfocused: default_true(),
            voice_input_device: None,
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
    /// When true: screen_shake forced to 0, animations capped at 0.25×,
    /// parallax halved, particle density reduced to 30%.
    #[serde(default)]
    pub reduce_motion: bool,
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
            reduce_motion: false,
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
    // S83: captain's log replaces the old ship log as the L-key target.
    OpenCaptainsLog,
    // S33: call a crew conference (co-deliberation) from the comms panel.
    OpenCrewConference,
    // S64: dedicated panel toggles — each panel gets its own InputAction
    // so one keypress doesn't open six overlapping panels.
    OpenCareerPanel,
    OpenCulturePanel,
    OpenDiscoveryPanel,
    OpenFactionsPanel,
    OpenMarketPanel,
    LeaveHelm,

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
    /// S81/D9: install the workshop draft (or an imported contract) into the
    /// live ContractRuntime so it actually runs on the ship.
    InstallContract,

    // OnBoard consoles
    ConsoleDigit1,
    ConsoleDigit2,
    ConsoleDigit3,
    ConsoleDigit4,

    // UI Navigation (gamepad / keyboard focus ring)
    UiUp,
    UiDown,
    UiLeft,
    UiRight,
    UiConfirm,
    UiCancel,

    // S72: diegetic help mode toggle.
    OpenHelp,

    // Reserved (do not assign defaults that collide; variant exists for S29 /
    // save-management so future sprints don't hardcode literals).
    VoicePushToTalk,
    /// Cycle to the next available microphone input device.
    MicCycleDevice,
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
            // KeyX, not KeyF: KeyF is FireWeapons, and both live in the
            // Combat group during SpaceFlight — firing also cycled the target.
            (CycleTargetReverse, KeyBind(KeyX)),
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
            // S83 moved the captain's log onto KeyL; the older ship log keeps
            // its own binding rather than firing both panels at once.
            (OpenShipLog, KeyBind(KeyZ)),
            (OpenCaptainsLog, KeyBind(KeyL)),
            (OpenMissionBoard, KeyBind(KeyJ)),
            // S33: call a crew conference (co-deliberation) from the comms panel.
            (OpenCrewConference, KeyBind(KeyY)),
            // S64: dedicated panel toggles (defaults overlap combat/flight
            // keys where contexts are disjoint).
            // KeyO, not KeyU: KeyU is OpenCrewRoster, same Interaction group.
            (OpenCareerPanel, KeyBind(KeyO)),
            (OpenCulturePanel, KeyBind(KeyP)),
            (OpenDiscoveryPanel, KeyBind(KeyH)),
            (OpenFactionsPanel, KeyBind(KeyN)),
            (OpenMarketPanel, KeyBind(KeyK)),
            (LeaveHelm, KeyBind(KeyB)),
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
            (InstallContract, KeyBind(F2)),
            // OnBoard consoles
            (ConsoleDigit1, KeyBind(Digit1)),
            (ConsoleDigit2, KeyBind(Digit2)),
            (ConsoleDigit3, KeyBind(Digit3)),
            (ConsoleDigit4, KeyBind(Digit4)),
            // UI Navigation (gamepad D-pad/left-stick mapped too)
            (UiUp, KeyBind(ArrowUp)),
            (UiDown, KeyBind(ArrowDown)),
            (UiLeft, KeyBind(ArrowLeft)),
            (UiRight, KeyBind(ArrowRight)),
            (UiConfirm, KeyBind(Enter)),
            (UiCancel, KeyBind(Escape)),
            // S72: diegetic help
            (OpenHelp, KeyBind(F1)),
            // Reserved
            (VoicePushToTalk, KeyBind(KeyV)),
            // F7, not KeyB: KeyB is LeaveHelm, and both are live in flight.
            (MicCycleDevice, KeyBind(F7)),
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
            // S83: captain's log toggle.
            OpenCaptainsLog,
            // S33: call a crew conference (co-deliberation) from the comms panel.
            OpenCrewConference,
            // S64: dedicated panel toggles.
            OpenCareerPanel,
            OpenCulturePanel,
            OpenDiscoveryPanel,
            OpenFactionsPanel,
            OpenMarketPanel,
            LeaveHelm,
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
            InstallContract,
            ConsoleDigit1,
            ConsoleDigit2,
            ConsoleDigit3,
            ConsoleDigit4,
            UiUp,
            UiDown,
            UiLeft,
            UiRight,
            UiConfirm,
            UiCancel,
            VoicePushToTalk,
            MicCycleDevice,
            QuickSave,
            QuickLoad,
            OpenHelp,
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
            OpenCaptainsLog => "Open captain's log",
            OpenMissionBoard => "Open mission board",
            OpenCrewConference => "Open crew conference",
            // S64: dedicated panel toggles.
            OpenCareerPanel => "Open career panel",
            OpenCulturePanel => "Open culture panel",
            OpenDiscoveryPanel => "Open discovery panel",
            OpenFactionsPanel => "Open factions panel",
            OpenMarketPanel => "Open market panel",
            LeaveHelm => "Leave helm",
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
            InstallContract => "Install contract",
            ConsoleDigit1 => "Console 1",
            ConsoleDigit2 => "Console 2",
            ConsoleDigit3 => "Console 3",
            ConsoleDigit4 => "Console 4",
            UiUp => "UI up",
            UiDown => "UI down",
            UiLeft => "UI left",
            UiRight => "UI right",
            UiConfirm => "UI confirm",
            UiCancel => "UI back / cancel",
            VoicePushToTalk => "Voice push-to-talk",
            MicCycleDevice => "Cycle mic device",
            QuickSave => "Quick save",
            QuickLoad => "Quick load",
            OpenHelp => "Open help",
        }
    }

    /// Which group this action belongs to (for the keybind table UI).
    pub fn group(&self) -> &'static str {
        use InputAction::*;
        match self {
            ThrustForward | ThrustBackward | StrafeLeft | StrafeRight | RollLeft | RollRight
            | Boost | Brake => "Movement",
            FireWeapons | FireMissile | CycleTarget | CycleTargetReverse | PowerSelectUp
            | PowerSelectDown | PowerAdjustLeft | PowerAdjustRight | LaunchChaff => "Combat",
            LockOnCycleNext | LockOnCyclePrev | AttackLight | AttackHeavy | Dodge | Block => {
                "Landed Combat"
            }
            Interact | Pause | OpenComms | OpenMap | OpenInventory | OpenCrewRoster
            | OpenShipLog | OpenCaptainsLog | OpenMissionBoard | OpenCrewConference
            | OpenCareerPanel | OpenCulturePanel | OpenDiscoveryPanel | OpenFactionsPanel
            | OpenMarketPanel | LeaveHelm => "Interaction",
            EditorConfirm | EditorCancel | EditorCursorUp | EditorCursorDown | EditorCursorLeft
            | EditorCursorRight | EditorCycleNext | EditorCyclePrev | EditorTabNext
            | EditorRotate | EditorDelete | InstallContract => "Editor",
            ConsoleDigit1 | ConsoleDigit2 | ConsoleDigit3 | ConsoleDigit4 => "OnBoard",
            UiUp | UiDown | UiLeft | UiRight | UiConfirm | UiCancel => "Navigation",
            VoicePushToTalk | MicCycleDevice => "Voice",
            QuickSave | QuickLoad => "Reserved",
            OpenHelp => "Interaction",
        }
    }
}

// ---------------------------------------------------------------------------
// Settings consumer registry — used by the completeness gate test
// ---------------------------------------------------------------------------

/// Returns a map of every settings field path to its consumer description.
/// Every field in `Settings` (and all sub-structs) must have an entry here.
/// Adding a new field without a registry entry causes a test failure.
pub fn settings_consumer_registry() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("version", "settings.rs — serialization schema tracking"),
        (
            "audio.master_volume",
            "voice/audio_feed_voice, sfx/process_sfx, music/tick_music, setup/play_music",
        ),
        ("audio.music_volume", "music/tick_music, setup/play_music"),
        ("audio.sfx_volume", "sfx/process_sfx"),
        ("audio.voice_volume", "voice/audio_feed_voice"),
        ("audio.mute_when_unfocused", "music/tick_music"),
        (
            "audio.voice_input_device",
            "voice/start_voice_thread, voice/mic_cycle_system",
        ),
        ("video.fullscreen", "setup/apply_video_settings"),
        ("video.resolution", "setup/apply_video_settings"),
        ("video.vsync", "setup/apply_video_settings"),
        ("video.render_scale", "setup/apply_video_settings"),
        ("video.ui_scale", "setup/apply_video_settings"),
        (
            "video.show_fps",
            "settings_ui (display-config exception — toggles FPS overlay)",
        ),
        ("controls.keybinds", "ALL systems via settings.key()"),
        ("controls.mouse_sensitivity", "ship/control"),
        ("controls.invert_y", "ship/control"),
        (
            "controls.controller_deadzone",
            "ship/control (apply_deadzone)",
        ),
        (
            "gameplay.aim_assist",
            "ship/fire_weapons (aim_assisted_forward)",
        ),
        ("gameplay.auto_dock", "docking/try_dock"),
        ("gameplay.show_tutorial_hints", "onboarding/tutorial_hints"),
        (
            "gameplay.combat_log_verbosity",
            "log_capture/capture_combat_damage, log_ui",
        ),
        (
            "gameplay.auto_save_interval_secs",
            "inventory/autosave_system",
        ),
        (
            "accessibility.colorblind_mode",
            "hud/semantic_glyphs, hud/update_hud_status",
        ),
        ("accessibility.text_scale", "hud/text_scaling"),
        ("accessibility.high_contrast_ui", "hud/high_contrast_ui"),
        ("accessibility.screen_shake", "ship/camera_follow"),
        ("accessibility.subtitles", "captions"),
        ("accessibility.subtitle_size", "captions"),
        (
            "accessibility.hold_for_interact",
            "interaction/try_interact",
        ),
        (
            "accessibility.reduce_motion",
            "ship/camera_follow, starfield/dust_parallax, ship/control",
        ),
        ("network.server_url", "network/connect_on_enter_playing"),
        ("network.auto_connect", "network/connect_on_enter_playing"),
        ("network.show_latency", "network/poll_network"),
    ])
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
            KeyY => "KeyY",
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
            KeyH => "KeyH",
            KeyO => "KeyO",
            KeyZ => "KeyZ",
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
            "KeyY" => KeyY,
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
            "KeyH" => KeyH,
            "KeyO" => KeyO,
            "KeyZ" => KeyZ,
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
        assert!(!s.accessibility.reduce_motion); // S71: new field defaults to false
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

    /// Every key actually used in the shipped defaults must survive the
    /// string table. A `KeyCode` missing from `name()`/`from_name()` silently
    /// round-trips as `KeyF`, so a settings save would rebind the action.
    /// The five-keycode spot check above cannot catch that.
    #[test]
    fn every_default_keybind_round_trips() {
        for (action, bind) in InputAction::default_keybinds() {
            let name = KeyBind::name(bind.0);
            assert_eq!(
                KeyBind::from_name(name),
                bind.0,
                "{action:?} is bound to a KeyCode missing from the string table \
                 (it would load back as KeyF); add it to KeyBind::name/from_name"
            );
            assert!(
                !KeyBind::display(bind.0).is_empty(),
                "{action:?} has no display string"
            );
        }
    }

    /// Two actions in the same group must not share a default key: those
    /// actions are live at the same time, so one press fires both. Reuse
    /// ACROSS groups is deliberate (flight / editor / UI navigation are
    /// never active simultaneously) and is not checked here.
    #[test]
    fn no_default_key_collisions_within_a_group() {
        use std::collections::HashMap;
        let binds = InputAction::default_keybinds();
        let mut seen: HashMap<(&str, KeyCode), InputAction> = HashMap::new();
        for action in InputAction::all() {
            let Some(bind) = binds.get(action) else {
                continue;
            };
            let key = (action.group(), bind.0);
            if let Some(other) = seen.insert(key, *action) {
                panic!(
                    "{:?} and {:?} are both in the \"{}\" group and both default to {} \
                     — one keypress fires both",
                    other,
                    action,
                    action.group(),
                    KeyBind::display(bind.0)
                );
            }
        }
    }

    #[test]
    fn key_display_known() {
        assert_eq!(KeyBind::display(KeyCode::KeyW), "W");
        assert_eq!(KeyBind::display(KeyCode::ArrowUp), "↑");
        assert_eq!(KeyBind::display(KeyCode::ShiftLeft), "LShift");
    }

    /// Every `Settings` field has a registered consumer outside `settings_ui`.
    ///
    /// The field list is *derived* by serializing `Settings::default()` and
    /// walking it, not hand-written. That matters: the previous version of
    /// this test compared the registry against a hardcoded list of 33 paths,
    /// so a newly added field was invisible to it — the gate could only ever
    /// re-confirm what someone had already remembered to type.
    #[test]
    fn all_settings_have_consumers() {
        let consumers = settings_consumer_registry();
        let expected = enumerate_settings_field_paths();
        assert!(
            expected.len() >= 30,
            "field enumeration collapsed ({} paths) — the walker is broken, \
             not the settings struct",
            expected.len()
        );

        let mut missing: Vec<String> = expected
            .iter()
            .filter(|path| !consumers.contains_key(path.as_str()))
            .cloned()
            .collect();
        missing.sort();
        assert!(
            missing.is_empty(),
            "these Settings fields have no entry in settings_consumer_registry(): \
             {missing:?} — wire the field, then name its consumer"
        );

        // A registry entry that names no real consumer is worse than a missing
        // one: it makes the gate certify a dead setting. `gameplay.aim_assist`
        // shipped claiming "combat/cycle_target, combat/enemy_fly" while being
        // read by neither.
        let placeholders: Vec<&&str> = consumers
            .iter()
            .filter(|(_, v)| {
                let v = v.to_ascii_lowercase();
                v.contains("placeholder") || v.contains("todo") || v.contains("future")
            })
            .map(|(k, _)| k)
            .collect();
        assert!(
            placeholders.is_empty(),
            "these registry entries are placeholders, so the gate is green on a \
             setting that does nothing: {placeholders:?}"
        );

        // Stale entries point at fields that no longer exist.
        let mut stale: Vec<&&str> = consumers
            .keys()
            .filter(|k| !expected.iter().any(|e| e == *k))
            .collect();
        stale.sort();
        assert!(
            stale.is_empty(),
            "registry entries for fields that no longer exist: {stale:?}"
        );
    }

    /// Walk `Settings::default()` as JSON and collect every leaf field path
    /// (`audio.master_volume`, …). Containers that are themselves the unit of
    /// configuration — the keybind map, the resolution tuple — are leaves.
    fn enumerate_settings_field_paths() -> Vec<String> {
        fn walk(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        let path = if prefix.is_empty() {
                            k.clone()
                        } else {
                            format!("{prefix}.{k}")
                        };
                        // Only recurse into the top-level sub-structs; below
                        // that, a map or array IS the configured value.
                        if matches!(v, serde_json::Value::Object(_)) && prefix.is_empty() {
                            walk(v, &path, out);
                        } else {
                            out.push(path);
                        }
                    }
                }
                _ => out.push(prefix.to_string()),
            }
        }
        let json = serde_json::to_value(Settings::default()).expect("Settings serializes");
        let mut out = Vec::new();
        walk(&json, "", &mut out);
        out
    }

    #[test]
    fn all_input_actions_have_labels() {
        for action in InputAction::all() {
            let label = action.label();
            assert!(!label.is_empty(), "InputAction {action:?} has empty label");
            let group = action.group();
            assert!(!group.is_empty(), "InputAction {action:?} has empty group");
        }
    }

    #[test]
    fn reduce_motion_defaults_to_false() {
        let s = Settings::default();
        assert!(!s.accessibility.reduce_motion);
    }

    #[test]
    fn animation_speed_multiplier_from_settings() {
        let mut s = Settings::default();
        s.accessibility.reduce_motion = false;
        assert_eq!(AnimationSpeedMultiplier::from_settings(&s).0, 1.0);
        s.accessibility.reduce_motion = true;
        assert_eq!(AnimationSpeedMultiplier::from_settings(&s).0, 0.25);
    }
}
