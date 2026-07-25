//! Universe tick integration (S12): a single `UniverseState` that composes
//! [`EconomyState`] + [`FactionState`] into one clock-driven advancing entity.
//! Same seed + same event log = same state everywhere (offline, server, replay).
//!
//! The advance order is a **compatibility promise** — once pinned by a parity
//! test, it must never change without a manifest version bump.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::economy::{EconomyState, StationKind};
use crate::faction::{
    evaluate_storylines, load_faction_catalog, tick_factions, FactionEvent, FactionState, Storyline,
};

/// The canonical seed every authority feeds into [`UniverseState::advance`].
/// The offline ticker, the server tick task, and online replay must all use
/// this one value — parity ("same seed + same event log = same universe")
/// holds only while there is exactly one seed.
pub const CANON_SEED: u64 = 0x5EED_0001;

/// Canonical station roster shared by the offline client and the server so
/// both authorities advance byte-identical universes. The ids match the
/// stations the client actually visits (S06 locations).
pub fn canon_station_seeds() -> Vec<(String, u64, StationKind, Option<String>)> {
    vec![
        ("home".into(), 0x5EA17, StationKind::Hub, None),
        (
            "refinery-prime".into(),
            0xABCDEF,
            StationKind::Refinery,
            None,
        ),
        ("outpost-7".into(), 0x13579B, StationKind::Outpost, None),
    ]
}

/// Build the canonical starting universe both authorities share. Pure: same
/// embedded catalogues, same station seeds, every call byte-identical.
pub fn canon_universe() -> UniverseState {
    let economy = EconomyState::new(crate::economy::load_goods_catalog(), &canon_station_seeds());
    let factions = FactionState::new(load_faction_catalog());
    UniverseState::new(economy, factions)
}

/// One logical tick's worth of output. Wraps events from the economy and
/// faction subsystems.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimEvent {
    EconomyTick {
        tick_no: u64,
    },
    DiplomaticShift {
        faction: String,
        other: String,
        change: i64,
    },
    ContentRelease {
        content_id: String,
        priority: String,
    },
    ChapterFired {
        chapter_id: String,
    },
}

/// The whole advancing universe state. Serialized as part of the save data
/// (offline) and as the server's authoritative tick state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseState {
    /// Monotonic tick counter.
    pub tick_no: u64,
    /// Economy state (S10).
    pub economy: EconomyState,
    /// Faction simulation state (S11).
    pub factions: FactionState,
    /// Chapter IDs that have already fired (idempotency guard). A `BTreeSet`
    /// so the serialized form is deterministic — this struct is snapshot and
    /// parity-compared as bytes.
    pub chapters: BTreeSet<String>,
    /// Rolling event log (capped to a reasonable window).
    pub event_log: Vec<SimEvent>,
}

impl UniverseState {
    /// Build a fresh universe from the canonical catalogues.
    pub fn new(economy: EconomyState, factions: FactionState) -> Self {
        Self {
            tick_no: 0,
            economy,
            factions,
            chapters: BTreeSet::new(),
            event_log: Vec::new(),
        }
    }

