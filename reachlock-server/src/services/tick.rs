//! Universe tick (spec §8): NPC economy, faction updates, event generation.
//! Runs on its own task; talks to sessions only through the broadcast
//! channel; skips missed ticks instead of queueing them (adversarial
//! finding #6 — the tick must never back-pressure the WebSocket handlers).

use std::sync::Arc;

use reachlock_core::network::ServerMessage;
use tokio::time::MissedTickBehavior;

use crate::ws::AppState;

pub async fn run(state: Arc<AppState>, interval_secs: u64) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut tick: u64 = 0;

    loop {
        interval.tick().await;
        tick += 1;

        // v0 tick body: a heartbeat event. The economy/faction simulation
        // fills in here; whatever it computes, it publishes the same way.
        let event = ServerMessage::UniverseEvent {
            event: serde_json::json!({
                "kind": "tick",
                "tick": tick,
                "connected": state.connected_count(),
            }),
        };
        // Send fails only when no sessions are subscribed — that's fine.
        let _ = state.events.send(event);
    }
}
