//! Application states (spec §9, §14). `AppState` is the top-level machine;
//! `GameMode` is a sub-state that only exists while `AppState::InGame`,
//! implementing the three-mode skeleton (spec §14): SpaceFlight, Landed,
//! OnBoard, plus the transition beats Docking/Undocking and Paused.
//!
//! Transition diagram (frozen once — S07/S08/S09 build against this exact
//! interface):
//!
//! ```text
//! MainMenu ──Enter──▶ InGame ⋈ GameMode::SpaceFlight
//! SpaceFlight ──dock(E)──▶ Docking ──▶ Landed
//! Landed ──launch(E)──▶ Undocking ──▶ SpaceFlight
//! Landed ──airlock(E)──▶ OnBoard{is_docked:true}
//! OnBoard ──airlock(E)──▶ Landed
//! OnBoard ──cockpit(E)──▶ SpaceFlight
//! SpaceFlight ──B──▶ OnBoard{is_docked:false}
//! * ──Esc──▶ Paused ──Esc──▶ (previous mode)
//! ```
//!
//! Entities scoped to a mode carry [`ModeScope`] and are despawned on mode
//! exit by the generic teardown in `systems/mode.rs`. `Paused` is a
//! *transient overlay*: entering it must NOT despawn the underlying scene, so
//! the teardown early-outs whenever the new state is `Paused`.

use bevy::prelude::*;
use reachlock_core::generator::station::StationKind;

/// Top-level app machine. `InGame` is the single state under which the
/// `GameMode` sub-state exists (spec §14).
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    MainMenu,
    InGame,
}

/// The three-mode sub-state machine (spec §14). A sub-state of `AppState`:
/// it only exists while `AppState::InGame` is active.
///
/// All variants are unit variants. The S06 gotcha is explicit: "keep a
/// `CurrentLocation` resource as the source of truth and the state variant
/// data minimal" — so location/boarding data lives in [`CurrentLocation`],
/// not in the enum payload. This also keeps `OnEnter`/`OnExit`/`in_state`
/// usable for every variant (data-carrying variants can't be used as
/// transition targets without a concrete value).
#[derive(SubStates, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[source(AppState = AppState::InGame)]
pub enum GameMode {
    /// Flying the ship in a system's space volume (spec §14 Mode 3).
    #[default]
    SpaceFlight,
    /// Top-down on a station/planet surface. The station id is in
    /// [`CurrentLocation`].
    Landed,
    /// Side-on inside the player's ship. Whether docked is in
    /// [`CurrentLocation`].
    OnBoard,
    /// Brief camera-ease beat entering a station (spec §14: "dock").
    Docking,
    /// Brief beat leaving a station back to space (spec §14: "undock").
    Undocking,
    /// Transient overlay: stops the sim clock (time paused) in every mode.
    /// Not a scene — entering it must not despawn the underlying scene.
    Paused,
    /// Gate-jump transit sequence (spec §14 Mode 3; S09). The ship entity
    /// persists, the space scene is swapped for the cryo tunnel; rapier is
    /// paused via the S06 pattern. Added in S09 (deliberately absent from
    /// S06's first cut).
    #[allow(dead_code)]
    Hyperspace,
}

/// Tags an entity as belonging to one mode's scene. On mode exit the generic
/// teardown despawns every `ModeScope` entity, so no per-mode cleanup list is
/// needed (spec §14 deliverable: "no per-mode cleanup lists").
#[derive(Component, Clone, PartialEq, Eq, Debug)]
pub struct ModeScope(pub GameMode);

/// Source of truth for "where the player is right now". `GameMode`'s data
/// variants mirror this; systems read here rather than reconstructing the
/// payload at every transition.
#[derive(Resource, Default, Clone, Debug)]
pub struct CurrentLocation {
    /// The system seed currently loaded. One system per S06; multi-system
    /// arrives with S09's gate jump.
    pub system_seed: u64,
    /// Station id when docked/landed, else empty.
    pub station_id: String,
    /// Whether the player's ship is docked at a station (vs in flight).
    pub is_docked: bool,
    /// Human-readable banner text for the HUD.
    pub display_name: String,
    /// World position of the current station (for undock ship placement).
    pub station_position: Vec2,
    /// Seed of the station currently docked at (drives its interior layout).
    pub station_seed: u64,
    /// Kind of the docked station, if any.
    pub station_kind: Option<StationKind>,
}

/// The scene currently spawned into the world. Used to make pause a no-op
/// round-trip: re-entering a mode we never despawned must not rebuild it.
#[derive(Resource, Default, Clone, Debug)]
pub struct SceneRegistry {
    pub scene: Option<GameMode>,
}
