//! On-Board consoles + crew orders (spec §14 Mode 2; S08). Consoles are
//! plain `Interactable`s dropped by `systems/interior.rs`; this module owns
//! the panel text entities, the per-frame renderer, and the input that takes
//! the helm, vents fuel, or issues a crew order. All per S08's "ship is a
//! place" outcome.

use bevy::prelude::*;

use reachlock_core::sim::SimEvent;
use reachlock_core::util::rng::Fixed;

use crate::states::{CurrentLocation, GameMode, ModeScope, SceneRegistry};
use crate::systems::contract::ShipLog;
use crate::systems::crew::{CrewFigure, CrewRoster, ORDER_ROOMS};
use crate::systems::interaction::ActivePanel;
use crate::systems::ship::{ShipCommand, ShipSystems, POWER_BUDGET, POWER_MAX_NOTCH};
use crate::systems::ticker::UniverseTicker;

/// Which console is showing the live flight scene (S09d station views —
/// docs/SHIPS.md §1: "each station has its own view of the same live world").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StationView {
    Gunner,
    Scanner,
    Miner,
}

/// The station view open this frame, if any. Written once per frame by
/// [`update_station_view`]; read by the camera manager, the flight systems
/// (which key F/T/G to the matching console), and the reticle.
#[derive(Resource, Default, PartialEq)]
pub struct ActiveStationView(pub Option<StationView>);

/// A station view is open when the crew is on board a ship *in flight* (the
/// space scene is alive underneath the interior) and one of the flight
/// consoles has focus. Docked, the same consoles keep their plain config
/// panels — there is no live outside to show. Pure so it unit-tests without
/// a Bevy world.
pub fn station_view(
    mode: GameMode,
    panel: &ActivePanel,
    is_docked: bool,
    space_alive: bool,
) -> Option<StationView> {
    if mode != GameMode::OnBoard || is_docked || !space_alive {
        return None;
    }
    match panel {
        ActivePanel::Gunner => Some(StationView::Gunner),
        ActivePanel::Scanner => Some(StationView::Scanner),
        ActivePanel::Miner => Some(StationView::Miner),
        _ => None,
    }
}

/// Publish [`ActiveStationView`] for the frame.
pub fn update_station_view(
    mode: Option<Res<State<GameMode>>>,
    panel: Res<ActivePanel>,
    location: Res<CurrentLocation>,
    registry: Res<SceneRegistry>,
    mut view: ResMut<ActiveStationView>,
) {
    let current =
        mode.and_then(|m| station_view(**m, &panel, location.is_docked, registry.space_alive));
    if view.0 != current {
        view.0 = current;
    }
}

