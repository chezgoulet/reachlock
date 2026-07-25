//! Universe ticker (S12): a client-side resource that advances
//! [`UniverseState`] on a game clock, respects pause, persists to save,
//! and fast-forwards on load with a hard cap.
//!
//! This replaces the old per-frame `tick_economy` and `tick_faction_system`
//! systems — the universe is one clock now, and `UniverseTicker.state` is
//! the *only* copy of the economy + factions the client holds. The market,
//! HUD, and reputation systems all read through it.

use bevy::prelude::*;

use reachlock_core::sim::{canon_universe, SimEvent, UniverseState, CANON_SEED};

/// One universe tick every N seconds of real (non-paused) play. Integer so
/// save-file catch-up arithmetic stays in integers (iron rule #2 — the f64
/// twin below exists only for frame-time accumulation on the render side).
pub const TICK_SECS: u64 = 5;
const TICK_INTERVAL_SECS: f64 = TICK_SECS as f64;
/// Hard cap for catch-up: at most this many ticks may be fast-forwarded on
/// load. Anything beyond is logged as "the markets moved while you slept."
const CATCHUP_CAP: u64 = 200;

/// The live universe simulation, ticked on a game clock.
#[derive(Resource)]
pub struct UniverseTicker {
    pub state: UniverseState,
    accumulator: f64,
    pub storylines: Vec<reachlock_core::faction::Storyline>,
    /// When true, the local ticker is paused and universe state is driven by
    /// server events instead. Set by the network system on connect/disconnect.
    pub online_mode: bool,
}

impl UniverseTicker {
    /// Build a fresh ticker from the canonical universe — the same
    /// construction the server uses, so offline and online agree (parity).
    pub fn new() -> Self {
        Self {
            state: canon_universe(),
            accumulator: 0.0,
            storylines: reachlock_core::faction::load_storylines(),
            online_mode: false,
        }
    }

    /// Called once per frame. `dt` is the delta time of the current frame
    /// (0.0 when paused). Skips everything when `online_mode` is true.
    /// Returns the ticks that actually advanced (for broadcasting to event
    /// subscribers).
    pub fn tick_frame(&mut self, dt: f64) -> Vec<Vec<SimEvent>> {
        if self.online_mode || dt <= 0.0 {
            return Vec::new();
        }
        self.accumulator += dt;
        let mut batch = Vec::new();
        while self.accumulator >= TICK_INTERVAL_SECS {
            self.accumulator -= TICK_INTERVAL_SECS;
            let events = self.state.advance(CANON_SEED, &self.storylines);
            batch.push(events);
        }
        batch
    }

    /// Replay one authoritative server tick. Online mode receives
    /// `EconomyTick` events instead of ticking locally; advancing with the
    /// same canonical seed reproduces the server's step exactly (parity),
    /// which is what updates local prices, standings, and news.
    pub fn replay_server_tick(&mut self) -> Vec<SimEvent> {
        self.state.advance(CANON_SEED, &self.storylines)
    }

    /// Fast-forward `elapsed_ticks` after loading a save, capped at
    /// [`CATCHUP_CAP`]. Returns the per-tick event batches that ran.
    pub fn catch_up(&mut self, elapsed_ticks: u64) -> Vec<Vec<SimEvent>> {
        let ticks = elapsed_ticks.min(CATCHUP_CAP);
        let mut batch = Vec::new();
        for _ in 0..ticks {
            let events = self.state.advance(CANON_SEED, &self.storylines);
            batch.push(events);
        }
        if elapsed_ticks > CATCHUP_CAP {
            bevy::log::info!(
                "catch-up truncated at {CATCHUP_CAP} of {elapsed_ticks} ticks \
                 (the markets moved while you slept)."
            );
        } else if ticks > 0 {
            bevy::log::info!("catch-up: advanced {ticks} ticks.");
        }
        batch
    }
}

impl Default for UniverseTicker {
    fn default() -> Self {
        Self::new()
    }
}

/// Advance the universe ticker on the game clock (respects pause).
pub fn tick_universe(time: Res<Time>, mut ticker: ResMut<UniverseTicker>) {
    // Time::delta is 0.0 when paused (the Pause system stops the clock).
    // The ticker handles dt <= 0.0 gracefully.
    ticker.tick_frame(time.delta_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_up_advances_elapsed_ticks() {
        let mut t = UniverseTicker::new();
        let batches = t.catch_up(7);
        assert_eq!(batches.len(), 7);
        assert_eq!(t.state.tick_no, 7);
    }

    #[test]
    fn catch_up_is_capped() {
        let mut t = UniverseTicker::new();
        let batches = t.catch_up(CATCHUP_CAP + 5_000);
        assert_eq!(batches.len() as u64, CATCHUP_CAP);
        assert_eq!(t.state.tick_no, CATCHUP_CAP);
    }

    #[test]
    fn tick_frame_respects_pause_and_online_mode() {
        let mut t = UniverseTicker::new();
        assert!(t.tick_frame(0.0).is_empty(), "paused frames never tick");
        t.online_mode = true;
        assert!(
            t.tick_frame(TICK_INTERVAL_SECS * 3.0).is_empty(),
            "online mode never ticks locally (one authority per mode)"
        );
        t.online_mode = false;
        let batches = t.tick_frame(TICK_INTERVAL_SECS * 3.0);
        assert_eq!(batches.len(), 3, "accumulated time drains into ticks");
    }

    #[test]
    fn offline_ticker_matches_server_replay() {
        // The offline path (tick_frame) and the online path
        // (replay_server_tick) must advance the universe identically.
        let mut offline = UniverseTicker::new();
        let mut online = UniverseTicker::new();
        offline.tick_frame(TICK_INTERVAL_SECS * 4.0);
        for _ in 0..4 {
            online.replay_server_tick();
        }
        assert_eq!(offline.state, online.state);
    }
}
