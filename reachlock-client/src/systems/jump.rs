//! Gate jump, hyperspace transit, emergency self-jump, and fuel dock
//! (spec §14 Mode 3; S09). The headline: flying into a gate ring and
//! pressing ENTER plays the cryo-pilot transit — a `Hyperspace` mode where
//! a seeded anomaly can force Boris to deliberate mid-jump, and waking
//! regenerates the world into the destination system. Determinism discipline:
//! every roll derives from `(system_seed, jump_count)` — never wall time.

use bevy::prelude::*;
use bevy::time::TimerMode;

use reachlock_core::contract::types::Action;
use reachlock_core::generator::transit::{
    anomaly_rolls, malfunction_roll, malfunction_roll_under_fire, transit_destination,
    SELF_JUMP_BURN,
};
use reachlock_core::network::ClientMessage;
use reachlock_core::seed::types::{Seed, SystemId};

use crate::net::{NetMode, NetOutbox};
use crate::settings::{InputAction, Settings};
use crate::states::{CurrentLocation, GameMode, SceneRegistry};
use crate::systems::contract::{DeliberationState, ShipLog};
use crate::systems::inventory::PlayerInventory;
use crate::systems::setup::Gate;
use crate::systems::ship::{PlayerShip, ShipSystems};

/// Seconds the hyperspace transit lasts.
pub const TRANSIT_SECS: f32 = 4.0;
/// How close (world units) the ship must be to a gate to jump.
// The gate torus is ~165 units in radius; reach must cover a ship anywhere
// inside or brushing the ring (the old 70 required threading the exact
// center at speed).
pub const GATE_REACH: f32 = 190.0;
/// Credits per 1/1024 of fuel when refueling at a dock.
pub const FUEL_PRICE_PER_UNIT: i64 = 1;

/// Live transit bookkeeping. `active` gates the systems; the world is
/// regenerated on wake.
#[derive(Resource)]
pub struct TransitState {
    pub active: bool,
    pub timer: Timer,
    pub dest_seed: u64,
    pub jump_count: u64,
    pub anomaly_fired: bool,
    /// Who runs the crossing: Boris for gate transits (the cryo-pilot
    /// contract), Prudence for programmed self-jumps (SHIPS.md §3 — the
    /// synthetic crew has the ship while the humans sleep).
    pub pilot: String,
}

impl Default for TransitState {
    fn default() -> Self {
        TransitState {
            active: false,
            timer: Timer::default(),
            dest_seed: 0,
            jump_count: 0,
            anomaly_fired: false,
            pilot: "Boris".into(),
        }
    }
}

/// Screen-fixed hyperspace wash, spawned/despawned by `hyperspace_tick`.
#[derive(Component)]
pub struct TransitVisual;

/// ENTER near a gate → engage the jump drive (Hyperspace mode).
#[allow(clippy::too_many_arguments)]
pub fn try_gate_jump(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    ship: Query<&Transform, With<PlayerShip>>,
    gates: Query<&Transform, With<Gate>>,
    mut state: ResMut<TransitState>,
    location: Res<CurrentLocation>,
    mut next: ResMut<NextState<GameMode>>,
    mut log: ResMut<ShipLog>,
) {
    if state.active {
        return;
    }
    let Ok(ship_pos) = ship.single() else {
        return;
    };
    let near = gates
        .iter()
        .any(|g| g.translation.distance(ship_pos.translation) <= GATE_REACH);
    if !near || !keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        return;
    }
    state.active = true;
    state.anomaly_fired = false;
    state.dest_seed = transit_destination(location.system_seed, state.jump_count);
    state.timer = Timer::from_seconds(TRANSIT_SECS, TimerMode::Once);
    state.pilot = "Boris".into();
    log.log(format!(
        "Gate transit engaged → system {:#x} (stable window; crew stays awake)",
        state.dest_seed
    ));
    next.set(GameMode::Hyperspace);
}

