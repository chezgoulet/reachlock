//! Docking / boarding / launching transitions (spec §14 Mode states).
//! SpaceFlight → dock (E near a station) → Docking beat → Landed; Landed →
//! launch (L) → Undocking beat → SpaceFlight; Landed ↔ OnBoard at the
//! airlock; OnBoard → SpaceFlight at the cockpit; SpaceFlight → OnBoard (B)
//! to walk the ship in flight. The `Docking`/`Undocking` beats are short
//! timers so the camera ease reads as a transition, not a teleport.

use bevy::prelude::*;

use reachlock_core::generator::station::StationKind;
use reachlock_core::generator::RoomKind;

use crate::states::{CurrentLocation, GameMode};
use crate::systems::interior::CurrentInterior;
use crate::systems::mode::{avatar_in_room, PlayerAvatar};
use crate::systems::ship::PlayerShip;

/// Radius (world units) within which `E` docks with a station.
const DOCK_RADIUS: f32 = 160.0;
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
    ship: Query<&Transform, With<PlayerShip>>,
    stations: Query<(&Transform, &Dockable)>,
    mut next: ResMut<NextState<GameMode>>,
    mut location: ResMut<CurrentLocation>,
    mut beat: ResMut<TransitionBeat>,
) {
    // Enter is the flight "commit transit" key (dock here, gate jump in
    // jump.rs). E can't be used: it's the roll-right axis in flight, and a
    // roll near a station must not slam the ship into a dock.
    if !keys.just_pressed(KeyCode::Enter) {
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

/// `E`/`L`/`B` handling inside Landed and On-Board. What each key does
/// depends on which room the avatar stands in (airlock = board, cockpit =
/// take helm), a pure function of the current `GeneratedLayout`.
#[allow(clippy::too_many_arguments)]
pub fn try_interior_transitions(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<State<GameMode>>,
    avatar: Query<&Transform, With<PlayerAvatar>>,
    mut location: ResMut<CurrentLocation>,
    interior: Res<CurrentInterior>,
    mut next: ResMut<NextState<GameMode>>,
    mut beat: ResMut<TransitionBeat>,
) {
    let Some(layout) = &interior.layout else {
        return;
    };
    let Ok(avatar) = avatar.single() else {
        return;
    };
    let in_hangar = avatar_in_room(avatar, layout, RoomKind::Hangar);
    let in_bridge = avatar_in_room(avatar, layout, RoomKind::Bridge);
    let e = keys.just_pressed(KeyCode::KeyE);
    let l = keys.just_pressed(KeyCode::KeyL);
    let b = keys.just_pressed(KeyCode::KeyB);

    match **mode {
        GameMode::Landed => {
            if e && in_hangar {
                location.is_docked = true;
                next.set(GameMode::OnBoard);
            } else if l {
                next.set(GameMode::Undocking);
                beat.timer = Some(Timer::from_seconds(TRANSITION_SECS, TimerMode::Once));
            }
        }
        GameMode::OnBoard => {
            if e && in_hangar {
                next.set(GameMode::Landed);
            } else if e && in_bridge {
                next.set(GameMode::SpaceFlight);
            } else if b {
                location.is_docked = false;
                next.set(GameMode::OnBoard);
            }
        }
        _ => {}
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
