//! Docking / boarding / launching transitions (spec §14 Mode states).
//! SpaceFlight → dock (Enter near a station) → Docking beat → Landed;
//! Landed → launch (L) → Undocking beat → SpaceFlight. Landed ↔ OnBoard and
//! OnBoard → SpaceFlight are interaction verbs now — the parked ship, the
//! airlock hatch, and the pilot seat are `Interactable`s routed in
//! `interaction::try_interact`, so the transition points are visible in the
//! world instead of hidden keybinds. The `Docking`/`Undocking` beats are
//! short timers so the camera ease reads as a transition, not a teleport.

use bevy::prelude::*;

use reachlock_core::generator::station::StationKind;

use crate::settings::{InputAction, Settings};
use crate::states::{CurrentLocation, GameMode};
use crate::systems::ship::{PlayerShip, ShipSystems};

/// Radius (world units) within which Enter docks with a station. Stations
/// run up to ~160 units of collider radius themselves, so anything smaller
/// forces a hull-scraping approach.
const DOCK_RADIUS: f32 = 320.0;
/// Duration of the Docking/Undocking camera-ease beats.
const TRANSITION_SECS: f32 = 0.5;

/// Marks a station entity as dockable, carrying the data needed to build its
/// interior and name the location on docking.
#[derive(Component, Clone, Debug)]
pub struct Dockable {
    pub seed: u64,
    pub kind: StationKind,
    pub station_id: String,
}

/// Drives the Docking/Undocking beats: counts down, then completes the
/// transition to the destination mode.
#[derive(Resource, Default)]
pub struct TransitionBeat {
    pub timer: Option<Timer>,
}

pub fn try_dock(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    ship: Query<&Transform, With<PlayerShip>>,
    stations: Query<(&Transform, &Dockable)>,
    mut next: ResMut<NextState<GameMode>>,
    mut location: ResMut<CurrentLocation>,
    mut beat: ResMut<TransitionBeat>,
) {
    // Enter is the flight "commit transit" key (dock here, gate jump in
    // jump.rs). E can't be used: it's the roll-right axis in flight, and a
    // roll near a station must not slam the ship into a dock.
    let dock_pressed = keys.just_pressed(settings.key(InputAction::EditorConfirm));
    if !dock_pressed && !settings.gameplay.auto_dock {
        return;
    }
    let Ok(ship) = ship.single() else {
        return;
    };
    for (st, dock) in &stations {
        let d = ship.translation.distance(st.translation);
        if d <= DOCK_RADIUS {
            location.station_id = dock.station_id.clone();
            location.station_seed = dock.seed;
            location.station_kind = Some(dock.kind);
            // Store the station's XZ plane position (used to place the ship on
            // undock); y is always the flight plane (0).
            location.station_position = Vec2::new(st.translation.x, st.translation.z);
            location.display_name = format!("Station {}", dock.station_id);
            location.is_docked = true;
            next.set(GameMode::Docking);
            beat.timer = Some(Timer::from_seconds(TRANSITION_SECS, TimerMode::Once));
            return;
        }
    }
}

/// `B` in flight: stand up from the helm and walk the ship (S09d — the
/// reverse of the pilot seat's `TakeHelm`, closing the loop the S09c handoff
/// flagged). The space scene stays alive underneath (`SceneRegistry::
/// space_alive`); the ship coasts with its last velocity — nobody is flying
/// her until someone sits back down, which is the point (docs/SHIPS.md §1).
pub fn leave_helm(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    location: Res<CurrentLocation>,
    mut systems: ResMut<ShipSystems>,
    mut deck: ResMut<crate::systems::interior::ActiveDeck>,
    mut next: ResMut<NextState<GameMode>>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
) {
    if !keys.just_pressed(settings.key(InputAction::OpenCrewRoster))
        || location.is_docked
        || systems.dead
    {
        return;
    }
    let Some((deck_index, spawn)) = crate::systems::interior::cockpit_seat_spawn() else {
        return;
    };
    deck.index = deck_index;
    deck.spawn = Some(spawn);
    // Hands off the stick: thrust stops, momentum keeps whatever it had.
    systems.thrusting = false;
    log.log("You stand up from the helm. Lou coasts.");
    next.set(GameMode::OnBoard);
}

/// `L` (launch) handling inside Landed. Boarding, disembarking, and taking
/// the helm are `Interactable`s (`interaction::try_interact`) — walk up to
/// the parked ship / airlock hatch / pilot seat and press E.
pub fn try_interior_transitions(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mode: Res<State<GameMode>>,
    mut next: ResMut<NextState<GameMode>>,
    mut beat: ResMut<TransitionBeat>,
) {
    if **mode == GameMode::Landed && keys.just_pressed(settings.key(InputAction::OpenShipLog)) {
        next.set(GameMode::Undocking);
        beat.timer = Some(Timer::from_seconds(TRANSITION_SECS, TimerMode::Once));
    }
}

/// Advances the Docking/Undocking beat and completes the transition.
pub fn transition_beat(
    time: Res<Time<Virtual>>,
    mode: Res<State<GameMode>>,
    mut next: ResMut<NextState<GameMode>>,
    mut beat: ResMut<TransitionBeat>,
) {
    let Some(timer) = beat.timer.as_mut() else {
        return;
    };
    if !timer.tick(time.delta()).is_finished() {
        return;
    }
    beat.timer = None;
    match **mode {
        GameMode::Docking => next.set(GameMode::Landed),
        GameMode::Undocking => next.set(GameMode::SpaceFlight),
        _ => {}
    }
}