/// Tick the transit: wash in, optional anomaly (cryo-pilot deliberation),
/// then wake into the destination system.
#[allow(clippy::too_many_arguments)]
pub fn hyperspace_tick(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<TransitState>,
    mut location: ResMut<CurrentLocation>,
    mut registry: ResMut<SceneRegistry>,
    mut next: ResMut<NextState<GameMode>>,
    mut deliberation: ResMut<DeliberationState>,
    visuals: Query<Entity, With<TransitVisual>>,
    mut log: ResMut<ShipLog>,
    mode: Res<NetMode>,
    mut outbox: ResMut<NetOutbox>,
    mut plan: ResMut<crate::systems::cryojump::JumpPlan>,
    mut roster: ResMut<crate::systems::crew::CrewRoster>,
    mut deck: ResMut<crate::systems::interior::ActiveDeck>,
    mut feed: ResMut<crate::systems::comms::CommFeed>,
) {
    if !state.active {
        return;
    }
    // Wash in on the first tick.
    if visuals.is_empty() {
        commands.spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.25, 0.5, 0.55)),
            TransitVisual,
        ));
    }

    state.timer.tick(time.delta());
    let frac = (state.timer.elapsed_secs() as f64) / (TRANSIT_SECS as f64);

    // Mid-transit anomaly: force the cryo-pilot to deliberate.
    if frac >= 0.5 && !state.anomaly_fired {
        state.anomaly_fired = true;
        let story = "Uncovered field with no covering rule. Holding course until \
                      the edge resolves."
            .to_string();
        if anomaly_rolls(location.system_seed, state.jump_count) {
            let d: crate::systems::contract::Deliberation =
                crate::systems::contract::Deliberation {
                    crew_member: state.pilot.clone(),
                    context_summary: story.clone(),
                    remaining: Timer::from_seconds(1.5, TimerMode::Once),
                    fallback: Action::verb("hold course"),
                    call_id: None,
                    overlay_visible: true,
                };
            deliberation.active = Some(d);
            log.log(format!("Hyperspace anomaly: {story}"));
        }
    }

    if state.timer.is_finished() {
        // Wake: regenerate the world into the destination system.
        location.system_seed = state.dest_seed;
        state.jump_count = state.jump_count.wrapping_add(1);
        state.active = false;
        deliberation.active = None;
        for e in &visuals {
            commands.entity(e).despawn();
        }
        // Invalidate the scene registry so `enter_spaceflight` (triggered by
        // the transition back to SpaceFlight below) does NOT early-return and
        // actually rebuilds the world from `location.system_seed`.
        registry.scene = None;
        // S02 integration: discover the destination seed with the server.
        let dest_id = SystemId(format!("spike-{:x}", location.system_seed));
        match &*mode {
            NetMode::Online { universe, .. } => {
                outbox.push(ClientMessage::SeedDiscover {
                    universe: *universe,
                    system_id: dest_id,
                    seed: Seed::new(location.system_seed),
                });
                log.log(format!(
                    "Arrived — synchronizing system {:#x}…",
                    location.system_seed
                ));
            }
            NetMode::Offline => {
                log.log(format!("Arrived in system {:#x}", location.system_seed));
            }
        }
        // SHIPS.md §3 step 4: a cryo transit wakes the sleepers in the
        // cryo chamber — the walk back to the cockpit is part of arrival.
        // Gate transits stay at the helm (crew was awake the whole way).
        if plan.cryo_wake {
            crate::systems::cryojump::revive(&mut plan, &mut roster, &mut log, &mut feed);
            if let Some((deck_index, spawn)) = crate::systems::interior::cryo_wake_spawn() {
                deck.index = deck_index;
                deck.spawn = Some(spawn);
            }
            next.set(GameMode::OnBoard);
        } else {
            next.set(GameMode::SpaceFlight);
        }
    }
}

