//! On-Board consoles + crew orders (spec §14 Mode 2; S08). Consoles are
//! plain `Interactable`s dropped by `systems/interior.rs`; this module owns
//! the panel text entities, the per-frame renderer, and the input that takes
//! the helm, vents fuel, or issues a crew order. All per S08's "ship is a
//! place" outcome.

use bevy::prelude::*;

use reachlock_core::sim::SimEvent;
use reachlock_core::util::rng::Fixed;

use crate::states::{CurrentLocation, GameMode};
use crate::systems::contract::ShipLog;
use crate::systems::crew::{CrewFigure, CrewRoster, ORDER_ROOMS};
use crate::systems::interaction::ActivePanel;
use crate::systems::ship::{ShipCommand, ShipSystems, POWER_BUDGET, POWER_MAX_NOTCH};
use crate::systems::ticker::UniverseTicker;

/// Panel marker components (screen-fixed via `Node` absolute positioning).
#[derive(Component, Default)]
pub struct HelmPanel;
#[derive(Component, Default)]
pub struct EngPanel;
#[derive(Component, Default)]
pub struct NavPanel;
#[derive(Component, Default)]
pub struct LogPanel;
#[derive(Component, Default)]
pub struct OrderPanel;
/// S09b flight-console panels (spec §22).
#[derive(Component, Default)]
pub struct GunnerPanel;
#[derive(Component, Default)]
pub struct ScannerPanel;
#[derive(Component, Default)]
pub struct MinerPanel;
#[derive(Component, Default)]
pub struct PowerPanel;
/// S12: Galactic News feed panel.
#[derive(Component, Default)]
pub struct NewsPanel;

