//! On-board crisis model (S09f, docs/SHIPS.md §4): damage lands INSIDE the
//! ship. Compartment fires ignite from hull hits, grow, spread room-to-room
//! through open doors, damage the systems in the rooms they burn, and pull
//! crew off their stations. Fighting one is a crew task; venting a zero-g
//! compartment is fast, brutal, and final.
//!
//! Pure and deterministic (iron rules #1/#2): integer intensities
//! (1024 = fully involved), rolls derived from `(seed, tick, room)` through
//! the seed-protocol hash — never wall time. The client owns rendering and
//! input; this module owns what fire does.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::generator::{Door, GeneratedLayout, RoomKind};

/// Intensity gained per crisis tick while a fire burns unfought.
pub const GROWTH_PER_TICK: i64 = 96;
/// Intensity above which a fire can jump a door.
pub const SPREAD_THRESHOLD: i64 = 512;
/// Chance in 1024 that a hot fire jumps one door on one tick.
pub const SPREAD_CHANCE: i64 = 224;
/// Intensity removed by one extinguisher action.
pub const FIGHT_AMOUNT: i64 = 288;
/// Intensity at which a room's system counts as damaged (SHIPS.md §4:
/// "damaged systems operate below capacity or not at all until repaired").
pub const SYSTEM_DAMAGE_THRESHOLD: i64 = 640;

/// Fires per `(deck, room index)`. The inter-deck ladder is a sealed hatch:
/// fire never crosses decks (the threshold between the work and the life
/// holds, structurally).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FireState {
    pub burning: BTreeMap<(usize, usize), i64>,
}

/// What one crisis tick did — the client narrates and applies consequences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrisisEvent {
    Ignited { deck: usize, room: usize },
    Spread { deck: usize, from: usize, to: usize },
    SystemsBurning { deck: usize, room: usize },
    BurnedOut { deck: usize, room: usize },
    Extinguished { deck: usize, room: usize },
    Vented { deck: usize, room: usize },
}

impl FireState {
    pub fn is_burning(&self, deck: usize, room: usize) -> bool {
        self.burning.contains_key(&(deck, room))
    }

    pub fn intensity(&self, deck: usize, room: usize) -> i64 {
        self.burning.get(&(deck, room)).copied().unwrap_or(0)
    }

    /// Start (or feed) a fire.
    pub fn ignite(&mut self, deck: usize, room: usize, intensity: i64) -> CrisisEvent {
        let slot = self.burning.entry((deck, room)).or_insert(0);
        *slot = (*slot + intensity).clamp(1, 1024);
        CrisisEvent::Ignited { deck, room }
    }

    /// One extinguisher action against a room's fire.
    pub fn fight(&mut self, deck: usize, room: usize) -> Option<CrisisEvent> {
        let slot = self.burning.get_mut(&(deck, room))?;
        *slot -= FIGHT_AMOUNT;
        if *slot <= 0 {
            self.burning.remove(&(deck, room));
            Some(CrisisEvent::Extinguished { deck, room })
        } else {
            None
        }
    }

    /// Vent a compartment to vacuum: the fire dies instantly. The client
    /// owns the brutality (unsecured contents, anyone inside).
    pub fn vent(&mut self, deck: usize, room: usize) -> Option<CrisisEvent> {
        self.burning.remove(&(deck, room))?;
        Some(CrisisEvent::Vented { deck, room })
    }

    /// Advance every fire one tick against one deck's layout: growth,
    /// system damage, door spread, burn-out. Deterministic from
    /// `(seed, tick_no, room)`.
    pub fn tick_deck(
        &mut self,
        deck: usize,
        layout: &GeneratedLayout,
        seed: u64,
        tick_no: u64,
    ) -> Vec<CrisisEvent> {
        let mut events = Vec::new();
        let burning_now: Vec<(usize, i64)> = self
            .burning
            .iter()
            .filter(|((d, _), _)| *d == deck)
            .map(|((_, r), i)| (*r, *i))
            .collect();
        for (room, intensity) in burning_now {
            let next = intensity + GROWTH_PER_TICK;
            if next >= 1024 {
                // Fully involved: the room burns out — the fire dies with
                // nothing left to eat, the systems in it are already gone.
                self.burning.remove(&(deck, room));
                events.push(CrisisEvent::BurnedOut { deck, room });
                continue;
            }
            self.burning.insert((deck, room), next);
            if intensity < SYSTEM_DAMAGE_THRESHOLD && next >= SYSTEM_DAMAGE_THRESHOLD {
                events.push(CrisisEvent::SystemsBurning { deck, room });
            }
            if next >= SPREAD_THRESHOLD {
                for (i, neighbor) in adjacent_rooms(layout, room).into_iter().enumerate() {
                    if self.is_burning(deck, neighbor) {
                        continue;
                    }
                    if crisis_roll(seed, tick_no, room as u64, i as u64) % 1024
                        < SPREAD_CHANCE as u64
                    {
                        self.burning.insert((deck, neighbor), 128);
                        events.push(CrisisEvent::Spread {
                            deck,
                            from: room,
                            to: neighbor,
                        });
                    }
                }
            }
        }
        events
    }
}

