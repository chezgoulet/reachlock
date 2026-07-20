//! Space combat (S19, spec §22 "Star Fox 64"): enemy wings spawned from the
//! system's threat level fly core behavior-tree Intents through rapier,
//! trade fire with the player, and die (or strand, or run) by the core
//! damage model. Division of labor per the S19 gotcha: ALL combat math is
//! `reachlock_core::combat` (integers); this module is presentation and
//! plumbing (floats are fine here — they never feed back into core state).
//!
//! Hardpoints: S17 is not merged, so both sides fire from hull center
//! (noted in the sprint brief as the fallback).
//!
//! Key map: `R` cycles the targeted subsystem (the brief suggests `T`, but
//! `T` has been the scanner pulse since S09d). Arrows drive the in-flight
//! power split (Up/Down select, Left/Right adjust). `C` pops chaff.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use std::collections::HashSet;

use reachlock_core::combat::{
    apply_hit, enemy_step, generate_encounters, BehaviorState, CombatVessel, EnemyClass, Intent,
    Senses, SubsystemKind, SubsystemState, WeaponKind, WeaponStats,
};
use reachlock_core::contract::{
    engine::{evaluate, EvalContext, Outcome},
    types::{Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger},
};
use reachlock_core::generator::hull::HullHandling;
use reachlock_core::generator::system::generate_system;
use reachlock_core::generator::FixedVec2;
use reachlock_core::util::rng::{Fixed, SeededRng};

use crate::bridge;
use crate::settings::{InputAction, Settings};
use crate::states::{CurrentLocation, GameMode, ModeScope};
use crate::systems::contract::{Deliberation, DeliberationState, ShipLog};
use crate::systems::sensors::Contact;
use crate::systems::ship::{
    FlightFeel, PlayerShip, Projectile, ShipCommand, ShipSystems, POWER_BUDGET, POWER_MAX_NOTCH,
};

// --- collision groups (S19 gotcha: separate groups from the start) ----------
pub const G_PLAYER: Group = Group::GROUP_1;
pub const G_ENEMY: Group = Group::GROUP_2;
pub const G_PLAYER_PROJ: Group = Group::GROUP_3;
pub const G_ENEMY_PROJ: Group = Group::GROUP_4;

/// Player bolts hit everything except the player and each other.
pub fn player_projectile_groups() -> CollisionGroups {
    CollisionGroups::new(G_PLAYER_PROJ, Group::ALL & !G_PLAYER & !G_PLAYER_PROJ)
}

/// Enemy bolts hit everything except enemies and each other — friendly fire
/// between wings is a launch-week bug we refuse to ship.
fn enemy_projectile_groups() -> CollisionGroups {
    CollisionGroups::new(G_ENEMY_PROJ, Group::ALL & !G_ENEMY & !G_ENEMY_PROJ)
}

fn enemy_ship_groups() -> CollisionGroups {
    CollisionGroups::new(G_ENEMY, Group::ALL & !G_ENEMY_PROJ)
}

// --- scales (presentation layer; core keeps raw stat units) ------------------
/// Item-stat range units → world units (a tier-4 energy repeater reaches
/// ~500 world units; the flight world is laid out on a ~2500-unit radius).
const RANGE_SCALE: f32 = 6.0;
/// Enemy sensor pickup, world units (blind ships fall back to gun range).
const ENGAGE_RANGE: i64 = 700;
/// Beyond this the fight is over.
const DISENGAGE_RANGE: i64 = 2000;
/// Enemy bolt speed, world units/s (slower than the player's 520 — dodging
/// is the game).
const ENEMY_BOLT_SPEED: f32 = 420.0;
/// Seconds between behavior-tree decision ticks.
const DECISION_TICK: f32 = 0.25;
/// Same cruise scale the player's flight model uses (ship.rs).
const CRUISE_SCALE: f32 = 85.0;
const TURN_SCALE: f32 = 10.0;
/// Shield points per notch of shield power (max notch 3 → 384).
const SHIELD_PER_NOTCH: i64 = 128;
/// Shield points regained per second per notch.
const SHIELD_REGEN_PER_NOTCH: f32 = 12.0;

/// The player's fixed gun profile for the core damage model (matches the
/// 60-damage bolt `ship::fire_weapons` has always fired; kinetic mass
/// driver per the Loup-Garou's chin gun).
fn player_weapon(damage: i64) -> WeaponStats {
    WeaponStats {
        kind: WeaponKind::Kinetic,
        damage,
        range: 90,
        fire_rate: 2,
    }
}

/// Max player shield points for the current power routing.
pub fn player_shield_max(power_shields: u8) -> i64 {
    SHIELD_PER_NOTCH * power_shields as i64
}

// --- components / resources --------------------------------------------------

