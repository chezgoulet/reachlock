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

use crate::states::{CurrentLocation, GameMode};

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
    /// Damage this bolt deals on impact (fixed-point units; shared by the
    /// future S19 enemy projectiles — see `Damager`).
    #[allow(dead_code)]
    pub damage: i64,
}

/// A mineable rock. `ore` is the remaining integer units; when it hits zero
/// the rock is spent (it despawns, leaving debris). This is the cargo source
/// the S10 market will later buy.
#[derive(Component)]
pub struct Asteroid {
    pub ore: i64,
}

/// Damageable health for any ship/hull in the flight scene. Generic on
/// purpose — the player ship carries one now, and S19 enemy ships will carry
/// the same component so the collision terminal below stays unchanged.
#[derive(Component)]
pub struct Hull {
    pub hp: i64,
    pub max: i64,
}

/// Marks a collider as dealing `damage` on contact (projectiles today; S19
/// kinetic impacts reuse this). `source` is a tag for the future combat log.
#[derive(Component)]
pub struct Damager {
    pub damage: i64,
    pub source: DamageSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DamageSource {
    PlayerGun,
    Ram,
}

/// The mining beam visual, re-aimed each frame while the beam is engaged.
#[derive(Component)]
pub struct MiningBeam;

/// The engine exhaust cone, a child of the ship hull. `base_z` is the local Z
/// of the hull's rear face; `length` the cone's unscaled height. The flame is
/// anchored at the rear face and stretches backward with thrust.
#[derive(Component)]
pub struct EngineExhaust {
    pub base_z: f32,
    pub length: f32,
}

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
    /// Hull integrity, fixed-point (1024 = pristine). Drained by impacts and
    /// projectile hits; zero triggers the death/respawn loop.
    pub hull_hp: Fixed,
    /// Real-time gun cooldown (render-layer flight feel, seconds).
    pub gun_cooldown: f32,
    /// True while the hull is breached and the ship is in its death/respawning
    /// beat. Suppresses flight control until `respawn_ship` rebuilds it.
    pub dead: bool,
}

impl Default for ShipSystems {
    fn default() -> Self {
        ShipSystems {
            fuel: Fixed(1024),
            thrusting: false,
            unknown_signal: false,
            sensor_range: Fixed(400 * 1024),
            ore: 0,
            hull_hp: Fixed(1024),
            gun_cooldown: 0.0,
            dead: false,
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
            weapons_armed: false,
            mining_enabled: false,
            scanner_boost: false,
            power_weapons: 1,
            power_engines: 2,
            power_sensors: 1,
            scan_pulse: false,
        }
    }
}

// --- Star Fox feel layer (spec §14 Mode 3: "cinematic feel") -----------------
// Everything below is render-layer state: floats are fine here, none of it is
// contract-visible gameplay state.

/// How fast a control axis ramps toward the held input, per second.
const AXIS_ATTACK: f32 = 9.0;
/// How fast a released axis settles back to neutral, per second.
const AXIS_RELEASE: f32 = 7.0;
/// Hull lean (radians) in a full-rate yaw turn — the SF64 bank-into-turn.
const MAX_BANK: f32 = 0.85;
/// Proportional gain driving roll toward the bank target.
const BANK_GAIN: f32 = 5.0;
/// How strongly velocity re-aligns to the nose, per second. High = the ship
/// goes where it points (arcade); low = Newtonian drift.
const GRIP: f32 = 2.2;
/// Double-tap window for triggering a barrel roll, seconds.
const BARREL_WINDOW: f32 = 0.28;
/// Spin rate during a barrel roll (rad/s) — a full roll in ~½ s.
const BARREL_RATE: f32 = 12.5;
/// Chase-cam lens: resting field of view, radians.
const BASE_FOV: f32 = 0.95;
/// Turn rate scale: hull `turn_rate` (fixed-point, corvette ≈ 0.195) →
/// rad/s. At 10.0 a corvette turns ~2 rad/s — a full circle in ~3 s.
const TURN_SCALE: f32 = 10.0;
/// Cruise speed scale: hull `thrust` (fixed-point, corvette ≈ 1.66) →
/// world units/s. At 85 a corvette cruises ~140 u/s against a system laid
/// out on a ~2500-unit radius — stations are seconds apart, not minutes.
const CRUISE_SCALE: f32 = 85.0;
/// Coasting speed decay per second (space keeps most of your momentum).
const IDLE_DECAY: f32 = 0.25;
/// Retro-thrust decay per second while braking.
const BRAKE_DECAY: f32 = 3.0;
/// Mining beam reach from ship center, world units. Must clear the hull's
/// own ~50-unit collider — the old 45 couldn't touch a rock without ramming.
const BEAM_REACH: f32 = 150.0;

