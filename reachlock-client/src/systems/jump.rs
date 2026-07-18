//! Gate jump, hyperspace transit, emergency self-jump, and fuel dock
//! (spec §14 Mode 3; S09). The headline: flying into a gate ring and
//! pressing ENTER plays the cryo-pilot transit — a `Hyperspace` mode where
//! a seeded anomaly can force Boris to deliberate mid-jump, and waking
//! regenerates the world into the destination system. Determinism discipline:
//! every roll derives from `(system_seed, jump_count)` — never wall time.

use bevy::prelude::*;
use bevy::time::TimerMode;

use reachlock_core::contract::types::Action;
use reachlock_core::galaxy::{deep_space_seed, GalaxyCoord};
use reachlock_core::generator::transit::{
    anomaly_rolls, malfunction_roll, malfunction_roll_under_fire, transit_destination,
    SELF_JUMP_BURN,
};
use reachlock_core::network::ClientMessage;
use reachlock_core::seed::types::{Biome, Seed, SystemId};
use reachlock_core::universe::tier::UniverseTier;

use crate::net::{NetMode, NetOutbox};
use crate::settings::{InputAction, Settings};
use crate::states::{CurrentLocation, GameMode, SceneRegistry};
use crate::systems::content_index::ContentIndex;
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
    /// S21: destination system id in the gate network or uncharted hash.
    pub dest_system_id: SystemId,
    /// S21: destination system biome.
    pub dest_biome: Biome,
    /// S21: whether the destination is a charted gate transit or deep space.
    pub dest_is_charted: bool,
    /// S21: galactic coordinate of the destination, set for FTL deep-space
    /// jumps. Also stored on the arrival system in CurrentLocation.
    pub dest_coord: Option<GalaxyCoord>,
    pub jump_count: u64,
    pub anomaly_fired: bool,
    /// Who runs the crossing: Boris for gate transits (the cryo-pilot
    /// contract), Prudence for programmed self-jumps (SHIPS.md §3 — the
    /// synthetic crew has the ship while the humans sleep).
    pub pilot: String,
    /// S21: which gate the player chose (by destination SystemId). When set,
    /// `try_gate_jump` looks up the gate by matching `to`. `None` means
    /// auto-select the first active gate.
    pub chosen_gate_id: Option<SystemId>,
    /// S21: true while the player is choosing among multiple outgoing gates.
    pub gate_awaiting_choice: bool,
    /// S21: the outgoing gate destinations shown in the choice UI.
    pub gate_choices: Vec<SystemId>,
}

impl Default for TransitState {
    fn default() -> Self {
        TransitState {
            active: false,
            timer: Timer::default(),
            dest_seed: 0,
            dest_system_id: SystemId(String::new()),
            dest_biome: Biome::Frontier,
            dest_is_charted: true,
            dest_coord: None,
            jump_count: 0,
            anomaly_fired: false,
            pilot: "Boris".into(),
            chosen_gate_id: None,
            gate_awaiting_choice: false,
            gate_choices: Vec::new(),
        }
    }
}

/// S21: FTL route target set from the galaxy map. When non-None, pressing J
/// triggers a deep-space FTL jump instead of an in-system panic jump.
#[derive(Resource, Default)]
pub struct FtlRoute {
    pub coord: Option<GalaxyCoord>,
}

/// S21: marker for the gate-choice overlay shown when a system has >1 gate.
#[derive(Component)]
pub struct GateChoiceOverlay;

/// Screen-fixed hyperspace wash, spawned/despawned by `hyperspace_tick`.
#[derive(Component)]
pub struct TransitVisual;