/// One hostile ship: behavior state + core combat state + its own physics
/// limits. The behavior tree asks; `HullHandling` answers.
#[derive(Component)]
pub struct EnemyShip {
    pub class: EnemyClass,
    pub wing: u32,
    pub state: BehaviorState,
    pub vessel: CombatVessel,
    pub weapon: WeaponStats,
    pub handling: HullHandling,
    pub tier: u8,
    /// Latest intent, applied every frame between decision ticks.
    pub heading: Vec3,
    pub throttle: f32,
    pub fire: bool,
    pub patrol_dir: Vec3,
    pub decision: Timer,
    pub fire_cooldown: f32,
}

/// An enemy bolt. Deliberately NOT a `Damager` — `combat_hits` routes it
/// through the core damage model instead of ship.rs's plain-hull terminal.
#[derive(Component)]
pub struct EnemyProjectile {
    pub life: Timer,
    pub weapon: WeaponStats,
}

/// Which system the player wants to spawn encounters for; `None` until the
/// first flight frame. Keyed on the system seed so a self-jump escape
/// doesn't respawn the wing you just fled.
#[derive(Resource, Default)]
pub struct SpawnedEncounters {
    pub seed: Option<u64>,
}

/// Wings that already got their one reinforcement call.
#[derive(Resource, Default)]
pub struct ReinforcedWings(pub HashSet<u32>);

/// The subsystem the player's guns are calling (spec §22 subsystem
/// targeting). `None` = center-mass.
#[derive(Resource, Default)]
pub struct PlayerTargeting {
    pub subsystem: Option<SubsystemKind>,
}

/// Fractional shield-regen carry (points accumulate below 1 per frame).
#[derive(Resource, Default)]
pub struct ShieldRegenCarry(pub f32);

/// In-flight power split UI state: which of weapons/shields/engines the
/// arrows are pointed at.
#[derive(Resource, Default)]
pub struct PowerSelect(pub u8);

const POWER_SLOTS: [&str; 3] = ["WPN", "SHD", "ENG"];

// --- encounter spawning ------------------------------------------------------

/// Spawn the system's seeded encounters once per system (S19 deliverable).
#[allow(clippy::too_many_arguments)]
pub fn spawn_encounters(
    mut commands: Commands,
    mut spawned: ResMut<SpawnedEncounters>,
    mut reinforced: ResMut<ReinforcedWings>,
    location: Res<CurrentLocation>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut log: ResMut<ShipLog>,
) {
    let seed = location.system_seed;
    if spawned.seed == Some(seed) {
        return;
    }
    spawned.seed = Some(seed);
    reinforced.0.clear();

    let system = generate_system(seed, location.system_biome, location.system_fidelity);
    let spawns = generate_encounters(seed, &system);
    let tier = reachlock_core::combat::encounter::threat_tier(&system);
    if spawns.is_empty() {
        return;
    }
    for spawn in &spawns {
        spawn_enemy(
            &mut commands,
            &mut meshes,
            &mut materials,
            spawn.class,
            spawn.wing,
            spawn.seed,
            tier,
            Vec3::new(spawn.position.x.to_f32(), 0.0, spawn.position.y.to_f32()),
        );
    }
    log.log(format!(
        "Sensors: {} hostile contact(s) in-system (threat {}).",
        spawns.len(),
        system.threat_level
    ));
}

/// Build one enemy entity: generated hull mesh (nose local +X, rotated to
/// fly -Z like everything else), hostile-red material, physics, sensors.
#[allow(clippy::too_many_arguments)]
fn spawn_enemy(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    class: EnemyClass,
    wing: u32,
    seed: u64,
    tier: u8,
    position: Vec3,
) {
    let mesh = reachlock_core::generator::hull::generate_hull_class(seed, class.hull_class());
    let radius = mesh
        .vertices
        .iter()
        .map(|v| v.x.to_f32().abs().max(v.y.to_f32().abs()))
        .fold(0.0_f32, f32::max)
        .max(2.0);
    let mut rng = SeededRng::new(seed ^ 0x9A7);
    let angle = (rng.next_below(65536) as f32) / 65536.0 * std::f32::consts::TAU;
    let patrol_dir = Vec3::new(angle.cos(), 0.0, angle.sin());

    commands
        .spawn((
            EnemyShip {
                class,
                wing,
                state: BehaviorState::Patrol,
                vessel: class.vessel(tier),
                weapon: class.weapon(seed, tier),
                handling: HullHandling::for_class(seed, class.hull_class()),
                tier,
                heading: patrol_dir,
                throttle: 0.0,
                fire: false,
                patrol_dir,
                decision: Timer::from_seconds(DECISION_TICK, TimerMode::Repeating),
                fire_cooldown: 0.0,
            },
            Transform::from_translation(position),
            Visibility::default(),
            RigidBody::Dynamic,
            GravityScale(0.0),
            Collider::ball(radius),
            ActiveEvents::COLLISION_EVENTS,
            enemy_ship_groups(),
            Velocity::default(),
            Damping {
                linear_damping: 0.0,
                angular_damping: 5.0,
            },
            Contact,
            ModeScope(GameMode::SpaceFlight),
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(bridge::mesh3d_from_generated(&mesh, radius * 0.5))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.55, 0.16, 0.14),
                    metallic: 0.6,
                    perceptual_roughness: 0.5,
                    emissive: LinearRgba::rgb(0.35, 0.04, 0.03),
                    ..default()
                })),
                // Generated hulls nose along local +X; the flight convention
                // is nose -Z. Rotate the visual, not the physics.
                Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
            ));
        });
}