/// Hide the interior sprites while a station view fills the screen with the
/// live flight scene, and restore them when the view closes (walking off the
/// console — `InteractionPrompt::anchor` — or Esc both close it). Flips
/// visibility only on a state change, not every frame.
pub fn station_view_mask(
    view: Res<ActiveStationView>,
    mut was_open: Local<bool>,
    mut interior_entities: Query<(&ModeScope, &mut Visibility)>,
) {
    let open = view.0.is_some();
    if open == *was_open {
        return;
    }
    *was_open = open;
    for (scope, mut vis) in &mut interior_entities {
        if matches!(scope.0, GameMode::OnBoard | GameMode::Landed) {
            *vis = if open {
                Visibility::Hidden
            } else {
                // Inherited is the spawn default for every interior entity;
                // the interact glow re-resolves its own visibility each frame.
                Visibility::Inherited
            };
        }
    }
}

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
    view: Res<ActiveStationView>,
    mut command: ResMut<ShipCommand>,
    systems: Res<ShipSystems>,
    feel: Res<crate::systems::ship::FlightFeel>,
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
                **t = if view.0 == Some(StationView::Gunner) {
                    // Live view: the reticle marks the guns' axis in the
                    // scene behind this text; F fires for real.
                    format!(
                        "GUNNER — LIVE\nweapons {state} · power {}\nspeed {:.0}\n\
                         F fire · ENTER arm/safe\nwalk away to stand down",
                        command.power_weapons, feel.speed
                    )
                } else {
                    format!(
                        "GUNNER\nweapons {state}\npower {}\nENTER: arm/safe\n(fly: F to fire)",
                        command.power_weapons
                    )
                };
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
                **t = if view.0 == Some(StationView::Scanner) {
                    format!(
                        "SCANNER — LIVE\nrange {mode} · power {}\n\
                         T pulse · ENTER toggle range\nwalk away to stand down",
                        command.power_sensors
                    )
                } else {
                    format!(
                        "SCANNER\nrange {mode}\npower {}\nENTER: toggle range\n(fly: T to pulse)",
                        command.power_sensors
                    )
                };
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
                **t = if view.0 == Some(StationView::Miner) {
                    format!(
                        "MINER — LIVE\nrig {state} · ore {}\n\
                         hold G to run the beam · ENTER toggle\nwalk away to stand down",
                        systems.ore
                    )
                } else {
                    format!(
                        "MINER\nrig {state}\nore {}\nENTER: toggle\n(fly: hold G)",
                        systems.ore
                    )
                };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_views_open_only_on_board_in_flight() {
        // In flight, undocked, space alive: the three flight consoles open
        // their live views.
        for (panel, expect) in [
            (ActivePanel::Gunner, StationView::Gunner),
            (ActivePanel::Scanner, StationView::Scanner),
            (ActivePanel::Miner, StationView::Miner),
        ] {
            assert_eq!(
                station_view(GameMode::OnBoard, &panel, false, true),
                Some(expect)
            );
        }
    }

    #[test]
    fn station_views_stay_closed_everywhere_else() {
        let g = ActivePanel::Gunner;
        // Docked: config panel only, no live outside to show.
        assert_eq!(station_view(GameMode::OnBoard, &g, true, true), None);
        // Space scene torn down (docked boarding path): no view.
        assert_eq!(station_view(GameMode::OnBoard, &g, false, false), None);
        // Not on board.
        assert_eq!(station_view(GameMode::Landed, &g, false, true), None);
        assert_eq!(station_view(GameMode::SpaceFlight, &g, false, true), None);
        // Non-flight consoles never open a view.
        for panel in [
            ActivePanel::None,
            ActivePanel::Helm,
            ActivePanel::Engineering,
            ActivePanel::Nav,
            ActivePanel::Power,
        ] {
            assert_eq!(station_view(GameMode::OnBoard, &panel, false, true), None);
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
    souls: Res<crate::systems::soul::SoulRegistry>,
    mut plan: ResMut<crate::systems::cryojump::JumpPlan>,
    transit: Res<crate::systems::jump::TransitState>,
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
                // S09e: the jump-cryo loop starts here (SHIPS.md §3 step 1).
                // J programs + arms a self-generated jump; the window opens
                // on a clock and every human must be in a pod first.
                if keys.just_pressed(KeyCode::KeyJ) && !location.is_docked {
                    crate::systems::cryojump::arm_jump(
                        &mut plan,
                        &transit,
                        location.system_seed,
                        &mut roster,
                        &mut log,
                    );
                }
                let jump_line = match &plan.armed {
                    Some(armed) => format!(
                        "JUMP ARMED → {:#x} · window opens in {:.0}s\n{}",
                        armed.dest_seed,
                        (armed.window.duration().as_secs_f32() - armed.window.elapsed_secs())
                            .max(0.0),
                        if plan.player_in_pod {
                            "you are in a pod"
                        } else {
                            "GET TO A CRYO POD"
                        }
                    ),
                    None if location.is_docked => "J: self-jump (undock first)".to_string(),
                    None => "J: program + arm self-jump (humans must reach cryo)".to_string(),
                };
                let mut news_lines = vec![format!(
                    "NAV · system {:#x}\ntick {}\n{jump_line}\n\n── GALACTIC NEWS ──",
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
                // S13: the inspect block — public bio, visible mood, standing
                // with the player. Secrets stay hidden until unlocked.
                let mut s = match crate::systems::soul::inspect_text(&souls, &id) {
                    Some(inspect) => format!("{inspect}\n\n"),
                    None => String::new(),
                };
                s.push_str(&format!("ORDER {}:\n", m.name));
                for (i, room) in ORDER_ROOMS.iter().enumerate() {
                    let cur = m.order == Some(*room);
                    s.push_str(&format!(
                        "  {}. {room:?}{}\n",
                        i + 1,
                        if cur { " *" } else { "" }
                    ));
                }
                s.push_str("press 1-5 to set · 0 to clear · T talk");
                **t = s;
                // S16: talking is the other half of the crew surface — T
                // hands this figure to the dialogue session (soul-backed).
                let target = *e;
                if keys.just_pressed(KeyCode::KeyT) {
                    *panel = ActivePanel::Dialogue(target);
                    return;
                }
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