/// Per-frame flight feel state: smoothed control axes, the active barrel
/// roll, and the camera's boost/brake/shake blends. Pure presentation — it
/// never feeds back into contract state.
#[derive(Resource)]
pub struct FlightFeel {
    /// Smoothed input axes in [-1, 1].
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
    /// Signed radians remaining in an active barrel roll (sign = direction);
    /// zero when not rolling.
    pub barrel: f32,
    /// Seconds since the last Q / E press, for double-tap detection.
    tap_left: f32,
    tap_right: f32,
    /// Smoothed 0..1 blends the camera and exhaust read.
    pub boost_blend: f32,
    pub brake_blend: f32,
    /// Camera shake energy; fed by hull hits, decays exponentially.
    pub shake: f32,
    /// Current flight speed (u/s), published by `control` for the camera,
    /// dust streaks, and exhaust.
    pub speed: f32,
    /// The camera's smoothed up vector — chases the hull's lean slowly, and
    /// holds still during a barrel roll so the world doesn't spin.
    pub cam_up: Vec3,
    /// Alternating muzzle side for the twin-laser fire pattern.
    muzzle_left: bool,
}

impl Default for FlightFeel {
    fn default() -> Self {
        FlightFeel {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
            barrel: 0.0,
            tap_left: f32::MAX,
            tap_right: f32::MAX,
            boost_blend: 0.0,
            brake_blend: 0.0,
            shake: 0.0,
            speed: 0.0,
            cam_up: Vec3::Y,
            muzzle_left: false,
        }
    }
}

/// Frame-rate-independent exponential approach of `current` toward `target`.
pub fn approach(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-rate * dt).exp())
}

