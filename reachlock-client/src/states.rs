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
use reachlock_core::generator::system::Fidelity;
use reachlock_core::seed::types::Biome;

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
#[derive(Resource, Clone, Debug)]
pub struct CurrentLocation {
    /// The system seed currently loaded. One system per S06; multi-system
    /// arrives with S09's gate jump.
    pub system_seed: u64,
    /// S21: the system id in the gate network ("aethon", "verne") or the
    /// uncharted hash ("uncharted_{coord_hash}").
    pub system_id: reachlock_core::seed::types::SystemId,
    /// S21: the biome for the current system — drives generator output.
    pub system_biome: Biome,
    /// S21: the generation fidelity — Full for charted, Sparse for deep space.
    pub system_fidelity: Fidelity,
    /// S21: the galactic coordinate of the current system, if known (None for
    /// the spike startup before the gate network is loaded).
    pub galaxy_coord: Option<reachlock_core::galaxy::GalaxyCoord>,
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
    /// S20: id of the authored hostile location to fight through this Landed
    /// visit, if any. `None` for an ordinary station landing (no raiders);
    /// `Some(id)` when the player enters a derelict POI. Set by whatever POI
    /// approach flow routes the player into a fight; `spawn_landed_enemies`
    /// reads it on `OnEnter(Landed)`.
    pub hostile_location_id: Option<String>,
}

impl Default for CurrentLocation {
    fn default() -> Self {
        CurrentLocation {
            system_seed: 0,
            system_id: reachlock_core::seed::types::SystemId(String::new()),
            system_biome: Biome::Core,
            system_fidelity: Fidelity::Full,
            galaxy_coord: None,
            station_id: String::new(),
            is_docked: false,
            display_name: String::new(),
            station_position: Vec2::ZERO,
            station_seed: 0,
            station_kind: None,
            hostile_location_id: None,
        }
    }
}

/// The scene currently spawned into the world. Used to make pause a no-op
/// round-trip: re-entering a mode we never despawned must not rebuild it.
#[derive(Resource, Default, Clone, Debug)]
pub struct SceneRegistry {
    pub scene: Option<GameMode>,
    /// S09d: the SpaceFlight scene is still spawned (and rapier still
    /// simulating it) underneath an OnBoard interior — the crew is walking
    /// the ship mid-flight. Set by `enter_spaceflight`, kept by
    /// `enter_interior` when boarding in flight, cleared by any teardown
    /// that despawns the space entities (landing, docked boarding,
    /// hyperspace, leaving the game).
    pub space_alive: bool,
}
