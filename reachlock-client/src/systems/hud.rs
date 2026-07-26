//! HUD (spec §14 deliverable: "HUD adapts"): fuel gauge + ship's log in
//! `SpaceFlight`; a location-name banner in `Landed`/`OnBoard`; the
//! deliberation overlay ("Boris is considering the situation…" — spec §6
//! deliberation UX); the `OFFLINE` badge that appears whenever online mode
//! has no live connection (iron rule #3); and the pause overlay.

use bevy::prelude::*;

use crate::net::{ConnectionState, NetMode};
use crate::settings::{HelpTextCache, Settings};
use crate::states::{CurrentLocation, GameMode};
use crate::systems::contract::{DeliberationState, ShipLog};
use crate::systems::interaction::{ActivePanel, InteractionPrompt, Npc};
use crate::systems::inventory::PlayerInventory;
use crate::systems::market::{market_panel_text, MarketState};
use crate::systems::pause::PauseOverlay;
use crate::systems::ship::{FlightFeel, ShipSystems};
use crate::systems::ticker::UniverseTicker;
use crate::theme;

/// Threat severity hierarchy for the feedback HUD.
#[derive(Clone, Debug)]
pub struct Threat {
    pub severity: ThreatSeverity,
    pub label: String,
    pub glyph: &'static str,
}

// Baseline severity. The HUD only styles elevated states today.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ThreatSeverity {
    Normal,
    Medium,
    Top,
}

// Transition animation state; the mode banner cuts rather than animates.
#[allow(dead_code)]
/// Resource for mode transition state.
#[derive(Resource, Default)]
pub struct TransitionState {
    pub animating: bool,
    pub elapsed: f32,
    pub total: f32,
    pub banner_text: String,
}

#[derive(Component)]
pub struct FuelReadout;

#[derive(Component)]
pub struct LogReadout;

#[derive(Component)]
pub struct DeliberationOverlay;

/// FPS counter overlay (shown when settings.video.show_fps is true).
#[derive(Component)]
pub struct FpsCounter;

/// Latency display overlay (shown when settings.network.show_latency is true).
#[derive(Component)]
pub struct LatencyDisplay;

/// S02: shown only in online mode when the socket isn't `Connected` — never
/// shown offline, since offline is the normal default, not a degraded state.
#[derive(Component)]
pub struct OfflineBadge;

/// Location-name banner in Landed/OnBoard (spec §14: "location name banner
/// in Landed/OnBoard").
#[derive(Component)]
pub struct LocationBanner;

/// Dialogue panel (S07): shows the talked-to NPC's name + authored lines.
#[derive(Component)]
pub struct DialoguePanel;

/// Market panel (S07): buy/sell UI text rendered from `market_panel_text`.
#[derive(Component)]
pub struct MarketPanel;

/// Dilemma panel (S82): generated dilemma with choices.
#[derive(Component)]
pub struct DilemmaPanel;

/// Encounter panel (S82): scripted encounter narrative and choices.
#[derive(Component)]
pub struct EncounterPanel;

/// Trope popup (S82): lightweight procedural narrative seasoning.
#[derive(Component)]
pub struct TropePanel;

/// Key-binding help line. Swapped per mode by `update_hud_status` so the
/// flight bindings and the interior bindings never show at the wrong time.
#[derive(Component)]
pub struct HelpText;

/// The "[E] Mara" interaction prompt, bottom-center. Until this element
/// existed the prompt was computed every frame and shown nowhere — the whole
/// interaction system was invisible to the player.
#[derive(Component)]
pub struct PromptText;

// S31: help strings are rebuilt from settings (see `HelpTextCache`), not
// hardcoded here — these statics are gone. The HUD reads the cache below.

/// Rebuild the help-text cache whenever the keybind settings change (spec S31
/// §6). Cheap: runs only on a `Settings` mutation, not every frame.
pub fn refresh_help_cache(settings: Res<Settings>, mut cache: ResMut<HelpTextCache>) {
    if settings.is_changed() {
        *cache = HelpTextCache::rebuild(&settings);
    }
}

