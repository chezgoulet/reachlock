//! Authored ship interiors (docs/SHIPS.md). Unlike station interiors, a
//! ship's layout is not seeded — it's the deck plan of a specific hull
//! class, laid out to make sense within the ship's footprint. The
//! Loup-Garou (docs/LORE.md §IV) is the first and the design anchor: two
//! decks joined by a ladder, zero-g Upstairs where the ship works, gravity
//! Downstairs where the crew lives.
//!
//! Grid units match the station generator (the client scales them the same
//! way), fore is +y.

use super::{Door, GeneratedLayout, Room, RoomKind};

/// One deck of a ship interior: a layout plus its gravity profile and the
/// grid-unit point where the inter-deck ladder stands. Ladder points are
/// vertically aligned across decks so climbing keeps your position.
#[derive(Debug, Clone)]
pub struct ShipDeck {
    pub name: &'static str,
    /// Zero-g deck: humans move slow (mag boots), robots move fast.
    pub zero_g: bool,
    pub layout: GeneratedLayout,
    /// Grid-unit position of the ladder between decks.
    pub ladder: (i32, i32),
}

/// A whole ship interior. `decks[0]` is where boarding puts you (the deck
/// with the airlock).
#[derive(Debug, Clone)]
pub struct ShipInterior {
    pub decks: Vec<ShipDeck>,
}

fn room(kind: RoomKind, x: i32, y: i32, width: i32, height: i32) -> Room {
    Room {
        kind,
        x,
        y,
        width,
        height,
    }
}

fn door(from: u32, to: u32, x: i32, y: i32) -> Door {
    Door { from, to, x, y }
}

/// Grid-unit position of the Loup-Garou's ladder, shared by both decks
/// (inside the spine corridor on each).
pub const LOUP_GAROU_LADDER: (i32, i32) = (4, 48);