// --- enemy flight ------------------------------------------------------------

/// Fly every enemy: on its decision tick, build fixed-point senses and step
/// the core behavior tree; every frame, steer toward the stored intent with
/// thrust and turn capped by the enemy's OWN `HullHandling` and subsystem
/// state (disabled engines strand it — spec §22).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn enemy_fly(
    time: Res<Time>,
    mut commands: Commands,
    player: Query<(&Transform, &Velocity), (With<PlayerShip>, Without<EnemyShip>)>,
    mut enemies: Query<(Entity, &Transform, &mut Velocity, &mut EnemyShip), Without<PlayerShip>>,
    mut reinforced: ResMut<ReinforcedWings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut log: ResMut<ShipLog>,
) {
    let Ok((player_tx, _)) = player.single() else {
        return;
    };
    let dt = time.delta_secs();
    // Wing roster snapshot for ally counts (taken before the mutable pass).
    let roster: Vec<(Entity, u32)> = enemies.iter().map(|(e, _, _, s)| (e, s.wing)).collect();

    let mut backup_wings: Vec<(u32, Vec3)> = Vec::new();
    for (entity, tx, mut vel, mut enemy) in &mut enemies {
        let to_player = player_tx.translation - tx.translation;
        let dist = to_player.length();

        if enemy.decision.tick(time.delta()).just_finished() {
            let allies = roster
                .iter()
                .filter(|(e, w)| *e != entity && *w == enemy.wing)
                .count() as u32;
            // Interceptors shield-tank: the shield trickles back between
            // decision ticks (bombers barely have one).
            let trickle = 1 + enemy.tier as i64 / 2;
            enemy.vessel.recharge_shield(trickle);

            let sensors_blind =
                enemy.vessel.state(SubsystemKind::Sensors) == SubsystemState::Disabled;
            let weapon_range = (enemy.weapon.range as f32 * RANGE_SCALE) as i64;
            let senses = Senses {
                to_player: FixedVec2 {
                    x: Fixed::from_int(to_player.x as i64),
                    y: Fixed::from_int(to_player.z as i64),
                },
                dist_to_player: dist as i64,
                hull_frac: enemy.vessel.hull_frac(),
                shield_frac: enemy.vessel.shield_frac(),
                ally_count: allies,
                reinforcements_called: reinforced.0.contains(&enemy.wing),
                patrol_dir: FixedVec2 {
                    x: Fixed::from_int((enemy.patrol_dir.x * 100.0) as i64),
                    y: Fixed::from_int((enemy.patrol_dir.z * 100.0) as i64),
                },
                weapon_range,
                // Blinded sensors shrink the pickup bubble to gun range.
                engage_range: if sensors_blind {
                    weapon_range
                } else {
                    ENGAGE_RANGE
                },
                disengage_range: DISENGAGE_RANGE,
                engines_disabled: enemy.vessel.state(SubsystemKind::Engines)
                    == SubsystemState::Disabled,
                weapons_disabled: enemy.vessel.state(SubsystemKind::Weapons)
                    == SubsystemState::Disabled,
            };
            let was = enemy.state;
            let (next, intent) = enemy_step(enemy.state, &senses);
            enemy.state = next;
            apply_intent(&mut enemy, &intent, to_player, player_tx.translation.y);
            if intent.request_reinforcements && reinforced.0.insert(enemy.wing) {
                backup_wings.push((enemy.wing, tx.translation));
                log.log(format!(
                    "Intercepted transmission: {} calling for backup!",
                    enemy.class.label()
                ));
            }
            // A retreating ship that makes the edge with a working drive
            // jumps clear; a dead drive means it never leaves (stranding
            // disabled ships is the whole point of targeting the drive).
            if was == BehaviorState::Retreat
                && next == BehaviorState::Patrol
                && enemy.vessel.state(SubsystemKind::Drive) != SubsystemState::Disabled
            {
                log.log(format!("The {} jumps clear.", enemy.class.label()));
                commands.entity(entity).despawn();
                continue;
            }
        }

        // Per-frame steering toward the stored intent, physics-capped.
        let engine_eff: f32 = match enemy.vessel.state(SubsystemKind::Engines) {
            SubsystemState::Nominal => 1.0,
            SubsystemState::Damaged => 0.5,
            SubsystemState::Disabled => 0.0,
        };
        let turn = HullHandling::f32(enemy.handling.turn_rate).clamp(0.0, 4.0)
            * TURN_SCALE
            * engine_eff.max(0.2);
        let cruise = HullHandling::f32(enemy.handling.thrust) * CRUISE_SCALE * engine_eff;
        let target_speed = cruise * enemy.throttle;
        let desired = if enemy.heading.length_squared() > 1e-6 {
            enemy.heading.normalize()
        } else {
            Vec3::NEG_Z
        };
        let current = if vel.linear.length_squared() > 1.0 {
            vel.linear.normalize()
        } else {
            desired
        };
        // Rotate the velocity direction toward the desired heading at the
        // hull's turn rate (rad/s) — AI obeys the same physics the player does.
        let max_step = turn * dt;
        let angle = current.angle_between(desired);
        let dir = if angle <= max_step || angle < 1e-4 {
            desired
        } else {
            let axis = current.cross(desired).normalize_or(Vec3::Y);
            Quat::from_axis_angle(axis, max_step) * current
        };
        let speed = vel.linear.length();
        let new_speed = speed + (target_speed - speed) * (1.0 - (-1.5 * dt).exp());
        vel.linear = dir * new_speed;
        vel.angular = Vec3::ZERO;
    }

    // Reinforcements: a fresh interceptor pair drops in at the fight's edge.
    for (wing, near) in backup_wings {
        let mut rng = SeededRng::new(wing as u64 ^ 0xBAC_C0DE);
        for i in 0..2u64 {
            let angle = (rng.next_below(65536) as f32) / 65536.0 * std::f32::consts::TAU;
            let offset = Vec3::new(angle.cos(), 0.0, angle.sin()) * 900.0;
            spawn_enemy(
                &mut commands,
                &mut meshes,
                &mut materials,
                EnemyClass::Interceptor,
                wing,
                rng.next_u64() ^ i,
                3,
                near + offset,
            );
        }
    }
}