/// Emergency self-jump (`J` in flight): higher fuel cost + a seeded
/// malfunction roll — a WORSE one with hostiles engaged (S19 escape wiring:
/// spooling the drive under fire raises the malfunction odds, spec §22).
#[allow(clippy::too_many_arguments)]
/// Never a silent fail — the log narrates.
pub fn self_jump(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut state: ResMut<TransitState>,
    location: Res<CurrentLocation>,
    mut systems: ResMut<ShipSystems>,
    enemies: Query<&crate::systems::combat::EnemyShip>,
    mut next: ResMut<NextState<GameMode>>,
    mut log: ResMut<ShipLog>,
) {
    if state.active || !keys.just_pressed(settings.key(InputAction::OpenMissionBoard)) {
        return;
    }
    let cost = SELF_JUMP_BURN;
    // Burn the panic tax from the current tank (clamped to empty).
    if systems.fuel.0 > 0 {
        systems.fuel = fixed_clamp(systems.fuel.0.saturating_sub(cost));
    }
    let under_fire = enemies
        .iter()
        .any(|e| !matches!(e.state, reachlock_core::combat::BehaviorState::Patrol));
    let sev = if under_fire {
        malfunction_roll_under_fire(location.system_seed, state.jump_count)
    } else {
        malfunction_roll(location.system_seed, state.jump_count)
    };
    state.jump_count = state.jump_count.wrapping_add(1);
    let outcome = match sev {
        0 => "clean arrival",
        1 => "arrived off-course",
        2 => "hull stress damage",
        _ => "hull stress + cargo shift",
    };
    // Self-jump is a panic: it uses the same hyperspace wash but stays
    // in-system (destination = current seed) and pre-arms so the transit
    // anomaly doesn't double-fire on top of the malfunction roll.
    state.active = true;
    state.dest_seed = location.system_seed;
    state.anomaly_fired = true;
    state.pilot = "Boris".into();
    state.timer = Timer::from_seconds(TRANSIT_SECS, TimerMode::Once);
    // You were AWAKE for that (docs/LORE.md §III): unshielded proximity to
    // an open window costs flesh. Recoverable — barely — and never free.
    // The programmed cryo route (nav console → pods) is the survivable one.
    systems.hull_hp = fixed_clamp((systems.hull_hp.0 - 400).max(64));
    // S19: a bad roll under fire stresses the frame further (still never
    // below the survivable floor).
    if sev >= 2 {
        systems.hull_hp = fixed_clamp((systems.hull_hp.0 - 200).max(64));
    }
    let fire_note = if under_fire {
        " Spooling under fire — the drive screamed the whole way."
    } else {
        ""
    };
    log.log(format!(
        "Emergency self-jump: {outcome} (fuel {cost}/1024).{fire_note} You were awake for it. \
         Your nose is bleeding and the corridor lights look wrong."
    ));
    next.set(GameMode::Hyperspace);
}

/// Refuel at a dock for credits (S09: "fuel dock"). Press `F` while
/// landed/docked; fills the tank and charges the wallet.
pub fn fuel_dock(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    location: Res<CurrentLocation>,
    mut inv: ResMut<PlayerInventory>,
    mut systems: ResMut<ShipSystems>,
    mut log: ResMut<ShipLog>,
) {
    if !location.is_docked || !keys.just_pressed(settings.key(InputAction::FireMissile)) {
        return;
    }
    let need = (1024 - systems.fuel.0).max(0);
    let cost = need * FUEL_PRICE_PER_UNIT;
    if inv.credits < cost {
        log.log(format!("Can't afford refuel ({cost}cr needed)"));
        return;
    }
    inv.credits -= cost;
    systems.fuel = reachlock_core::util::rng::Fixed(1024);
    log.log(format!("Refueled at {:.0}cr", cost));
}

/// Clamp a raw fuel value into `[0, 1024]` as a `Fixed`.
fn fixed_clamp(v: i64) -> reachlock_core::util::rng::Fixed {
    reachlock_core::util::rng::Fixed(v.clamp(0, 1024))
}