/// The Loup-Garou's authored deck plan (docs/SHIPS.md §6).
///
/// Deck 0 — LOWER (artificial gravity), fore at +y:
/// bridge over a central spine corridor; scanner / med bay, engineering /
/// cryo, and the quarters flanking it; galley and the airlock aft.
///
/// Deck 1 — UPPER (zero-g): cockpit fore of the spine, and the tech bay —
/// processing floor plus the shuttle pad — taking up the aft.
pub fn loup_garou_interior() -> ShipInterior {
    // Lower deck. Room 0 is the spine so door indices read naturally.
    let lower = GeneratedLayout {
        rooms: vec![
            room(RoomKind::Corridor, 0, -16, 8, 72), // 0: spine, y -16..56
            room(RoomKind::Bridge, -12, 56, 32, 16), // 1: fore
            room(RoomKind::Scanner, -20, 40, 20, 12), // 2
            room(RoomKind::MedBay, 8, 40, 18, 12),   // 3
            room(RoomKind::Reactor, -22, 18, 22, 18), // 4: engineering
            room(RoomKind::Cryo, 8, 16, 24, 22),     // 5: 10 pods
            room(RoomKind::Quarters, -20, 2, 20, 12), // 6: shared berths
            room(RoomKind::Quarters, 8, 2, 18, 14),  // 7: officers' + guest
            room(RoomKind::Bar, -20, -16, 20, 14),   // 8: galley
            room(RoomKind::Hangar, 8, -16, 18, 14),  // 9: airlock
        ],
        doors: vec![
            door(0, 1, 4, 56),
            door(0, 2, 0, 46),
            door(0, 3, 8, 46),
            door(0, 4, 0, 27),
            door(0, 5, 8, 27),
            door(0, 6, 0, 8),
            door(0, 7, 8, 9),
            door(0, 8, 0, -9),
            door(0, 9, 8, -9),
        ],
    };

    // Upper deck: same spine axis so the ladder lines up.
    let upper = GeneratedLayout {
        rooms: vec![
            room(RoomKind::Corridor, 0, 8, 8, 48),     // 0: spine, y 8..56
            room(RoomKind::Cockpit, -12, 56, 32, 16),  // 1: fore, over bridge
            room(RoomKind::TechBay, -24, -24, 56, 32), // 2: processing + pad
        ],
        doors: vec![door(0, 1, 4, 56), door(0, 2, 4, 8)],
    };

    ShipInterior {
        decks: vec![
            ShipDeck {
                name: "LOWER DECK",
                zero_g: false,
                layout: lower,
                ladder: LOUP_GAROU_LADDER,
            },
            ShipDeck {
                name: "UPPER DECK",
                zero_g: true,
                layout: upper,
                ladder: LOUP_GAROU_LADDER,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn overlaps(a: &Room, b: &Room) -> bool {
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
    }

    fn contains(r: &Room, x: i32, y: i32) -> bool {
        x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
    }

    /// A door must sit on the shared boundary of the two rooms it joins —
    /// otherwise the walkability aperture opens into a wall.
    fn door_on_shared_edge(layout: &GeneratedLayout, d: &Door) -> bool {
        let a = &layout.rooms[d.from as usize];
        let b = &layout.rooms[d.to as usize];
        contains(a, d.x, d.y)
            && contains(b, d.x, d.y)
            && (a.x == b.x + b.width
                || b.x == a.x + a.width
                || a.y == b.y + b.height
                || b.y == a.y + a.height)
    }

    fn connected(layout: &GeneratedLayout) -> bool {
        let n = layout.rooms.len();
        let mut seen = HashSet::from([0usize]);
        let mut frontier = vec![0usize];
        while let Some(r) = frontier.pop() {
            for d in &layout.doors {
                let (a, b) = (d.from as usize, d.to as usize);
                let next = if a == r {
                    b
                } else if b == r {
                    a
                } else {
                    continue;
                };
                if seen.insert(next) {
                    frontier.push(next);
                }
            }
        }
        seen.len() == n
    }

    #[test]
    fn decks_have_no_overlapping_rooms() {
        for deck in loup_garou_interior().decks {
            let rooms = &deck.layout.rooms;
            for i in 0..rooms.len() {
                for j in i + 1..rooms.len() {
                    assert!(
                        !overlaps(&rooms[i], &rooms[j]),
                        "{}: rooms {i} and {j} overlap",
                        deck.name
                    );
                }
            }
        }
    }

    #[test]
    fn every_door_sits_on_a_shared_edge() {
        for deck in loup_garou_interior().decks {
            for d in &deck.layout.doors {
                assert!(
                    door_on_shared_edge(&deck.layout, d),
                    "{}: door {d:?} not on the shared edge",
                    deck.name
                );
            }
        }
    }

    #[test]
    fn every_deck_is_connected() {
        for deck in loup_garou_interior().decks {
            assert!(connected(&deck.layout), "{} is disconnected", deck.name);
        }
    }

    #[test]
    fn ladders_align_and_stand_inside_a_room_on_both_decks() {
        let ship = loup_garou_interior();
        let (lx, ly) = ship.decks[0].ladder;
        for deck in &ship.decks {
            assert_eq!(deck.ladder, (lx, ly), "{} ladder misaligned", deck.name);
            assert!(
                deck.layout
                    .rooms
                    .iter()
                    .any(|r| contains(r, deck.ladder.0, deck.ladder.1)),
                "{} ladder floats outside every room",
                deck.name
            );
        }
    }

    #[test]
    fn loup_garou_has_the_lore_rooms_on_the_right_decks() {
        let ship = loup_garou_interior();
        let kinds = |i: usize| -> Vec<RoomKind> {
            ship.decks[i].layout.rooms.iter().map(|r| r.kind).collect()
        };
        let lower = kinds(0);
        let upper = kinds(1);
        // Boarding deck: gravity, airlock aft, the living spaces.
        assert!(!ship.decks[0].zero_g);
        for kind in [
            RoomKind::Bridge,
            RoomKind::Scanner,
            RoomKind::MedBay,
            RoomKind::Reactor,
            RoomKind::Cryo,
            RoomKind::Quarters,
            RoomKind::Bar,
            RoomKind::Hangar,
        ] {
            assert!(lower.contains(&kind), "lower deck missing {kind:?}");
        }
        // Work deck: zero-g, cockpit fore, tech bay aft.
        assert!(ship.decks[1].zero_g);
        assert!(upper.contains(&RoomKind::Cockpit));
        assert!(upper.contains(&RoomKind::TechBay));
    }
}
