//! ReachLock v2 client (spec §9): Bevy shell around reachlock-core.
//! Menu → Playing; a generated system you can fly through, with the
//! contract engine holding the helm and deliberating when rules run out.

mod bridge;
mod states;
mod systems;

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;

use states::AppState;
use systems::{content_index, contract, hud, menu, setup, ship, starfield};

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
        .add_systems(
            Startup,
            (content_index::load_content_index, menu::spawn_menu),
        )
        .add_systems(
            Update,
            menu::menu_input.run_if(in_state(AppState::MainMenu)),
        )
        .add_systems(
            OnEnter(AppState::Playing),
            (setup::spawn_world, hud::spawn_hud),
        )
        .add_systems(
            Update,
            (
                ship::control,
                ship::camera_follow,
                starfield::parallax,
                contract::evaluate_contracts,
                contract::tick_deliberation,
                hud::update_hud,
            )
                .run_if(in_state(AppState::Playing)),
        )
        .run();
}
