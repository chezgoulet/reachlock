//! Landed combat (S20, spec §22 "Landed Combat"): Zelda-school top-down melee
//! in `GameMode::Landed`. Division of labor mirrors S19 exactly — ALL combat
//! math is `reachlock_core::combat` (integer state machine, arc geometry,
//! parry/dodge timing); this module is presentation and plumbing (floats are
//! fine here, they never feed back into core state).
//!
//! Everything simulates on a fixed 10 Hz integer tick so feel is frame-rate
//! independent (the classic i-frame bug the brief calls out). A single
//! accumulator ([`LandedTick`]) gates every tick-based system, so player
//! timers, enemy steps, hit resolution, and the companion all advance in
//! lockstep regardless of render rate. Input is captured every frame (edge-
//! triggered) into pending flags the next tick consumes.
//!
//! The derelict's *geometry* (walls, doors, the keycard gate) is authored
//! content the interior renderer will realize later; S20 delivers the combat
//! verbs. Enemies/props are laid out from `derelict_hold.ron` at scaled cell
//! coordinates in the Landed world and fought with the real state machine.

use bevy::prelude::*;

use reachlock_core::combat::{
    block_reduce, humanoid_step, in_melee_arc, is_dodging, AttackWindow, HostileArchetype,
    HumanoidIntent, HumanoidSenses, HumanoidState,
};
use reachlock_core::generator::FixedVec2;
use reachlock_core::util::rng::Fixed;

use crate::settings::{InputAction, Settings};
use crate::states::{CurrentLocation, GameMode, ModeScope};
use crate::systems::content_index::ContentIndex;
use crate::systems::contract::ShipLog;
use crate::systems::interior::Figure;
use crate::systems::inventory::PlayerInventory;
use crate::systems::mode::PlayerAvatar;

// --- tuning (presentation scale; core keeps raw fixed-point stat units) ------

/// Seconds per integer combat tick (10 Hz).
const TICK: f32 = 0.1;
/// World pixels per authored location cell.
const CELL: f32 = 40.0;
/// World pixels per 1.0 core fixed-point unit (`ONE == 1024`). Chosen so the
/// authored ranges read naturally: a 2.0-unit melee reach is ~80 px, an
/// 8.0-unit chase radius is ~320 px.
const WORLD_PER_UNIT: f32 = 40.0;
/// Horizontal spacing between the derelict's rooms on their linear spine.
const ROOM_PITCH: f32 = 640.0;

/// Melee cone half-angle, degrees (a forgiving ~110° frontal swing).
const ARC_HALF_DEGREES: u16 = 55;

const PLAYER_HP_MAX: i64 = 10_000;
const STAMINA_MAX: i64 = 1_000;
/// Stamina regained per tick.
const STAMINA_REGEN: i64 = 45;
const DODGE_COST: i64 = 300;
const DODGE_I_FRAMES: u32 = 4;
/// Dodge lunge speed, world px per frame while i-frames burn.
const DODGE_LUNGE: f32 = 22.0;
/// Parry window at the front of a raised guard (ticks).
const PLAYER_PARRY_TICKS: u32 = 2;

/// The player's melee profile (equip-slot integration with S17 `ItemRef` is a
/// follow-up — `PlayerInventory` has no weapon slot yet). Ranges are in core
/// fixed units like the archetypes.
const PLAYER_LIGHT: AttackWindow = AttackWindow {
    startup_ticks: 2,
    active_ticks: 2,
    recovery_ticks: 4,
    damage: 1300,
    range: 2600,
};
const PLAYER_HEAVY: AttackWindow = AttackWindow {
    startup_ticks: 5,
    active_ticks: 3,
    recovery_ticks: 8,
    damage: 2800,
    range: 3000,
};

/// Explosive barrel starting HP, its blast radius, and blast damage.
const BARREL_HP: i64 = 1200;
const BARREL_AOE_RADIUS: f32 = 120.0;
const BARREL_AOE_DAMAGE: i64 = 3000;
/// Breakable crate HP and the credits it drops.
const CRATE_HP: i64 = 900;
const CRATE_LOOT_CREDITS: i64 = 75;

/// Facing vectors indexed like `pixel::DIR_*` (down, up, left, right).
const DIRS: [Vec2; 4] = [Vec2::NEG_Y, Vec2::Y, Vec2::NEG_X, Vec2::X];

// --- components / resources --------------------------------------------------

/// Tags a hostile combatant (an enemy). Companions carry [`CompanionMarker`]
/// instead so the two never share a mutable [`Combatant`] borrow.
#[derive(Component)]
pub struct HostileMarker;

/// Tags the crew companion fighting alongside the player.
#[derive(Component)]
pub struct CompanionMarker;

