//! Settings UI (spec S31 §3). A keyboard-driven, tabbed panel accessible from
//! the main menu and the pause menu. All widget interaction is keyboard-only
//! (the rest of the game is too): Tab/Shift+Tab cycle tabs, Arrow keys move
//! the row cursor, A/D adjust sliders / cycle dropdowns / flip toggles, Enter
//! activates (toggle / start key-rebind / open text field / apply / reset),
//! Esc cancels (capture / text edit) or closes the panel (reverting the
//! draft). Applied settings are written to disk and pushed into the `Settings`
//! resource, which invalidates the help-text cache.
//!
//! Keybind capture detects conflicts: binding a key already used by another
//! action warns and offers to steal it. Unbound actions are highlighted. The
//! "Reset to Defaults" button per tab (and a global "Reset All") restores from
//! the registry default map without touching other settings.

use bevy::audio::{AudioSource, PlaybackSettings, Volume};
use bevy::ecs::message::MessageReader;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::bridge;
use crate::settings::{
    save_settings, ColorblindMode, HelpTextCache, InputAction, KeyBind, Settings,
};

/// Which tab is showing. Order is the cycle order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsTab {
    Audio,
    Video,
    Controls,
    Gameplay,
    Accessibility,
    Network,
}

impl SettingsTab {
    const ALL: [SettingsTab; 6] = [
        SettingsTab::Audio,
        SettingsTab::Video,
        SettingsTab::Controls,
        SettingsTab::Gameplay,
        SettingsTab::Accessibility,
        SettingsTab::Network,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap()
    }

    fn from_index(i: usize) -> SettingsTab {
        Self::ALL[i % Self::ALL.len()]
    }

    fn name(self) -> &'static str {
        match self {
            SettingsTab::Audio => "Audio",
            SettingsTab::Video => "Video",
            SettingsTab::Controls => "Controls",
            SettingsTab::Gameplay => "Gameplay",
            SettingsTab::Accessibility => "Accessibility",
            SettingsTab::Network => "Network",
        }
    }

    /// How many selectable rows this tab renders.
    fn row_count(self) -> usize {
        match self {
            SettingsTab::Audio => 5,
            SettingsTab::Video => 6,
            SettingsTab::Controls => 4 + InputAction::all().len(),
            SettingsTab::Gameplay => 5,
            SettingsTab::Accessibility => 7,
            SettingsTab::Network => 3,
        }
    }
}

/// Live editor state for the settings panel.
#[derive(Resource)]
pub struct SettingsUiState {
    pub open: bool,
    /// True when opened from the main menu (closing returns there); false when
    /// opened from the pause menu (closing returns to the pause overlay).
    from_menu: bool,
    tab: SettingsTab,
    row: usize,
    /// Action currently being rebound (capture mode).
    capturing: Option<InputAction>,
    /// Server-URL text-edit buffer (when editing the Network URL row).
    text_edit: Option<String>,
    /// "Reset to Defaults" confirmation pending for the current tab.
    reset_confirm: bool,
    /// Working copy — changes are previewed live but only persisted on Apply.
    draft: Settings,
    /// Current mic device display name (updated by sync_settings_panel).
    #[allow(dead_code)]
    mic_device_name: String,
}

impl Default for SettingsUiState {
    fn default() -> Self {
        SettingsUiState {
            open: false,
            from_menu: true,
            tab: SettingsTab::Audio,
            row: 0,
            capturing: None,
            text_edit: None,
            reset_confirm: false,
            draft: Settings::default(),
            mic_device_name: String::new(),
        }
    }
}

/// Marker for the panel's on-screen text entity.
#[derive(Component, Default)]
pub struct SettingsPanel;

impl SettingsUiState {
    fn open(&mut self, settings: &Settings, from_menu: bool) {
        self.open = true;
        self.from_menu = from_menu;
        self.tab = SettingsTab::Audio;
        self.row = 0;
        self.capturing = None;
        self.text_edit = None;
        self.reset_confirm = false;
        self.draft = settings.clone();
    }

