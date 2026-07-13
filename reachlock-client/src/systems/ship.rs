//! Ship control in 3D (spec §14 Mode 3, "Space Flight — Star Fox 64"):
//! full 6-DOF flight — roll/pitch/yaw + thrust/brake/boost — driven through a
//! rapier3d rigid body in zero gravity, with a chase-cam that trails the hull.
//!
//! Cross-mode coupling (spec §22): the OnBoard consoles (gunner/scanner/miner/
//! power) don't fly the ship — they *configure* it. What they set on the
//! [`ShipCommand`] bus (weapons armed, mining rig enabled, sensor mode, power
//! routing) changes what the ship can do and how it looks here in flight. In a
//! single-player session the pilot still pulls the trigger, but only the
//! systems the crew powered and armed at their consoles respond.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use reachlock_core::generator::hull::{HullClass, HullHandling};
use reachlock_core::util::rng::Fixed;

use crate::states::GameMode;

/// Seed for the player ship's handling profile. Independent of the interior
/// layout seed; both are "the ship you fly", S17 will unify them.
const PLAYER_HULL_SEED: u64 = 0x5EED_0001;

/// Max power notches per subsystem, and the total budget the power console may
/// distribute across weapons/engines/sensors (spec §22 power management).
pub const POWER_MAX_NOTCH: u8 = 3;
pub const POWER_BUDGET: u8 = 5;

#[derive(Component)]
pub struct PlayerShip;

/// The 3D chase-cam used only in SpaceFlight. Spawned in `menu.rs`; activated
/// by [`manage_cameras`].
#[derive(Component)]
pub struct SpaceCamera;

/// A weapon bolt fired by the guns. Render-layer combat (rapier/floats), not
/// contract state — it lives and dies inside the flight scene.
#[derive(Component)]
pub struct Projectile {
    pub life: Timer,
}

/// The mining beam visual, re-aimed each frame while the beam is engaged.
#[derive(Component)]
pub struct MiningBeam;

/// The expanding scanner-pulse ring, shrinking-in-opacity as it grows.
#[derive(Component)]
pub struct ScanPulse {
    pub age: Timer,
}

/// Fixed-point ship state — the game state contracts see, so it obeys spec §5
/// (no floats in gameplay values). Fuel: 1024 = full tank.
#[derive(Resource)]
pub struct ShipSystems {
    pub fuel: Fixed,
    pub thrusting: bool,
    /// Set by the anomaly key (X): a situation no rule covers.
    pub unknown_signal: bool,
    /// Sensor range in world units × 1024 (fixed-point). Default 400 units.
    pub sensor_range: Fixed,
    /// Ore collected by the mining rig (integer units — cargo, contract state).
    pub ore: i64,
    /// Real-time gun cooldown (render-layer flight feel, seconds).
    pub gun_cooldown: f32,
}

impl Default for ShipSystems {
    fn default() -> Self {
        ShipSystems {
            fuel: Fixed(1024),
            thrusting: false,
            unknown_signal: false,
            sensor_range: Fixed(400 * 1024),
            ore: 0,
            gun_cooldown: 0.0,
        }
    }
}

/// Cross-mode command bus (spec §22). OnBoard consoles write it; the flight
/// systems read it. Persists across mode switches (it's a plain resource, not
/// `ModeScope`) so a routing you set at the power console stays set when you
/// return to the helm.
#[derive(Resource)]
pub struct ShipCommand {
    /// Gunner console: guns are armed and will fire on the trigger.
    pub weapons_armed: bool,
    /// Miner console: the mining rig is installed/enabled.
    pub mining_enabled: bool,
    /// Scanner console: long-range sensor sweep mode.
    pub scanner_boost: bool,
    /// Power routing notches (0..=POWER_MAX_NOTCH), summing to <= POWER_BUDGET.
    pub power_weapons: u8,
    pub power_engines: u8,
    pub power_sensors: u8,
    /// Edge flag set by the scanner console / flight key: emit one pulse.
    pub scan_pulse: bool,
}

impl Default for ShipCommand {
    fn default() -> Self {
        ShipCommand {
            weapons_armed: true,
            mining_enabled: false,
            scanner_boost: false,
            power_weapons: 1,
            power_engines: 2,
            power_sensors: 1,
            scan_pulse: false,
        }
    }
}