/// Per-combatant state: the archetype it runs on plus the mutable bits the
/// core state machine reads/writes. Shared by enemies and the companion.
#[derive(Component)]
pub struct Combatant {
    pub archetype: HostileArchetype,
    pub state: HumanoidState,
    pub sub_timer: u32,
    pub hp: i64,
    pub patrol: [(i64, i64); 4],
    pub waypoint_index: u32,
    /// Ticks until the weapon is ready again (post-swing cooldown).
    pub attack_cooldown: u32,
    /// Took damage since the last decision tick (feeds `under_attack`).
    pub took_damage: bool,
    /// This swing has already connected — one hit per swing.
    pub swing_connected: bool,
    pub last_intent: HumanoidIntent,
}

impl Combatant {
    fn new(archetype: HostileArchetype, state: HumanoidState, patrol: [(i64, i64); 4]) -> Self {
        let hp = archetype.hp;
        Combatant {
            archetype,
            state,
            sub_timer: 0,
            hp,
            patrol,
            waypoint_index: 0,
            attack_cooldown: 0,
            took_damage: false,
            swing_connected: false,
            last_intent: HumanoidIntent::Idle,
        }
    }
}

/// An explosive barrel (HP). Detonates for AOE when destroyed.
#[derive(Component)]
pub struct ExplosiveBarrel(pub i64);

/// A breakable crate (HP). Drops loot when destroyed.
#[derive(Component)]
pub struct BreakableCrate(pub i64);

/// The landed-combat HUD text entity.
#[derive(Component)]
pub struct LandedHudText;

/// The whole player-side combat state for the current Landed visit.
#[derive(Resource, Default)]
pub struct LandedCombatState {
    pub lock_on: Option<Entity>,
    pub player_hp: i64,
    pub player_hp_max: i64,
    pub player_stamina: i64,
    pub player_stamina_max: i64,
    /// Elapsed ticks of the current player swing (only meaningful while
    /// `attacking`).
    pub attack_timer: u32,
    pub attacking: bool,
    pub player_attack_heavy: bool,
    pub attack_total: u32,
    pub player_swing_connected: bool,
    /// Ticks the guard has been held (0 at the moment it goes up).
    pub block_timer: u32,
    pub is_blocking: bool,
    /// Remaining dodge i-frames.
    pub dodge_timer: u32,
    pub is_dodging: bool,
    pub dodge_dir: Vec2,
    pub companion_entity: Option<Entity>,
    pub combat_active: bool,
    // input intents, captured each frame, consumed each tick
    pending_light: bool,
    pending_heavy: bool,
    pending_dodge: bool,
    pending_block: bool,
}

/// The shared 10 Hz tick gate: every tick-based system checks `fired`.
#[derive(Resource, Default)]
pub struct LandedTick {
    accum: f32,
    fired: bool,
}

// --- small conversions -------------------------------------------------------

/// World pixels → core fixed-point units.
fn to_fixed(px: f32) -> i64 {
    (px * (1024.0 / WORLD_PER_UNIT)) as i64
}

/// Core fixed-point units → world pixels.
fn from_fixed(v: i64) -> f32 {
    v as f32 * (WORLD_PER_UNIT / 1024.0)
}

/// Fixed-point HP fraction, 0..=1024.
fn hp_frac(hp: i64, max: i64) -> i64 {
    (hp.max(0) * 1024) / max.max(1)
}

/// Direction vector → 16-bit turn angle (the [`in_melee_arc`] convention:
/// +X == 0 turns, CCW). Float math is fine in the render layer.
fn angle_turns(v: Vec2) -> u16 {
    if v.length_squared() < 1.0e-6 {
        return 0;
    }
    let a = v.y.atan2(v.x); // -π..π
    let t = a.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    (t * 65536.0) as u16
}

/// The built-in crew companion class (spec §22 default: "follow player, attack
/// nearest, retreat at low HP"). Authored in code rather than content because
/// there is exactly one and it is not player-tunable.
fn companion_archetype() -> HostileArchetype {
    HostileArchetype {
        id: "companion".into(),
        display_name: "Tib".into(),
        hp: 9000,
        speed: 288,
        light_attack: AttackWindow {
            startup_ticks: 4,
            active_ticks: 3,
            recovery_ticks: 6,
            damage: 1100,
            range: 2560,
        },
        heavy_attack: AttackWindow {
            startup_ticks: 8,
            active_ticks: 4,
            recovery_ticks: 12,
            damage: 2200,
            range: 3072,
        },
        block: reachlock_core::combat::BlockWindow {
            active_ticks: 18,
            cooldown_ticks: 24,
            parry_ticks: 4,
        },
        dodge: reachlock_core::combat::DodgeWindow {
            i_frame_ticks: 8,
            recovery_ticks: 12,
            distance: 3072,
        },
        chase_radius: 12000,
        disengage_radius: 26000,
        // Retreat threshold: the companion breaks off at 30% HP per its
        // contract default (pure, instant — no LLM, so it never freezes the
        // fight). LLM-edge deliberation for "unexpected" is a follow-up.
        flee_hp_frac: 307,
    }
}

