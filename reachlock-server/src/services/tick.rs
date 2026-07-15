//! Universe tick (spec §8): NPC economy, faction updates, event generation.
//! Runs on its own task; talks to sessions only through the broadcast
//! channel; skips missed ticks instead of queueing them (adversarial
//! finding #6 — the tick must never back-pressure the WebSocket handlers).
//!
//! Owns a [`UniverseState`] that advances each tick, snapshots to disk, and
//! broadcasts every [`SimEvent`] as a `universe.event` message (S12).

use std::path::Path;
use std::sync::Arc;

use reachlock_core::economy::{EconomyState, StationKind};
use reachlock_core::faction::{load_faction_catalog, FactionState};
use reachlock_core::network::ServerMessage;
use reachlock_core::sim::UniverseState;
use tokio::time::MissedTickBehavior;

use crate::ws::AppState;

/// Path for the authoritative tick snapshot (offline-first parity).
const SNAPSHOT_PATH: &str = "data/tick/snap.json";

pub async fn run(state: Arc<AppState>, interval_secs: u64) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Build the initial universe from embedded catalogues.
    let mut universe = build_universe();

    // Load prior snapshot if present (catches up on restart).
    if let Ok(text) = std::fs::read_to_string(SNAPSHOT_PATH) {
        match serde_json::from_str::<UniverseState>(&text) {
            Ok(saved) => {
                tracing::info!("loaded tick snapshot tick_no={}", saved.tick_no);
                universe = saved;
            }
            Err(e) => {
                tracing::warn!("tick snapshot corrupt, starting fresh: {e}");
            }
        }
    }

    // Canonical seed used for catch-up and ongoing ticks.
    let canon_seed: u64 = 0x5EED_0001;
    let storylines = load_storylines();

    loop {
        interval.tick().await;

        // Advance the universe: economy → factions → storylines.
        let events = universe.advance(canon_seed, &storylines);

        // Broadcast every SimEvent as a universe.event message.
        for sim_ev in &events {
            let msg = ServerMessage::UniverseEvent {
                event: serde_json::to_value(sim_ev)
                    .unwrap_or_else(|_| serde_json::json!({"kind":"tick","tick":universe.tick_no})),
            };
            let _ = state.events.send(msg);
        }

        // Snapshot every 10 ticks to avoid hammering disk.
        if universe.tick_no.is_multiple_of(10) {
            if let Some(parent) = Path::new(SNAPSHOT_PATH).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match serde_json::to_string(&universe) {
                Ok(text) => {
                    let _ = std::fs::write(SNAPSHOT_PATH, text);
                }
                Err(e) => {
                    tracing::error!("tick snapshot serialization failed: {e}");
                }
            }
        }
    }
}

/// Create a fresh universe from the embedded canon catalogues.
fn build_universe() -> UniverseState {
    let goods = reachlock_core::economy::load_goods_catalog();
    let station_seeds = vec![
        ("hub-1".into(), 0x5EA17u64, StationKind::Hub, None),
        ("ref-1".into(), 0xABCDEF, StationKind::Refinery, None),
        ("bm-1".into(), 0x13579B, StationKind::BlackMarket, None),
    ];
    let economy = EconomyState::new(goods, &station_seeds);
    let factions = FactionState::new(load_faction_catalog());
    UniverseState::new(economy, factions)
}

fn load_storylines() -> Vec<reachlock_core::faction::Storyline> {
    reachlock_core::faction::load_storylines()
}
