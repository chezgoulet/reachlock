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

    // Breached hull: no flight control during the death/respawning beat. The
    // `respawn_ship` system clears `dead` and restores control.
    if systems.dead {
        velocity.linear = Vec3::ZERO;
        velocity.angular = Vec3::ZERO;
        force.force = Vec3::ZERO;
        force.torque = Vec3::ZERO;
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
    let back = ship.rotation * Vec3::new(0.0, 15.0, 80.0);
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
                    let speed = src_vel.map(|v| v.linear.length()).unwrap_or(0.0);
                    (speed * 3.0) as i64
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
        ActiveEvents::COLLISION_EVENTS,
        Sensor,
        Projectile {
            life: Timer::from_seconds(2.0, TimerMode::Once),
            damage: 60,
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
    // Drain ore from the nearest asteroid within beam reach into the cargo
    // hold (a real `GoodId` the S10 market will later sell). Honors hold
    // capacity and despawns spent rocks.
    const BEAM_REACH: f32 = 45.0;
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
    use super::apply_hull_damage;

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
