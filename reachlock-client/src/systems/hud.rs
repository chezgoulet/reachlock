//! HUD: fuel gauge, ship's log, and the deliberation overlay ("Boris is
//! considering the situation…" — spec §6 deliberation UX).

use bevy::prelude::*;

use crate::systems::contract::{DeliberationState, ShipLog};
use crate::systems::ship::ShipSystems;

#[derive(Component)]
pub struct FuelReadout;

#[derive(Component)]
pub struct LogReadout;

#[derive(Component)]
pub struct DeliberationOverlay;

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
}

// Bevy query filters are inherently type-heavy; the standard allowance.
#[allow(clippy::type_complexity)]
pub fn update_hud(
    systems: Res<ShipSystems>,
    log: Res<ShipLog>,
    deliberation: Res<DeliberationState>,
    mut fuel: Query<
        &mut Text,
        (
            With<FuelReadout>,
            Without<LogReadout>,
            Without<DeliberationOverlay>,
        ),
    >,
    mut log_text: Query<
        &mut Text,
        (
            With<LogReadout>,
            Without<FuelReadout>,
            Without<DeliberationOverlay>,
        ),
    >,
    mut overlay: Query<
        &mut Text,
        (
            With<DeliberationOverlay>,
            Without<FuelReadout>,
            Without<LogReadout>,
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
        **text = match &deliberation.active {
            Some(d) => format!(
                "⟳ {} is considering the situation…\n  \"{}. My rules don't cover this.\"",
                d.crew_member, d.context_summary
            ),
            None => String::new(),
        };
    }
}