/// Convert a core Intent into the per-frame steering fields. Planar heading
/// from core, plus a gentle vertical close toward the player's altitude so
/// fights don't stratify (render-layer garnish, not gameplay math).
fn apply_intent(enemy: &mut EnemyShip, intent: &Intent, to_player: Vec3, _player_y: f32) {
    let hx = intent.heading.x.to_f32();
    let hz = intent.heading.y.to_f32();
    let mut heading = Vec3::new(hx, 0.0, hz);
    if heading.length_squared() > 1e-6 {
        heading = heading.normalize();
        heading.y = (to_player.y * 0.002).clamp(-0.3, 0.3)
            * if intent.heading.x.0.signum() == (to_player.x as i64).signum() {
                1.0
            } else {
                -1.0
            };
    }
    enemy.heading = heading;
    enemy.throttle = intent.throttle.0 as f32 / 1024.0;
    enemy.fire = intent.fire;
}

/// Enemy guns: cooldown from the weapon's fire rate (halved throughput when
/// the weapon subsystem is damaged), bolts lead the player's motion.
#[allow(clippy::type_complexity)]
pub fn enemy_fire(
    time: Res<Time>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    player: Query<(&Transform, &Velocity), (With<PlayerShip>, Without<EnemyShip>)>,
    mut enemies: Query<(&Transform, &mut EnemyShip)>,
) {
    let Ok((player_tx, player_vel)) = player.single() else {
        return;
    };
    let dt = time.delta_secs();
    for (tx, mut enemy) in &mut enemies {
        enemy.fire_cooldown = (enemy.fire_cooldown - dt).max(0.0);
        if !enemy.fire || enemy.fire_cooldown > 0.0 {
            continue;
        }
        let weapon_state = enemy.vessel.state(SubsystemKind::Weapons);
        if weapon_state == SubsystemState::Disabled {
            continue;
        }
        let mut cooldown = 2.4 / enemy.weapon.fire_rate.max(1) as f32;
        if weapon_state == SubsystemState::Damaged {
            cooldown *= 2.0;
        }
        enemy.fire_cooldown = cooldown;

        // Lead the target: aim where the player will be when the bolt lands.
        let dist = tx.translation.distance(player_tx.translation);
        let lead = player_tx.translation + player_vel.linear * (dist / ENEMY_BOLT_SPEED) * 0.6;
        let dir = (lead - tx.translation).normalize_or(Vec3::NEG_Z);
        let range = enemy.weapon.range as f32 * RANGE_SCALE;
        let (color, emissive, r) = match enemy.weapon.kind {
            WeaponKind::Energy => (
                Color::srgb(1.0, 0.3, 0.3),
                LinearRgba::rgb(3.0, 0.5, 0.4),
                0.7,
            ),
            WeaponKind::Kinetic => (
                Color::srgb(1.0, 0.75, 0.4),
                LinearRgba::rgb(2.2, 1.2, 0.3),
                1.1,
            ),
        };
        commands.spawn((
            Mesh3d(meshes.add(Sphere::new(r))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                emissive,
                ..default()
            })),
            Transform::from_translation(tx.translation + dir * 20.0),
            RigidBody::KinematicVelocityBased,
            Velocity {
                linear: dir * ENEMY_BOLT_SPEED,
                angular: Vec3::ZERO,
            },
            Collider::ball(r),
            ActiveEvents::COLLISION_EVENTS,
            Sensor,
            enemy_projectile_groups(),
            EnemyProjectile {
                life: Timer::from_seconds(range / ENEMY_BOLT_SPEED, TimerMode::Once),
                weapon: enemy.weapon,
            },
            ModeScope(GameMode::SpaceFlight),
        ));
    }
}

