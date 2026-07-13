//! Ship control: thrust and rotation drive the rapier body; fuel burns
//! while thrusting. Camera follows the ship.

use bevy::prelude::*;
use bevy_rapier2d::prelude::*;
use reachlock_core::generator::hull::{HullClass, HullHandling};
use reachlock_core::util::rng::Fixed;

use crate::states::GameMode;

/// Seed for the player ship's handling profile. Independent of the interior
/// layout seed; both are "the ship you fly", S17 will unify them.
const PLAYER_HULL_SEED: u64 = 0x5EED_0001;

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
    /// Sensor range in world units × 1024 (fixed-point). Default 400 units.
    pub sensor_range: Fixed,
}

impl Default for ShipSystems {
    fn default() -> Self {
        ShipSystems {
            fuel: Fixed(1024),
            thrusting: false,
            unknown_signal: false,
            sensor_range: Fixed(400 * 1024),
        }
    }
}

pub fn control(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut systems: ResMut<ShipSystems>,
    mut query: Query<(&Transform, &mut ExternalForce, &mut Damping), With<PlayerShip>>,
) {
    let Ok((transform, mut force, mut damping)) = query.single_mut() else {
        return;
    };

    // S09: handling comes from the hull profile, not a fixed force. The
    // corvette must feel different from a freighter in hand.
    let h = HullHandling::for_class(PLAYER_HULL_SEED, HullClass::Corvette);

    let thrust_key = keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp);
    let boost = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let brake = keys.pressed(KeyCode::Space);
    let has_fuel = systems.fuel.0 > 0;
    systems.thrusting = thrust_key && has_fuel;

    force.force = Vec2::ZERO;
    force.torque = 0.0;
    // Drift damping is a hull property: heavier hulls coast further.
    damping.linear_damping = HullHandling::f32(h.drift_damping).clamp(0.0, 100.0);

    if systems.thrusting {
        let forward = (transform.rotation * Vec3::X).truncate();
        let mut mag = HullHandling::f32(h.thrust);
        let mut burn = h.fuel_burn; // integer units/sec at cruise
        if boost {
            let bm = HullHandling::f32(h.boost_mult);
            mag *= bm;
            burn = ((burn as f32) * bm) as i64;
        }
        force.force = forward * mag;
        let dt = time.delta().as_millis() as i64;
        let used = (dt * burn).max(1000) / 1000;
        systems.fuel = Fixed((systems.fuel.0 - used.max(1)).max(0));
    }
    if brake && has_fuel {
        // Brake: reverse thrust + a little burn, so "stop" costs fuel.
        let backward = -(transform.rotation * Vec3::X).truncate();
        force.force += backward * (HullHandling::f32(h.thrust) * 0.5);
        let dt = time.delta().as_millis() as i64;
        let used = (dt * (h.fuel_burn / 2).max(1)) / 1000;
        systems.fuel = Fixed((systems.fuel.0 - used.max(1)).max(0));
    }

    let torque = HullHandling::f32(h.turn_rate);
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        force.torque = torque;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        force.torque = -torque;
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

/// The flying ship is only meaningful in `SpaceFlight`; hide it while in an
/// interior or paused-in-interior so it doesn't float in the background of
/// the top-down view. (It is never despawned, so its transform survives the
/// loop — see `systems/setup.rs`.)
pub fn sync_ship_visibility(
    mode: Res<State<GameMode>>,
    mut ship: Query<&mut Visibility, With<PlayerShip>>,
) {
    let Ok(mut visibility) = ship.single_mut() else {
        return;
    };
    *visibility = if *mode == GameMode::SpaceFlight {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}
