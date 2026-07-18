//! Galaxy map (S21): an overlay screen showing charted systems, gate edges,
//! the player's current system, and discovered deep-space systems. Toggled
//! with G in the flight scene. Read-only in S21 — FTL to coordinates and
//! gate selection are separate UI panels.

use bevy::prelude::*;

use crate::states::{CurrentLocation, GameMode};
use crate::systems::content_index::ContentIndex;

/// Marker for the galaxy map overlay entity.
#[derive(Component)]
pub struct GalaxyMapOverlay;

/// Toggle the galaxy map overlay with G.
pub fn galaxy_map_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<State<GameMode>>,
    mut commands: Commands,
    overlay: Query<Entity, With<GalaxyMapOverlay>>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    if **mode != GameMode::SpaceFlight {
        return;
    }
    if let Ok(entity) = overlay.single() {
        commands.entity(entity).despawn();
    } else {
        commands.spawn((
            GalaxyMapOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            Visibility::default(),
        ));
    }
}

/// Render the galaxy map: charted system nodes, gate edges, player position.
pub fn render_galaxy_map(
    content: Res<ContentIndex>,
    location: Res<CurrentLocation>,
    overlay: Query<&Node, With<GalaxyMapOverlay>>,
    mut gizmos: Gizmos,
) {
    if overlay.single().is_err() {
        return;
    }
    // Only render if the gate network and charted systems are loaded.
    let Some(network) = content.gate_network.as_ref() else {
        return;
    };
    if content.charted_systems.is_empty() {
        return;
    }

    // Map coordinate bounds for centering. Charted systems range from
    // earth (-1200?) to fringe (1100?) — normalize to a viewport.
    let all_positions: Vec<_> = content
        .charted_systems
        .values()
        .map(|s| s.position)
        .collect();
    let min_x = all_positions.iter().map(|p| p.x).min().unwrap_or(-2000);
    let max_x = all_positions.iter().map(|p| p.x).max().unwrap_or(2000);
    let min_y = all_positions.iter().map(|p| p.y).min().unwrap_or(-2000);
    let max_y = all_positions.iter().map(|p| p.y).max().unwrap_or(2000);
    let span_x = (max_x - min_x).max(1) as f32;
    let span_y = (max_y - min_y).max(1) as f32;

    let center_x = 960.0; // half of 1920
    let center_y = 540.0; // half of 1080
    let scale = 600.0 / span_x.max(span_y); // fit within ~600px

    let to_screen = |x: i64, y: i64| -> Vec2 {
        Vec2::new(
            center_x + (x - min_x) as f32 * scale,
            center_y - (y - min_y) as f32 * scale, // y-axis inverted
        )
    };

    // Draw gate edges first (behind nodes).
    for gate in &network.gates {
        let Some(from_sys) = content.charted_systems.get(&gate.from.0) else {
            continue;
        };
        let Some(to_sys) = content.charted_systems.get(&gate.to.0) else {
            continue;
        };
        let from_pos = to_screen(from_sys.position.x, from_sys.position.y);
        let to_pos = to_screen(to_sys.position.x, to_sys.position.y);
        let color = match gate.status {
            reachlock_core::galaxy::GateStatus::Active => Color::srgb(0.3, 0.8, 0.3),
            reachlock_core::galaxy::GateStatus::Blockaded => Color::srgb(0.9, 0.2, 0.2),
            reachlock_core::galaxy::GateStatus::Restricted => Color::srgb(0.9, 0.7, 0.1),
            reachlock_core::galaxy::GateStatus::Contested => Color::srgb(0.9, 0.5, 0.0),
            reachlock_core::galaxy::GateStatus::Destroyed => Color::srgb(0.4, 0.4, 0.4),
        };
        gizmos.line_2d(from_pos, to_pos, color);
    }

    // Draw charted system nodes.
    for system in content.charted_systems.values() {
        let pos = to_screen(system.position.x, system.position.y);
        let is_current = location.system_id.0 == system.id;
        let color = if is_current {
            Color::srgb(0.2, 0.8, 1.0)
        } else {
            Color::srgb(0.6, 0.6, 0.8)
        };
        gizmos.circle_2d(pos, if is_current { 8.0 } else { 5.0 }, color);
    }
}