/// 6-DOF control: roll (Q/E), pitch (W/S), yaw (A/D), thrust (Space), boost
/// (Shift), brake (Ctrl). Rotation is set directly on the angular velocity for
/// crisp arcade handling; thrust is a force along the hull's forward axis. Fuel
/// burns while thrusting. Engine power (set at the power console) scales thrust
/// and turn rate.
#[allow(clippy::type_complexity)]
pub fn control(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut systems: ResMut<ShipSystems>,
    command: Res<ShipCommand>,
    mut query: Query<(&Transform, &mut Velocity, &mut ExternalForce), With<PlayerShip>>,
) {
    let Ok((transform, mut velocity, mut force)) = query.single_mut() else {
        return;
    };

    let h = HullHandling::for_class(PLAYER_HULL_SEED, HullClass::Corvette);
    // Engine power routes into how hard the hull thrusts and turns.
    let engine_mult = 0.5 + 0.25 * command.power_engines as f32;

    let has_fuel = systems.fuel.0 > 0;
    let thrust_key = keys.pressed(KeyCode::Space);
    let boost = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let brake = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    systems.thrusting = thrust_key && has_fuel;

    force.force = Vec3::ZERO;
    force.torque = Vec3::ZERO;

    // --- rotation (crisp, direct angular velocity about local axes) ---
    let turn = HullHandling::f32(h.turn_rate).clamp(0.0, 40.0) * 0.08 * engine_mult;
    let mut pitch = 0.0;
    let mut yaw = 0.0;
    let mut roll = 0.0;
    if keys.pressed(KeyCode::KeyW) {
        pitch -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        pitch += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        yaw += 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        yaw -= 1.0;
    }
    if keys.pressed(KeyCode::KeyQ) {
        roll += 1.0;
    }
    if keys.pressed(KeyCode::KeyE) {
        roll -= 1.0;
    }
    let local = Vec3::new(pitch, yaw, roll);
    if local != Vec3::ZERO {
        // Local pitch=X, yaw=Y, roll=Z → world angular velocity.
        velocity.angular = transform.rotation * (local.normalize() * turn);
    } else {
        velocity.angular = Vec3::ZERO;
    }

    // --- translation ---
    let forward = transform.forward().as_vec3();
    if systems.thrusting {
        let mut mag = HullHandling::f32(h.thrust) * engine_mult;
        let mut burn = h.fuel_burn;
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
    if brake {
        // Bleed velocity toward zero (retro-thrust); coasting otherwise.
        velocity.linear *= 0.92;
        if has_fuel {
            let dt = time.delta().as_millis() as i64;
            let used = (dt * (h.fuel_burn / 2).max(1)) / 1000;
            systems.fuel = Fixed((systems.fuel.0 - used.max(1)).max(0));
        }
    }

    // X injects the anomaly: something the player's rules don't cover.
    if keys.just_pressed(KeyCode::KeyX) {
        systems.unknown_signal = true;
    }
}

/// Requests a scanner pulse from the flight key `T`. Split from `control` so
/// the mutable `ShipCommand` borrow doesn't collide with the read in `control`.
pub fn request_scan_from_key(keys: Res<ButtonInput<KeyCode>>, mut command: ResMut<ShipCommand>) {
    if keys.just_pressed(KeyCode::KeyT) {
        command.scan_pulse = true;
    }
}

/// Chase-cam: trail the ship from behind and above, easing toward the target
/// pose each frame so hard turns read as a swing, not a snap (Star Fox feel).
pub fn camera_follow(
    time: Res<Time>,
    ship: Query<&Transform, (With<PlayerShip>, Without<SpaceCamera>)>,
    mut camera: Query<&mut Transform, With<SpaceCamera>>,
) {
    let (Ok(ship), Ok(mut camera)) = (ship.single(), camera.single_mut()) else {
        return;
    };
    let back = ship.rotation * Vec3::new(0.0, 6.0, 22.0);
    let target = ship.translation + back;
    let t = (time.delta_secs() * 6.0).clamp(0.0, 1.0);
    camera.translation = camera.translation.lerp(target, t);
    let look = ship.translation + ship.forward().as_vec3() * 30.0;
    let desired =
        Transform::from_translation(camera.translation).looking_at(look, ship.rotation * Vec3::Y);
    camera.rotation = camera.rotation.slerp(desired.rotation, t);
}

/// The flying ship is only meaningful in `SpaceFlight`; hide it in interiors.
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

/// Activate the 3D chase-cam in SpaceFlight and the 2D camera everywhere else.
/// In space the 2D camera keeps rendering the HUD on top (its clear is turned
/// off so it overlays the flight view); in interiors it clears and draws the
/// top-down scene.
pub fn manage_cameras(
    mode: Res<State<GameMode>>,
    mut space_cam: Query<&mut Camera, (With<SpaceCamera>, Without<Camera2d>)>,
    mut ui_cam: Query<&mut Camera, (With<Camera2d>, Without<SpaceCamera>)>,
) {
    let in_space = *mode == GameMode::SpaceFlight;
    if let Ok(mut cam) = space_cam.single_mut() {
        cam.is_active = in_space;
    }
    if let Ok(mut cam) = ui_cam.single_mut() {
        cam.clear_color = if in_space {
            ClearColorConfig::None
        } else {
            ClearColorConfig::Custom(Color::srgb(0.02, 0.02, 0.05))
        };
    }
}

/// Fire the guns on `F` — but only if the gunner armed them and the power
/// console routed power to weapons. Cooldown (rate of fire) scales with weapon
/// power. Bolts are emissive spheres launched along the hull's forward axis.
#[allow(clippy::too_many_arguments)]
pub fn fire_weapons(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut systems: ResMut<ShipSystems>,
    command: Res<ShipCommand>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ship: Query<&Transform, With<PlayerShip>>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
) {
    systems.gun_cooldown = (systems.gun_cooldown - time.delta_secs()).max(0.0);
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Ok(ship) = ship.single() else { return };
    if !command.weapons_armed || command.power_weapons == 0 {
        log.log("Weapons offline — arm at the gunner console and route power.");
        return;
    }
    if systems.gun_cooldown > 0.0 {
        return;
    }
    systems.gun_cooldown = 0.6 / (command.power_weapons as f32);
    let forward = ship.forward().as_vec3();
    let muzzle = ship.translation + forward * 6.0;
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.7, 0.2),
            emissive: LinearRgba::rgb(3.0, 1.5, 0.2),
            ..default()
        })),
        Transform::from_translation(muzzle),
        RigidBody::KinematicVelocityBased,
        Velocity {
            linear: forward * 220.0,
            angular: Vec3::ZERO,
        },
        Collider::ball(0.6),
        Sensor,
        Projectile {
            life: Timer::from_seconds(2.0, TimerMode::Once),
        },
        crate::states::ModeScope(GameMode::SpaceFlight),
    ));
}