/// Rooms connected to `room` by a door.
pub fn adjacent_rooms(layout: &GeneratedLayout, room: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for Door { from, to, .. } in &layout.doors {
        let (from, to) = (*from as usize, *to as usize);
        if from == room && !out.contains(&to) {
            out.push(to);
        }
        if to == room && !out.contains(&from) {
            out.push(from);
        }
    }
    out
}

/// Deterministic crisis roll from the seed-protocol hash primitives.
pub fn crisis_roll(seed: u64, tick_no: u64, room: u64, salt: u64) -> u64 {
    use crate::seed::resolver::{finalize, fnv1a, FNV_OFFSET};
    let mut h = FNV_OFFSET;
    h = fnv1a(h, b"ship_fire");
    h = fnv1a(h, &seed.to_le_bytes());
    h = fnv1a(h, &tick_no.to_le_bytes());
    h = fnv1a(h, &room.to_le_bytes());
    h = fnv1a(h, &salt.to_le_bytes());
    finalize(h)
}

/// SHIPS.md §4 power triage: a fire in engineering cuts the reactor's
/// distributable budget. `base_budget` is the power console's normal total.
pub fn effective_power_budget(
    base_budget: u8,
    fires: &FireState,
    decks: &[&GeneratedLayout],
) -> u8 {
    for (deck, layout) in decks.iter().enumerate() {
        for (room_index, room) in layout.rooms.iter().enumerate() {
            if room.kind == RoomKind::Reactor && fires.is_burning(deck, room_index) {
                return base_budget.saturating_sub(2);
            }
        }
    }
    base_budget
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::Room;

    /// Three rooms in a row: 0-1 and 1-2 doored.
    fn corridor() -> GeneratedLayout {
        let room = |x| Room {
            kind: RoomKind::Quarters,
            x,
            y: 0,
            width: 32,
            height: 32,
        };
        GeneratedLayout {
            rooms: vec![room(0), room(32), room(64)],
            doors: vec![
                Door {
                    from: 0,
                    to: 1,
                    x: 32,
                    y: 16,
                },
                Door {
                    from: 1,
                    to: 2,
                    x: 64,
                    y: 16,
                },
            ],
        }
    }

    #[test]
    fn fires_grow_and_burn_out_deterministically() {
        let layout = corridor();
        let mut a = FireState::default();
        let mut b = FireState::default();
        a.ignite(0, 0, 200);
        b.ignite(0, 0, 200);
        for t in 0..20 {
            let ea = a.tick_deck(0, &layout, 42, t);
            let eb = b.tick_deck(0, &layout, 42, t);
            assert_eq!(ea, eb, "same seed, same fire");
        }
        assert_eq!(a, b);
        // A fed, unfought fire eventually burns its room out.
        let mut c = FireState::default();
        c.ignite(0, 0, 200);
        let mut burned_out = false;
        for t in 0..20 {
            for e in c.tick_deck(0, &layout, 7, t) {
                if matches!(e, CrisisEvent::BurnedOut { room: 0, .. }) {
                    burned_out = true;
                }
            }
        }
        assert!(burned_out);
    }

    #[test]
    fn hot_fires_spread_through_doors_only() {
        let layout = corridor();
        let mut fires = FireState::default();
        fires.ignite(0, 0, SPREAD_THRESHOLD);
        let mut reached_1 = false;
        let mut reached_2_before_1 = false;
        for t in 0..64 {
            for e in fires.tick_deck(0, &layout, 3, t) {
                match e {
                    CrisisEvent::Spread { from: 0, to: 1, .. } => reached_1 = true,
                    CrisisEvent::Spread { from: 0, to: 2, .. } => reached_2_before_1 = true,
                    _ => {}
                }
            }
        }
        assert!(reached_1, "fire crossed the shared door");
        assert!(
            !reached_2_before_1,
            "room 2 shares no door with room 0 — no teleporting fire"
        );
    }

    #[test]
    fn fighting_and_venting_put_fires_out() {
        let mut fires = FireState::default();
        fires.ignite(0, 1, 500);
        assert!(
            fires.fight(0, 1).is_none(),
            "one action isn't enough at 500"
        );
        assert!(matches!(
            fires.fight(0, 1),
            Some(CrisisEvent::Extinguished { .. })
        ));
        assert!(!fires.is_burning(0, 1));

        fires.ignite(1, 2, 1000);
        assert!(matches!(fires.vent(1, 2), Some(CrisisEvent::Vented { .. })));
        assert!(!fires.is_burning(1, 2));
        assert!(
            fires.vent(1, 2).is_none(),
            "venting an empty room is a no-op"
        );
    }

    #[test]
    fn engineering_fire_cuts_the_power_budget() {
        let mut layout = corridor();
        layout.rooms[1].kind = RoomKind::Reactor;
        let mut fires = FireState::default();
        assert_eq!(effective_power_budget(5, &fires, &[&layout]), 5);
        fires.ignite(0, 1, 300);
        assert_eq!(effective_power_budget(5, &fires, &[&layout]), 3);
    }

    #[test]
    fn fires_never_cross_decks() {
        let layout = corridor();
        let mut fires = FireState::default();
        fires.ignite(0, 0, 1000);
        for t in 0..40 {
            fires.tick_deck(0, &layout, 9, t);
            fires.tick_deck(1, &layout, 9, t);
        }
        assert!(
            fires.burning.keys().all(|(deck, _)| *deck == 0),
            "the ladder hatch is a bulkhead"
        );
    }
}