/// Spawn the five on-board panel texts once (on entering `InGame`). They're
/// empty until their `ActivePanel` opens; `onboard_panels` fills them.
pub fn spawn_onboard_panels(mut commands: Commands) {
    let base = |top: f32, left: f32| Node {
        position_type: PositionType::Absolute,
        top: Val::Px(top),
        left: Val::Px(left),
        ..default()
    };
    commands.spawn((
        HelmPanel,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.8, 0.95, 0.9)),
        base(120.0, 360.0),
    ));
    commands.spawn((
        EngPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.7, 0.6)),
        base(200.0, 360.0),
    ));
    commands.spawn((
        NavPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.7, 0.85, 0.95)),
        base(260.0, 360.0),
    ));
    commands.spawn((
        LogPanel,
        Text::new(""),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.8, 0.85, 0.9)),
        base(320.0, 360.0),
    ));
    commands.spawn((
        OrderPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.9, 0.8)),
        base(120.0, 8.0),
    ));
    // S09b flight consoles, stacked on the right.
    let flight = |top: f32| Node {
        position_type: PositionType::Absolute,
        top: Val::Px(top),
        right: Val::Px(12.0),
        ..default()
    };
    commands.spawn((
        GunnerPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.7, 0.6)),
        flight(120.0),
    ));
    commands.spawn((
        ScannerPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.6, 0.85, 0.95)),
        flight(200.0),
    ));
    commands.spawn((
        MinerPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.7, 0.95, 0.8)),
        flight(280.0),
    ));
    commands.spawn((
        PowerPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.9, 0.6)),
        flight(360.0),
    ));
    // S12: galactic news feed, accessible from any interactable.
    commands.spawn((
        NewsPanel,
        Text::new(""),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.6, 0.95, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
}

/// S09b: the gunner/scanner/miner/power consoles (spec §22). They don't fly the
/// ship — they configure it, writing the [`ShipCommand`] bus that the flight
/// systems read. A separate system from `onboard_panels` because a `ParamSet`
/// caps at eight members and the original already holds five.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn onboard_ship_consoles(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<ActivePanel>,
    mut command: ResMut<ShipCommand>,
    systems: Res<ShipSystems>,
    mut log: ResMut<ShipLog>,
    mut panels: ParamSet<(
        Query<&mut Text, With<GunnerPanel>>,
        Query<&mut Text, With<ScannerPanel>>,
        Query<&mut Text, With<MinerPanel>>,
        Query<&mut Text, With<PowerPanel>>,
    )>,
) {
    if let Ok(mut t) = panels.p0().single_mut() {
        match &*panel {
            ActivePanel::Gunner => {
                if keys.just_pressed(KeyCode::Enter) {
                    command.weapons_armed = !command.weapons_armed;
                    log.log(if command.weapons_armed {
                        "Gunner: weapons ARMED"
                    } else {
                        "Gunner: weapons safe"
                    });
                }
                let state = if command.weapons_armed {
                    "ARMED"
                } else {
                    "SAFE"
                };
                **t = format!(
                    "GUNNER\nweapons {state}\npower {}\nENTER: arm/safe\n(fly: F to fire)",
                    command.power_weapons
                );
            }
            _ => **t = String::new(),
        }
    }
    if let Ok(mut t) = panels.p1().single_mut() {
        match &*panel {
            ActivePanel::Scanner => {
                if keys.just_pressed(KeyCode::Enter) {
                    command.scanner_boost = !command.scanner_boost;
                    log.log(if command.scanner_boost {
                        "Scanner: long-range sweep ON"
                    } else {
                        "Scanner: standard range"
                    });
                }
                let mode = if command.scanner_boost { "LONG" } else { "STD" };
                **t = format!(
                    "SCANNER\nrange {mode}\npower {}\nENTER: toggle range\n(fly: T to pulse)",
                    command.power_sensors
                );
            }
            _ => **t = String::new(),
        }
    }
    if let Ok(mut t) = panels.p2().single_mut() {
        match &*panel {
            ActivePanel::Miner => {
                if keys.just_pressed(KeyCode::Enter) {
                    command.mining_enabled = !command.mining_enabled;
                    log.log(if command.mining_enabled {
                        "Miner: rig ONLINE"
                    } else {
                        "Miner: rig stowed"
                    });
                }
                let state = if command.mining_enabled {
                    "ONLINE"
                } else {
                    "STOWED"
                };
                **t = format!(
                    "MINER\nrig {state}\nore {}\nENTER: toggle\n(fly: hold G)",
                    systems.ore
                );
            }
            _ => **t = String::new(),
        }
    }
    if let Ok(mut t) = panels.p3().single_mut() {
        match &*panel {
            ActivePanel::Power => {
                // 1/2/3 bump a subsystem's notch; wraps to 0 past the cap or the
                // shared budget. This is the spec §22 reference console.
                let used: u8 =
                    command.power_weapons + command.power_engines + command.power_sensors;
                let bump = |cur: u8, add: bool| -> u8 {
                    if !add {
                        return cur;
                    }
                    let next = cur + 1;
                    if next > POWER_MAX_NOTCH || used - cur + next > POWER_BUDGET {
                        0
                    } else {
                        next
                    }
                };
                if keys.just_pressed(KeyCode::Digit1) {
                    command.power_weapons = bump(command.power_weapons, true);
                }
                if keys.just_pressed(KeyCode::Digit2) {
                    command.power_engines = bump(command.power_engines, true);
                }
                if keys.just_pressed(KeyCode::Digit3) {
                    command.power_sensors = bump(command.power_sensors, true);
                }
                let free = POWER_BUDGET as i32
                    - (command.power_weapons + command.power_engines + command.power_sensors)
                        as i32;
                **t = format!(
                    "POWER  (budget {POWER_BUDGET})\n1 WPN {}\n2 ENG {}\n3 SEN {}\nfree {free}",
                    command.power_weapons, command.power_engines, command.power_sensors
                );
            }
            _ => **t = String::new(),
        }
    }
}