fn enemy_color(archetype_id: &str) -> Color {
    match archetype_id {
        "raider_boss" => Color::srgb(0.85, 0.2, 0.55),
        "security_bot" => Color::srgb(0.55, 0.6, 0.7),
        "raider_gunner" => Color::srgb(0.9, 0.6, 0.2),
        _ => Color::srgb(0.9, 0.25, 0.25),
    }
}

// --- 1. spawn ----------------------------------------------------------------

/// Realize the authored hostile location on entering Landed (spec §22). No
/// `hostile_location_id` on [`CurrentLocation`] means an ordinary station
/// landing — combat stays dormant. Always resets the combat state so a fresh
/// visit starts clean.
pub fn spawn_landed_enemies(
    mut commands: Commands,
    location: Res<CurrentLocation>,
    content: Res<ContentIndex>,
    mut state: ResMut<LandedCombatState>,
    mut log: ResMut<ShipLog>,
) {
    *state = LandedCombatState {
        player_hp: PLAYER_HP_MAX,
        player_hp_max: PLAYER_HP_MAX,
        player_stamina: STAMINA_MAX,
        player_stamina_max: STAMINA_MAX,
        ..default()
    };

    let Some(loc_id) = location.hostile_location_id.clone() else {
        return;
    };
    let Some(loc) = content.hostile_locations.get(&loc_id) else {
        warn!("landed combat: hostile location {loc_id:?} not in content index");
        return;
    };

    let mut count = 0u32;
    for (ri, room) in loc.rooms.iter().enumerate() {
        let origin = Vec2::new(ri as f32 * ROOM_PITCH, 0.0);
        for spawn in &room.spawns {
            let Some(arch) = content.hostile_archetypes.get(&spawn.archetype) else {
                warn!("landed combat: unknown archetype {}", spawn.archetype);
                continue;
            };
            let pos = origin + Vec2::new(spawn.pos.0 as f32 * CELL, spawn.pos.1 as f32 * CELL);
            let mut patrol = [(0i64, 0i64); 4];
            for (i, slot) in patrol.iter_mut().enumerate() {
                let cell = spawn.patrol.get(i).copied().unwrap_or(spawn.pos);
                let wp = origin + Vec2::new(cell.0 as f32 * CELL, cell.1 as f32 * CELL);
                *slot = (to_fixed(wp.x), to_fixed(wp.y));
            }
            commands.spawn((
                HostileMarker,
                Combatant::new(arch.clone(), HumanoidState::Patrol, patrol),
                Sprite::from_color(enemy_color(&spawn.archetype), Vec2::splat(22.0)),
                Transform::from_xyz(pos.x, pos.y, 6.0),
                ModeScope(GameMode::Landed),
            ));
            count += 1;
        }
        for prop in &room.props {
            let pos = origin + Vec2::new(prop.pos.0 as f32 * CELL, prop.pos.1 as f32 * CELL);
            match prop.kind.as_str() {
                "barrel" => {
                    commands.spawn((
                        ExplosiveBarrel(BARREL_HP),
                        Sprite::from_color(Color::srgb(0.85, 0.45, 0.12), Vec2::splat(18.0)),
                        Transform::from_xyz(pos.x, pos.y, 5.0),
                        ModeScope(GameMode::Landed),
                    ));
                }
                "crate" => {
                    commands.spawn((
                        BreakableCrate(CRATE_HP),
                        Sprite::from_color(Color::srgb(0.6, 0.46, 0.26), Vec2::splat(18.0)),
                        Transform::from_xyz(pos.x, pos.y, 5.0),
                        ModeScope(GameMode::Landed),
                    ));
                }
                other => warn!("landed combat: unknown prop kind {other:?}"),
            }
        }
    }

    // The crew companion accompanies the player, spawned near the entrance.
    let companion = commands
        .spawn((
            CompanionMarker,
            Combatant::new(companion_archetype(), HumanoidState::Idle, [(0, 0); 4]),
            Sprite::from_color(Color::srgb(0.3, 0.75, 0.95), Vec2::splat(20.0)),
            Transform::from_xyz(0.0, -40.0, 6.0),
            ModeScope(GameMode::Landed),
        ))
        .id();
    state.companion_entity = Some(companion);
    state.combat_active = count > 0;

    commands.spawn((
        LandedHudText,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.86, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(52.0),
            left: Val::Px(8.0),
            ..default()
        },
        ModeScope(GameMode::Landed),
    ));
    log.log(format!(
        "Boarding {}: {count} hostile(s). Tib is with you.",
        loc.display_name
    ));
}

// --- 2. player input (every frame) -------------------------------------------