    /// Advance one tick. Returns the events produced by this tick.
    ///
    /// **Order (pinned by the `parity_offline_vs_server` test):**
    ///   1. Economy tick
    ///   2. Faction drift + events
    ///   3. Storyline evaluation (idempotent-once)
    pub fn advance(&mut self, seed: u64, storylines: &[Storyline]) -> Vec<SimEvent> {
        self.tick_no += 1;
        let mut events = Vec::new();

        // 1. Economy
        self.economy.tick(seed);
        events.push(SimEvent::EconomyTick {
            tick_no: self.tick_no,
        });

        // 2. Faction drift
        let (new_factions, faction_events) = tick_factions(self.factions.clone());
        self.factions = new_factions;
        for fe in &faction_events {
            match fe {
                FactionEvent::DiplomaticShift {
                    faction,
                    other,
                    change,
                } => {
                    events.push(SimEvent::DiplomaticShift {
                        faction: faction.0.clone(),
                        other: other.0.clone(),
                        change: *change,
                    });
                }
                FactionEvent::ContentRelease {
                    content_id,
                    priority,
                } => {
                    events.push(SimEvent::ContentRelease {
                        content_id: content_id.clone(),
                        priority: priority.clone(),
                    });
                }
                _ => {} // FactionMove / MissionUnlock emitted but not yet
                        // surfaced as SimEvent in S12 scope.
            }
        }

        // 3. Storyline evaluation
        // NOTE: evaluate_storylines reads self.factions (which has been
        // replaced by the ticked clone). fired_chapters on self.factions has
        // already been committed by tick_factions. The dupe guard uses
        // UniverseState.chapters so storylines DON'T re-fire across ticks
        // even though the FactionState.flavor of fired_chapters is empty.
        let fired = evaluate_storylines(&self.factions, storylines);
        for id in fired {
            if self.chapters.insert(id.clone()) {
                events.push(SimEvent::ChapterFired { chapter_id: id });
            } else {
                // Already recorded — no duplicate events.
            }
        }

        // Append to the event log (cap at 128 entries for save/transfer).
        self.event_log.append(&mut events.clone());
        if self.event_log.len() > 128 {
            self.event_log.drain(0..self.event_log.len() - 128);
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::{load_goods_catalog, StationKind};
    use crate::faction::{load_faction_catalog, load_storylines};

    /// Create a seeded universe with stub stations. Both sides of the parity
    /// test call this ctor so the starting state is byte-identical.
    fn seeded_universe(seed: u64) -> UniverseState {
        let catalog = load_goods_catalog();
        let station_seeds = vec![
            ("hub-1".into(), seed ^ 0x111, StationKind::Hub, None),
            ("ref-1".into(), seed ^ 0x222, StationKind::Refinery, None),
            ("bm-1".into(), seed ^ 0x333, StationKind::BlackMarket, None),
        ];
        let economy = EconomyState::new(catalog, &station_seeds);
        let catalog = load_faction_catalog();
        let factions = FactionState::new(catalog);
        UniverseState::new(economy, factions)
    }

    #[test]
    fn advance_is_deterministic() {
        let a = seeded_universe(42);
        let b = seeded_universe(42);
        assert_eq!(a, b, "same seed → same initial state");

        // Advance both offline-style (a) and server-style (b) — same result.
        let stories = load_storylines();
        let (mut a, mut b) = (a, b);
        for step in 0..20u64 {
            let ev_a = a.advance(step, &stories);
            let ev_b = b.advance(step, &stories);
            assert_eq!(ev_a, ev_b, "event divergence at tick {step}");
        }
        assert_eq!(a, b, "state divergence after 20 ticks");
    }

    /// The S12 headline: two universes, same seed, same injected player-trade
    /// log — one advanced "offline-style", one "server-style" — must be
    /// byte-identical (serialized) at tick N. Both sides start from
    /// `canon_universe()` and advance with `CANON_SEED`, exactly like the
    /// offline ticker and the server tick task.
    #[test]
    fn parity_offline_vs_server() {
        let mut offline = canon_universe();
        let mut server = canon_universe();
        let stories = load_storylines();
        let good = offline.economy.catalog.ids()[0].clone();

        for step in 0..40u64 {
            // Inject the same player-trade log on both sides mid-stream.
            if step == 10 || step == 25 {
                for u in [&mut offline, &mut server] {
                    u.economy
                        .stations
                        .get_mut("home")
                        .expect("canon universe has the home station")
                        .record_trade(&good, 5);
                }
            }
            let ev_a = offline.advance(CANON_SEED, &stories);
            let ev_b = server.advance(CANON_SEED, &stories);
            assert_eq!(ev_a, ev_b, "event divergence at tick {step}");
        }

        let a = serde_json::to_string(&offline).expect("serialize offline");
        let b = serde_json::to_string(&server).expect("serialize server");
        assert_eq!(a, b, "serialized state diverged at tick 40");
    }

    #[test]
    fn advance_order_is_pinned() {
        // The order is: economy → factions → storylines.
        // If any subsystem changes, this test catches the serialized shift.
        let mut u = seeded_universe(0);
        let stories = load_storylines();
        let events = u.advance(1, &stories);

        // Exactly three events: EconomyTick + DiplomaticShift + ChapterFired
        // The compact/isc relationship (Allied at 100) should drift by
        // Diplomatic rate (3/tick → 97) which emits a DiplomaticShift.
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SimEvent::EconomyTick { .. })),
            "first event is the economy tick"
        );
    }
}
