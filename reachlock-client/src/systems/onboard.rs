//! On-Board consoles + crew orders (spec §14 Mode 2; S08). Consoles are
//! plain `Interactable`s dropped by `systems/interior.rs`; this module owns
//! the panel text entities, the per-frame renderer, and the input that takes
//! the helm, vents fuel, or issues a crew order. All per S08's "ship is a
//! place" outcome.

use bevy::ecs::query::{Has, Or, With};
use bevy::prelude::*;

use reachlock_core::util::rng::Fixed;

use crate::states::{CurrentLocation, GameMode};
use crate::systems::contract::ShipLog;
use crate::systems::crew::{CrewFigure, CrewRoster, ORDER_ROOMS};
use crate::systems::interaction::ActivePanel;
use crate::systems::ship::ShipSystems;

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
    crew_figs: Query<&CrewFigure>,
    // One `&mut Text` query for every onboard panel. A single mutable-text
    // query (rather than five) avoids the intra-system B0001 Bevy 0.18 raises
    // when several `&mut Text` queries share a schedule. The panel kind is
    // read off each entity's marker components via `Has<_>`.
    mut texts: Query<
        (
            &mut Text,
            Has<HelmPanel>,
            Has<EngPanel>,
            Has<NavPanel>,
            Has<LogPanel>,
            Has<OrderPanel>,
        ),
        Or<(
            With<HelmPanel>,
            With<EngPanel>,
            With<NavPanel>,
            With<LogPanel>,
            With<OrderPanel>,
        )>,
    >,
) {
    for (mut t, is_helm, is_eng, is_nav, is_log, is_order) in &mut texts {
        if is_helm {
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
        } else if is_eng {
            match &*panel {
                ActivePanel::Engineering => {
                    let pct = systems.fuel.0 * 100 / 1024;
                    **t = format!("ENGINEERING\nfuel {pct}%\nPress V to vent/refill (debug)");
                    if keys.just_pressed(KeyCode::KeyV) {
                        systems.fuel = Fixed(1024);
                        log.log("Engineering: refilled fuel (debug)");
                    }
                }
                _ => **t = String::new(),
            }
        } else if is_nav {
            match &*panel {
                ActivePanel::Nav => {
                    **t = format!(
                        "NAV\nsystem {:#x}\nmap: press M in flight",
                        location.system_seed
                    );
                }
                _ => **t = String::new(),
            }
        } else if is_log {
            match &*panel {
                ActivePanel::Log => **t = log.entries.join("\n"),
                _ => **t = String::new(),
            }
        } else if is_order {
            match &*panel {
                ActivePanel::Order(e) => {
                    let id = crew_figs.get(*e).ok().map(|f| f.0.clone());
                    let Some(id) = id else {
                        **t = String::new();
                        continue;
                    };
                    let Some(m) = roster.by_id(&id) else {
                        **t = String::new();
                        continue;
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
}
