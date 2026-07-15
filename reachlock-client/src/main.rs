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
use bevy_rapier3d::prelude::*;

use net::NetMode;
use states::{AppState, CurrentLocation, GameMode, SceneRegistry};
use systems::{
    content_index, contract, crew, docking, factions, hud, interaction, interior, inventory, jump,
    market, menu, mode, network, onboard, pause, reticle, sensors, setup, ship,
};

/// Run condition: the player is flying (the SpaceFlight sub-state).
///
/// Uses `Option<Res<…>>` like Bevy's own `in_state`: the `GameMode` sub-state
/// resource only exists while `AppState::InGame` is active, so it is absent on
/// the main menu. Returning `false` there (instead of demanding the resource)
/// avoids a "resource does not exist" panic at startup.
fn in_spaceflight(mode: Option<Res<State<GameMode>>>) -> bool {
    match mode {
        Some(mode) => **mode == GameMode::SpaceFlight,
        None => false,
    }
}

/// Run condition: the player is in a top-down interior (Landed or OnBoard).
fn in_any_interior(mode: Option<Res<State<GameMode>>>) -> bool {
    match mode {
        Some(mode) => matches!(**mode, GameMode::Landed | GameMode::OnBoard),
        None => false,
    }
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
        // S09b: SpaceFlight is 3D (spec §14 Mode 3). Interiors don't use
        // physics, so rapier3d is the only physics context now.
        .add_plugins(RapierPhysicsPlugin::<()>::default())
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
        // S09: live jump/transit bookkeeping + sensors.
        .init_resource::<jump::TransitState>()
        .init_resource::<sensors::MapOverlayState>()
        // S09b: cross-mode command bus — OnBoard consoles (gunner/scanner/
        // miner/power) write it, the flight systems read it (spec §22).
        .init_resource::<ship::ShipCommand>()
        // S09b-2: death/respawn beat after a hull breach.
        .init_resource::<ship::RespawnTimer>()
        // S09c: Star Fox feel layer — smoothed axes, bank, barrel roll,
        // camera blends. Render-layer only.
        .init_resource::<ship::FlightFeel>()
        // S08: start with the canonical crew (stable ids for S13 souls).
        .insert_resource(crew::CrewRoster::default_crew())
        .add_systems(
            Startup,
            (
                content_index::load_content_index,
                inventory::load_save,
                menu::spawn_menu,
                sensors::init_blip_assets,
                market::init_economy,
                factions::init_faction_state,
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
                reticle::spawn_reticle,
                onboard::spawn_onboard_panels,
                network::connect_on_enter_playing,
                factions::spawn_reputation_panel,
                factions::spawn_faction_banner,
            ),
        )
        .add_systems(OnExit(AppState::InGame), mode::teardown_on_leave_game)
        // S09b: activate the 3D chase-cam in SpaceFlight, the 2D camera
        // everywhere else. Runs every InGame frame so mode switches (and the
        // Docking/Undocking/Hyperspace beats) always land on the right view.
        .add_systems(
            Update,
            ship::manage_cameras.run_if(in_state(AppState::InGame)),
        )
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
                docking::try_dock,
                jump::try_gate_jump,
                jump::self_jump,
            )
                .run_if(in_spaceflight),
        )
        // S09b: weapons/scanner/mining driven by the OnBoard consoles via the
        // ShipCommand bus, rendered in the 3D flight scene.
        .add_systems(
            Update,
            (
                ship::fire_weapons,
                ship::step_projectiles,
                ship::mining_beam,
                ship::scanner_pulse,
                ship::request_scan_from_key,
                ship::engine_glow,
                systems::starfield::dust_parallax,
            )
                .run_if(in_spaceflight),
        )
        .add_systems(Update, (ship::collisions,).run_if(in_spaceflight))
        // S09c: the aiming reticle runs in every InGame mode so leaving
        // SpaceFlight hides it the same frame.
        .add_systems(
            Update,
            reticle::update_reticle.run_if(in_state(AppState::InGame)),
        )
        // S09b-2: revive the ship after a hull breach (runs in all InGame
        // modes so the beat completes regardless of which scene is active).
        .add_systems(
            Update,
            ship::respawn_ship.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            (
                sensors::sensor_visibility,
                sensors::sensor_blips,
                sensors::sensor_blip_follow,
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
                market::tick_economy,
                market::market_system,
                crew::crew_shift_system,
                onboard::onboard_panels,
                onboard::onboard_ship_consoles,
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
                factions::tick_faction_system,
                factions::reputation_panel_toggle,
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
                factions::render_reputation_panel,
                factions::render_faction_banner,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .run();
}