/// Capture the player's combat input each frame: lock-on cycling (immediate)
/// and edge-triggered action requests the next tick consumes. Also applies the
/// dodge lunge, which must move at frame rate to look smooth (the i-frame
/// *timing* stays on the tick).
#[allow(clippy::type_complexity)]
pub fn landed_combat_player(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut state: ResMut<LandedCombatState>,
    hostiles: Query<(Entity, &Transform), (With<HostileMarker>, Without<PlayerAvatar>)>,
    mut avatar: Query<(&mut Transform, &Figure), With<PlayerAvatar>>,
) {
    if !state.combat_active {
        return;
    }
    let Ok((mut atx, figure)) = avatar.single_mut() else {
        return;
    };
    let player = atx.translation.truncate();

    // Lock-on candidates, nearest first (stable cycle order).
    let mut ranked: Vec<(Entity, f32)> = hostiles
        .iter()
        .map(|(e, t)| (e, (t.translation.truncate() - player).length()))
        .collect();
    ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let order: Vec<Entity> = ranked.into_iter().map(|(e, _)| e).collect();

    if keys.just_pressed(settings.key(InputAction::LockOnCycleNext)) {
        state.lock_on = cycle_lock(&order, state.lock_on, 1);
    }
    if keys.just_pressed(settings.key(InputAction::LockOnCyclePrev)) {
        state.lock_on = cycle_lock(&order, state.lock_on, -1);
    }
    if let Some(e) = state.lock_on {
        if !order.contains(&e) {
            state.lock_on = None;
        }
    }

    // Facing: toward the lock-on if any, else the walk facing.
    let facing = match state.lock_on.and_then(|e| hostiles.get(e).ok()) {
        Some((_, t)) => (t.translation.truncate() - player).normalize_or_zero(),
        None => DIRS[figure.dir.min(3)],
    };
    state.dodge_dir = facing;

    if keys.just_pressed(settings.key(InputAction::AttackLight)) {
        state.pending_light = true;
    }
    if keys.just_pressed(settings.key(InputAction::AttackHeavy)) {
        state.pending_heavy = true;
    }
    if keys.just_pressed(settings.key(InputAction::Dodge)) {
        state.pending_dodge = true;
    }
    state.pending_block = keys.pressed(settings.key(InputAction::Block));

    // Dodge lunge (render-rate motion; the i-frames are counted on the tick).
    if state.is_dodging && state.dodge_dir.length_squared() > 1.0e-6 {
        let step = state.dodge_dir.normalize() * DODGE_LUNGE * (time.delta_secs() / TICK);
        atx.translation.x += step.x;
        atx.translation.y += step.y;
    }
}

/// Advance the lock-on to the next/previous target in `order`.
fn cycle_lock(order: &[Entity], current: Option<Entity>, dir: i32) -> Option<Entity> {
    if order.is_empty() {
        return None;
    }
    let idx = current
        .and_then(|c| order.iter().position(|e| *e == c))
        .map(|i| (i as i32 + dir).rem_euclid(order.len() as i32) as usize)
        .unwrap_or(0);
    Some(order[idx])
}

// --- 3. the tick gate + enemy stepping ---------------------------------------

/// Accumulate real time into the 10 Hz combat tick. Runs first in the
/// FixedUpdate chain; the other tick-based systems read `fired`.
pub fn advance_landed_tick(
    time: Res<Time>,
    state: Res<LandedCombatState>,
    mut tick: ResMut<LandedTick>,
) {
    tick.fired = false;
    if !state.combat_active {
        tick.accum = 0.0;
        return;
    }
    tick.accum += time.delta_secs();
    if tick.accum >= TICK {
        tick.accum -= TICK;
        tick.fired = true;
    }
}

/// Step every hostile one combat tick: advance player timers, then run each
/// enemy's core state machine and apply the returned intent to its transform.
#[allow(clippy::type_complexity)]
pub fn step_landed_enemies(
    mut commands: Commands,
    tick: Res<LandedTick>,
    mut state: ResMut<LandedCombatState>,
    avatar: Query<&Transform, (With<PlayerAvatar>, Without<HostileMarker>)>,
    mut hostiles: Query<(Entity, &mut Combatant, &mut Transform), With<HostileMarker>>,
) {
    if !tick.fired {
        return;
    }
    advance_player_timers(&mut state);

    let Ok(ptx) = avatar.single() else {
        return;
    };
    let player = ptx.translation.truncate();
    let living = hostiles.iter().count() as u32;

    for (entity, mut c, mut tx) in &mut hostiles {
        if c.hp <= 0 {
            if state.lock_on == Some(entity) {
                state.lock_on = None;
            }
            commands.entity(entity).despawn();
            continue;
        }
        if c.attack_cooldown > 0 {
            c.attack_cooldown -= 1;
        }
        let self_pos = tx.translation.truncate();
        // Reborrow the inner `Combatant`: split-field borrows (needed by
        // `humanoid_step`) don't reach through Bevy's `Mut<T>`.
        let c = c.into_inner();
        let senses = build_senses(c, self_pos, player, living.saturating_sub(1));
        let intent = humanoid_step(&mut c.state, &mut c.sub_timer, &senses, &c.archetype);
        c.last_intent = intent;
        if c.state != HumanoidState::Attack {
            c.swing_connected = false;
            if matches!(
                intent,
                HumanoidIntent::LightAttack(..) | HumanoidIntent::HeavyAttack(..)
            ) {
                // Swing just ended this tick — start the cooldown.
                c.attack_cooldown = c.archetype.light_attack.recovery_ticks.max(1);
            }
        }
        apply_intent_movement(c, &mut tx, intent);
        c.took_damage = false;
    }

    // Victory: the last hostile fell. Revive a downed companion and stand down.
    if living == 0 && state.combat_active {
        state.combat_active = false;
    }
}

