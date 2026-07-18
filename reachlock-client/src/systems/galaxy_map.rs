//! Galaxy map (S21): an overlay screen showing charted systems and gate edges
//! in the gate network. Toggled with G in the flight scene. Left-click on
//! empty space to set an FTL route target, then press J to jump to deep space
//! at those coordinates.

use bevy::prelude::*;

use reachlock_core::galaxy::GalaxyCoord;

use crate::states::{CurrentLocation, GameMode};
use crate::systems::content_index::ContentIndex;
use crate::systems::jump::FtlRoute;

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

/// Hand-compiled projection parameters for screen ↔ galaxy coord conversion.
struct MapProjection {
    min_x: i64,
    min_y: i64,
    scale: f32,
    center_x: f32,
    center_y: f32,
}

impl MapProjection {
    fn compute(content: &ContentIndex, window: &Window) -> Option<Self> {
        let positions: Vec<_> = content
            .charted_systems
            .values()
            .map(|s| s.position)
            .collect();
        if positions.is_empty() {
            return None;
        }
        let min_x = positions.iter().map(|p| p.x).min().unwrap_or(-2000);
        let max_x = positions.iter().map(|p| p.x).max().unwrap_or(2000);
        let min_y = positions.iter().map(|p| p.y).min().unwrap_or(-2000);
        let max_y = positions.iter().map(|p| p.y).max().unwrap_or(2000);
        let span_x = (max_x - min_x).max(1) as f32;
        let span_y = (max_y - min_y).max(1) as f32;
        let scale = 600.0 / span_x.max(span_y);
        Some(MapProjection {
            min_x,
            min_y,
            scale,
            center_x: window.width() * 0.5,
            center_y: window.height() * 0.5,
        })
    }

    fn to_screen(&self, x: i64, y: i64) -> Vec2 {
        Vec2::new(
            self.center_x + (x - self.min_x) as f32 * self.scale,
            self.center_y - (y - self.min_y) as f32 * self.scale,
        )
    }

    fn screen_to_coord(&self, pos: Vec2) -> GalaxyCoord {
        GalaxyCoord {
            x: ((pos.x - self.center_x) / self.scale) as i64 + self.min_x,
            y: self.min_y - ((pos.y - self.center_y) / self.scale) as i64,
            z: 0,
        }
    }

    fn screen_dist(&self, screen: Vec2, coord: GalaxyCoord) -> f32 {
        let sp = self.to_screen(coord.x, coord.y);
        (screen - sp).length()
    }
}

/// Left-click on empty space in the galaxy map to set an FTL route target.
pub fn galaxy_map_click(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    content: Res<ContentIndex>,
    overlay: Query<&Node, With<GalaxyMapOverlay>>,
    mut ftl: ResMut<FtlRoute>,
) {
    if overlay.single().is_err() || !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(w) = windows.single() else { return };
    let Some(proj) = MapProjection::compute(&content, w) else {
        return;
    };
    let cursor = w.cursor_position().unwrap_or_default();
    let coord = proj.screen_to_coord(cursor);

    // Don't set FTL target if clicking on a charted system (within 12px).
    let near_system = content
        .charted_systems
        .values()
        .any(|s| proj.screen_dist(cursor, s.position) < 12.0);
    if !near_system {
        ftl.coord = Some(coord);
    }
}

/// Cancel the FTL route with right-click or X key.
pub fn galaxy_map_cancel_ftl(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    overlay: Query<&Node, With<GalaxyMapOverlay>>,
    mut ftl: ResMut<FtlRoute>,
) {
    if overlay.single().is_err() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyX) || buttons.just_pressed(MouseButton::Right) {
        ftl.coord = None;
    }
}

/// Render the galaxy map: charted system nodes, gate edges, player position,
/// and the FTL route target (if set).
pub fn render_galaxy_map(
    time: Res<Time>,
    content: Res<ContentIndex>,
    location: Res<CurrentLocation>,
    ftl: Res<FtlRoute>,
    overlay: Query<&Node, With<GalaxyMapOverlay>>,
    windows: Query<&Window>,
    mut gizmos: Gizmos,
) {
    if overlay.single().is_err() {
        return;
    }
    let Some(network) = content.gate_network.as_ref() else {
        return;
    };
    let Ok(w) = windows.single() else { return };
    let Some(proj) = MapProjection::compute(&content, w) else {
        return;
    };

    // Draw gate edges first (behind nodes).
    for gate in &network.gates {
        let Some(from_sys) = content.charted_systems.get(&gate.from.0) else {
            continue;
        };
        let Some(to_sys) = content.charted_systems.get(&gate.to.0) else {
            continue;
        };
        let from_pos = proj.to_screen(from_sys.position.x, from_sys.position.y);
        let to_pos = proj.to_screen(to_sys.position.x, to_sys.position.y);
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
        let pos = proj.to_screen(system.position.x, system.position.y);
        let is_current = location.system_id.0 == system.id;
        let color = if is_current {
            Color::srgb(0.2, 0.8, 1.0)
        } else {
            Color::srgb(0.6, 0.6, 0.8)
        };
        gizmos.circle_2d(pos, if is_current { 8.0 } else { 5.0 }, color);
    }

    // Draw FTL route target marker (pulsing orange circle).
    if let Some(coord) = ftl.coord {
        let pos = proj.to_screen(coord.x, coord.y);
        let pulse = time.elapsed_secs().sin().abs() * 3.0 + 4.0;
        gizmos.circle_2d(pos, pulse, Color::srgb(1.0, 0.6, 0.0));
        gizmos.circle_2d(pos, pulse + 6.0, Color::srgba(1.0, 0.6, 0.0, 0.3));
    }
}
