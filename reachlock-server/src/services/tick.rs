//! Universe tick (spec §8): NPC economy, faction updates, event generation.
//! Runs on its own task; talks to sessions only through the broadcast
//! channel; skips missed ticks instead of queueing them (adversarial
//! finding #6 — the tick must never back-pressure the WebSocket handlers).
//!
//! Owns a [`UniverseState`] that advances each tick, snapshots to disk, and
//! broadcasts every [`SimEvent`] as a `universe.event` message (S12). The
//! universe and the seed are the canonical ones from `reachlock_core::sim` —
//! the same construction the offline client ticker uses, which is what makes
//! offline/online parity hold (see `sim::tests::parity_offline_vs_server`).

use std::path::Path;
use std::sync::Arc;

use reachlock_core::faction::Storyline;
use reachlock_core::network::ServerMessage;
use reachlock_core::sim::{canon_universe, UniverseState, CANON_SEED};
use tokio::time::MissedTickBehavior;

use crate::ws::AppState;

/// Path for the authoritative tick snapshot (offline-first parity).
const SNAPSHOT_PATH: &str = "data/tick/snap.json";
/// Snapshot every N ticks to avoid hammering disk.
const SNAPSHOT_EVERY: u64 = 10;

pub async fn run(state: Arc<AppState>, interval_secs: u64) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // The canonical universe; a prior snapshot resumes it across restarts.
    let mut universe = match load_snapshot(Path::new(SNAPSHOT_PATH)) {
        Some(saved) => {
            tracing::info!("loaded tick snapshot tick_no={}", saved.tick_no);
            saved
        }
        None => canon_universe(),
    };
    let storylines = reachlock_core::faction::load_storylines();

    loop {
        interval.tick().await;

        // Advance and broadcast every SimEvent as a universe.event message.
        for msg in tick_once(&mut universe, &storylines) {
            let _ = state.events.send(msg);
        }

        if universe.tick_no.is_multiple_of(SNAPSHOT_EVERY) {
            if let Err(e) = write_snapshot(&universe, Path::new(SNAPSHOT_PATH)) {
                tracing::error!("tick snapshot failed: {e}");
            }
        }
    }
}

/// Advance the universe one tick and build the broadcast messages for the
/// events it produced. Pure with respect to IO — unit-testable.
fn tick_once(universe: &mut UniverseState, storylines: &[Storyline]) -> Vec<ServerMessage> {
    universe
        .advance(CANON_SEED, storylines)
        .iter()
        .map(|sim_ev| ServerMessage::UniverseEvent {
            event: serde_json::to_value(sim_ev)
                .unwrap_or_else(|_| serde_json::json!({"kind":"tick","tick":universe.tick_no})),
        })
        .collect()
}

/// Serialize the universe to `path`, creating parent directories.
fn write_snapshot(universe: &UniverseState, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string(universe).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

/// Load a prior snapshot; `None` on missing or corrupt (fresh start).
fn load_snapshot(path: &Path) -> Option<UniverseState> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<UniverseState>(&text) {
        Ok(u) => Some(u),
        Err(e) => {
            tracing::warn!("tick snapshot corrupt, starting fresh: {e}");
            None
        }
    }
}

#[cfg(feature = "postgres")]
pub mod pg {
    //! S16B: the `universe_events` append the S12 brief named. Same shape
    //! as every pg module here: the implementation + an env-gated test;
    //! the S03 store-selection follow-up threads the pool into `run`.

    use reachlock_core::sim::SimEvent;
    use reachlock_core::universe::UniverseTier;
    use sqlx::PgPool;

    /// Append one tick's events to the ledger.
    pub async fn append_events(
        pool: &PgPool,
        tier: UniverseTier,
        events: &[SimEvent],
    ) -> Result<(), sqlx::Error> {
        for event in events {
            let (event_type, payload) = match serde_json::to_value(event) {
                Ok(v) => {
                    let t = v
                        .as_object()
                        .and_then(|o| o.keys().next().cloned())
                        .unwrap_or_else(|| "sim_event".into());
                    (t, v)
                }
                Err(_) => continue,
            };
            sqlx::query(
                "INSERT INTO universe_events (universe, event_type, payload)
                 VALUES ($1::universe_tier, $2, $3)",
            )
            .bind(tier.as_str())
            .bind(&event_type)
            .bind(&payload)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod pg_tests {
        use super::*;

        /// Runs only where `DATABASE_URL` is set (CI's postgres job).
        #[tokio::test]
        async fn events_append_when_db_available() {
            let Ok(url) = std::env::var("DATABASE_URL") else {
                return;
            };
            let pool = PgPool::connect(&url).await.expect("connect");
            sqlx::migrate!("./migrations").run(&pool).await.ok();
            let events = vec![SimEvent::EconomyTick { tick_no: 1 }];
            append_events(&pool, UniverseTier::FairPlay, &events)
                .await
                .expect("append");
            let (count,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM universe_events WHERE event_type = 'economy_tick'",
            )
            .fetch_one(&pool)
            .await
            .expect("count");
            assert!(count >= 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::sim::SimEvent;

    #[test]
    fn tick_once_broadcasts_wire_shaped_events() {
        let mut universe = canon_universe();
        let storylines = reachlock_core::faction::load_storylines();
        let msgs = tick_once(&mut universe, &storylines);
        assert_eq!(universe.tick_no, 1);
        assert!(!msgs.is_empty(), "every tick emits at least EconomyTick");
        // Every broadcast payload must round-trip back into a SimEvent —
        // that is the contract the client replay relies on.
        for msg in &msgs {
            let ServerMessage::UniverseEvent { event } = msg else {
                panic!("tick_once only emits UniverseEvent");
            };
            let sim: SimEvent =
                serde_json::from_value(event.clone()).expect("payload is a SimEvent");
            if let SimEvent::EconomyTick { tick_no } = sim {
                assert_eq!(tick_no, 1);
            }
        }
    }

    #[test]
    fn snapshot_round_trips() {
        let mut universe = canon_universe();
        let storylines = reachlock_core::faction::load_storylines();
        for _ in 0..3 {
            tick_once(&mut universe, &storylines);
        }
        let path = std::env::temp_dir().join(format!("reachlock-tick-test-{}.json", process_id()));
        write_snapshot(&universe, &path).expect("snapshot writes");
        let loaded = load_snapshot(&path).expect("snapshot loads");
        std::fs::remove_file(&path).ok();
        assert_eq!(universe, loaded, "snapshot must round-trip byte-faithful");
    }

    #[test]
    fn corrupt_snapshot_is_a_fresh_start() {
        let path =
            std::env::temp_dir().join(format!("reachlock-tick-corrupt-{}.json", process_id()));
        std::fs::write(&path, "not json {").expect("write corrupt file");
        let loaded = load_snapshot(&path);
        std::fs::remove_file(&path).ok();
        assert!(loaded.is_none());
    }

    fn process_id() -> u32 {
        std::process::id()
    }
}