pub fn spawn_hud(mut commands: Commands, settings: Res<Settings>) {
    commands.spawn((
        FuelReadout,
        Text::new("FUEL 100%"),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        theme::fg("text.ok"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        LocationBanner,
        Text::new(""),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        theme::fg("text"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Percent(40.0),
            ..default()
        },
    ));
    commands.spawn((
        LogReadout,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        theme::fg("text"),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        DeliberationOverlay,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        theme::fg("text.warn"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(60.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        OfflineBadge,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        theme::fg("text.danger"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        FpsCounter,
        Text::new(""),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        theme::fg("text.muted"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(24.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        LatencyDisplay,
        Text::new(""),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        theme::fg("text.muted"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(36.0),
            right: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        PauseOverlay,
        Text::new(""),
        TextFont {
            font_size: 28.0,
            ..default()
        },
        theme::fg("text.warn"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(45.0),
            left: Val::Percent(40.0),
            ..default()
        },
    ));
    commands.spawn((
        HelpText,
        Text::new(HelpTextCache::rebuild(&settings).flight),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        theme::fg("text.muted"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(30.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        PromptText,
        Text::new(""),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        theme::fg("text.warn"),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(18.0),
            left: Val::Percent(46.0),
            ..default()
        },
    ));
    commands.spawn((
        DialoguePanel,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        theme::fg("text.accent"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        MarketPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        theme::fg("text.ok"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(360.0),
            ..default()
        },
    ));
    commands.spawn((
        DilemmaPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        theme::fg("text.warn"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(100.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        EncounterPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        theme::fg("text"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(100.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        TropePanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        theme::fg("text"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(100.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
}

/// Severity-sorted threats from ship state. Top 3, highest severity first.
fn hud_threats(systems: &ShipSystems) -> Vec<Threat> {
    let mut t = Vec::new();
    if systems.dead {
        t.push(Threat {
            severity: ThreatSeverity::Top,
            label: "BREACH".into(),
            glyph: "⚠",
        });
    }
    if systems.hull_hp.0 < 307 {
        t.push(Threat {
            severity: ThreatSeverity::Top,
            label: "HULL STRESS".into(),
            glyph: "▲",
        });
    } else if systems.hull_hp.0 < 512 {
        t.push(Threat {
            severity: ThreatSeverity::Medium,
            label: "HULL DAMAGE".into(),
            glyph: "▲",
        });
    }
    if systems.fuel.0 < 154 {
        t.push(Threat {
            severity: ThreatSeverity::Medium,
            label: "LOW FUEL".into(),
            glyph: "•",
        });
    }
    t.sort_by_key(|b| std::cmp::Reverse(b.severity));
    t.truncate(3);
    t
}

// Bevy's `SystemParamFunction` impl is capped at a fixed arity, so the HUD
// reader is split: `update_hud_status` covers the always-on status texts and
// `update_hud_panels` covers the interaction panels. Splitting keeps each
// system's param list under the cap.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_hud_status(
    mode: Res<State<GameMode>>,
    location: Res<CurrentLocation>,
    systems: Res<ShipSystems>,
    feel: Res<FlightFeel>,
    prompt: Res<InteractionPrompt>,
    log: Res<ShipLog>,
    deliberation: Res<DeliberationState>,
    net_mode: Res<NetMode>,
    conn: Res<ConnectionState>,
    pause_sel: Res<crate::systems::pause::PauseSelection>,
    transition: Res<TransitionState>,
    settings: Res<Settings>,
    mut texts: ParamSet<(
        Query<&mut Text, With<FuelReadout>>,
        Query<&mut Text, With<LocationBanner>>,
        Query<&mut Text, With<LogReadout>>,
        Query<&mut Text, With<DeliberationOverlay>>,
        Query<&mut Text, With<OfflineBadge>>,
        Query<&mut Text, With<PauseOverlay>>,
        Query<&mut Text, With<HelpText>>,
        Query<&mut Text, With<PromptText>>,
    )>,
) {
    // --- Hierarchical HUD status line (FuelReadout slot) ---
    if let Ok(mut text) = texts.p0().single_mut() {
        if *mode == GameMode::SpaceFlight {
            let pct = systems.fuel.0 * 100 / 1024;
            let hull = systems.hull_hp.0 * 100 / 1024;
            let spd = feel.speed.round() as i64;

            // Top priority threats (red/orange)
            let threats = hud_threats(&systems);
            let top: Vec<&Threat> = threats
                .iter()
                .filter(|t| t.severity == ThreatSeverity::Top)
                .collect();
            let med: Vec<&Threat> = threats
                .iter()
                .filter(|t| t.severity == ThreatSeverity::Medium)
                .collect();

            let mut display = String::new();
            // Top priority line — only the most critical alerts
            if !top.is_empty() {
                let alerts: Vec<String> = top
                    .iter()
                    .map(|t| format!("{} {}", t.glyph, t.label))
                    .collect();
                display.push_str(&format!("{}\n", alerts.join(" · ")));
            }
            // Medium priority line — yellow warnings
            if !med.is_empty() {
                let warns: Vec<String> = med
                    .iter()
                    .map(|t| format!("{} {}", t.glyph, t.label))
                    .collect();
                display.push_str(&format!("{}\n", warns.join(" · ")));
            }
            // Normal priority — always-on data
            display.push_str(&format!(
                "SPD {spd}  FUEL {pct}%{}  HULL {hull}%",
                if systems.thrusting { " ▲" } else { "" }
            ));

            **text = display;
        } else {
            **text = "—".to_string();
        }
    }
    // --- Mode transition + location banner (LocationBanner slot) ---
    if let Ok(mut text) = texts.p1().single_mut() {
        let duration_mult = if settings.accessibility.reduce_motion {
            0.0
        } else {
            1.0
        };
        **text = match **mode {
            GameMode::Docking => {
                let loc = if location.display_name.is_empty() {
                    &location.station_id
                } else {
                    &location.display_name
                };
                if transition.animating && duration_mult > 0.0 {
                    "DOCKING…".into()
                } else {
                    format!("DOCKING WITH {}", loc.to_uppercase())
                }
            }
            GameMode::Undocking => {
                if transition.animating && duration_mult > 0.0 {
                    "UNDOCKING…".into()
                } else {
                    "UNDOCKING — CLEAR SPACE".into()
                }
            }
            GameMode::Hyperspace => {
                let dest = &location.display_name;
                if dest.is_empty() {
                    "HYPERSPACE…".into()
                } else {
                    format!("JUMP TO {} — ETA 14m", dest.to_uppercase())
                }
            }
            GameMode::SpaceFlight => format!("SPACE · system {:#x}", location.system_seed),
            GameMode::Landed => {
                if location.display_name.is_empty() {
                    format!("LANDED · {}", location.station_id)
                } else {
                    format!("LANDED · {}", location.display_name)
                }
            }
            GameMode::OnBoard => {
                let where_ = if location.is_docked {
                    "docked"
                } else {
                    "in transit"
                };
                format!("ON BOARD · your ship ({where_})")
            }
            GameMode::Paused => "PAUSED".to_string(),
        };
    }
    if let Ok(mut text) = texts.p2().single_mut() {
        **text = log.entries.join("\n");
    }
    if let Ok(mut text) = texts.p3().single_mut() {
        // Deliberation overlay — the panel replaces this; keep minimal fallback.
        **text = match &deliberation.active {
            Some(d) if d.overlay_visible => format!("⟳ {} is considering…", d.crew_member),
            _ => String::new(),
        };
    }
    if let Ok(mut text) = texts.p4().single_mut() {
        // Offline mode is the normal default — no badge. Online mode shows
        // OFFLINE whenever the socket isn't actually Connected (still
        // connecting, or dropped and retrying): the game keeps playing
        // locally either way (iron rule #3).
        **text = match (&*net_mode, &*conn) {
            (NetMode::Online { .. }, ConnectionState::Connected) => String::new(),
            (NetMode::Online { .. }, _) => "OFFLINE".to_string(),
            (NetMode::Offline, _) => String::new(),
        };
    }
    if let Ok(mut text) = texts.p5().single_mut() {
        **text = match **mode {
            GameMode::Paused => {
                let sel = *pause_sel == crate::systems::pause::PauseSelection::Resume;
                let resume = if sel { "> " } else { "  " };
                let settings = if !sel { "> " } else { "  " };
                format!(
                    "⏸ PAUSED\n\n{resume}Resume\n{settings}Settings\n\nTab select · Enter activate · Esc resume"
                )
            }
            _ => String::new(),
        };
    }
    if let Ok(mut text) = texts.p6().single_mut() {
        let cache = HelpTextCache::rebuild(&settings);
        let help = match **mode {
            GameMode::SpaceFlight => cache.flight.clone(),
            GameMode::Landed | GameMode::OnBoard => cache.interior.clone(),
            _ => String::new(),
        };
        if **text != help {
            **text = help;
        }
    }
    if let Ok(mut text) = texts.p7().single_mut() {
        **text = prompt.text.clone().unwrap_or_default();
    }
}

/// Drives the interaction panels (dialogue / market) when an `ActivePanel` is
/// open. Split out of `update_hud_status` to stay under the param-arity cap.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_hud_panels(
    location: Res<CurrentLocation>,
    panel: Res<ActivePanel>,
    inventory: Res<PlayerInventory>,
    market_state: Res<MarketState>,
    ticker: Res<UniverseTicker>,
    dialogue: Res<crate::systems::dialogue::DialogueSession>,
    souls: Res<crate::systems::soul::SoulRegistry>,
    editor_state: Res<crate::systems::shipeditor::ShipEditorState>,
    shipcfg: Res<crate::systems::shipeditor::ShipConfig>,
    interior_editor_state: Res<crate::systems::shipeditor::InteriorEditorState>,
    interior_cfg: Res<crate::systems::shipeditor::InteriorConfig>,
    content: Res<crate::systems::content_index::ContentIndex>,
    npcs: Query<&Npc>,
    mut texts: ParamSet<(
        Query<&mut Text, With<DialoguePanel>>,
        Query<&mut Text, With<MarketPanel>>,
    )>,
) {
    if let Ok(mut text) = texts.p0().single_mut() {
        **text = match &*panel {
            // S16: soul-backed conversations render through the session
            // (choices, free input, in-panel deliberation); legacy NPCs
            // keep the S07 authored-lines rendering below.
            ActivePanel::Dialogue(_)
                if crate::systems::dialogue::panel_text(&dialogue, &souls).is_some() =>
            {
                crate::systems::dialogue::panel_text(&dialogue, &souls).unwrap_or_default()
            }
            ActivePanel::Dialogue(e) => match npcs.get(*e) {
                Ok(npc) => {
                    let mut s = format!("{}:\n", npc.name);
                    for line in &npc.dialogue {
                        s.push_str("  ");
                        s.push_str(line);
                        s.push('\n');
                    }
                    if npc.dialogue.is_empty() {
                        s.push_str("  *says nothing*");
                    }
                    s
                }
                Err(_) => String::new(),
            },
            _ => String::new(),
        };
    }
    if let Ok(mut text) = texts.p1().single_mut() {
        **text = match &*panel {
            ActivePanel::Market => market_panel_text(
                &inventory,
                &location,
                &market_state,
                &ticker.state.economy,
                &ticker.state.factions,
            ),
            // S17: the exterior editor shares the market's panel surface.
            ActivePanel::ShipExterior => crate::systems::shipeditor::editor_panel_text(
                &editor_state,
                &shipcfg,
                &inventory,
                &content,
                &ticker,
            ),
            // S18: the interior editor shares the same panel surface.
            ActivePanel::ShipInterior => crate::systems::shipeditor::interior_panel_text(
                &interior_editor_state,
                &interior_cfg,
                &inventory,
                &content,
            ),
            _ => String::new(),
        };
    }
}

/// Renders the dilemma panel text when ActivePanel::Dilemma is active.
pub fn render_dilemma_panel(
    panel: Res<ActivePanel>,
    active: Res<crate::systems::dilemma::ActiveDilemma>,
    outcome: Res<crate::systems::dilemma::DilemmaOutcomeText>,
    selected: Res<crate::systems::dilemma::DilemmaChoiceSelected>,
    mut query: Query<&mut Text, With<DilemmaPanel>>,
) {
    if let Ok(mut text) = query.single_mut() {
        **text = if *panel == ActivePanel::Dilemma {
            crate::systems::dilemma::dilemma_panel_text(active, outcome, selected)
                .unwrap_or_default()
        } else {
            String::new()
        };
    }
}

/// Renders the encounter panel text when ActivePanel::Encounter is active.
pub fn render_encounter_panel(
    panel: Res<ActivePanel>,
    active: Res<crate::systems::encounter_executor::ActiveEncounter>,
    mut query: Query<&mut Text, With<EncounterPanel>>,
) {
    if let Ok(mut text) = query.single_mut() {
        **text = if *panel == ActivePanel::Encounter {
            crate::systems::encounter_executor::encounter_panel_text(active).unwrap_or_default()
        } else {
            String::new()
        };
    }
}

/// Renders the trope popup text when ActivePanel::TropePopup is active.
pub fn render_trope_panel(
    panel: Res<ActivePanel>,
    popup: Res<crate::systems::trope_dispatcher::ActiveTropePopup>,
    mut query: Query<&mut Text, With<TropePanel>>,
) {
    if let Ok(mut text) = query.single_mut() {
        **text = if *panel == ActivePanel::TropePopup {
            crate::systems::trope_dispatcher::trope_panel_text(popup).unwrap_or_default()
        } else {
            String::new()
        };
    }
}

/// Update the voice-status badge (VoiceBadge) with current transmission state.
/// Shows "[TX]" when transmitting, "[name]" when a remote player speaks, or
/// empty when silent. Early-outs when no badge entity exists.
pub fn update_voice_hud(
    state: Res<crate::systems::voice::VoiceHudState>,
    mut query: Query<&mut Text, With<crate::systems::voice::VoiceBadge>>,
) {
    if let Ok(mut text) = query.single_mut() {
        **text = if state.transmitting {
            "[TX]".to_string()
        } else if !state.current_speakers.is_empty() {
            format!("[{}]", state.current_speakers[0])
        } else {
            String::new()
        };
    }
}
