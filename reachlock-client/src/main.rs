//! ReachLock v2 client (spec §9): Bevy shell around reachlock-core.
//! Menu → InGame; a generated system you can fly through, with the contract
//! engine holding the helm and deliberating when rules run out. The
//! three-mode state machine (spec §14) — SpaceFlight / Landed / OnBoard —
//! lives in `GameMode`, a sub-state of `AppState::InGame`.

mod bridge;
mod net;
mod states;
mod systems;

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;

use net::NetMode;
use states::{AppState, CurrentLocation, GameMode, SceneRegistry};
use systems::{
    content_index, contract, crew, docking, hud, interaction, interior, inventory, jump, market,
    menu, mode, network, onboard, pause, sensors, setup, ship, starfield,
};

/// Run condition: the player is flying (the SpaceFlight sub-state).
fn in_spaceflight(mode: Option<Res<State<GameMode>>>) -> bool {
    matches!(mode, Some(m) if **m == GameMode::SpaceFlight)
}

/// Run condition: the player is in a top-down interior (Landed or OnBoard).
/// `Option` because `State<GameMode>` may not be present yet during early
/// schedule evaluation (before the sub-state is initialized); treat its
/// absence as "not interior" rather than panicking.
fn in_any_interior(mode: Option<Res<State<GameMode>>>) -> bool {
    matches!(mode, Some(m) if matches!(**m, GameMode::Landed | GameMode::OnBoard))
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "ReachLock".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(ShapePlugin)
        .add_plugins(RapierPhysicsPlugin::<()>::pixels_per_meter(100.0))
        .init_state::<AppState>()
        .add_sub_state::<GameMode>()
        .init_resource::<contract::ShipLog>()
        .init_resource::<contract::DeliberationState>()
        .init_resource::<contract::ContractRuntime>()
        // S02: NetMode is frozen once at startup from REACHLOCK_SERVER;
        // everything else here defaults to the offline-safe state and is
        // only ever touched by systems that early-out when NetMode::Offline.
        .insert_resource(NetMode::from_env())
        .init_resource::<net::ConnectionState>()
        .init_resource::<net::NetOutbox>()
        .init_non_send_resource::<network::NetworkClient>()
        .init_resource::<network::ReconnectBackoff>()
        .init_resource::<network::SeedState>()
        // S06: mode machine resources. Seed the first system from the
        // canonical seed so a fresh launch loads the authored starting system.
        .insert_resource(CurrentLocation {
            system_seed: systems::setup::SYSTEM_SEED,
            ..default()
        })
        .init_resource::<SceneRegistry>()
        .init_resource::<interior::CurrentInterior>()
        .init_resource::<docking::TransitionBeat>()
        .init_resource::<pause::PausedFrom>()
        // S07/S08: inventory, crew, interaction, autosave.
        .init_resource::<inventory::PlayerInventory>()
        .init_resource::<crew::CrewRoster>()
        .init_resource::<interaction::InteractionPrompt>()
        .init_resource::<interaction::ActivePanel>()
        .init_resource::<inventory::SaveTimer>()
        .init_resource::<market::MarketState>()
        // S10: live economy. Built at startup from the embedded goods
        // catalogue; ticks forward each frame.
        .add_systems(Startup, market::init_economy)
        // S09: live jump/transit bookkeeping + sensors.
        .init_resource::<jump::TransitState>()
        .init_resource::<sensors::MapOverlayState>()
        // S08: start with the canonical crew (stable ids for S13 souls).
        .insert_resource(crew::CrewRoster::default_crew())
        .add_systems(
            Startup,
            (
                content_index::load_content_index,
                inventory::load_save,
                menu::spawn_menu,
                sensors::init_blip_assets,
            ),
        )
        .add_systems(
            Update,
            menu::menu_input.run_if(in_state(AppState::MainMenu)),
        )
        // HUD spawns once when the game starts; it adapts per mode in
        // `update_hud`.
        .add_systems(
            OnEnter(AppState::InGame),
            (
                hud::spawn_hud,
                onboard::spawn_onboard_panels,
                network::connect_on_enter_playing,
            ),
        )
        .add_systems(OnExit(AppState::InGame), mode::teardown_on_leave_game)
        // --- SpaceFlight scene ---
        // No OnExit teardown: the *enter* systems tear down whatever was
        // there when a different scene is requested, which keeps the
        // Docking/Undocking beats showing the live space scene and lets
        // Pause round-trip without rebuilding anything (the enter systems
        // early-out when `SceneRegistry` already holds the target mode).
        .add_systems(OnEnter(GameMode::SpaceFlight), setup::enter_spaceflight)
        // --- Landed scene ---
        .add_systems(OnEnter(GameMode::Landed), interior::enter_interior)
        // --- OnBoard scene ---
        .add_systems(OnEnter(GameMode::OnBoard), interior::enter_interior)
        // --- SpaceFlight-only gameplay ---
        .add_systems(
            Update,
            (
                ship::control,
                ship::camera_follow,
                ship::sync_ship_visibility,
                starfield::parallax,
                docking::try_dock,
                jump::try_gate_jump,
                jump::self_jump,
            )
                .run_if(in_spaceflight),
        )
        .add_systems(
            Update,
            (
                sensors::sensor_visibility,
                sensors::sensor_blips,
                sensors::scan_contact,
                sensors::system_map,
                sensors::map_overlay_text,
            )
                .run_if(in_spaceflight),
        )
        // --- Hyperspace transit (cryo-pilot, anomaly, wake) ---
        // Tear down the space scene on entry (S09 gotcha: scope out the
        // old scene so rapier doesn't keep simulating it during transit).
        .add_systems(OnEnter(GameMode::Hyperspace), mode::teardown_for_hyperspace)
        .add_systems(
            Update,
            jump::hyperspace_tick.run_if(in_state(GameMode::Hyperspace)),
        )
        // --- Interior-only gameplay ---
        .add_systems(
            Update,
            (
                interior::walk_avatar,
                mode::interior_camera_follow,
                docking::try_interior_transitions,
                interaction::try_interact,
                market::market_system,
                crew::crew_shift_system,
                onboard::onboard_panels,
                jump::fuel_dock,
            )
                .run_if(in_any_interior),
        )
        // --- All InGame modes (contracts keep evaluating everywhere) ---
        // Split into two `add_systems` groups: Bevy's tuple arity for
        // `run_if` is capped, so 8 systems go in two 4-tuples sharing
        // the same run condition.
        .add_systems(
            Update,
            (
                contract::evaluate_contracts,
                contract::tick_deliberation,
                network::poll_network,
                network::reconnect_backoff,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            (
                docking::transition_beat,
                inventory::autosave_system,
                pause::toggle_pause,
                hud::update_hud_status,
                hud::update_hud_panels,
            )
                .run_if(in_state(AppState::InGame)),
        )
        // Onboard (interior) panels own their own `Text` components, in their
        // own group (Bevy 0.18 caps `run_if` tuple arity and flags the
        // ambiguity between two systems' `&mut Text` queries as a B0001 if
        // they share a group). `in_any_interior` means it never runs
        // concurrently with the HUD text systems above.
        .add_systems(Update, onboard::onboard_panels.run_if(in_any_interior))
        // S10: advance the live economy every frame (cheap; tiny per-tick
        // move). Its own group so the all-mode tuple stays under Bevy's
        // `run_if` arity cap.
        .add_systems(
            Update,
            market::tick_economy.run_if(in_state(AppState::InGame)),
        )
        .run();
}
