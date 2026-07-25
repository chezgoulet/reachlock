//! Mode scene management (spec §14): generic scene teardown + interior
//! camera follow. The space camera (ship follow) lives in `systems/ship.rs`.
//!
//! Teardown lives in the *enter* systems (`enter_spaceflight`,
//! `enter_interior`): when entering a scene different from the one already
//! built, every `ModeScope` entity is despawned before the new scene is
//! built. Pausing is a no-op round-trip because the enter system early-outs
//! when `SceneRegistry` already holds the target mode — the underlying
//! scene is never torn down.

use bevy::prelude::*;

use crate::states::{ModeScope, SceneRegistry};

/// Tear down the SpaceFlight scene when entering Hyperspace — the space
/// entities (ModeScope) are not valid during transit and must not render or
/// participate in physics (S09 gotcha: "Hyperspace must pause rapier or
/// scope it out"). The wash overlay spawned by `jump::hyperspace_tick` is
/// NOT tagged ModeScope so it survives this teardown.
pub fn teardown_for_hyperspace(
    mut commands: Commands,
    mut registry: ResMut<SceneRegistry>,
    mode_entities: Query<Entity, With<ModeScope>>,
) {
    for entity in &mode_entities {
        commands.entity(entity).despawn();
    }
    registry.space_alive = false;
}

/// Despawn leftover scene entities when leaving `InGame` entirely (the
/// `GameMode` sub-state is dropped without firing its own `OnExit`).
pub fn teardown_on_leave_game(
    mut commands: Commands,
    mut registry: ResMut<SceneRegistry>,
    mode_entities: Query<Entity, With<ModeScope>>,
) {
    for entity in &mode_entities {
        commands.entity(entity).despawn();
    }
    registry.space_alive = false;
    registry.scene = None;
}

/// Follows the walking avatar for top-down interior modes (Landed/OnBoard):
/// eased follow with a small lookahead toward the walk direction, so moving
/// reveals where you're going instead of pinning you to dead center (the
/// same trick the space chase-cam plays). Zoom lives in
/// `ship::manage_cameras`, which owns the 2D camera's per-mode projection.
#[allow(clippy::type_complexity)]
pub fn interior_camera_follow(
    time: Res<Time>,
    avatar: Query<
        (&Transform, &crate::systems::interior::Figure),
        (With<PlayerAvatar>, Without<Camera2d>),
    >,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    let (Ok((avatar, figure)), Ok(mut camera)) = (avatar.single(), camera.single_mut()) else {
        return;
    };
    // Facing vectors indexed like pixel::DIR_* (down, up, left, right).
    const DIRS: [Vec2; 4] = [Vec2::NEG_Y, Vec2::Y, Vec2::NEG_X, Vec2::X];
    let lookahead = if figure.moving {
        DIRS[figure.dir.min(3)] * 48.0
    } else {
        Vec2::ZERO
    };
    let target = avatar.translation.truncate() + lookahead;
    let k = 1.0 - (-6.0 * time.delta_secs()).exp();
    camera.translation.x += (target.x - camera.translation.x) * k;
    camera.translation.y += (target.y - camera.translation.y) * k;
}

/// The walking player square used in Landed/On-Board modes. Distinct from the
/// flying `PlayerShip` so the two scenes never collide.
#[derive(Component)]
pub struct PlayerAvatar;