/// Build a decision-tick's senses for one combatant from world positions.
fn build_senses(c: &Combatant, self_pos: Vec2, target: Vec2, ally_count: u32) -> HumanoidSenses {
    let d = target - self_pos;
    let dist_px = d.length();
    let dist = to_fixed(dist_px);
    let reach = c
        .archetype
        .light_attack
        .range
        .max(c.archetype.heavy_attack.range);
    HumanoidSenses {
        to_target: FixedVec2 {
            x: Fixed(to_fixed(d.x)),
            y: Fixed(to_fixed(d.y)),
        },
        dist_to_target: dist,
        hp_frac: hp_frac(c.hp, c.archetype.hp),
        weapon_ready: c.attack_cooldown == 0,
        target_in_range: dist <= reach,
        target_telegraphing: false,
        under_attack: c.took_damage,
        ally_count,
        patrol_waypoints: c.patrol,
        waypoint_index: c.waypoint_index,
    }
}

/// Apply a returned intent to a combatant's transform. Walk carries an
/// absolute waypoint in Patrol (steer to it, advance on arrival) and a
/// direction otherwise; attacks are rooted.
fn apply_intent_movement(c: &mut Combatant, tx: &mut Transform, intent: HumanoidIntent) {
    let speed_px = (c.archetype.speed as f32 / 1024.0) * WORLD_PER_UNIT;
    if let HumanoidIntent::Walk(x, y) = intent {
        let here = tx.translation.truncate();
        let dir = if c.state == HumanoidState::Patrol {
            let wp = Vec2::new(from_fixed(x), from_fixed(y));
            let to = wp - here;
            if to.length() < CELL * 0.5 {
                c.waypoint_index = (c.waypoint_index + 1) % 4;
            }
            to
        } else {
            Vec2::new(x as f32, y as f32) // direction only; magnitude irrelevant
        };
        if dir.length_squared() > 1.0e-6 {
            let step = dir.normalize() * speed_px;
            tx.translation.x += step.x;
            tx.translation.y += step.y;
        }
    }
}

/// Advance the player's own tick timers: stamina regen, block hold, dodge
/// i-frames, and swing progression. Consumes the pending input flags.
fn advance_player_timers(state: &mut LandedCombatState) {
    state.player_stamina = (state.player_stamina + STAMINA_REGEN).min(state.player_stamina_max);

    // Block is a hold; it can't overlap a dodge or an in-progress swing.
    if state.pending_block && state.dodge_timer == 0 && !state.attacking {
        if state.is_blocking {
            state.block_timer = state.block_timer.saturating_add(1);
        } else {
            state.is_blocking = true;
            state.block_timer = 0;
        }
    } else {
        state.is_blocking = false;
        state.block_timer = 0;
    }

    // Dodge start (costs stamina; grants i-frames).
    if state.pending_dodge
        && state.dodge_timer == 0
        && !state.is_blocking
        && state.player_stamina >= DODGE_COST
    {
        state.player_stamina -= DODGE_COST;
        state.dodge_timer = DODGE_I_FRAMES;
        state.is_dodging = true;
    }
    state.pending_dodge = false;
    if state.dodge_timer > 0 {
        state.dodge_timer -= 1;
        if state.dodge_timer == 0 {
            state.is_dodging = false;
        }
    }

    // Attack start (light/heavy), when idle and not mid-dodge.
    if !state.attacking && !state.is_dodging {
        if state.pending_heavy {
            start_player_attack(state, true);
        } else if state.pending_light {
            start_player_attack(state, false);
        }
    }
    state.pending_light = false;
    state.pending_heavy = false;

    // Attack progression.
    if state.attacking {
        state.attack_timer += 1;
        if state.attack_timer >= state.attack_total {
            state.attacking = false;
            state.attack_timer = 0;
            state.player_swing_connected = false;
        }
    }
}

fn start_player_attack(state: &mut LandedCombatState, heavy: bool) {
    let win = if heavy { PLAYER_HEAVY } else { PLAYER_LIGHT };
    state.attacking = true;
    state.player_attack_heavy = heavy;
    state.attack_timer = 0;
    state.attack_total = win.total_ticks();
    state.player_swing_connected = false;
}