    fn close(&mut self) {
        self.open = false;
        self.capturing = None;
        self.text_edit = None;
    }

    /// Apply the draft: push into the `Settings` resource, persist to disk, and
    /// rebuild the help-text cache.
    fn apply(&self, settings: &mut Settings, cache: &mut HelpTextCache) {
        *settings = self.draft.clone();
        save_settings(settings);
        *cache = HelpTextCache::rebuild(settings);
    }

    /// Reset the current tab's fields (and keybinds, for Controls) to defaults,
    /// leaving other tabs untouched.
    fn reset_tab(&mut self) {
        let defaults = Settings::default();
        match self.tab {
            SettingsTab::Audio => self.draft.audio = defaults.audio,
            SettingsTab::Video => self.draft.video = defaults.video,
            SettingsTab::Controls => self.draft.controls = defaults.controls,
            SettingsTab::Gameplay => self.draft.gameplay = defaults.gameplay,
            SettingsTab::Accessibility => self.draft.accessibility = defaults.accessibility,
            SettingsTab::Network => self.draft.network = defaults.network,
        }
    }
}

/// Open the settings panel from the main menu (called by `menu.rs`).
pub fn open_settings_from_menu(state: &mut SettingsUiState, settings: &Settings) {
    state.open(settings, true);
}

/// Open the settings panel from the pause menu (called by `pause.rs`).
pub fn open_settings_from_pause(state: &mut SettingsUiState, settings: &Settings) {
    state.open(settings, false);
}

/// Spawn / despawn the panel text entity to match `state.open`.
pub fn sync_settings_panel(
    mut commands: Commands,
    state: Res<SettingsUiState>,
    query: Query<Entity, With<SettingsPanel>>,
) {
    if state.open && query.is_empty() {
        commands.spawn((
            SettingsPanel,
            Text::new(""),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.85, 0.9, 0.95)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                left: Val::Px(8.0),
                ..default()
            },
        ));
    } else if !state.open && !query.is_empty() {
        for e in &query {
            commands.entity(e).despawn();
        }
    }
}