/// Wrap an angle into `[-π, π]`.
pub fn wrap_angle(a: f32) -> f32 {
    (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Bank of `rotation` about its own forward axis relative to the world
/// horizon: 0 = wings level, positive = leaning left (right wing up), which
/// is also the direction a positive local-Z roll rate moves it.
pub fn bank_angle(rotation: Quat) -> f32 {
    let right = rotation * Vec3::X;
    let up = rotation * Vec3::Y;
    right.dot(Vec3::Y).atan2(up.dot(Vec3::Y))
}

/// Star Fox flight control (spec §14 Mode 3, §22): pitch (W/S), yaw (A/D),
/// roll (Q/E, double-tap for a barrel roll), thrust (Space), boost (Shift),
/// brake (Ctrl). Inputs are smoothed so turns ramp in and out; yaw leans the
/// hull into the turn and hands-off re-levels the horizon; velocity re-aligns
/// to the nose so the ship goes where it points. Fuel burns while thrusting.
/// Engine power (set at the power console) scales thrust and turn rate.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn control(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut systems: ResMut<ShipSystems>,
    command: Res<ShipCommand>,
    mut feel: ResMut<FlightFeel>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
    mut query: Query<(&Transform, &mut Velocity, &mut ExternalForce), With<PlayerShip>>,
) {
    let dt = time.delta_secs();
    // Double-tap timers advance even when the ship is missing so stale taps
    // age out. Saturating: the default is f32::MAX ("never tapped").
    feel.tap_left = (feel.tap_left + dt).min(f32::MAX);
    feel.tap_right = (feel.tap_right + dt).min(f32::MAX);

    let Ok((transform, mut velocity, mut force)) = query.single_mut() else {
        return;
    };

    // Breached hull: no flight control during the death/respawning beat. The
    // `respawn_ship` system clears `dead` and restores control.
    if systems.dead {
        velocity.linear = Vec3::ZERO;
        velocity.angular = Vec3::ZERO;
        force.force = Vec3::ZERO;
        force.torque = Vec3::ZERO;
        feel.pitch = 0.0;
        feel.yaw = 0.0;
        feel.roll = 0.0;
        feel.barrel = 0.0;
        return;
    }

    let h = HullHandling::for_class(PLAYER_HULL_SEED, HullClass::Corvette);
    // Engine power routes into how hard the hull thrusts and turns.
    let engine_mult = 0.5 + 0.25 * command.power_engines as f32;

    let has_fuel = systems.fuel.0 > 0;
    let thrust_key = keys.pressed(KeyCode::Space);
    let boost = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let brake = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    systems.thrusting = thrust_key && has_fuel;
    feel.boost_blend = approach(
        feel.boost_blend,
        if systems.thrusting && boost { 1.0 } else { 0.0 },
        6.0,
        dt,
    );
    feel.brake_blend = approach(feel.brake_blend, if brake { 1.0 } else { 0.0 }, 6.0, dt);

    force.force = Vec3::ZERO;
    force.torque = Vec3::ZERO;

    // --- raw input axes ---
    let mut raw_pitch = 0.0;
    let mut raw_yaw = 0.0;
    let mut raw_roll = 0.0;
    if keys.pressed(KeyCode::KeyW) {
        raw_pitch -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        raw_pitch += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        raw_yaw += 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        raw_yaw -= 1.0;
    }
    if keys.pressed(KeyCode::KeyQ) {
        raw_roll += 1.0;
    }
    if keys.pressed(KeyCode::KeyE) {
        raw_roll -= 1.0;
    }

    // Double-tap Q/E: barrel roll. One full 360° spin; steering stays live.
    if keys.just_pressed(KeyCode::KeyQ) {
        if feel.tap_left <= BARREL_WINDOW && feel.barrel == 0.0 {
            feel.barrel = std::f32::consts::TAU;
            log.log("Barrel roll!");
        }
        feel.tap_left = 0.0;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        if feel.tap_right <= BARREL_WINDOW && feel.barrel == 0.0 {
            feel.barrel = -std::f32::consts::TAU;
            log.log("Barrel roll!");
        }
        feel.tap_right = 0.0;
    }

    // --- smoothed axes: ramp toward held input, settle when released ---
    let rate = |raw: f32| {
        if raw != 0.0 {
            AXIS_ATTACK
        } else {
            AXIS_RELEASE
        }
    };
    feel.pitch = approach(feel.pitch, raw_pitch, rate(raw_pitch), dt);
    feel.yaw = approach(feel.yaw, raw_yaw, rate(raw_yaw), dt);
    feel.roll = approach(feel.roll, raw_roll, rate(raw_roll), dt);

    // --- rotation (direct angular velocity about local axes) ---
    let turn = HullHandling::f32(h.turn_rate).clamp(0.0, 4.0) * TURN_SCALE * engine_mult;
    let roll_rate = if feel.barrel != 0.0 {
        // Barrel roll owns the roll axis: constant spin until the full turn
        // is spent.
        let spin = feel.barrel.signum() * BARREL_RATE;
        let step = BARREL_RATE * dt;
        if feel.barrel.abs() <= step {
            feel.barrel = 0.0;
        } else {
            feel.barrel -= feel.barrel.signum() * step;
        }
        spin
    } else if raw_roll != 0.0 {
        feel.roll * turn * 1.4
    } else {
        // Auto-bank: lean into the yaw turn; hands-off re-levels the horizon.
        // Fades out when the nose points near straight up/down, where the
        // horizon reference is degenerate.
        let level = 1.0 - transform.forward().y.abs();
        let err = wrap_angle(feel.yaw * MAX_BANK - bank_angle(transform.rotation));
        (err * BANK_GAIN * level).clamp(-turn * 2.0, turn * 2.0)
    };
    let local = Vec3::new(feel.pitch * turn, feel.yaw * turn, roll_rate);
    velocity.angular = if local.length_squared() > 1e-6 {
        transform.rotation * local
    } else {
        Vec3::ZERO
    };

    // --- translation: direct-velocity arcade model ---
    // The hull's rapier mass is deliberately irrelevant: SF64 ships obey the
    // stick, not F=ma. (The old force-based thrust fought a ball collider
    // whose default-density mass was ~500k — acceleration was microscopic
    // and the ship never visibly moved.) `thrust` sets the speed cap,
    // `boost_mult` multiplies it, and the grip keeps the vector on the nose.
    let forward = transform.forward().as_vec3();
    let cruise = HullHandling::f32(h.thrust) * CRUISE_SCALE * engine_mult;
    let bm = HullHandling::f32(h.boost_mult);
    let mut speed = velocity.linear.length();
    if systems.thrusting {
        let cap = if boost { cruise * bm * bm } else { cruise };
        speed = approach(speed, cap, if boost { 1.8 } else { 1.2 }, dt);
        let mut burn = h.fuel_burn;
        if boost {
            burn = ((burn as f32) * bm) as i64;
        }
        let dtms = time.delta().as_millis() as i64;
        let used = (dtms * burn).max(1000) / 1000;
        systems.fuel = Fixed((systems.fuel.0 - used.max(1)).max(0));
    } else if brake {
        // Retro-thrust: kill momentum fast, snapping to a full stop at the
        // end so docking/mining line-ups don't fight residual creep.
        speed *= (-BRAKE_DECAY * dt).exp();
        if speed < 2.0 {
            speed = 0.0;
        }
        if has_fuel {
            let dtms = time.delta().as_millis() as i64;
            let used = (dtms * (h.fuel_burn / 2).max(1)) / 1000;
            systems.fuel = Fixed((systems.fuel.0 - used.max(1)).max(0));
        }
    } else {
        // Coasting: space keeps most of the speed, shedding it slowly.
        speed *= (-IDLE_DECAY * dt).exp();
    }
    // Arcade grip: the velocity vector re-aligns toward the nose, so the
    // ship flies where it points. Newtonian drift survives only as a brief
    // slide out of hard turns (Star Fox, not Kerbal).
    let dir = if speed > 0.5 {
        let t = 1.0 - (-GRIP * dt).exp();
        velocity
            .linear
            .normalize_or(forward)
            .lerp(forward, t)
            .normalize_or(forward)
    } else {
        forward
    };
    velocity.linear = dir * speed;
    feel.speed = speed;

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
/// Boost pulls the camera back and widens the lens; brake tucks it in; turns
/// slide the ship across the frame; hull hits shake the view; and the camera's
/// up vector holds still during a barrel roll so the ship spins, not the world.
pub fn camera_follow(
    time: Res<Time>,
    mut feel: ResMut<FlightFeel>,
    ship: Query<&Transform, (With<PlayerShip>, Without<SpaceCamera>)>,
    mut camera: Query<(&mut Transform, &mut Projection), With<SpaceCamera>>,
) {
    let (Ok(ship), Ok((mut camera, mut projection))) = (ship.single(), camera.single_mut()) else {
        return;
    };
    let dt = time.delta_secs();

    // Frame the hull small and low (corvettes run ~50 units of radius —
    // any closer and the ship fills the screen and hides the world). Speed
    // and boost stretch the leash; brake reels it in. Turning offsets the
    // camera opposite the turn so the ship slides into the frame edge (SF64).
    let dist = 260.0 + feel.speed * 0.25 + 60.0 * feel.boost_blend - 55.0 * feel.brake_blend;
    let height = 55.0 + 10.0 * feel.brake_blend - 8.0 * feel.boost_blend;
    let lateral = feel.yaw * 30.0;
    let vertical = -feel.pitch * 14.0;
    let back = ship.rotation * Vec3::new(lateral, height + vertical, dist);
    let target = ship.translation + back;
    let t = (dt * 6.0).clamp(0.0, 1.0);
    camera.translation = camera.translation.lerp(target, t);

    // The camera's up chases the hull's lean slowly — and not at all during a
    // barrel roll, so the world holds still while the ship spins.
    if feel.barrel == 0.0 {
        let ship_up = ship.rotation * Vec3::Y;
        let ut = 1.0 - (-3.0 * dt).exp();
        feel.cam_up = feel.cam_up.lerp(ship_up, ut).normalize_or(Vec3::Y);
    }
    // Look well past the nose so the hull sits low in frame and the sky —
    // where the flying happens — owns the screen.
    let look = ship.translation + ship.forward().as_vec3() * 160.0;
    let desired = Transform::from_translation(camera.translation).looking_at(look, feel.cam_up);
    camera.rotation = camera.rotation.slerp(desired.rotation, t);

    // Hit shake: decaying screen-space jitter fed by `collisions`.
    if feel.shake > 0.001 {
        let e = time.elapsed_secs();
        let jitter =
            camera.rotation * (Vec3::new((e * 47.0).sin(), (e * 53.0).cos(), 0.0) * feel.shake);
        camera.translation += jitter;
        feel.shake *= (-7.0 * dt).exp();
    } else {
        feel.shake = 0.0;
    }

    // FOV kick: raw speed widens the lens continuously, boost adds a punch
    // on top, brake narrows it.
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = BASE_FOV + 0.18 * (feel.speed / 280.0).min(1.0) + 0.15 * feel.boost_blend
            - 0.12 * feel.brake_blend;
    }
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

/// Ortho zoom for the walking interiors: <1 magnifies. At 0.25 each 16px
/// tile renders 64 screen pixels — the 4× integer scale pixel art wants,
/// showing ~30×17 tiles on a 1080p window (Stardew framing).
const INTERIOR_ZOOM: f32 = 0.25;

/// Activate the 3D chase-cam in SpaceFlight and the 2D camera everywhere else.
/// In space the 2D camera keeps rendering the HUD on top (its clear is turned
/// off so it overlays the flight view) at 1:1 scale for the sensor overlay;
/// in interiors it clears, zooms in to walking scale, and draws the top-down
/// scene.
#[allow(clippy::type_complexity)]
pub fn manage_cameras(
    mode: Res<State<GameMode>>,
    mut space_cam: Query<&mut Camera, (With<SpaceCamera>, Without<Camera2d>)>,
    mut ui_cam: Query<(&mut Camera, &mut Projection), (With<Camera2d>, Without<SpaceCamera>)>,
) {
    let in_space = *mode == GameMode::SpaceFlight;
    let in_interior = matches!(**mode, GameMode::Landed | GameMode::OnBoard);
    if let Ok(mut cam) = space_cam.single_mut() {
        cam.is_active = in_space;
    }
    if let Ok((mut cam, mut projection)) = ui_cam.single_mut() {
        cam.clear_color = if in_space {
            ClearColorConfig::None
        } else {
            ClearColorConfig::Custom(Color::srgb(0.02, 0.02, 0.05))
        };
        if let Projection::Orthographic(o) = &mut *projection {
            o.scale = if in_interior { INTERIOR_ZOOM } else { 1.0 };
        }
    }
}

/// Query the collision terminal reads: every body's identity plus the
/// optional combat components it may carry. Factored out so the tuple type
/// doesn't trip clippy's `type_complexity` in `collisions`.
type CollisionBodies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static Damager>,
        Option<&'static Hull>,
        Option<&'static Asteroid>,
        Option<&'static Velocity>,
    ),
>;

/// Pure hull-damage clamp (spec §14 Mode 3 survival). Subtracts `damage` from
/// `hp` and clamps the result into `[0, max]`. Kept free of any Bevy type so it
/// is unit-testable without a runtime (see the `tests` module at the foot of
/// this file). Mirrors the inline math that previously lived in `collisions`.
pub fn apply_hull_damage(hp: i64, damage: i64, max: i64) -> i64 {
    ((hp - damage).max(0)).min(max)
}

/// Damage terminal — the single collision handler all flight combat routes
/// through (spec §22 + S19-ready). One entity in a colliding pair carries a
/// `Damager` (projectiles, kinetic impacts); the other carries `Hull` and/or
/// `Asteroid` and takes the hit. Generic so enemy ships (S19) slot in by
/// adding the same `Hull`/`Damager` components — no new query paths.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn collisions(
    mut commands: Commands,
    mut events: bevy_ecs::message::MessageReader<CollisionEvent>,
    mut queries: ParamSet<(
        CollisionBodies<'_, '_>,
        Query<&mut Hull>,
        Query<&mut Asteroid, Without<PlayerShip>>,
    )>,
    player: Query<Entity, With<PlayerShip>>,
    mut ship_systems: ResMut<ShipSystems>,
    mut feel: ResMut<FlightFeel>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let bodies = queries.p0();
    let player_id = player.single().ok();
    let mut pending_damage: Vec<(Entity, i64)> = Vec::new();
    let mut pending_ore: Vec<(Entity, i64)> = Vec::new();
    for e in events.read() {
        let (a, b) = match e {
            CollisionEvent::Started(a, b, _) => (a, b),
            CollisionEvent::Stopped(..) => continue,
        };
        // Re-read each side independently so both mutable borrows stay
        // disjoint (a single `get_many_mut` over the whole tuple would
        // borrow conflicting fields).
        let (a_id, a_dmg, a_hull, a_ast, a_vel) = match bodies.get(*a) {
            Ok(v) => (a, v.1, v.2, v.3, v.4),
            Err(_) => continue,
        };
        let (b_id, b_dmg, b_hull, b_ast, b_vel) = match bodies.get(*b) {
            Ok(v) => (b, v.1, v.2, v.3, v.4),
            Err(_) => continue,
        };

        // Identify which side is the damage source and which is the target.
        let ((_src_id, damager, src_vel), (tgt_id, tgt_hull, tgt_ast)) = match (a_dmg, b_dmg) {
            (Some(d), None) => ((a_id, d, a_vel), (b_id, b_hull, b_ast)),
            (None, Some(d)) => ((b_id, d, b_vel), (a_id, a_hull, a_ast)),
            _ => continue, // no damager in this pair
        };

        // Gun bolt into an asteroid: queue ore drain (applied via a mutable
        // query below — the immutable `bodies` borrow can't be re-borrowed
        // mutably here).
        if damager.source == DamageSource::PlayerGun {
            if let Some(ast) = tgt_ast {
                let drained = damager.damage.min(ast.ore);
                pending_ore.push((*tgt_id, drained));
                continue;
            }
        }

        // Anything with a `Hull` takes the hit. Ramming scales with the
        // striking body's speed so a gentle nudge isn't lethal. Queued and
        // applied through a dedicated mutable query below.
        if tgt_hull.is_some() {
            let dmg_amount = match damager.source {
                DamageSource::PlayerGun => damager.damage,
                DamageSource::Ram => {
                    // Tuned to the arcade speed range: a cruise-speed ram
                    // (~140 u/s) costs ~16% hull, a boost ram ~30%.
                    let speed = src_vel.map(|v| v.linear.length()).unwrap_or(0.0);
                    (speed * 1.2) as i64
                }
            };
            pending_damage.push((*tgt_id, dmg_amount));
        }
    }

    // Apply asteroid ore drains.
    let mut asteroids = queries.p2();
    for (e, drained) in pending_ore {
        if let Ok(mut ast) = asteroids.get_mut(e) {
            ast.ore -= drained;
            ship_systems.ore += drained;
            if ast.ore <= 0 {
                commands.entity(e).despawn();
                spawn_debris(&mut commands, &mut meshes, &mut materials);
            }
        }
    }

    // Apply hull damage through a fresh mutable query, and report player-hull
    // changes to `ShipSystems` for the HUD / death loop.
    for (tgt_id, dmg_amount) in pending_damage {
        if let Ok(mut hull) = queries.p1().get_mut(tgt_id) {
            let before = hull.hp;
            hull.hp = apply_hull_damage(hull.hp, dmg_amount, hull.max);
            if hull.hp < before && Some(&tgt_id) == player_id.as_ref() {
                ship_systems.hull_hp = Fixed(hull.hp);
                // Kick the chase-cam: harder hits shake more.
                feel.shake = (feel.shake + dmg_amount as f32 * 0.02).clamp(0.3, 2.5);
                if hull.hp <= 0 && !ship_systems.dead {
                    ship_systems.dead = true;
                    log.log("HULL BREACH — emergency revive in progress…");
                }
            }
        }
    }
}