/// Age out enemy bolts (range = lifetime × speed).
pub fn step_enemy_projectiles(
    time: Res<Time>,
    mut commands: Commands,
    mut bolts: Query<(Entity, &mut EnemyProjectile)>,
) {
    for (e, mut p) in &mut bolts {
        if p.life.tick(time.delta()).is_finished() {
            commands.entity(e).despawn();
        }
    }
}

// --- the damage terminal -----------------------------------------------------

/// Route combat collisions through the core damage model: player bolts into
/// enemy vessels (with subsystem targeting), enemy bolts into the player's
/// shield/hull. Both directions call the same frozen `apply_hit` — spec §22
/// symmetry in one function.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn combat_hits(
    mut commands: Commands,
    mut events: bevy_ecs::message::MessageReader<CollisionEvent>,
    player_bolts: Query<&Projectile>,
    enemy_bolts: Query<&EnemyProjectile>,
    mut enemies: Query<(&Transform, &mut EnemyShip)>,
    mut player: Query<(Entity, &mut crate::systems::ship::Hull), With<PlayerShip>>,
    targeting: Res<PlayerTargeting>,
    command: Res<ShipCommand>,
    mut systems: ResMut<ShipSystems>,
    mut feel: ResMut<FlightFeel>,
    mut log: ResMut<ShipLog>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let player_id = player.single().map(|(e, _)| e).ok();
    for event in events.read() {
        let CollisionEvent::Started(a, b, _) = event else {
            continue;
        };
        for (bolt_e, target_e) in [(*a, *b), (*b, *a)] {
            // Player bolt → enemy vessel.
            if let (Ok(bolt), Ok((tx, mut enemy))) =
                (player_bolts.get(bolt_e), enemies.get_mut(target_e))
            {
                let weapon = player_weapon(bolt.damage);
                let result = apply_hit(&mut enemy.vessel, &weapon, targeting.subsystem);
                commands.entity(bolt_e).despawn();
                if let Some((kind, state)) = result.subsystem {
                    if state != SubsystemState::Nominal {
                        log.log(format!(
                            "{} {} {}.",
                            enemy.class.label(),
                            kind.label(),
                            match state {
                                SubsystemState::Damaged => "DAMAGED",
                                SubsystemState::Disabled => "DISABLED",
                                SubsystemState::Nominal => {
                                    log::warn!(
                                        "combat: nominal subsystem reached damage path — skipping"
                                    );
                                    continue;
                                }
                            }
                        ));
                    }
                }
                if result.destroyed {
                    log.log(format!("{} destroyed.", enemy.class.label()));
                    spawn_explosion(&mut commands, &mut meshes, &mut materials, tx.translation);
                    commands.entity(target_e).despawn();
                }
                continue;
            }
            // Enemy bolt → the player. Shield absorbs by weapon type, the
            // remainder lands on the hull; death defers to the existing
            // respawn loop.
            if let Ok(bolt) = enemy_bolts.get(bolt_e) {
                if Some(target_e) == player_id {
                    let shield_max = player_shield_max(command.power_shields);
                    let mut vessel = CombatVessel::new(1024, shield_max.max(1));
                    vessel.hull = systems.hull_hp.0;
                    vessel.shield = systems.shield.0.min(shield_max);
                    let result = apply_hit(&mut vessel, &bolt.weapon, None);
                    systems.shield = Fixed(vessel.shield);
                    systems.hull_hp = Fixed(vessel.hull);
                    if let Ok((_, mut hull)) = player.single_mut() {
                        hull.hp = vessel.hull;
                    }
                    feel.shake =
                        (feel.shake + result.hull_damage as f32 * 0.02 + 0.15).clamp(0.3, 2.5);
                    if result.destroyed && !systems.dead {
                        systems.dead = true;
                        log.log("HULL BREACH — emergency revive in progress…");
                    }
                    commands.entity(bolt_e).despawn();
                }
            }
        }
    }
}