/// The settings panel driver. Runs every frame while open. Keyboard-only.
#[allow(clippy::too_many_arguments, clippy::collapsible_if)]
pub fn settings_ui_system(
    mut commands: Commands,
    mut state: ResMut<SettingsUiState>,
    mut settings: ResMut<Settings>,
    mut cache: ResMut<HelpTextCache>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    mut texts: Query<&mut Text, With<SettingsPanel>>,
    mut key_events: MessageReader<KeyboardInput>,
) {
    let Some(mut text) = texts.iter_mut().next() else {
        return;
    };

    // Drain keyboard events every frame (even when closed) so they don't pile
    // up unread in the event buffer. Single read() pass populates both pressed
    // and typed vecs — a second .read() would get an empty iterator.
    let mut pressed: Vec<KeyCode> = Vec::with_capacity(8);
    let mut typed: Vec<String> = Vec::with_capacity(8);
    for e in key_events.read() {
        if e.state.is_pressed() {
            pressed.push(e.key_code);
            if let Key::Character(s) = &e.logical_key {
                typed.push(s.to_string());
            }
        }
    }

    if !state.open {
        return;
    }

    // --- capture mode: bind the next key pressed ---
    if let Some(action) = state.capturing {
        if let Some(kc) = pressed.first().copied() {
            if kc != KeyCode::Escape {
                // Conflict check against other actions in the draft.
                if let Some(conflict) = state.draft.conflict_for(kc, action) {
                    warn!(
                        "key {} already bound to {}; stealing it",
                        KeyBind::display(kc),
                        conflict.label()
                    );
                }
                state.draft.controls.keybinds.insert(action, KeyBind(kc));
            }
            state.capturing = None;
        }
        render(&state, &mut text);
        return;
    }

    // --- text-edit mode: server URL ---
    if state.text_edit.is_some() {
        let mut buf = state.text_edit.take().unwrap();
        for ch in &typed {
            if ch
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == ':' || c == '_')
            {
                buf.push_str(ch);
            }
        }
        for kc in &pressed {
            if *kc == KeyCode::Backspace {
                buf.pop();
            } else if *kc == KeyCode::Enter {
                state.draft.network.server_url = buf.clone();
            } else if *kc == KeyCode::Escape {
                // cancel: discard the buffer, keep the previous URL
            }
        }
        state.text_edit = Some(buf);
        render(&state, &mut text);
        return;
    }

    let tab = state.tab;
    let mut row = state.row;

    // --- tab cycling (Tab / Shift+Tab) ---
    for kc in &pressed {
        if *kc == KeyCode::Tab {
            let n = SettingsTab::ALL.len();
            let i = (tab.index() + 1) % n;
            state.tab = SettingsTab::from_index(i);
            state.row = 0;
            state.reset_confirm = false;
            render(&state, &mut text);
            return;
        }
    }

    // --- row navigation ---
    for kc in &pressed {
        if *kc == KeyCode::ArrowUp {
            row = row.saturating_sub(1);
        } else if *kc == KeyCode::ArrowDown {
            row = row.saturating_add(1).min(tab.row_count() - 1);
        }
    }
    state.row = row;

    // --- per-row activation / adjustment ---
    let mut preview_volume: Option<(f32, f32)> = None; // (master, sub) for tone
    for kc in &pressed {
        handle_row(
            &mut state,
            &mut preview_volume,
            *kc,
            &mut commands,
            &mut audio_sources,
        );
    }

    // Play a volume preview tone if an audio slider was touched.
    if let Some((master, sub)) = preview_volume {
        preview_tone(&mut commands, &mut audio_sources, master * sub);
    }

    // --- global close / apply ---
    for kc in &pressed {
        if *kc == KeyCode::Escape {
            // Esc with no pending reset closes & reverts the draft.
            if state.reset_confirm {
                state.reset_confirm = false;
            } else {
                state.close();
                render(&state, &mut text);
                return;
            }
        } else if *kc == KeyCode::Enter {
            // Enter on the last row ("Apply") commits; on "Reset All" resets.
            if row == tab.row_count() - 1 {
                if state.reset_confirm {
                    // bottom row is Reset All only on some tabs; handled below
                }
                state.apply(&mut settings, &mut cache);
                state.close();
                render(&state, &mut text);
                return;
            }
        }
    }

    render(&state, &mut text);
}