/// Small visual puff when a rock is fully mined out.
fn spawn_debris(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(2.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.8, 0.6, 0.4, 0.6),
            emissive: LinearRgba::rgb(1.0, 0.6, 0.3),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::default(),
        crate::states::ModeScope(GameMode::SpaceFlight),
    ));
}

/// Tracks the death/respawn beat so the ship re-enters the world at the last
/// docked station (or the origin) after a hull breach.
#[derive(Resource, Default)]
pub struct RespawnTimer(pub Option<Timer>);

/// Death/respawn loop (spec §14 Mode 3 survival). When the hull is breached
/// (`ShipSystems::dead`), start a short beat; when it elapses, rebuild the ship
/// at the last docked station (or the origin) and clear the breach.
#[allow(clippy::too_many_arguments)]
pub fn respawn_ship(
    time: Res<Time>,
    location: Res<CurrentLocation>,
    mut systems: ResMut<ShipSystems>,
    mut timer: ResMut<RespawnTimer>,
    mut ship: Query<(&mut Transform, &mut Velocity), With<PlayerShip>>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
) {
    if !systems.dead {
        return;
    }
    let t = timer
        .0
        .get_or_insert_with(|| Timer::from_seconds(3.0, TimerMode::Once));
    if !t.tick(time.delta()).is_finished() {
        return;
    }
    // Revive at the last docked station if we know one, else the origin.
    let spawn = if location.station_position == Vec2::ZERO {
        Vec3::ZERO
    } else {
        Vec3::new(
            location.station_position.x,
            0.0,
            location.station_position.y,
        )
    };
    if let Ok((mut tx, mut vel)) = ship.single_mut() {
        tx.translation = spawn;
        tx.rotation = Quat::IDENTITY;
        vel.linear = Vec3::ZERO;
        vel.angular = Vec3::ZERO;
    }
    systems.hull_hp = Fixed(1024);
    systems.fuel = Fixed(1024);
    systems.dead = false;
    timer.0 = None;
    log.log("Hull rebuilt — back in the black.");
}

