//! Station generation (spec §5): exterior mesh + interior room layout.
//! Layout is a spine corridor with rooms budding off both sides —
//! guaranteed connected by construction, no reachability solver needed.

use serde::{Deserialize, Serialize};

use super::hull::{generate_hull_class, HullClass};
use super::{Door, GeneratedLayout, GeneratedMesh, Room, RoomKind};
use crate::util::rng::SeededRng;

/// Station flavor: which rooms can bud off the spine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationKind {
    Trade,
    Mining,
    Military,
}

impl StationKind {
    fn room_pool(self) -> &'static [RoomKind] {
        match self {
            StationKind::Trade => &[
                RoomKind::Market,
                RoomKind::Bar,
                RoomKind::Quarters,
                RoomKind::Shipyard,
            ],
            StationKind::Mining => &[
                RoomKind::Reactor,
                RoomKind::Quarters,
                RoomKind::Market,
                RoomKind::Bar,
            ],
            StationKind::Military => &[
                RoomKind::Bridge,
                RoomKind::Quarters,
                RoomKind::Reactor,
                RoomKind::Shipyard,
            ],
        }
    }
}

pub struct GeneratedStation {
    pub exterior: GeneratedMesh,
    pub layout: GeneratedLayout,
}

/// `size` scales the room count: 0 = outpost (3 rooms), each step adds ~2.
pub fn generate_station(seed: u64, kind: StationKind, size: u32) -> GeneratedStation {
    let exterior = generate_hull_class(seed, HullClass::Station);
    let mut rng = SeededRng::new(seed ^ 0x57A7_1014); // distinct stream from exterior

    const GRID: i32 = 8; // grid unit = 8 world units when rendered
    let room_count = 3 + 2 * size as usize + rng.next_below(2) as usize;
    let pool = kind.room_pool();

    let mut rooms = Vec::with_capacity(room_count + 2);
    let mut doors = Vec::new();

    // Room 0: the hangar — every station starts at its dock.
    rooms.push(Room {
        kind: RoomKind::Hangar,
        x: 0,
        y: 0,
        width: 6 * GRID,
        height: 4 * GRID,
    });

    // Room 1: the spine corridor, long and thin, north of the hangar.
    let spine_len = (room_count as i32 + 1) * 4 * GRID;
    rooms.push(Room {
        kind: RoomKind::Corridor,
        x: 0,
        y: 4 * GRID,
        width: spine_len,
        height: 2 * GRID,
    });
    doors.push(Door {
        from: 0,
        to: 1,
        x: 2 * GRID,
        y: 4 * GRID,
    });

    // Bud rooms alternately above and below the spine.
    for i in 0..room_count {
        let kind = pool[rng.next_below(pool.len() as u64) as usize];
        let w = (3 + rng.next_below(3) as i32) * GRID;
        let h = (2 + rng.next_below(3) as i32) * GRID;
        let along = (i as i32 + 1) * 4 * GRID;
        let above = i % 2 == 0;
        let y = if above { 6 * GRID } else { 4 * GRID - h };
        rooms.push(Room {
            kind,
            x: along,
            y,
            width: w,
            height: h,
        });
        let room_index = (rooms.len() - 1) as u32;
        doors.push(Door {
            from: 1,
            to: room_index,
            x: along + w / 2,
            y: if above { 6 * GRID } else { 4 * GRID },
        });
    }

    GeneratedStation {
        exterior,
        layout: GeneratedLayout { rooms, doors },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_station(9, StationKind::Trade, 2);
        let b = generate_station(9, StationKind::Trade, 2);
        assert_eq!(a.layout, b.layout);
        assert_eq!(a.exterior, b.exterior);
    }

    #[test]
    fn every_room_reachable_from_hangar() {
        let station = generate_station(1234, StationKind::Mining, 3);
        let n = station.layout.rooms.len();
        let mut reached = vec![false; n];
        reached[0] = true;
        // Doors form a tree rooted at the spine; fixpoint-iterate.
        for _ in 0..n {
            for door in &station.layout.doors {
                let (f, t) = (door.from as usize, door.to as usize);
                if reached[f] || reached[t] {
                    reached[f] = true;
                    reached[t] = true;
                }
            }
        }
        assert!(reached.iter().all(|&r| r), "unreachable room in layout");
    }

    #[test]
    fn size_scales_room_count() {
        let small = generate_station(5, StationKind::Trade, 0);
        let large = generate_station(5, StationKind::Trade, 3);
        assert!(large.layout.rooms.len() > small.layout.rooms.len());
    }

    #[test]
    fn doors_reference_valid_rooms() {
        let station = generate_station(77, StationKind::Military, 2);
        let n = station.layout.rooms.len() as u32;
        for door in &station.layout.doors {
            assert!(door.from < n && door.to < n);
        }
    }
}