// --- 4. hit resolution -------------------------------------------------------

/// Resolve melee connects this tick: hostile swings against the player (blocked
/// / parried / dodged per the core math) and the player's swing against the
/// locked target and any barrels/crates in the arc.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn apply_landed_hits(
    tick: Res<LandedTick>,
    mut state: ResMut<LandedCombatState>,
    mut log: ResMut<ShipLog>,
    avatar: Query<(&Transform, &Figure), (With<PlayerAvatar>, Without<HostileMarker>)>,
    mut hostiles: Query<(Entity, &Transform, &mut Combatant), With<HostileMarker>>,
    mut barrels: Query<(&Transform, &mut ExplosiveBarrel)>,
    mut crates: Query<(&Transform, &mut BreakableCrate)>,
) {
    if !tick.fired {
        return;
    }
    let Ok((ptx, figure)) = avatar.single() else {
        return;
    };
    let player = ptx.translation.truncate();
    let player_i = (player.x as i64, player.y as i64);

    // --- hostile swings vs the player ---
    for (_e, tx, mut c) in &mut hostiles {
        if c.state != HumanoidState::Attack || c.swing_connected {
            continue;
        }
        let heavy = hp_frac(c.hp, c.archetype.hp) * 2 < 1024;
        let win = if heavy {
            c.archetype.heavy_attack
        } else {
            c.archetype.light_attack
        };
        if !win.is_active(c.sub_timer) {
            continue;
        }
        let epos = tx.translation.truncate();
        let facing = angle_turns(player - epos);
        let hit = in_melee_arc(
            (epos.x as i64, epos.y as i64),
            facing,
            player_i,
            from_fixed(win.range) as i64,
            ARC_HALF_DEGREES,
        );
        if !hit {
            continue;
        }
        c.swing_connected = true;
        if is_dodging(state.dodge_timer) {
            log.log(format!("You dodge {}'s strike.", c.archetype.display_name));
            continue;
        }
        let dmg = if state.is_blocking {
            block_reduce(win.damage, state.block_timer, PLAYER_PARRY_TICKS)
        } else {
            win.damage
        };
        state.player_hp -= dmg;
        if dmg == 0 {
            log.log(format!(
                "Parry! {}'s blow deflected.",
                c.archetype.display_name
            ));
        } else if state.is_blocking {
            log.log(format!("Blocked — {dmg} through the guard."));
        }
        if state.player_hp <= 0 {
            state.player_hp = 0;
            log.log("You are downed. Tib drags you clear.");
            state.combat_active = false;
        }
    }

    // --- player swing vs the locked target + destructibles ---
    if !state.attacking {
        return;
    }
    let win = if state.player_attack_heavy {
        PLAYER_HEAVY
    } else {
        PLAYER_LIGHT
    };
    if state.player_swing_connected || !win.is_active(state.attack_timer) {
        return;
    }
    let facing = match state.lock_on.and_then(|e| hostiles.get(e).ok()) {
        Some((_, t, _)) => angle_turns(t.translation.truncate() - player),
        None => angle_turns(DIRS[figure.dir.min(3)]),
    };
    let range_px = from_fixed(win.range) as i64;
    let mut connected = false;

    if let Some(target) = state.lock_on {
        if let Ok((_, ttx, mut tc)) = hostiles.get_mut(target) {
            let tpos = ttx.translation.truncate();
            if in_melee_arc(
                player_i,
                facing,
                (tpos.x as i64, tpos.y as i64),
                range_px,
                ARC_HALF_DEGREES,
            ) {
                tc.hp -= win.damage;
                tc.took_damage = true;
                connected = true;
                log.log(format!(
                    "You strike {} for {}.",
                    tc.archetype.display_name, win.damage
                ));
            }
        }
    }
    // Player swings also shatter props in the arc.
    for (btx, mut barrel) in &mut barrels {
        let bp = btx.translation.truncate();
        if in_melee_arc(
            player_i,
            facing,
            (bp.x as i64, bp.y as i64),
            range_px,
            ARC_HALF_DEGREES,
        ) {
            barrel.0 -= win.damage;
            connected = true;
        }
    }
    for (ctx, mut crate_hp) in &mut crates {
        let cp = ctx.translation.truncate();
        if in_melee_arc(
            player_i,
            facing,
            (cp.x as i64, cp.y as i64),
            range_px,
            ARC_HALF_DEGREES,
        ) {
            crate_hp.0 -= win.damage;
            connected = true;
        }
    }
    if connected {
        state.player_swing_connected = true;
    }
}

// --- 5. companion ------------------------------------------------------------