/// Advance bolts and expire them.
pub fn step_projectiles(
    time: Res<Time>,
    mut commands: Commands,
    mut bolts: Query<(Entity, &mut Projectile)>,
) {
    for (e, mut p) in &mut bolts {
        if p.life.tick(time.delta()).is_finished() {
            commands.entity(e).despawn();
        }
    }
}

/// Mining beam: while `G` is held and the rig is enabled, draw a beam from the
/// nose and accrue ore. Purely a flight-scene effect; ore is cargo state.
#[allow(clippy::too_many_arguments)]
pub fn mining_beam(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    command: Res<ShipCommand>,
    mut systems: ResMut<ShipSystems>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ship: Query<&Transform, With<PlayerShip>>,
    beams: Query<Entity, With<MiningBeam>>,
    mut inv: ResMut<crate::systems::inventory::PlayerInventory>,
) {
    let active = keys.pressed(KeyCode::KeyG) && command.mining_enabled;
    // Rebuild the beam each frame so it tracks the hull.
    for e in &beams {
        commands.entity(e).despawn();
    }
    if !active {
        return;
    }
    let Ok(ship) = ship.single() else { return };
    let forward = ship.forward().as_vec3();
    let mid = ship.translation + forward * 20.0;
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.3, 36.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.4, 1.0, 0.8, 0.6),
            emissive: LinearRgba::rgb(0.3, 2.0, 1.2),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(mid)
            .looking_at(mid + forward, Vec3::Y)
            .mul_transform(Transform::from_rotation(Quat::from_rotation_x(
                std::f32::consts::FRAC_PI_2,
            ))),
        MiningBeam,
        crate::states::ModeScope(GameMode::SpaceFlight),
    ));
    // Accrue ore at a steady tick while mining (integer cargo).
    let gained = (time.delta_secs() * 10.0) as i64;
    if gained > 0 {
        systems.ore += gained;
        inv.credits += gained; // ore sells 1:1 for the slice
    }
}

/// Emit an expanding scanner ring when a pulse is requested (scanner console or
/// the `T` key) and marks nearby contacts known via `KnownContacts`. The ring
/// reach scales with sensor power and the console's long-range mode.
#[allow(clippy::too_many_arguments)]
pub fn scanner_pulse(
    time: Res<Time>,
    mut commands: Commands,
    mut command: ResMut<ShipCommand>,
    systems: Res<ShipSystems>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ship: Query<&Transform, With<PlayerShip>>,
    contacts: Query<(Entity, &Transform), With<crate::systems::sensors::Contact>>,
    mut known: ResMut<crate::systems::sensors::KnownContacts>,
    mut pulses: Query<(Entity, &mut ScanPulse, &mut Transform), Without<PlayerShip>>,
) {
    // Grow / fade existing pulses.
    for (e, mut pulse, mut tx) in &mut pulses {
        let f = pulse.age.fraction();
        tx.scale = Vec3::splat(1.0 + f * 40.0);
        if pulse.age.tick(time.delta()).is_finished() {
            commands.entity(e).despawn();
        }
    }
    if !command.scan_pulse {
        return;
    }
    command.scan_pulse = false;
    let Ok(ship) = ship.single() else { return };
    let base = systems.sensor_range.0 as f32 / 1024.0;
    let power = 1.0 + 0.5 * command.power_sensors as f32;
    let boost = if command.scanner_boost { 1.8 } else { 1.0 };
    let range = base * power * boost;

    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.4, 0.8, 1.0, 0.25),
            emissive: LinearRgba::rgb(0.2, 0.6, 1.2),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(ship.translation),
        ScanPulse {
            age: Timer::from_seconds(1.2, TimerMode::Once),
        },
        crate::states::ModeScope(GameMode::SpaceFlight),
    ));
    for (e, t) in &contacts {
        if t.translation.distance(ship.translation) <= range {
            known.known.insert(e);
        }
    }
}