/// Fire the guns while `F` is held (hold-to-autofire, gated by the cooldown) —
/// but only if the gunner armed them and the power console routed power to
/// weapons. Cooldown (rate of fire) scales with weapon power. Bolts are
/// emissive spheres launched along the hull's forward axis, alternating
/// muzzle sides like twin lasers.
#[allow(clippy::too_many_arguments)]
pub fn fire_weapons(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut systems: ResMut<ShipSystems>,
    command: Res<ShipCommand>,
    mut feel: ResMut<FlightFeel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ship: Query<(&Transform, &Collider), With<PlayerShip>>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
) {
    systems.gun_cooldown = (systems.gun_cooldown - time.delta_secs()).max(0.0);
    if !keys.pressed(KeyCode::KeyF) {
        return;
    }
    let Ok((ship, collider)) = ship.single() else {
        return;
    };
    if !command.weapons_armed || command.power_weapons == 0 {
        // Only nag on the initial press, not every held frame.
        if keys.just_pressed(KeyCode::KeyF) {
            log.log("Weapons offline — arm at the gunner console and route power.");
        }
        return;
    }
    if systems.gun_cooldown > 0.0 {
        return;
    }
    systems.gun_cooldown = 0.6 / (command.power_weapons as f32);
    let forward = ship.forward().as_vec3();
    // Muzzle sits past the ship's own collider — a corvette hull runs ~50
    // units of radius, and a bolt born inside it collides with (and damages)
    // the ship that fired it.
    let hull_r = collider.as_ball().map(|b| b.radius()).unwrap_or(50.0);
    // Twin lasers: alternate wingtip muzzles each shot.
    feel.muzzle_left = !feel.muzzle_left;
    let wing = hull_r * 0.45;
    let side = ship.right().as_vec3() * if feel.muzzle_left { -wing } else { wing };
    let muzzle = ship.translation + forward * (hull_r + 8.0) + side;
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
            // Comfortably faster than a boosting ship (~270 u/s) so the
            // player can never outrun their own bolts.
            linear: forward * 520.0,
            angular: Vec3::ZERO,
        },
        Collider::ball(0.6),
        ActiveEvents::COLLISION_EVENTS,
        Sensor,
        Projectile {
            life: Timer::from_seconds(2.0, TimerMode::Once),
            damage: 60,
        },
        crate::states::ModeScope(GameMode::SpaceFlight),
    ));
}

