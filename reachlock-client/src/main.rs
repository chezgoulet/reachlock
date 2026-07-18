//! ReachLock v2 client (spec §9): Bevy shell around reachlock-core.
//! Menu → InGame; a generated system you can fly through, with the contract
//! engine holding the helm and deliberating when rules run out. The
//! three-mode state machine (spec §14) — SpaceFlight / Landed / OnBoard —
//! lives in `GameMode`, a sub-state of `AppState::InGame`.

mod bridge;
mod net;
mod pixel;
mod settings;
mod states;
mod systems;

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier3d::prelude::*;

use net::NetMode;
use states::{AppState, CurrentLocation, GameMode, SceneRegistry};
use systems::{
    combat, comms, content_index, contract, crew, crisis, cryojump, dialogue, docking, factions,
    galaxy_map, hud, interaction, interior, inventory, jump, landed_combat, market, menu, mode,
    network, onboard, pause, reticle, sensors, settings_ui, setup, ship, shipeditor, soul, ticker,
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

/// Run condition: the space scene exists and is simulating — flying, or on
/// board mid-flight with the world alive underneath (S09d station views).
/// Weapons, projectiles, collisions, sensors, and the chase-cam run here so
/// shots fired from the gunner console land in the same live scene the pilot
/// would see.
fn space_live(mode: Option<Res<State<GameMode>>>, registry: Res<SceneRegistry>) -> bool {
    match mode {
        Some(mode) => match **mode {
            GameMode::SpaceFlight => true,
            GameMode::OnBoard => registry.space_alive,
            _ => false,
        },
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
        // S06/S21: mode machine resources. The player starts in Aethon,
        // the Compact's seat — the gate network's default origin.
        .insert_resource(CurrentLocation {
            system_seed: 16843009,
            system_id: reachlock_core::seed::types::SystemId("aethon".into()),
            system_biome: reachlock_core::seed::types::Biome::Core,
            system_fidelity: reachlock_core::generator::system::Fidelity::Full,
            ..default()
        })
        .init_resource::<SceneRegistry>()
        .init_resource::<interior::CurrentInterior>()
        .init_resource::<interior::ActiveDeck>()
        .init_resource::<docking::TransitionBeat>()
        .init_resource::<pause::PausedFrom>()
        .init_resource::<pause::PauseSelection>()
        .init_resource::<menu::MenuSelection>()
        // S07/S08: inventory, crew, interaction, autosave.
        .init_resource::<inventory::PlayerInventory>()
        .init_resource::<crew::CrewRoster>()
        .init_resource::<interaction::InteractionPrompt>()
        .init_resource::<interaction::ActivePanel>()
        .init_resource::<inventory::SaveTimer>()
        .init_resource::<market::MarketState>()
        // S17: the applied exterior config (restored by load_save) + the
        // editor's live state. Must exist before Startup: load_save reads it.
        .init_resource::<shipeditor::ShipConfig>()
        .init_resource::<shipeditor::ShipEditorState>()
        // S18: the applied interior layout (restored by load_save) + the
        // interior editor's live state. Must exist before Startup: load_save
        // reads it; enter_interior realizes it on boarding.
        .init_resource::<shipeditor::InteriorConfig>()
        .init_resource::<shipeditor::InteriorEditorState>()
        // S31: settings loaded from disk BEFORE any system reads them. The
        // cached help text is derived from it.
        .insert_resource(settings::load_settings())
        .init_resource::<settings::HelpTextCache>()
        .init_resource::<settings_ui::SettingsUiState>()
        // S12: the one universe — economy + factions + news, advanced by the
        // ticker. Built before Startup so load_save can restore into it.
        .init_resource::<ticker::UniverseTicker>()
        .init_resource::<factions::ReputationPanelVisible>()
        // S09: live jump/transit bookkeeping + sensors.
        .init_resource::<jump::TransitState>()
        .init_resource::<jump::FtlRoute>()
        .init_resource::<sensors::MapOverlayState>()
        // S09b: cross-mode command bus — OnBoard consoles (gunner/scanner/
        // miner/power) write it, the flight systems read it (spec §22).
        .init_resource::<ship::ShipCommand>()
        // S09b-2: death/respawn beat after a hull breach.
        .init_resource::<ship::RespawnTimer>()
        // S09c: Star Fox feel layer — smoothed axes, bank, barrel roll,
        // camera blends. Render-layer only.
        .init_resource::<ship::FlightFeel>()
        // S09d: which console is showing the live flight scene this frame.
        .init_resource::<onboard::ActiveStationView>()
        // S13: authored souls + live soul state (filled by init_souls,
        // restored over by load_save).
        .init_resource::<soul::SoulRegistry>()
        // S16: the one live conversation (soul-backed dialogue panel).
        .init_resource::<dialogue::DialogueSession>()
        // S09e: the jump-cryo loop's plan/clock (SHIPS.md §3).
        .init_resource::<cryojump::JumpPlan>()
        // S09f: compartment fires + the crisis clock (SHIPS.md §4).
        .init_resource::<crisis::ShipFires>()
        // S16B: crew comm traffic (HUD lines + speech bubbles).
        .init_resource::<comms::CommFeed>()
        // S19: space combat — seeded encounters, subsystem targeting, the
        // in-flight power split, and the damage-control contract.
        .init_resource::<combat::SpawnedEncounters>()
        .init_resource::<combat::ReinforcedWings>()
        .init_resource::<combat::PlayerTargeting>()
        .init_resource::<combat::ShieldRegenCarry>()
        .init_resource::<combat::PowerSelect>()
        .init_resource::<combat::DamageControl>()
        // S20: landed (humanoid) combat — enemy/companion state, the 10 Hz
        // tick gate.
        .init_resource::<landed_combat::LandedCombatState>()
        .init_resource::<landed_combat::LandedTick>()
        // S08: start with the canonical crew (stable ids for S13 souls).
        .insert_resource(crew::CrewRoster::default_crew())
        .add_systems(
            Startup,
            (
                // Chained: souls come from the content index, and the save
                // restores live soul/universe state over the fresh defaults.
                (
                    content_index::load_content_index,
                    soul::init_souls,
                    inventory::load_save,
                )
                    .chain(),
                menu::spawn_menu,
                sensors::init_blip_assets,
                setup::apply_video_settings,
            ),
        )
        .add_systems(
            Update,
            menu::menu_input.run_if(in_state(AppState::MainMenu)),
        )
        // S31: keep the help-text cache in sync with keybind changes.
        .add_systems(Update, hud::refresh_help_cache)
        // S31: settings panel — spawn/despawn the text entity and drive it.
        // Both systems early-return when the panel is closed.
        .add_systems(
            Update,
            (
                settings_ui::sync_settings_panel,
                settings_ui::settings_ui_system,
            ),
        )
        // HUD spawns once when the game starts; it adapts per mode in
        // `update_hud`.
        .add_systems(
            OnEnter(AppState::InGame),
            (
                hud::spawn_hud,
                reticle::spawn_reticle,
                onboard::spawn_onboard_panels,
                comms::spawn_comm_hud,
                combat::spawn_combat_hud,
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
        .add_systems(
            OnEnter(GameMode::Landed),
            (
                interior::enter_interior,
                landed_combat::spawn_landed_enemies,
            )
                .chain(),
        )
        // S20: landed combat sim runs on a fixed 10 Hz tick (frame-rate
        // independent i-frames). The gate advances first; the rest read it.
        .add_systems(
            FixedUpdate,
            (
                landed_combat::advance_landed_tick,
                landed_combat::step_landed_enemies,
                landed_combat::apply_landed_hits,
                landed_combat::companion_combat_system,
            )
                .chain()
                .run_if(in_state(GameMode::Landed)),
        )
        // S20: input capture, gizmo/HUD render, and prop resolution run every
        // frame (input responsiveness + immediate-mode gizmos).
        .add_systems(
            Update,
            (
                landed_combat::landed_combat_player,
                landed_combat::render_landed_combat,
                landed_combat::step_props,
            )
                .run_if(in_state(GameMode::Landed)),
        )
        // --- OnBoard scene ---
        .add_systems(OnEnter(GameMode::OnBoard), interior::enter_interior)
        // Deck transit: the ladder clears `SceneRegistry::scene`, and this
        // Update copy of the builder rebuilds the interior on the new deck
        // (it early-outs every other frame).
        .add_systems(Update, interior::enter_interior.run_if(in_any_interior))
        // --- SpaceFlight-only gameplay (the pilot's hands on the stick) ---
        .add_systems(
            Update,
            (
                ship::control,
                docking::try_dock,
                docking::try_board_hostile,
                docking::leave_helm,
                jump::try_gate_jump,
                jump::gate_selection_input,
                jump::gate_choice_overlay,
                jump::self_jump,
                galaxy_map::galaxy_map_toggle,
                galaxy_map::galaxy_map_click,
                galaxy_map::galaxy_map_cancel_ftl,
                galaxy_map::render_galaxy_map,
                landed_combat::tumble_derelicts,
                landed_combat::pulse_beacons,
            )
                .run_if(in_spaceflight),
        )
        // S09b/S09d: weapons/scanner/mining driven by the OnBoard consoles
        // via the ShipCommand bus, rendered in the 3D flight scene. These run
        // whenever the space scene is live — flying, or crewing a console
        // mid-flight — so a shot fired from the gunner console flies in real
        // time in the same world.
        .add_systems(
            Update,
            (
                ship::camera_follow,
                ship::sync_ship_visibility,
                ship::fire_weapons,
                ship::step_projectiles,
                ship::mining_beam,
                ship::scanner_pulse,
                ship::request_scan_from_key,
                ship::engine_glow,
                systems::starfield::dust_parallax,
            )
                .run_if(space_live),
        )
        .add_systems(Update, (ship::collisions,).run_if(space_live))
        // S19: space combat. Enemies fly/fire whenever the space scene is
        // live (a gunner at the console fights the same battle the pilot
        // would); the quick-key controls belong to the stick, so they run
        // in SpaceFlight only.
        .add_systems(
            Update,
            (
                combat::spawn_encounters,
                combat::enemy_fly,
                combat::enemy_fire,
                combat::step_enemy_projectiles,
                combat::combat_hits,
                combat::step_explosions,
                combat::player_shield,
            )
                .run_if(space_live),
        )
        .add_systems(
            Update,
            (
                combat::cycle_target,
                combat::cycle_target_reverse,
                combat::power_quick_keys,
                combat::pop_chaff,
            )
                .run_if(in_spaceflight),
        )
        // The damage-control contract and combat HUD run in every InGame
        // mode: fires burn (and Tove triages) whichever deck you're on.
        .add_systems(
            Update,
            (combat::damage_control, combat::update_combat_hud).run_if(in_state(AppState::InGame)),
        )
        // S09d: publish which station view is open, then mask/unmask the
        // interior around it (chained: the mask must see this frame's view).
        .add_systems(
            Update,
            (onboard::update_station_view, onboard::station_view_mask)
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
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
                .run_if(space_live),
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
                interior::sync_crew_deck_presence,
                mode::interior_camera_follow,
                docking::try_interior_transitions,
                interaction::try_interact,
                dialogue::sync_dialogue_session,
                dialogue::dialogue_input,
                market::market_system,
                crew::crew_shift_system,
                onboard::onboard_panels,
                onboard::onboard_ship_consoles,
                jump::fuel_dock,
            )
                .run_if(in_any_interior),
        )
        // S17: the exterior editor (Shipyard terminal panel) + its orbit
        // preview. Interior-only — the panel opens while docked/landed.
        .add_systems(
            Update,
            (shipeditor::editor_system, shipeditor::editor_preview)
                .chain()
                .run_if(in_any_interior),
        )
        // S18: the interior editor (interior-refit terminal panel) + its 2D
        // grid preview. Same interior-only run condition and chained
        // dirty-flag pattern as the exterior editor.
        .add_systems(
            Update,
            (
                shipeditor::interior_editor_system,
                shipeditor::interior_editor_preview,
            )
                .chain()
                .run_if(in_any_interior),
        )
        // Interior feel layer: figure walk animation + y-sort (avatar, NPCs,
        // crew share it), NPC wandering, and the interaction highlight ring.
        .add_systems(
            Update,
            (
                interior::animate_figures,
                interior::wander_npcs,
                interior::highlight_interactable,
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
                ticker::tick_universe,
                factions::reputation_panel_toggle,
                // S13: the world writes to the crew's souls (ship damage →
                // mood shifts, logged). Runs everywhere InGame — the hull
                // doesn't care which deck you're standing on.
                soul::soul_ship_damage_events,
                // S09e: the jump clock runs wherever you are — that's the
                // point. The pod doesn't care which console you're at.
                cryojump::jump_clock,
                cryojump::pod_stasis,
                // S16B: comm lines age on the HUD; bubbles follow speakers.
                comms::tick_comms,
                comms::comm_bubbles,
            )
                .run_if(in_state(AppState::InGame)),
        )
        // S09f: fires ignite from flight damage and burn on their own clock
        // whichever deck (or mode) you're in; sprites render on the active
        // deck only.
        .add_systems(
            Update,
            (
                crisis::ignite_from_damage,
                crisis::tick_fires,
                crisis::sync_fire_sprites,
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