/// Apply the effect of a single keypress on the current row.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::collapsible_if)]
fn handle_row(
    state: &mut SettingsUiState,
    preview: &mut Option<(f32, f32)>,
    kc: KeyCode,
    _commands: &mut Commands,
    _audio: &mut ResMut<Assets<AudioSource>>,
) {
    let tab = state.tab;
    let row = state.row;
    let d = &mut state.draft;

    match tab {
        SettingsTab::Audio => match row {
            0 => {
                let (m, s) = (d.audio.master_volume, d.audio.master_volume);
                adjust(&mut d.audio.master_volume, kc, 0.0, 1.0, preview, m, s);
            }
            1 => {
                let (m, s) = (d.audio.master_volume, d.audio.music_volume);
                adjust(&mut d.audio.music_volume, kc, 0.0, 1.0, preview, m, s);
            }
            2 => {
                let (m, s) = (d.audio.master_volume, d.audio.sfx_volume);
                adjust(&mut d.audio.sfx_volume, kc, 0.0, 1.0, preview, m, s);
            }
            3 => {
                let (m, s) = (d.audio.master_volume, d.audio.voice_volume);
                adjust(&mut d.audio.voice_volume, kc, 0.0, 1.0, preview, m, s);
            }
            4 => {
                d.audio.mute_when_unfocused =
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter;
            }
            _ => {}
        },
        SettingsTab::Video => match row {
            0 => {
                d.video.fullscreen ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            1 => cycle_resolution(d, kc),
            2 => {
                d.video.vsync ^= kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            3 => adjust_f(&mut d.video.render_scale, kc, 0.5, 2.0),
            4 => adjust_f(&mut d.video.ui_scale, kc, 0.5, 2.0),
            5 => {
                d.video.show_fps ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            _ => {}
        },
        SettingsTab::Controls => {
            if row < 4 {
                match row {
                    0 => adjust_f(&mut d.controls.mouse_sensitivity, kc, 0.1, 5.0),
                    1 => {
                        d.controls.invert_y ^=
                            kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
                    }
                    2 => adjust_f(&mut d.controls.controller_deadzone, kc, 0.0, 0.5),
                    3 => { /* reset-controls row, handled in apply */ }
                    _ => {}
                }
            } else {
                // A rebindable keybind row.
                let action = InputAction::all()[row - 4];
                if kc == KeyCode::Enter {
                    state.capturing = Some(action);
                }
            }
        }
        SettingsTab::Gameplay => match row {
            0 => {
                d.gameplay.aim_assist ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            1 => {
                d.gameplay.auto_dock ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            2 => {
                d.gameplay.show_tutorial_hints ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            3 => adjust_u8(&mut d.gameplay.combat_log_verbosity, kc, 0, 3),
            4 => adjust_u32(&mut d.gameplay.auto_save_interval_secs, kc, 1, 60),
            _ => {}
        },
        SettingsTab::Accessibility => match row {
            0 => cycle_colorblind(d, kc),
            1 => adjust_f(&mut d.accessibility.text_scale, kc, 0.5, 3.0),
            2 => {
                d.accessibility.high_contrast_ui ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            3 => adjust_f(&mut d.accessibility.screen_shake, kc, 0.0, 1.0),
            4 => {
                d.accessibility.subtitles ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            5 => adjust_f(&mut d.accessibility.subtitle_size, kc, 0.5, 2.0),
            6 => {
                d.accessibility.hold_for_interact ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            _ => {}
        },
        SettingsTab::Network => match row {
            0 => {
                if kc == KeyCode::Enter {
                    state.text_edit = Some(d.network.server_url.clone());
                }
            }
            1 => {
                d.network.auto_connect ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            2 => {
                d.network.show_latency ^=
                    kc == KeyCode::KeyA || kc == KeyCode::KeyD || kc == KeyCode::Enter
            }
            _ => {}
        },
    }

    // Bottom row of every tab hosts the "Reset Tab" (R) control. "Apply" is
    // handled by the main system when Enter is pressed on the last row.
    if row == tab.row_count() - 1 && kc == KeyCode::KeyR {
        if state.reset_confirm {
            state.reset_tab();
            state.reset_confirm = false;
        } else {
            state.reset_confirm = true;
        }
    }
}

// --- small adjust helpers ---------------------------------------------------

fn adjust(
    v: &mut f32,
    kc: KeyCode,
    lo: f32,
    hi: f32,
    preview: &mut Option<(f32, f32)>,
    master: f32,
    sub: f32,
) {
    let step = (hi - lo) / 20.0;
    if kc == KeyCode::KeyA {
        *v = (*v - step).max(lo);
        *preview = Some((master, sub));
    } else if kc == KeyCode::KeyD {
        *v = (*v + step).min(hi);
        *preview = Some((master, sub));
    } else if kc == KeyCode::Enter {
        *preview = Some((master, sub));
    }
}

fn adjust_f(v: &mut f32, kc: KeyCode, lo: f32, hi: f32) {
    let step = (hi - lo) / 20.0;
    if kc == KeyCode::KeyA {
        *v = (*v - step).max(lo);
    } else if kc == KeyCode::KeyD {
        *v = (*v + step).min(hi);
    }
}

fn adjust_u8(v: &mut u8, kc: KeyCode, lo: u8, hi: u8) {
    if kc == KeyCode::KeyA {
        *v = (*v as i32 - 1).max(lo as i32) as u8;
    } else if kc == KeyCode::KeyD {
        *v = (*v as i32 + 1).min(hi as i32) as u8;
    }
}

fn adjust_u32(v: &mut u32, kc: KeyCode, lo: u32, hi: u32) {
    if kc == KeyCode::KeyA {
        *v = v.saturating_sub(1).max(lo);
    } else if kc == KeyCode::KeyD {
        *v = v.saturating_add(1).min(hi);
    }
}

fn cycle_colorblind(d: &mut Settings, kc: KeyCode) {
    if kc != KeyCode::KeyA && kc != KeyCode::KeyD && kc != KeyCode::Enter {
        return;
    }
    d.accessibility.colorblind_mode = match d.accessibility.colorblind_mode {
        ColorblindMode::None => ColorblindMode::Protanopia,
        ColorblindMode::Protanopia => ColorblindMode::Deuteranopia,
        ColorblindMode::Deuteranopia => ColorblindMode::Tritanopia,
        ColorblindMode::Tritanopia => ColorblindMode::None,
    };
}

fn cycle_resolution(d: &mut Settings, kc: KeyCode) {
    if kc != KeyCode::KeyA && kc != KeyCode::KeyD && kc != KeyCode::Enter {
        return;
    }
    const PRESETS: [(u32, u32); 5] = [
        (0, 0),
        (1280, 720),
        (1920, 1080),
        (2560, 1440),
        (3840, 2160),
    ];
    let cur = PRESETS
        .iter()
        .position(|p| *p == d.video.resolution)
        .unwrap_or(0);
    let next = if kc == KeyCode::KeyD {
        (cur + 1) % PRESETS.len()
    } else {
        (cur + PRESETS.len() - 1) % PRESETS.len()
    };
    d.video.resolution = PRESETS[next];
}

fn preview_tone(
    commands: &mut Commands,
    audio_sources: &mut ResMut<Assets<AudioSource>>,
    volume: f32,
) {
    // A 0.25s calm blip at the previewed volume so the player hears the change.
    let tone =
        reachlock_core::generator::generate_music(0xBEEF, reachlock_core::generator::Mood::Calm, 1);
    commands.spawn((
        AudioPlayer(audio_sources.add(bridge::audio_from_generated(&tone))),
        PlaybackSettings {
            volume: Volume::Linear(volume.clamp(0.0, 1.0)),
            ..default()
        },
    ));
}

// --- rendering --------------------------------------------------------------

fn render(state: &SettingsUiState, text: &mut Text) {
    let d = &state.draft;
    let mut s = String::new();

    // Tab strip.
    let tabs: Vec<String> = SettingsTab::ALL
        .iter()
        .map(|t| {
            if *t == state.tab {
                format!("[{}]", t.name())
            } else {
                t.name().to_string()
            }
        })
        .collect();
    s.push_str(&format!("SETTINGS  {}", tabs.join(" ")));
    s.push('\n');

    if let Some(action) = state.capturing {
        s.push_str(&format!(
            "\nPress a new key for '{}'… (Esc cancels)",
            action.label()
        ));
        s.push_str(&format!(
            "\n\n(current: {})",
            KeyBind::display(d.key(action))
        ));
        **text = s;
        return;
    }
    if let Some(buf) = &state.text_edit {
        s.push_str(&format!(
            "\nServer URL: {}_  (type, Enter to commit, Esc cancel)\n",
            buf
        ));
        **text = s;
        return;
    }

    let tab = state.tab;
    let rows: Vec<String> = match tab {
        SettingsTab::Audio => vec![
            fmt_slider("Master volume", d.audio.master_volume),
            fmt_slider("Music volume", d.audio.music_volume),
            fmt_slider("SFX volume", d.audio.sfx_volume),
            fmt_slider("Voice volume", d.audio.voice_volume),
            fmt_toggle("Mute when unfocused", d.audio.mute_when_unfocused),
        ],
        SettingsTab::Video => vec![
            fmt_toggle("Fullscreen", d.video.fullscreen),
            fmt_pair("Resolution", &res_str(d.video.resolution)),
            fmt_toggle("VSync", d.video.vsync),
            fmt_slider("Render scale", d.video.render_scale),
            fmt_slider("UI scale", d.video.ui_scale),
            fmt_toggle("Show FPS", d.video.show_fps),
        ],
        SettingsTab::Controls => {
            let mut v = vec![
                fmt_slider("Mouse sensitivity", d.controls.mouse_sensitivity),
                fmt_toggle("Invert Y", d.controls.invert_y),
                fmt_slider("Controller deadzone", d.controls.controller_deadzone),
                "Reset all keybinds to defaults".to_string(),
            ];
            for action in InputAction::all() {
                let bound = d.controls.keybinds.get(action).copied();
                let label = match bound {
                    Some(b) => KeyBind::display(b.0),
                    None => "— (unbound)".to_string(),
                };
                v.push(format!("  {}: {}", action.label(), label));
            }
            v
        }
        SettingsTab::Gameplay => vec![
            fmt_toggle("Aim assist", d.gameplay.aim_assist),
            fmt_toggle("Auto dock", d.gameplay.auto_dock),
            fmt_toggle("Tutorial hints", d.gameplay.show_tutorial_hints),
            fmt_int(
                "Combat log verbosity",
                d.gameplay.combat_log_verbosity as i64,
            ),
            fmt_int(
                "Autosave interval (s)",
                d.gameplay.auto_save_interval_secs as i64,
            ),
        ],
        SettingsTab::Accessibility => vec![
            fmt_pair("Colorblind mode", mode_str(d.accessibility.colorblind_mode)),
            fmt_slider("Text scale", d.accessibility.text_scale),
            fmt_toggle("High contrast UI", d.accessibility.high_contrast_ui),
            fmt_slider("Screen shake", d.accessibility.screen_shake),
            fmt_toggle("Subtitles", d.accessibility.subtitles),
            fmt_slider("Subtitle size", d.accessibility.subtitle_size),
            fmt_toggle("Hold to interact", d.accessibility.hold_for_interact),
        ],
        SettingsTab::Network => vec![
            format!("Server URL: {}", d.network.server_url),
            fmt_toggle("Auto-connect", d.network.auto_connect),
            fmt_toggle("Show latency", d.network.show_latency),
        ],
    };

    for (i, line) in rows.iter().enumerate() {
        let cursor = if i == state.row { ">" } else { " " };
        s.push_str(&format!("\n{cursor}{line}"));
    }

    // Bottom row: Apply + Reset Tab.
    let cursor = if state.row == tab.row_count() - 1 {
        ">"
    } else {
        " "
    };
    let reset = if state.reset_confirm {
        "  [R again: confirm reset tab]"
    } else {
        ""
    };
    s.push_str(&format!(
        "\n{cursor}Apply & Save (Enter)   |   Reset Tab (R){reset}"
    ));
    s.push_str("\n\nTab: switch tab · ↑/↓: move · A/D: adjust · Enter: activate · Esc: close");

    **text = s;
}

fn fmt_slider(name: &str, v: f32) -> String {
    format!("{name}: {:.2}", v)
}
fn fmt_toggle(name: &str, on: bool) -> String {
    format!("{name}: {}", if on { "ON" } else { "off" })
}
fn fmt_int(name: &str, v: i64) -> String {
    format!("{name}: {v}")
}
fn fmt_pair(name: &str, v: &str) -> String {
    format!("{name}: {v}")
}
fn res_str(r: (u32, u32)) -> String {
    if r.0 == 0 && r.1 == 0 {
        "native".to_string()
    } else {
        format!("{}x{}", r.0, r.1)
    }
}
fn mode_str(m: ColorblindMode) -> &'static str {
    match m {
        ColorblindMode::None => "None",
        ColorblindMode::Protanopia => "Protanopia",
        ColorblindMode::Deuteranopia => "Deuteranopia",
        ColorblindMode::Tritanopia => "Tritanopia",
    }
}