/// Engine exhaust: the flame cone behind the hull stretches with thrust,
/// doubles down on boost, flickers a little, and vanishes when coasting.
pub fn engine_glow(
    time: Res<Time>,
    feel: Res<FlightFeel>,
    systems: Res<ShipSystems>,
    mut flames: Query<(&mut Transform, &mut Visibility, &EngineExhaust)>,
) {
    for (mut tx, mut vis, flame) in &mut flames {
        if !systems.thrusting || systems.dead {
            *vis = Visibility::Hidden;
            continue;
        }
        // Inherited (not Visible) so a hidden ship still hides its flame.
        *vis = Visibility::Inherited;
        let flicker = (time.elapsed_secs() * 31.0).sin() * 0.08;
        let len = 0.55 + 0.95 * feel.boost_blend + flicker;
        let width = 1.0 + 0.35 * feel.boost_blend;
        tx.scale = Vec3::new(width, len, width);
        // Keep the wide end welded to the rear face as the cone stretches.
        tx.translation.z = flame.base_z + flame.length * len * 0.5;
    }
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
    mut asteroids: Query<(Entity, &Transform, &mut Asteroid), Without<PlayerShip>>,
    mut inv: ResMut<crate::systems::inventory::PlayerInventory>,
    location: Res<crate::states::CurrentLocation>,
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
    // Beam runs from the nose out to BEAM_REACH (the old 36-unit beam at
    // +20 sat entirely inside a ~50-unit corvette hull).
    let nose = 60.0;
    let mid = ship.translation + forward * (nose + (BEAM_REACH - nose) * 0.5);
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(1.2, BEAM_REACH - nose))),
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
    // Drain ore from the nearest asteroid within beam reach into the cargo
    // hold (a real `GoodId` the S10 market will later sell). Honors hold
    // capacity and despawns spent rocks.
    let ore_good = reachlock_core::economy::GoodId("raw_ferric_ore".into());
    for (e, t, mut ast) in &mut asteroids {
        if t.translation.distance(ship.translation) > BEAM_REACH {
            continue;
        }
        let rate = (time.delta_secs() * 12.0) as i64;
        let room = inv.capacity.saturating_sub(inv.cargo_units()) as i64;
        let take = rate.min(ast.ore).min(room);
        if take <= 0 {
            break;
        }
        ast.ore -= take;
        systems.ore += take;
        *inv.cargo.entry(ore_good.clone()).or_insert(0) += take as u32;
        if ast.ore <= 0 {
            commands.entity(e).despawn();
        }
        break; // one rock per frame is enough
    }
    // Mineable-only systems still auto-repair while docked (spec §22: the
    // power console's hull trickle). Kept light; the engineering console does
    // the real repair.
    if location.is_docked && systems.hull_hp.0 < 1024 {
        systems.hull_hp = Fixed((systems.hull_hp.0 + 4).min(1024));
    }
}

