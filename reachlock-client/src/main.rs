//! ReachLock v2 client (spec §9): Bevy shell around reachlock-core.
//! Menu → Playing; a generated system you can fly through, with the
//! contract engine holding the helm and deliberating when rules run out.

mod bridge;
mod net;
mod states;
mod systems;

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;

use net::NetMode;
use states::AppState;
use systems::{contract, hud, menu, network, setup, ship};

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
        .add_systems(Startup, menu::spawn_menu)
        .add_systems(
            Update,
            menu::menu_input.run_if(in_state(AppState::MainMenu)),
        )
        .add_systems(
            OnEnter(AppState::Playing),
            (
                setup::spawn_world,
                hud::spawn_hud,
                network::connect_on_enter_playing,
            ),
        )
        .add_systems(
            Update,
            (
                ship::control,
                ship::camera_follow,
                contract::evaluate_contracts,
                contract::tick_deliberation,
                network::poll_network,
                network::reconnect_backoff,
                hud::update_hud,
            )
                .chain()
                .run_if(in_state(AppState::Playing)),
        )
        .run();
}