/// Expanding emissive shell where a ship died (lyon-free, palette-free:
/// combat debris reads hostile-orange everywhere).
fn spawn_explosion(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    at: Vec3,
) {
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(6.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.5, 0.2, 0.7),
            emissive: LinearRgba::rgb(4.0, 1.6, 0.4),
            alpha_mode: AlphaMode::Blend,
            ..default()
        })),
        Transform::from_translation(at),
        Explosion {
            age: Timer::from_seconds(0.9, TimerMode::Once),
        },
        ModeScope(GameMode::SpaceFlight),
    ));
}

#[derive(Component)]
pub struct Explosion {
    pub age: Timer,
}

/// Grow and fade explosion shells.
pub fn step_explosions(
    time: Res<Time>,
    mut commands: Commands,
    mut blasts: Query<(Entity, &mut Explosion, &mut Transform)>,
) {
    for (e, mut blast, mut tx) in &mut blasts {
        tx.scale = Vec3::splat(1.0 + blast.age.fraction() * 7.0);
        if blast.age.tick(time.delta()).is_finished() {
            commands.entity(e).despawn();
        }
    }
}

// --- player-side combat controls --------------------------------------------

/// Shield recharge: points per second scale with the shield power notch.
/// Routing power away drops the ceiling immediately (spec §22 power
/// management is a real-time tradeoff, not a menu).
pub fn player_shield(
    time: Res<Time>,
    command: Res<ShipCommand>,
    mut systems: ResMut<ShipSystems>,
    mut carry: ResMut<ShieldRegenCarry>,
) {
    let max = player_shield_max(command.power_shields);
    if systems.shield.0 > max {
        systems.shield = Fixed(max);
    }
    if systems.shield.0 >= max || systems.dead {
        return;
    }
    carry.0 += SHIELD_REGEN_PER_NOTCH * command.power_shields as f32 * time.delta_secs();
    let whole = carry.0 as i64;
    if whole > 0 {
        carry.0 -= whole as f32;
        systems.shield = Fixed((systems.shield.0 + whole).min(max));
    }
}

/// `R` cycles the targeted subsystem: center-mass → engines → weapons →
/// sensors → drive (spec §22: strand it, silence it, blind it, trap it).
pub fn cycle_target(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut targeting: ResMut<PlayerTargeting>,
    mut log: ResMut<ShipLog>,
) {
    if !keys.just_pressed(settings.key(InputAction::CycleTarget)) {
        return;
    }
    targeting.subsystem = match targeting.subsystem {
        None => Some(SubsystemKind::Engines),
        Some(SubsystemKind::Engines) => Some(SubsystemKind::Weapons),
        Some(SubsystemKind::Weapons) => Some(SubsystemKind::Sensors),
        Some(SubsystemKind::Sensors) => Some(SubsystemKind::Drive),
        Some(SubsystemKind::Drive) => None,
    };
    log.log(match targeting.subsystem {
        Some(kind) => format!("Targeting: {}.", kind.label()),
        None => "Targeting: center mass.".into(),
    });
}

/// `Shift+R` (or whichever key is bound to `CycleTargetReverse`) cycles the
/// targeted subsystem in the opposite direction: drive → sensors → weapons →
/// engines → center mass.
pub fn cycle_target_reverse(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut targeting: ResMut<PlayerTargeting>,
    mut log: ResMut<ShipLog>,
) {
    if !keys.just_pressed(settings.key(InputAction::CycleTargetReverse)) {
        return;
    }
    targeting.subsystem = match targeting.subsystem {
        None => Some(SubsystemKind::Drive),
        Some(SubsystemKind::Drive) => Some(SubsystemKind::Sensors),
        Some(SubsystemKind::Sensors) => Some(SubsystemKind::Weapons),
        Some(SubsystemKind::Weapons) => Some(SubsystemKind::Engines),
        Some(SubsystemKind::Engines) => None,
    };
    log.log(match targeting.subsystem {
        Some(kind) => format!("Targeting: {}.", kind.label()),
        None => "Targeting: center mass.".into(),
    });
}

