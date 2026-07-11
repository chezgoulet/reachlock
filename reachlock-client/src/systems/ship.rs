//! Ship control: thrust and rotation drive the rapier body; fuel burns
//! while thrusting. Camera follows the ship.

use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use reachlock_core::util::rng::Fixed;

#[derive(Component)]
pub struct PlayerShip;

/// Fixed-point ship state — this is the game state contracts see, so it
/// obeys spec §5 (no floats in gameplay values). Fuel: 1024 = full tank.
#[derive(Resource)]
pub struct ShipSystems {
    pub fuel: Fixed,
    pub thrusting: bool,
    /// Set by the anomaly key (X): a situation no rule covers.
    pub unknown_signal: bool,
}

impl Default for ShipSystems {
    fn default() -> Self {
        ShipSystems {
            fuel: Fixed(1024),
            thrusting: false,
            unknown_signal: false,
        }
    }
}

const THRUST_FORCE: f32 = 90_000.0;
const TORQUE: f32 = 900_000.0;
/// Fuel burn per second of thrust, in 1/1024 units.
const BURN_PER_SEC: i64 = 20;

pub fn control(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut systems: ResMut<ShipSystems>,
    mut query: Query<(&Transform, &mut ExternalForce), With<PlayerShip>>,
) {
    let Ok((transform, mut force)) = query.single_mut() else {
        return;
    };

    let thrust_key = keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp);
    let has_fuel = systems.fuel.0 > 0;
    systems.thrusting = thrust_key && has_fuel;

    force.force = Vec2::ZERO;
    force.torque = 0.0;

    if systems.thrusting {
        let forward = (transform.rotation * Vec3::X).truncate();
        force.force = forward * THRUST_FORCE;
        // Integer burn accumulation: milliseconds * rate / 1000.
        let burn = (time.delta().as_millis() as i64 * BURN_PER_SEC) / 1000;
        systems.fuel = Fixed((systems.fuel.0 - burn.max(1)).max(0));
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        force.torque = TORQUE;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        force.torque = -TORQUE;
    }

    // X injects the anomaly: something the player's rules don't cover.
    if keys.just_pressed(KeyCode::KeyX) {
        systems.unknown_signal = true;
    }
}

pub fn camera_follow(
    ship: Query<&Transform, (With<PlayerShip>, Without<Camera2d>)>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    let (Ok(ship), Ok(mut camera)) = (ship.single(), camera.single_mut()) else {
        return;
    };
    camera.translation.x = ship.translation.x;
    camera.translation.y = ship.translation.y;
}
