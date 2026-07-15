//! HUD (spec §14 deliverable: "HUD adapts"): fuel gauge + ship's log in
//! `SpaceFlight`; a location-name banner in `Landed`/`OnBoard`; the
//! deliberation overlay ("Boris is considering the situation…" — spec §6
//! deliberation UX); the `OFFLINE` badge that appears whenever online mode
//! has no live connection (iron rule #3); and the pause overlay.

use bevy::prelude::*;

use crate::net::{ConnectionState, NetMode};
use crate::states::{CurrentLocation, GameMode};
use crate::systems::contract::{DeliberationState, ShipLog};
use crate::systems::factions::FactionState;
use crate::systems::interaction::{ActivePanel, Npc};
use crate::systems::inventory::PlayerInventory;
use crate::systems::market::{market_panel_text, Economy, MarketState};
use crate::systems::pause::PauseOverlay;
use crate::systems::ship::ShipSystems;

#[derive(Component)]
pub struct FuelReadout;

#[derive(Component)]
pub struct LogReadout;

#[derive(Component)]
pub struct DeliberationOverlay;

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

/// Key-binding help line. Swapped per mode by `update_hud_status` so the
/// flight bindings and the interior bindings never show at the wrong time.
#[derive(Component)]
pub struct HelpText;

const HELP_FLIGHT: &str =
    "W/S pitch · A/D yaw · Q/E roll (double-tap: barrel roll) · Space thrust · \
     Shift boost · Ctrl brake · F fire · G mine · T scan · M map · Enter dock/jump · J self-jump · \
     X anomaly · Esc pause";
const HELP_INTERIOR: &str =
    "WASD walk · E interact/board · L launch · B walk ship · F refuel (docked) · Esc pause";

pub fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        FuelReadout,
        Text::new("FUEL 100%"),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        TextColor(Color::srgb(0.7, 0.9, 0.7)),
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
        TextColor(Color::srgb(0.85, 0.9, 0.95)),
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
        TextColor(Color::srgb(0.75, 0.8, 0.9)),
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
        TextColor(Color::srgb(0.95, 0.85, 0.5)),
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
        TextColor(Color::srgb(0.95, 0.4, 0.4)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
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
        TextColor(Color::srgb(0.95, 0.95, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(45.0),
            left: Val::Percent(40.0),
            ..default()
        },
    ));
    commands.spawn((
        HelpText,
        Text::new(HELP_FLIGHT),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.5, 0.55, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(30.0),
            left: Val::Px(8.0),
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
        TextColor(Color::srgb(0.9, 0.9, 0.75)),
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
        TextColor(Color::srgb(0.7, 0.95, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(360.0),
            ..default()
        },
    ));
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
    log: Res<ShipLog>,
    deliberation: Res<DeliberationState>,
    net_mode: Res<NetMode>,
    conn: Res<ConnectionState>,
    mut texts: ParamSet<(
        Query<&mut Text, With<FuelReadout>>,
        Query<&mut Text, With<LocationBanner>>,
        Query<&mut Text, With<LogReadout>>,
        Query<&mut Text, With<DeliberationOverlay>>,
        Query<&mut Text, With<OfflineBadge>>,
        Query<&mut Text, With<PauseOverlay>>,
        Query<&mut Text, With<HelpText>>,
    )>,
) {
    if let Ok(mut text) = texts.p0().single_mut() {
        if *mode == GameMode::SpaceFlight {
            let pct = systems.fuel.0 * 100 / 1024;
            let hull = systems.hull_hp.0 * 100 / 1024;
            let breach = if systems.dead { "  ⚠ BREACH" } else { "" };
            **text = format!(
                "FUEL {pct}%{}  HULL {hull}%{breach}",
                if systems.thrusting { " ▲" } else { "" }
            );
        } else {
            **text = "—".to_string();
        }
    }
    if let Ok(mut text) = texts.p1().single_mut() {
        **text = match **mode {
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
            GameMode::Docking => "DOCKING…".to_string(),
            GameMode::Undocking => "UNDOCKING…".to_string(),
            GameMode::Hyperspace => "HYPERSPACE…".to_string(),
            GameMode::Paused => "PAUSED".to_string(),
        };
    }
    if let Ok(mut text) = texts.p2().single_mut() {
        **text = log.entries.join("\n");
    }
    if let Ok(mut text) = texts.p3().single_mut() {
        // S02: online deliberation stays invisible until `llm.deliberating`
        // confirms the server is on it — see `Deliberation::overlay_visible`.
        **text = match &deliberation.active {
            Some(d) if d.overlay_visible => format!(
                "⟳ {} is considering the situation…\n  \"{}. My rules don't cover this.\"",
                d.crew_member, d.context_summary
            ),
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
            GameMode::Paused => "⏸ PAUSED\n\nEsc to resume".to_string(),
            _ => String::new(),
        };
    }
    if let Ok(mut text) = texts.p6().single_mut() {
        let help = match **mode {
            GameMode::SpaceFlight => HELP_FLIGHT,
            GameMode::Landed | GameMode::OnBoard => HELP_INTERIOR,
            _ => "", // transition beats/pause: no bindings to advertise
        };
        if **text != help {
            **text = help.to_string();
        }
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
    economy: Res<Economy>,
    faction_state: Res<FactionState>,
    npcs: Query<&Npc>,
    mut texts: ParamSet<(
        Query<&mut Text, With<DialoguePanel>>,
        Query<&mut Text, With<MarketPanel>>,
    )>,
) {
    if let Ok(mut text) = texts.p0().single_mut() {
        **text = match &*panel {
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
                &economy,
                &faction_state,
            ),
            _ => String::new(),
        };
    }
}