/// Emit an expanding scanner ring when a pulse is requested (scanner console or
/// the `T` key) and marks nearby contacts known via `KnownContacts`. The ring
/// reach scales with sensor power and the console's long-range mode.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
    mut pulses: Query<
        (Entity, &mut ScanPulse, &mut Transform),
        (
            Without<PlayerShip>,
            Without<crate::systems::sensors::Contact>,
        ),
    >,
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

#[cfg(test)]
mod tests {
    use super::{apply_hull_damage, approach, bank_angle, wrap_angle, BANK_GAIN, MAX_BANK};
    use bevy::math::{Quat, Vec3};

    #[test]
    fn approach_converges_and_is_framerate_stable() {
        // One 100ms step lands (nearly) where ten 10ms steps land.
        let coarse = approach(0.0, 1.0, 8.0, 0.1);
        let mut fine = 0.0;
        for _ in 0..10 {
            fine = approach(fine, 1.0, 8.0, 0.01);
        }
        assert!((coarse - fine).abs() < 1e-4, "{coarse} vs {fine}");
        // It never overshoots the target.
        assert!(approach(0.0, 1.0, 1000.0, 1.0) <= 1.0);
    }

    #[test]
    fn wrap_angle_stays_in_pi_range() {
        use std::f32::consts::{PI, TAU};
        assert!((wrap_angle(TAU + 0.1) - 0.1).abs() < 1e-5);
        assert!((wrap_angle(-TAU - 0.1) + 0.1).abs() < 1e-5);
        assert!(wrap_angle(PI + 0.2) < 0.0); // wraps past π to the negative side
    }

