//! HUD: fuel gauge, ship's log, the deliberation overlay ("Boris is
//! considering the situation…" — spec §6 deliberation UX), and (S02) the
//! OFFLINE badge that appears whenever online mode has no live connection —
//! the game keeps playing locally regardless (iron rule #3).

use bevy::prelude::*;

use crate::net::{ConnectionState, NetMode};
use crate::systems::contract::{DeliberationState, ShipLog};
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
            font_size: 20.0,
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
        Text::new("W/↑ thrust · A/D turn · X inject anomaly"),
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
}

// Bevy query filters are inherently type-heavy, and the S02 badge adds a
// fifth Res param on top of the pre-existing four; the standard allowance.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn update_hud(
    systems: Res<ShipSystems>,
    log: Res<ShipLog>,
    deliberation: Res<DeliberationState>,
    mode: Res<NetMode>,
    conn: Res<ConnectionState>,
    mut fuel: Query<
        &mut Text,
        (
            With<FuelReadout>,
            Without<LogReadout>,
            Without<DeliberationOverlay>,
            Without<OfflineBadge>,
        ),
    >,
    mut log_text: Query<
        &mut Text,
        (
            With<LogReadout>,
            Without<FuelReadout>,
            Without<DeliberationOverlay>,
            Without<OfflineBadge>,
        ),
    >,
    mut overlay: Query<
        &mut Text,
        (
            With<DeliberationOverlay>,
            Without<FuelReadout>,
            Without<LogReadout>,
            Without<OfflineBadge>,
        ),
    >,
    mut badge: Query<
        &mut Text,
        (
            With<OfflineBadge>,
            Without<FuelReadout>,
            Without<LogReadout>,
            Without<DeliberationOverlay>,
        ),
    >,
) {
    if let Ok(mut text) = fuel.single_mut() {
        let pct = systems.fuel.0 * 100 / 1024;
        **text = format!("FUEL {pct}%{}", if systems.thrusting { " ▲" } else { "" });
    }
    if let Ok(mut text) = log_text.single_mut() {
        **text = log.entries.join("\n");
    }
    if let Ok(mut text) = overlay.single_mut() {
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
    if let Ok(mut text) = badge.single_mut() {
        // Offline mode is the normal default — no badge. Online mode shows
        // OFFLINE whenever the socket isn't actually Connected (still
        // connecting, or dropped and retrying): the game keeps playing
        // locally either way (iron rule #3).
        **text = match (&*mode, &*conn) {
            (crate::net::NetMode::Online { .. }, ConnectionState::Connected) => String::new(),
            (crate::net::NetMode::Online { .. }, _) => "OFFLINE".to_string(),
            (crate::net::NetMode::Offline, _) => String::new(),
        };
    }
}