/// ENTER near a gate → engage the jump drive (Hyperspace mode).
/// Reads the gate network from the content index to determine the
/// destination system. Blockaded/Restricted/destroyed gates refuse transit.
#[allow(clippy::too_many_arguments)]
pub fn try_gate_jump(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    ship: Query<&Transform, With<PlayerShip>>,
    gates: Query<&Transform, With<Gate>>,
    mut state: ResMut<TransitState>,
    location: Res<CurrentLocation>,
    content: Res<ContentIndex>,
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
    // Jump triggers on the dock/interact key, or automatically if the player
    // already chose a gate from the multi-gate selection overlay.
    if !near || (!keys.just_pressed(settings.key(InputAction::EditorConfirm)) && state.chosen_gate_id.is_none()) {
        return;
    }

    // Look up the gate network for the current system.
    let Some(network) = content.gate_network.as_ref() else {
        log.log("No gate network loaded — cannot transit.");
        return;
    };
    let gates = network.outgoing(&location.system_id);
    if gates.is_empty() {
        log.log(format!(
            "No outgoing gates from {} — cannot transit.",
            location.system_id.0
        ));
        return;
    }
    // If the player already chose a gate (via number-key selection), find
    // it by matching the destination SystemId. Otherwise auto-select if
    // only one active gate exists, or prompt for selection.
    let gate = if let Some(ref dest_id) = state.chosen_gate_id {
        let Some(g) = gates.iter().find(|g| &g.to == dest_id) else {
            log.log("Invalid gate selection.");
            return;
        };
        g
    } else {
        let active: Vec<_> = gates
            .iter()
            .filter(|g| matches!(g.status, reachlock_core::galaxy::GateStatus::Active))
            .collect();
        match active.len() {
            0 => {
                if let Some(blocked) = gates
                    .iter()
                    .find(|g| !matches!(g.status, reachlock_core::galaxy::GateStatus::Active))
                {
                    let reason = match blocked.status {
                        reachlock_core::galaxy::GateStatus::Blockaded => {
                            "sealed by the controlling faction"
                        }
                        reachlock_core::galaxy::GateStatus::Restricted => "access restricted",
                        reachlock_core::galaxy::GateStatus::Contested => "contested — combat zone",
                        reachlock_core::galaxy::GateStatus::Destroyed => "gate destroyed",
                        _ => "unavailable",
                    };
                    log.log(format!("Gate to {} is {}.", blocked.to.0, reason));
                } else {
                    log.log("No available gate for transit.");
                }
                return;
            }
            1 => active[0],
            _ => {
                // Multiple active gates: show choice, wait for player input.
                state.gate_awaiting_choice = true;
                state.chosen_gate_id = None;
                state.gate_choices = active.iter().map(|g| g.to.clone()).collect();
                log.log(format!(
                    "Multiple gates open: press 1-{} to choose destination.",
                    active.len()
                ));
                return;
            }
        }
    };
    let dest_id = &gate.to;
    let dest_system = content.charted_systems.get(&dest_id.0);
    let dest_seed = dest_system.map(|s| s.seed).unwrap_or_else(|| {
        // Fallback: if the charted system isn't loaded, derive from the id.
        transit_destination(location.system_seed, state.jump_count)
    });
    let dest_biome = dest_system.map(|s| s.biome).unwrap_or(Biome::Frontier);

    state.active = true;
    state.anomaly_fired = false;
    state.dest_seed = dest_seed;
    state.dest_system_id = dest_id.clone();
    state.dest_biome = dest_biome;
    state.dest_is_charted = true;
    state.timer = Timer::from_seconds(TRANSIT_SECS, TimerMode::Once);
    state.pilot = "Boris".into();
    log.log(format!(
        "Gate transit engaged → {} (stable window; crew stays awake)",
        dest_system
            .map(|s| s.display_name.as_str())
            .unwrap_or(&dest_id.0)
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
        location.system_id = state.dest_system_id.clone();
        location.system_biome = state.dest_biome;
        location.system_fidelity = if state.dest_is_charted {
            reachlock_core::generator::system::Fidelity::Full
        } else {
            reachlock_core::generator::system::Fidelity::Sparse
        };
        location.galaxy_coord = state.dest_coord;
        state.jump_count = state.jump_count.wrapping_add(1);
        state.chosen_gate_id = None;
        state.active = false;
        deliberation.active = None;
        for e in &visuals {
            commands.entity(e).despawn();
        }
        // Invalidate the scene registry so `enter_spaceflight` (triggered by
        // the transition back to SpaceFlight below) does NOT early-return and
        // actually rebuilds the world from `location.system_seed`.
        registry.scene = None;
        // S21 integration: discover the destination seed with the server.
        let dest_id = location.system_id.clone();
        match &*mode {
            NetMode::Online { universe, .. } => {
                outbox.push(ClientMessage::SeedDiscover {
                    universe: *universe,
                    system_id: dest_id,
                    seed: Seed::new(location.system_seed),
                });
                log.log(format!(
                    "Arrived at {} — synchronizing…",
                    location.system_id.0
                ));
            }
            NetMode::Offline => {
                log.log(format!("Arrived at {}.", location.system_id.0));
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
/// Never a silent fail — the log narrates.
///
/// S21: if an FTL route target is set from the galaxy map, the jump goes
/// to deep space (`deep_space_seed`) instead of staying in-system.
/// The FTL route is consumed (one-shot).
#[allow(clippy::too_many_arguments)]
pub fn self_jump(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut state: ResMut<TransitState>,
    location: Res<CurrentLocation>,
    mut systems: ResMut<ShipSystems>,
    enemies: Query<&crate::systems::combat::EnemyShip>,
    mode: Res<NetMode>,
    mut ftl: ResMut<FtlRoute>,
    mut next: ResMut<NextState<GameMode>>,
    mut log: ResMut<ShipLog>,
) {
    if state.active || !keys.just_pressed(settings.key(InputAction::OpenMissionBoard)) {
        return;
    }
    let cost = SELF_JUMP_BURN;
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
    let outcome = match sev {
        0 => "clean arrival",
        1 => "arrived off-course",
        2 => "hull stress damage",
        _ => "hull stress + cargo shift",
    };

    if let Some(coord) = ftl.coord.take() {
        // FTL deep-space jump to a galaxy coordinate.
        let uni = match &*mode {
            NetMode::Online { universe, .. } => *universe,
            NetMode::Offline => UniverseTier::Classic,
        };
        let seed = deep_space_seed(coord, uni);
        let hash = coord.coord_hash(uni);
        state.active = true;
        state.dest_seed = seed.value();
        state.dest_system_id = SystemId(format!("uncharted_{}", hash));
        state.dest_biome = Biome::DeepSpace;
        state.dest_is_charted = false;
        state.dest_coord = Some(coord);
        state.jump_count = state.jump_count.wrapping_add(1);
        state.anomaly_fired = true;
        state.pilot = "Boris".into();
        state.timer = Timer::from_seconds(TRANSIT_SECS, TimerMode::Once);
        systems.hull_hp = fixed_clamp((systems.hull_hp.0 - 400).max(64));
        if sev >= 2 {
            systems.hull_hp = fixed_clamp((systems.hull_hp.0 - 200).max(64));
        }
        log.log(format!(
            "FTL jump to uncharted space ({hash}) — {outcome}.{fire}",
            fire = if under_fire {
                " Spooling under fire. Your nose is bleeding."
            } else {
                ""
            }
        ));
    } else {
        // In-system panic jump: stay in the current system.
        state.jump_count = state.jump_count.wrapping_add(1);
        state.active = true;
        state.dest_seed = location.system_seed;
        state.dest_system_id = location.system_id.clone();
        state.dest_biome = location.system_biome;
        state.dest_is_charted = true;
        state.dest_coord = None;
        state.anomaly_fired = true;
        state.pilot = "Boris".into();
        state.timer = Timer::from_seconds(TRANSIT_SECS, TimerMode::Once);
        systems.hull_hp = fixed_clamp((systems.hull_hp.0 - 400).max(64));
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
    }
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

// --- S21: multi-gate choice UI ---

/// Spawn or despawn the gate choice overlay based on `TransitState`.
/// Shows a numbered list of available gate destinations.
pub fn gate_choice_overlay(
    state: Res<TransitState>,
    content: Res<ContentIndex>,
    mut commands: Commands,
    overlay: Query<Entity, With<GateChoiceOverlay>>,
) {
    let has_overlay = overlay.single().is_ok();
    if state.gate_awaiting_choice && !has_overlay {
        let lines: Vec<String> = state
            .gate_choices
            .iter()
            .enumerate()
            .map(|(i, sys_id)| {
                let name = content
                    .charted_systems
                    .get(&sys_id.0)
                    .map(|s| s.display_name.as_str())
                    .unwrap_or(&sys_id.0);
                format!("  {}: {}", i + 1, name)
            })
            .collect();
        commands.spawn((
            GateChoiceOverlay,
            Text::new(lines.join("\n")),
            TextFont {
                font_size: 18.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.85, 0.7)),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(80.0),
                left: Val::Px(16.0),
                ..default()
            },
        ));
    } else if !state.gate_awaiting_choice && has_overlay {
        if let Ok(entity) = overlay.single() {
            commands.entity(entity).despawn();
        }
    }
}

/// Number-key handler for gate selection: press 1-9 to choose a gate.
pub fn gate_selection_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<TransitState>,
    content: Res<ContentIndex>,
    mut log: ResMut<ShipLog>,
) {
    if !state.gate_awaiting_choice || state.gate_choices.is_empty() {
        return;
    }
    // Clone the choices so we can mutate state while iterating.
    let choices: Vec<_> = state.gate_choices.clone();
    let key_codes = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    for (i, kc) in key_codes.iter().enumerate().take(choices.len()) {
        if keys.just_pressed(*kc) {
            let name = content
                .charted_systems
                .get(&choices[i].0)
                .map(|s| s.display_name.as_str())
                .unwrap_or(&choices[i].0)
                .to_string();
            state.chosen_gate_id = Some(choices[i].clone());
            state.gate_awaiting_choice = false;
            log.log(format!("Gate selected: {name}."));
            return;
        }
    }
}