    #[test]
    fn bank_angle_reads_roll_about_forward() {
        // Level flight: no bank.
        assert!(bank_angle(Quat::IDENTITY).abs() < 1e-5);
        // A positive roll about the ship's own -Z-forward frame (local +Z)
        // reads back as the same positive bank.
        for a in [-1.2_f32, -0.4, 0.3, 1.0] {
            let q = Quat::from_rotation_z(a);
            assert!((bank_angle(q) - a).abs() < 1e-4, "bank {a}");
        }
        // Bank is measured about the hull's forward axis regardless of yaw.
        let yawed = Quat::from_rotation_y(0.8) * Quat::from_rotation_z(0.5);
        assert!((bank_angle(yawed) - 0.5).abs() < 1e-4);
    }

    #[test]
    fn auto_bank_converges_to_lean_into_turn() {
        // Simulate the control loop's bank branch: full left yaw, no manual
        // roll. The hull must settle leaning left (positive bank) at MAX_BANK.
        let yaw_axis = 1.0_f32; // holding A (turn left)
        let mut rotation = Quat::IDENTITY;
        let dt = 1.0 / 60.0;
        for _ in 0..240 {
            let err = wrap_angle(yaw_axis * MAX_BANK - bank_angle(rotation));
            let roll_rate = (err * BANK_GAIN).clamp(-6.0, 6.0);
            // Integrate angular velocity about the hull's local Z, as
            // `control` does via `transform.rotation * local`.
            rotation = (rotation * Quat::from_rotation_z(roll_rate * dt)).normalize();
        }
        let settled = bank_angle(rotation);
        assert!(
            (settled - MAX_BANK).abs() < 0.02,
            "expected lean {MAX_BANK}, got {settled}"
        );
        // And hands-off re-levels: err now targets zero bank.
        for _ in 0..240 {
            let err = wrap_angle(0.0 - bank_angle(rotation));
            let roll_rate = (err * BANK_GAIN).clamp(-6.0, 6.0);
            rotation = (rotation * Quat::from_rotation_z(roll_rate * dt)).normalize();
        }
        assert!(bank_angle(rotation).abs() < 0.02);
    }

    #[test]
    fn grip_realigns_velocity_to_nose_without_gaining_speed() {
        // The grip lerp from `control`: velocity eases toward forward*speed.
        let forward = Vec3::NEG_Z;
        let mut velocity = Vec3::X * 100.0; // sliding sideways at 100
        let dt = 1.0 / 60.0;
        let t = 1.0 - (-super::GRIP * dt).exp();
        for _ in 0..600 {
            let speed = velocity.length();
            velocity = velocity.lerp(forward * speed, t);
        }
        // After ten simulated seconds the ship flies where it points…
        assert!(velocity.normalize().dot(forward) > 0.999);
        // …and grip never manufactured speed.
        assert!(velocity.length() <= 100.0 + 1e-3);
    }

    #[test]
    fn small_hit_keeps_hp_above_zero() {
        // 100 hp, 30 damage -> 70 (no clamping beyond the subtraction).
        assert_eq!(apply_hull_damage(100, 30, 100), 70);
    }

    #[test]
    fn overkill_hit_clamps_to_zero() {
        // 10 hp, 999 damage -> 0, never negative.
        assert_eq!(apply_hull_damage(10, 999, 100), 0);
        assert!(apply_hull_damage(5, 6, 100) >= 0);
    }

    #[test]
    fn damage_respects_max_clamp() {
        // A malformed/overflowing current hp is clamped down to max, not allowed
        // to exceed it (defensive: hp coming in already at/above max).
        assert_eq!(apply_hull_damage(100, -50, 100), 100);
        assert_eq!(apply_hull_damage(250, 0, 100), 100);
    }

    #[test]
    fn exact_boundary_hits() {
        // Exactly lethal.
        assert_eq!(apply_hull_damage(40, 40, 100), 0);
        // Exactly survives.
        assert_eq!(apply_hull_damage(40, 39, 100), 1);
    }
}