/// Drive the crew companion one tick: target the nearest hostile, run the same
/// core state machine, and land its swings. Purely threshold-driven (instant),
/// so the fight never freezes on it (spec §22 gotcha). Revives when the room
/// is clear.
#[allow(clippy::type_complexity)]
pub fn companion_combat_system(
    tick: Res<LandedTick>,
    state: Res<LandedCombatState>,
    mut log: ResMut<ShipLog>,
    avatar: Query<
        &Transform,
        (
            With<PlayerAvatar>,
            Without<CompanionMarker>,
            Without<HostileMarker>,
        ),
    >,
    mut companion: Query<
        (&mut Combatant, &mut Transform),
        (With<CompanionMarker>, Without<HostileMarker>),
    >,
    mut hostiles: Query<
        (Entity, &Transform, &mut Combatant),
        (With<HostileMarker>, Without<CompanionMarker>),
    >,
) {
    if !tick.fired {
        return;
    }
    let Some(companion_e) = state.companion_entity else {
        return;
    };
    let Ok((mut c, mut tx)) = companion.get_mut(companion_e) else {
        return;
    };
    let self_pos = tx.translation.truncate();
    let player = avatar
        .single()
        .map(|t| t.translation.truncate())
        .unwrap_or(self_pos);

    // Nearest living hostile.
    let mut nearest: Option<(Entity, Vec2, f32)> = None;
    for (e, htx, _) in hostiles.iter() {
        let hp = htx.translation.truncate();
        let d = (hp - self_pos).length();
        if nearest.map(|(_, _, best)| d < best).unwrap_or(true) {
            nearest = Some((e, hp, d));
        }
    }

    // Downed companion revives once the room is clear.
    if c.state == HumanoidState::Downed {
        if nearest.is_none() {
            c.hp = c.archetype.hp;
            c.state = HumanoidState::Idle;
            log.log("Tib is back on her feet.");
        }
        return;
    }

    if c.attack_cooldown > 0 {
        c.attack_cooldown -= 1;
    }
    // Reborrow the inner `Combatant` for split-field borrows (see step system).
    let c = c.into_inner();
    // Target the nearest hostile, or follow the player when the room is clear.
    let target = nearest.map(|(_, p, _)| p).unwrap_or(player);
    let ally_count = hostiles.iter().count() as u32;
    let senses = build_senses(c, self_pos, target, ally_count);
    let intent = humanoid_step(&mut c.state, &mut c.sub_timer, &senses, &c.archetype);
    c.last_intent = intent;
    if c.state != HumanoidState::Attack {
        c.swing_connected = false;
    }
    apply_intent_movement(c, &mut tx, intent);
    c.took_damage = false;

    // Land the companion's swing on the nearest hostile.
    if c.state == HumanoidState::Attack && !c.swing_connected {
        let heavy = hp_frac(c.hp, c.archetype.hp) * 2 < 1024;
        let win = if heavy {
            c.archetype.heavy_attack
        } else {
            c.archetype.light_attack
        };
        if win.is_active(c.sub_timer) {
            if let Some((e, hpos, _)) = nearest {
                let facing = angle_turns(hpos - self_pos);
                if in_melee_arc(
                    (self_pos.x as i64, self_pos.y as i64),
                    facing,
                    (hpos.x as i64, hpos.y as i64),
                    from_fixed(win.range) as i64,
                    ARC_HALF_DEGREES,
                ) {
                    c.swing_connected = true;
                    c.attack_cooldown = win.recovery_ticks.max(1);
                    if let Ok((_, _, mut tc)) = hostiles.get_mut(e) {
                        tc.hp -= win.damage;
                        tc.took_damage = true;
                    }
                }
            }
        }
    }
}

// --- 6. render ---------------------------------------------------------------

/// Draw lock-on marker + health bars with gizmos and update the HUD text.
#[allow(clippy::type_complexity)]
pub fn render_landed_combat(
    state: Res<LandedCombatState>,
    mut gizmos: Gizmos,
    hostiles: Query<(Entity, &Transform, &Combatant), With<HostileMarker>>,
    companion: Query<(&Transform, &Combatant), (With<CompanionMarker>, Without<HostileMarker>)>,
    mut hud: Query<&mut Text, With<LandedHudText>>,
) {
    if !state.combat_active {
        if let Ok(mut text) = hud.single_mut() {
            text.clear();
        }
        return;
    }
    for (entity, tx, c) in &hostiles {
        let p = tx.translation.truncate();
        health_bar(
            &mut gizmos,
            p,
            c.hp,
            c.archetype.hp,
            Color::srgb(0.9, 0.3, 0.3),
        );
        if state.lock_on == Some(entity) {
            lock_box(&mut gizmos, p, 18.0, Color::srgb(1.0, 0.9, 0.2));
        }
    }
    if let Ok((tx, c)) = companion.single() {
        let color = if c.state == HumanoidState::Downed {
            Color::srgb(0.5, 0.5, 0.5)
        } else {
            Color::srgb(0.3, 0.8, 0.95)
        };
        health_bar(
            &mut gizmos,
            tx.translation.truncate(),
            c.hp,
            c.archetype.hp,
            color,
        );
    }
    if let Ok(mut text) = hud.single_mut() {
        let stance = if state.is_dodging {
            "DODGE"
        } else if state.is_blocking {
            "BLOCK"
        } else if state.attacking {
            "ATTACK"
        } else {
            "--"
        };
        **text = format!(
            "HP {}/{}   STAM {}/{}   {}   HOSTILES {}   LOCK {}",
            state.player_hp.max(0),
            state.player_hp_max,
            state.player_stamina.max(0),
            state.player_stamina_max,
            stance,
            hostiles.iter().count(),
            if state.lock_on.is_some() { "on" } else { "off" },
        );
    }
}