/// Render the open console / order panel and handle its input (take helm,
/// vent/refill, order a crew member). Kept under Bevy's system-param arity
/// cap by sharing one pass over all five panels.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn onboard_panels(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<ActivePanel>,
    mut systems: ResMut<ShipSystems>,
    mut log: ResMut<ShipLog>,
    mut roster: ResMut<CrewRoster>,
    location: Res<CurrentLocation>,
    mut next: ResMut<NextState<GameMode>>,
    mut inv: ResMut<crate::systems::inventory::PlayerInventory>,
    mut panels: ParamSet<(
        Query<&mut Text, With<HelmPanel>>,
        Query<&mut Text, With<EngPanel>>,
        Query<&mut Text, With<NavPanel>>,
        Query<&mut Text, With<LogPanel>>,
        Query<&mut Text, With<OrderPanel>>,
    )>,
    crew_figs: Query<&CrewFigure>,
    ticker_state: Res<UniverseTicker>,
) {
    if let Ok(mut t) = panels.p0().single_mut() {
        match &*panel {
            ActivePanel::Helm => {
                **t = "HELM\nTake the helm.\nPress ENTER to fly.".to_string();
                if keys.just_pressed(KeyCode::Enter) {
                    next.set(GameMode::SpaceFlight);
                    *panel = ActivePanel::None;
                }
            }
            _ => **t = String::new(),
        }
    }
    if let Ok(mut t) = panels.p1().single_mut() {
        match &*panel {
            ActivePanel::Engineering => {
                let pct = systems.fuel.0 * 100 / 1024;
                let hull = systems.hull_hp.0 * 100 / 1024;
                let docked = location.is_docked;
                **t = if docked {
                    format!(
                        "ENGINEERING\nfuel {pct}%\nhull {hull}%\nPress V refill · R repair (10cr/hp)",
                    )
                } else {
                    format!("ENGINEERING\nfuel {pct}%\nhull {hull}%  (dock to repair)")
                };
                if keys.just_pressed(KeyCode::KeyV) {
                    systems.fuel = Fixed(1024);
                    log.log("Engineering: refilled fuel");
                }
                if keys.just_pressed(KeyCode::KeyR) && docked && systems.hull_hp.0 < 1024 {
                    let missing = 1024 - systems.hull_hp.0;
                    let cost = missing * 10;
                    if inv.credits >= cost {
                        inv.credits -= cost;
                        systems.hull_hp = Fixed(1024);
                        log.log(format!("Engineering: hull restored ({cost}cr)"));
                    } else {
                        log.log("Engineering: not enough credits to repair");
                    }
                }
            }
            _ => **t = String::new(),
        }
    }
    if let Ok(mut t) = panels.p2().single_mut() {
        match &*panel {
            ActivePanel::Nav => {
                let mut news_lines = vec![format!(
                    "NAV · system {:#x}\ntick {}\n\n── GALACTIC NEWS ──",
                    location.system_seed, ticker_state.state.tick_no,
                )];
                for ev in ticker_state.state.event_log.iter().rev().take(10) {
                    let line = match ev {
                        SimEvent::EconomyTick { tick_no } => {
                            format!("  tick {tick_no}: market update")
                        }
                        SimEvent::DiplomaticShift {
                            faction,
                            other,
                            change,
                        } => {
                            format!(
                                "  tick {}: diplomatic shift {faction} → {other} ({change})",
                                ticker_state.state.tick_no
                            )
                        }
                        SimEvent::ContentRelease { content_id, .. } => {
                            format!(
                                "  tick {}: {content_id} released",
                                ticker_state.state.tick_no
                            )
                        }
                        SimEvent::ChapterFired { chapter_id } => {
                            format!(
                                "  tick {}: chapter '{chapter_id}'",
                                ticker_state.state.tick_no
                            )
                        }
                    };
                    news_lines.push(line);
                }
                **t = news_lines.join("\n");
            }
            _ => **t = String::new(),
        }
    }
    if let Ok(mut t) = panels.p3().single_mut() {
        match &*panel {
            ActivePanel::Log => **t = log.entries.join("\n"),
            _ => **t = String::new(),
        }
    }
    if let Ok(mut t) = panels.p4().single_mut() {
        match &*panel {
            ActivePanel::Order(e) => {
                let id = crew_figs.get(*e).ok().map(|f| f.0.clone());
                let Some(id) = id else {
                    **t = String::new();
                    return;
                };
                let Some(m) = roster.by_id(&id) else {
                    **t = String::new();
                    return;
                };
                let mut s = format!("ORDER {}:\n", m.name);
                for (i, room) in ORDER_ROOMS.iter().enumerate() {
                    let cur = m.order == Some(*room);
                    s.push_str(&format!(
                        "  {}. {room:?}{}\n",
                        i + 1,
                        if cur { " *" } else { "" }
                    ));
                }
                s.push_str("press 1-5 to set · 0 to clear");
                **t = s;
                // Number keys 1-5 set the order (matching `ORDER_ROOMS`);
                // 0 clears it.
                let pressed = (1..=ORDER_ROOMS.len()).position(|n| {
                    keys.just_pressed(match n {
                        1 => KeyCode::Digit1,
                        2 => KeyCode::Digit2,
                        3 => KeyCode::Digit3,
                        4 => KeyCode::Digit4,
                        _ => KeyCode::Digit5,
                    })
                });
                if let Some(i) = pressed {
                    let room = ORDER_ROOMS[i];
                    if let Some(mm) = roster.by_id_mut(&id) {
                        mm.order = Some(room);
                        log.log(format!("You ordered {} to {:?}", mm.name, room));
                    }
                    *panel = ActivePanel::None;
                }
                if keys.just_pressed(KeyCode::Digit0) {
                    if let Some(mm) = roster.by_id_mut(&id) {
                        mm.order = None;
                        log.log(format!("You cleared {}'s order", mm.name));
                    }
                    *panel = ActivePanel::None;
                }
            }
            _ => **t = String::new(),
        }
    }
}