/// In-flight power split on quick keys (spec §22): Up/Down pick a system
/// (weapons/shields/engines), Left/Right move a notch, all within the same
/// budget the OnBoard power console manages. Sensors keep whatever the
/// console gave them — the stick only juggles the combat triangle.
pub fn power_quick_keys(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut select: ResMut<PowerSelect>,
    mut command: ResMut<ShipCommand>,
    mut log: ResMut<ShipLog>,
) {
    if keys.just_pressed(settings.key(InputAction::PowerSelectUp)) {
        select.0 = (select.0 + 2) % 3;
    }
    if keys.just_pressed(settings.key(InputAction::PowerSelectDown)) {
        select.0 = (select.0 + 1) % 3;
    }
    let delta: i8 = if keys.just_pressed(settings.key(InputAction::PowerAdjustRight)) {
        1
    } else if keys.just_pressed(settings.key(InputAction::PowerAdjustLeft)) {
        -1
    } else {
        return;
    };
    let used = command.power_weapons
        + command.power_shields
        + command.power_engines
        + command.power_sensors;
    let slot = match select.0 {
        0 => &mut command.power_weapons,
        1 => &mut command.power_shields,
        _ => &mut command.power_engines,
    };
    let next = (*slot as i8 + delta).clamp(0, POWER_MAX_NOTCH as i8) as u8;
    if next > *slot && used >= POWER_BUDGET {
        log.log("Power budget is spent — pull a notch from somewhere first.");
        return;
    }
    *slot = next;
    log.log(format!(
        "Power: WPN {} · SHD {} · ENG {} · SEN {}",
        command.power_weapons, command.power_shields, command.power_engines, command.power_sensors
    ));
}

/// `C` pops chaff (S19 escape deliverable): every hostile bolt in flight
/// dies, and every engaged enemy loses lock and breaks off for a beat.
pub fn pop_chaff(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut commands: Commands,
    mut systems: ResMut<ShipSystems>,
    bolts: Query<Entity, With<EnemyProjectile>>,
    mut enemies: Query<&mut EnemyShip>,
    mut log: ResMut<ShipLog>,
) {
    if !keys.just_pressed(settings.key(InputAction::LaunchChaff)) {
        return;
    }
    if systems.chaff == 0 {
        log.log("Chaff launcher is empty.");
        return;
    }
    systems.chaff -= 1;
    for e in &bolts {
        commands.entity(e).despawn();
    }
    for mut enemy in &mut enemies {
        if enemy.state != BehaviorState::Patrol {
            enemy.state = BehaviorState::Evade;
            enemy.fire = false;
        }
    }
    log.log(format!(
        "Chaff away — hostile locks broken ({} left).",
        systems.chaff
    ));
}

// --- crew during combat: the damage-control contract -------------------------

/// Runtime for the authored damage-control contract (spec §22 LLM table:
/// "which system to repair first under fire", fallback "repair nearest").
#[derive(Resource)]
pub struct DamageControl {
    pub contract: Contract,
    pub timer: Timer,
    last_action: Option<String>,
    /// True while a damage-control deliberation is in flight; when it
    /// clears (timeout → fallback), the repair lands.
    deliberating: bool,
}

impl Default for DamageControl {
    fn default() -> Self {
        DamageControl {
            contract: damage_control_contract(),
            timer: Timer::from_seconds(2.0, TimerMode::Repeating),
            last_action: None,
            deliberating: false,
        }
    }
}

/// The authored contract: one fire is routine (rules cover it); two or more
/// at once is exactly the uncovered edge that forces deliberation —
/// deliberation under fire is allowed, logged, and has a safe fallback.
fn damage_control_contract() -> Contract {
    Contract {
        id: "damage-control".into(),
        label: "Tove runs damage control".into(),
        trigger: Trigger::Timer {
            interval_secs: 2,
            repeat: true,
        },
        rules: vec![
            Rule {
                condition: Condition::Compare {
                    field: "burning".into(),
                    op: Comparison::Eq,
                    value: 0,
                },
                action: Action::verb("stand_down"),
                priority: 0,
            },
            Rule {
                condition: Condition::Compare {
                    field: "burning".into(),
                    op: Comparison::Eq,
                    value: 1,
                },
                action: Action::verb("repair_nearest"),
                priority: 5,
            },
        ],
        llm_authority: Some(LlmConfig {
            fallback_on_timeout: true,
            timeout_ms: 3000,
            max_tokens: 96,
            system_prompt: "You are Tove, ship's engineer. Triage: reactor > life \
                            support > weapons. Answer with the one room to save."
                .into(),
            fallback_action: Some(Action::verb("repair_nearest")),
        }),
    }
}