/// A small floating HP bar above a combatant (two gizmo lines).
fn health_bar(gizmos: &mut Gizmos, pos: Vec2, hp: i64, max: i64, color: Color) {
    const W: f32 = 28.0;
    let y = pos.y + 20.0;
    let left = pos.x - W / 2.0;
    let frac = (hp.max(0) as f32 / max.max(1) as f32).clamp(0.0, 1.0);
    gizmos.line_2d(
        Vec2::new(left, y),
        Vec2::new(left + W, y),
        Color::srgb(0.15, 0.15, 0.15),
    );
    gizmos.line_2d(Vec2::new(left, y), Vec2::new(left + W * frac, y), color);
}

/// A square lock-on reticle drawn as four gizmo lines.
fn lock_box(gizmos: &mut Gizmos, c: Vec2, r: f32, color: Color) {
    let tl = Vec2::new(c.x - r, c.y + r);
    let tr = Vec2::new(c.x + r, c.y + r);
    let bl = Vec2::new(c.x - r, c.y - r);
    let br = Vec2::new(c.x + r, c.y - r);
    gizmos.line_2d(tl, tr, color);
    gizmos.line_2d(tr, br, color);
    gizmos.line_2d(br, bl, color);
    gizmos.line_2d(bl, tl, color);
}

// --- 7. props ----------------------------------------------------------------

/// Resolve destroyed props: barrels detonate (AOE to nearby hostiles), crates
/// drop loot. Runs every frame; HP is driven down on the combat tick.
pub fn step_props(
    mut commands: Commands,
    state: Res<LandedCombatState>,
    barrels: Query<(Entity, &Transform, &ExplosiveBarrel)>,
    crates: Query<(Entity, &BreakableCrate)>,
    mut hostiles: Query<(&Transform, &mut Combatant), With<HostileMarker>>,
    mut inventory: ResMut<PlayerInventory>,
    mut log: ResMut<ShipLog>,
) {
    if !state.combat_active {
        return;
    }
    for (entity, tx, barrel) in &barrels {
        if barrel.0 > 0 {
            continue;
        }
        let center = tx.translation.truncate();
        commands.entity(entity).despawn();
        for (htx, mut c) in &mut hostiles {
            if (htx.translation.truncate() - center).length() <= BARREL_AOE_RADIUS {
                c.hp -= BARREL_AOE_DAMAGE;
                c.took_damage = true;
            }
        }
        log.log("An explosive barrel detonates!");
    }
    for (entity, crate_hp) in &crates {
        if crate_hp.0 > 0 {
            continue;
        }
        commands.entity(entity).despawn();
        inventory.credits += CRATE_LOOT_CREDITS;
        log.log(format!("Cracked a crate (+{CRATE_LOOT_CREDITS} cr)."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_conversion_round_trips() {
        // 80 px == 2.0 core units == 2048 fixed.
        assert_eq!(to_fixed(80.0), 2048);
        assert!((from_fixed(2048) - 80.0).abs() < 0.01);
    }

    #[test]
    fn angle_turns_cardinals() {
        assert_eq!(angle_turns(Vec2::X), 0);
        assert_eq!(angle_turns(Vec2::Y), 16384);
        assert_eq!(angle_turns(Vec2::NEG_X), 32768);
        // Zero vector is a safe default, never a NaN.
        assert_eq!(angle_turns(Vec2::ZERO), 0);
    }

    #[test]
    fn lock_cycles_and_wraps() {
        let a = Entity::from_raw_u32(1).unwrap();
        let b = Entity::from_raw_u32(2).unwrap();
        let c = Entity::from_raw_u32(3).unwrap();
        let order = [a, b, c];
        assert_eq!(cycle_lock(&order, None, 1), Some(a));
        assert_eq!(cycle_lock(&order, Some(a), 1), Some(b));
        assert_eq!(cycle_lock(&order, Some(c), 1), Some(a), "wraps forward");
        assert_eq!(cycle_lock(&order, Some(a), -1), Some(c), "wraps back");
        assert_eq!(cycle_lock(&[], Some(a), 1), None);
    }

    #[test]
    fn companion_retreats_before_it_dies() {
        // The companion archetype must actually be able to flee (non-zero
        // threshold), or "retreat at low HP" is a dead letter.
        assert!(companion_archetype().flee_hp_frac > 0);
    }
}
