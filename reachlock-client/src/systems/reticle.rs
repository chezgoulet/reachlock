//! SF64-style dual aiming reticle (spec §14 Mode 3, §22): a near cross and a
//! far bracket projected ahead of the hull onto the screen, so the player
//! reads exactly where the nose — and therefore the guns — are pointing while
//! the chase-cam swings. Pure HUD: two UI text nodes repositioned each frame
//! via `Camera::world_to_viewport`; nothing here touches gameplay state.

use bevy::prelude::*;

use crate::states::GameMode;
use crate::systems::ship::{PlayerShip, ShipSystems, SpaceCamera};

/// Distances ahead of the nose the two reticles sit at (world units). The far
/// one leads the near one, SF64-style, so the pair reads as an aiming axis.
const NEAR_DIST: f32 = 110.0;
const FAR_DIST: f32 = 260.0;

#[derive(Component)]
pub struct ReticleNear;

#[derive(Component)]
pub struct ReticleFar;

/// Spawns the two reticle nodes, hidden until the first flight frame places
/// them. Spawned once with the HUD; visibility tracks the mode per frame.
pub fn spawn_reticle(mut commands: Commands) {
    commands.spawn((
        ReticleNear,
        Text::new("[ ]"),
        TextFont {
            font_size: 26.0,
            ..default()
        },
        TextColor(Color::srgba(0.4, 1.0, 0.6, 0.9)),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        Visibility::Hidden,
    ));
    commands.spawn((
        ReticleFar,
        Text::new("+"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextColor(Color::srgba(0.4, 1.0, 0.6, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Project one reticle point through the space camera onto the viewport and
/// park the node there (centered by `half`), hiding it when the point is
/// behind the camera or off-target.
fn place(
    camera: &Camera,
    camera_tx: &GlobalTransform,
    world: Vec3,
    half: Vec2,
    node: &mut Node,
    vis: &mut Visibility,
) {
    match camera.world_to_viewport(camera_tx, world) {
        Ok(v) => {
            node.left = Val::Px(v.x - half.x);
            node.top = Val::Px(v.y - half.y);
            *vis = Visibility::Visible;
        }
        Err(_) => *vis = Visibility::Hidden,
    }
}

/// Reposition both reticles each frame along the hull's forward axis. Runs in
/// every InGame mode so leaving SpaceFlight (or dying) hides them.
#[allow(clippy::type_complexity)]
pub fn update_reticle(
    mode: Res<State<GameMode>>,
    systems: Option<Res<ShipSystems>>,
    ship: Query<&Transform, With<PlayerShip>>,
    camera: Query<(&Camera, &GlobalTransform), With<SpaceCamera>>,
    mut nodes: ParamSet<(
        Query<(&mut Node, &mut Visibility), With<ReticleNear>>,
        Query<(&mut Node, &mut Visibility), With<ReticleFar>>,
    )>,
) {
    let flying = *mode == GameMode::SpaceFlight && systems.is_some_and(|s| !s.dead);
    let aim = match (ship.single(), camera.single()) {
        (Ok(ship), Ok((camera, camera_tx))) if flying && camera.is_active => {
            let forward = ship.forward().as_vec3();
            Some((
                camera,
                camera_tx,
                ship.translation + forward * NEAR_DIST,
                ship.translation + forward * FAR_DIST,
            ))
        }
        _ => None,
    };

    if let Ok((mut node, mut vis)) = nodes.p0().single_mut() {
        match aim {
            Some((camera, cam_tx, near, _)) => {
                place(
                    camera,
                    cam_tx,
                    near,
                    Vec2::new(19.0, 15.0),
                    &mut node,
                    &mut vis,
                );
            }
            None => *vis = Visibility::Hidden,
        }
    }
    if let Ok((mut node, mut vis)) = nodes.p1().single_mut() {
        match aim {
            Some((camera, cam_tx, _, far)) => {
                place(
                    camera,
                    cam_tx,
                    far,
                    Vec2::new(6.0, 11.0),
                    &mut node,
                    &mut vis,
                );
            }
            None => *vis = Visibility::Hidden,
        }
    }
}