/// Evaluate the damage-control contract against the live fire state. One
/// fire: Tove hits the worst room without being asked. Multiple fires: her
/// rules run out and she deliberates (visible, logged), falling back to
/// "repair nearest" on timeout — the spec §22 default.
pub fn damage_control(
    time: Res<Time>,
    mut control: ResMut<DamageControl>,
    mut fires: ResMut<crate::systems::crisis::ShipFires>,
    mut deliberation: ResMut<DeliberationState>,
    mut log: ResMut<ShipLog>,
    mut feed: ResMut<crate::systems::comms::CommFeed>,
) {
    if !control.timer.tick(time.delta()).just_finished() {
        return;
    }
    let burning = fires.state.burning.len() as i64;

    // A pending deliberation that has since cleared = the fallback (or the
    // answer) fired — land the repair now.
    if control.deliberating && deliberation.active.is_none() {
        control.deliberating = false;
        repair_worst_room(&mut fires, &mut log);
    }

    let mut ctx = EvalContext::default();
    ctx.set("burning", burning);
    enum Decision {
        Act(String),
        Deliberate { timeout_ms: u32, fallback: Action },
    }
    let decision = match evaluate(&control.contract, &ctx) {
        Outcome::Rule { action, .. } => Decision::Act(action.kind.clone()),
        Outcome::Deliberate { llm } => Decision::Deliberate {
            timeout_ms: llm.timeout_ms,
            fallback: llm
                .fallback_action
                .clone()
                .unwrap_or_else(|| Action::verb("repair_nearest")),
        },
        Outcome::NoDecision => return,
    };
    match decision {
        Decision::Act(kind) => {
            if kind == "repair_nearest" {
                repair_worst_room(&mut fires, &mut log);
            }
            if control.last_action.as_deref() != Some(kind.as_str()) {
                if kind == "repair_nearest" {
                    feed.say("Tove", "On it. One fire is just Tuesday.");
                }
                control.last_action = Some(kind);
            }
        }
        Decision::Deliberate {
            timeout_ms,
            fallback,
        } => {
            control.last_action = None;
            if deliberation.active.is_some() || control.deliberating {
                return; // someone is already thinking; don't pile on
            }
            control.deliberating = true;
            log.log("Tove: two fires and one of me. Thinking…");
            deliberation.active = Some(Deliberation {
                crew_member: "Tove".into(),
                context_summary: format!("{burning} compartment fires at once — triage order?"),
                remaining: Timer::from_seconds(timeout_ms as f32 / 1000.0, TimerMode::Once),
                fallback,
                call_id: None,
                overlay_visible: true,
            });
        }
    }
}

/// The "repair nearest" default: knock down the most intense fire.
fn repair_worst_room(fires: &mut crate::systems::crisis::ShipFires, log: &mut ShipLog) {
    let worst = fires
        .state
        .burning
        .iter()
        .max_by_key(|(_, intensity)| **intensity)
        .map(|((deck, room), _)| (*deck, *room));
    if let Some((deck, room)) = worst {
        fires.state.fight(deck, room);
        log.log(format!("Tove fights the deck-{deck} fire (room {room})."));
    }
}

// --- HUD ---------------------------------------------------------------------

#[derive(Component)]
pub struct CombatReadout;

/// Spawn the combat strip under the fuel readout.
pub fn spawn_combat_hud(mut commands: Commands) {
    commands.spawn((
        CombatReadout,
        Text::new(""),
        TextFont {
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.75, 0.55)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(30.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
}

/// Combat HUD strip: power split (with the arrow cursor), shield, target,
/// chaff, hostiles (spec §22: power management "displayed on the HUD").
pub fn update_combat_hud(
    mode: Option<Res<State<GameMode>>>,
    command: Res<ShipCommand>,
    systems: Res<ShipSystems>,
    targeting: Res<PlayerTargeting>,
    select: Res<PowerSelect>,
    enemies: Query<&EnemyShip>,
    mut readout: Query<&mut Text, With<CombatReadout>>,
) {
    let Ok(mut text) = readout.single_mut() else {
        return;
    };
    let in_flight = matches!(mode.as_deref(), Some(m) if **m == GameMode::SpaceFlight);
    if !in_flight {
        **text = String::new();
        return;
    }
    let notches = [
        command.power_weapons,
        command.power_shields,
        command.power_engines,
    ];
    let power: Vec<String> = POWER_SLOTS
        .iter()
        .zip(notches)
        .enumerate()
        .map(|(i, (name, n))| {
            let cursor = if i == select.0 as usize { ">" } else { " " };
            format!("{cursor}{name} {n}")
        })
        .collect();
    let shield_max = player_shield_max(command.power_shields).max(1);
    let hostiles = enemies.iter().count();
    **text = format!(
        "PWR {} | SHD {:>3}/{} | TGT {} | CHAFF {} | HOSTILES {}",
        power.join(" "),
        systems.shield.0.min(shield_max),
        shield_max,
        targeting.subsystem.map(|s| s.label()).unwrap_or("center"),
        systems.chaff,
        hostiles,
    );
}
