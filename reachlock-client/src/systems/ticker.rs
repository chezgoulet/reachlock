//! Universe ticker (S12): a client-side resource that advances
//! [`UniverseState`] on a game clock, respects pause, persists to save,
//! and fast-forwards on load with a hard cap.
//!
//! This replaces the old per-frame `tick_economy` and `tick_faction_system`
//! systems — the universe is one clock now.

use bevy::prelude::*;

use reachlock_core::economy::EconomyState;
use reachlock_core::faction::{load_faction_catalog, load_storylines, FactionState};
use reachlock_core::sim::{SimEvent, UniverseState};

/// One universe tick every N seconds of real (non-paused) play.
const TICK_INTERVAL_SECS: f64 = 5.0;
/// Hard cap for catch-up: at most this many ticks may be fast-forwarded on
/// load. Anything beyond is logged as "the markets moved while you slept."
const CATCHUP_CAP: u64 = 200;

/// The live universe simulation, ticked on a game clock.
#[derive(Resource)]
pub struct UniverseTicker {
    pub state: UniverseState,
    accumulator: f64,
    pub storylines: Vec<reachlock_core::faction::Storyline>,
}

impl UniverseTicker {
    /// Build a fresh ticker from the canonical economy + faction catalogues.
    pub fn new(economy: EconomyState, factions: FactionState) -> Self {
        let state = UniverseState::new(economy, factions);
        let storylines = load_storylines();
        Self {
            state,
            accumulator: 0.0,
            storylines,
        }
    }

    /// Called once per frame. `dt` is the delta time of the current frame
    /// (0.0 when paused). Returns the ticks that actually advanced (for
    /// broadcasting to event subscribers).
    pub fn tick_frame(&mut self, dt: f64, seed: u64) -> Vec<Vec<SimEvent>> {
        if dt <= 0.0 {
            return Vec::new();
        }
        self.accumulator += dt;
        let mut batch = Vec::new();
        while self.accumulator >= TICK_INTERVAL_SECS {
            self.accumulator -= TICK_INTERVAL_SECS;
            let events = self.state.advance(seed, &self.storylines);
            batch.push(events);
        }
        batch
    }

    /// Fast-forward after loading a save. Caps at CATCHUP_CAP ticks.
    pub fn catch_up(&mut self, seed: u64) -> Vec<Vec<SimEvent>> {
        let mut batch = Vec::new();
        for _ in 0..CATCHUP_CAP {
            let events = self.state.advance(seed, &self.storylines);
            batch.push(events);
        }
        bevy::log::info!(
            "catch-up: advanced {} ticks (markets moved while you slept).",
            CATCHUP_CAP,
        );
        batch
    }
}

/// Spawn the universe ticker at startup.
pub fn init_ticker(mut commands: Commands) {
    let catalog = load_goods_catalog();
    let station_seeds = vec![
        (
            "home".to_string(),
            0x5EA17,
            reachlock_core::economy::StationKind::Hub,
            None,
        ),
        (
            "refinery-prime".to_string(),
            0xABCDEF,
            reachlock_core::economy::StationKind::Refinery,
            None,
        ),
        (
            "outpost-7".to_string(),
            0x13579B,
            reachlock_core::economy::StationKind::Outpost,
            None,
        ),
    ];
    let economy = EconomyState::new(catalog, &station_seeds);
    let factions = FactionState::new(load_faction_catalog());
    commands.insert_resource(UniverseTicker::new(economy, factions));
}

/// Advance the universe ticker on the game clock (respects pause).
pub fn tick_universe(time: Res<Time>, mut ticker: ResMut<UniverseTicker>) {
    let dt = time.delta_secs_f64();
    // Time::delta is 0.0 when paused (the Pause system stops the clock).
    // The ticker handles dt <= 0.0 gracefully.
    let seed = (time.elapsed_secs_f64() as u64).wrapping_mul(0x9E3779B1);
    ticker.tick_frame(dt, seed);
}

fn load_goods_catalog() -> reachlock_core::economy::GoodsCatalog {
    reachlock_core::economy::load_goods_catalog()
}
