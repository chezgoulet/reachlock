//! Crew as data + onboard behaviour (spec §14 Mode 2; S08). Souls arrive in
//! S13; here a `CrewMember` is id/name/role/duty-room plus a live
//! `current_room` and an optional `order` the player can issue. The
//! `CrewRoster` resource persists in the save; the on-board sprites are
//! rebuilt each time you board (S06 `ModeScope` pattern). Ids are stable
//! strings ("boris", "tib", …) so S13 can attach personalities by id.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use reachlock_core::generator::{GeneratedLayout, RoomKind};

/// A crew member's job — maps to a duty room (S08: engineer→Reactor,
/// pilot→Bridge). S13 will layer personality/state on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrewRole {
    Pilot,
    Engineer,
    Navigator,
    Medic,
    Gunner,
}

/// One crew member. `id` is the stable handle S13 binds a soul to.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewMember {
    pub id: String,
    pub name: String,
    pub role: CrewRole,
    pub duty_room: RoomKind,
    /// Where the sprite currently is (driven by the shift cycle / orders).
    pub current_room: RoomKind,
    /// An order overrides the shift cycle until cleared (`None`).
    pub order: Option<RoomKind>,
}

/// The ship's crew. Persists in the save; the sprites don't.
#[derive(Resource, Default, Clone, Debug)]
#[allow(dead_code)]
pub struct CrewRoster {
    pub members: Vec<CrewMember>,
}

impl CrewRoster {
    /// The canonical starting crew: Boris the engineer, Tib the pilot,
    /// Ves the navigator. Stable ids so later sprints can find them.
    #[allow(dead_code)]
    pub fn default_crew() -> Self {
        Self {
            members: vec![
                CrewMember {
                    id: "boris".into(),
                    name: "Boris".into(),
                    role: CrewRole::Engineer,
                    duty_room: RoomKind::Reactor,
                    current_room: RoomKind::Reactor,
                    order: None,
                },
                CrewMember {
                    id: "tib".into(),
                    name: "Tib".into(),
                    role: CrewRole::Pilot,
                    duty_room: RoomKind::Bridge,
                    current_room: RoomKind::Bridge,
                    order: None,
                },
                CrewMember {
                    id: "ves".into(),
                    name: "Ves".into(),
                    role: CrewRole::Navigator,
                    duty_room: RoomKind::Bridge,
                    current_room: RoomKind::Bridge,
                    order: None,
                },
            ],
        }
    }

    /// Look up a member by id (used by the order system after an interaction).
    pub fn by_id(&self, id: &str) -> Option<&CrewMember> {
        self.members.iter().find(|m| m.id == id)
    }

    /// Mutable lookup by id.
    pub fn by_id_mut(&mut self, id: &str) -> Option<&mut CrewMember> {
        self.members.iter_mut().find(|m| m.id == id)
    }
}

/// Where a crew member should be when on/off shift. On shift they're at their
/// duty room; off shift they retire to quarters. Pure — no Bevy, unit-tested.
pub fn shift_room(duty: RoomKind, on_shift: bool) -> RoomKind {
    if on_shift {
        duty
    } else {
        RoomKind::Quarters
    }
}

/// Resolve the room a member occupies right now: an order wins over the shift
/// cycle. Pure — unit-tested.
pub fn resolve_room(m: &CrewMember, on_shift: bool) -> RoomKind {
    m.order.unwrap_or_else(|| shift_room(m.duty_room, on_shift))
}

/// The shift-cycle parity at time `t` given a `period` seconds per half-cycle.
/// `true` = on shift. Pure — unit-tested.
pub fn shift_parity(t: f32, period: f32) -> bool {
    if period <= 0.0 {
        return true;
    }
    let half = (t / period).floor() as i64;
    half % 2 == 0
}

/// Rooms the player can order a crew member to. Index → room, so number keys
/// map cleanly in the order panel. Drawn from the room kinds the generator
/// actually emits (no `Cargo`/`Galley` variants exist yet).
pub const ORDER_ROOMS: [RoomKind; 5] = [
    RoomKind::Quarters,
    RoomKind::Bridge,
    RoomKind::Reactor,
    RoomKind::Bar,
    RoomKind::Market,
];

/// Tag a crew sprite entity with its member id, so the shift system and the
/// order system can find the roster entry it represents.
#[derive(Component, Clone, Debug)]
pub struct CrewFigure(pub String);

/// A crew figure's live navigation: the room it's headed to and the door
/// waypoints left to walk. Rebuilt whenever the resolved room changes.
#[derive(Component, Default)]
pub struct CrewNav {
    pub target: Option<RoomKind>,
    pub path: Vec<Vec2>,
}

/// Seconds per shift half-cycle (duty ↔ quarters).
const SHIFT_PERIOD: f32 = 24.0;

/// Crew walking speed, world px per second (~4 tiles/s).
const CREW_SPEED: f32 = 64.0;

/// Room index containing the point, if any.
fn room_at(layout: &GeneratedLayout, p: Vec2) -> Option<usize> {
    layout.rooms.iter().position(|r| {
        p.x >= r.x as f32
            && p.x <= (r.x + r.width) as f32
            && p.y >= r.y as f32
            && p.y <= (r.y + r.height) as f32
    })
}

fn center_of(layout: &GeneratedLayout, index: usize) -> Vec2 {
    let r = &layout.rooms[index];
    Vec2::new((r.x + r.width / 2) as f32, (r.y + r.height / 2) as f32)
}

/// Door-honest route from `from` to the first room of `kind`: BFS over the
/// door graph, emitting each door crossing as a waypoint and the target room
/// center last. Crew walk corridors like the FTL crew they are — no lerping
/// through walls. Pure — unit-tested.
pub fn route(layout: &GeneratedLayout, from: Vec2, kind: RoomKind) -> Vec<Vec2> {
    let Some(start) = room_at(layout, from) else {
        return Vec::new();
    };
    let Some(goal) = layout.rooms.iter().position(|r| r.kind == kind) else {
        return Vec::new();
    };
    if start == goal {
        return vec![center_of(layout, goal)];
    }
    // BFS over rooms; `via[i]` = (previous room, the door used to enter i).
    let n = layout.rooms.len();
    let mut via: Vec<Option<(usize, Vec2)>> = vec![None; n];
    let mut queue = std::collections::VecDeque::from([start]);
    while let Some(room) = queue.pop_front() {
        if room == goal {
            break;
        }
        for d in &layout.doors {
            let (a, b) = (d.from as usize, d.to as usize);
            let next = if a == room {
                b
            } else if b == room {
                a
            } else {
                continue;
            };
            if next != start && via[next].is_none() {
                via[next] = Some((room, Vec2::new(d.x as f32, d.y as f32)));
                queue.push_back(next);
            }
        }
    }
    if via[goal].is_none() {
        return Vec::new(); // disconnected layout: stay put rather than clip
    }
    let mut doors = Vec::new();
    let mut room = goal;
    while room != start {
        let Some((prev, door)) = via[room] else {
            return Vec::new();
        };
        doors.push(door);
        room = prev;
    }
    doors.reverse();
    // Walk through each intermediate room's center between doors so the path
    // stays inside walkable floor (door → center → next door).
    let mut path = Vec::new();
    let mut room = start;
    for door in doors {
        if room != start {
            path.push(center_of(layout, room));
        }
        path.push(door);
        // Which room does this door lead to from `room`?
        room = layout
            .doors
            .iter()
            .find_map(|d| {
                let dp = Vec2::new(d.x as f32, d.y as f32);
                if dp != door {
                    return None;
                }
                let (a, b) = (d.from as usize, d.to as usize);
                if a == room {
                    Some(b)
                } else if b == room {
                    Some(a)
                } else {
                    None
                }
            })
            .unwrap_or(room);
    }
    path.push(center_of(layout, goal));
    path
}

/// Drive crew figures along door-honest routes on the shift cycle (orders
/// override). When the resolved room changes, a fresh route is computed;
/// figures then walk waypoint to waypoint at a steady pace.
#[allow(clippy::type_complexity)]
pub fn crew_shift_system(
    time: Res<Time>,
    interior: Res<crate::systems::interior::CurrentInterior>,
    roster: Res<CrewRoster>,
    mut elapsed: Local<f32>,
    mut figures: Query<(&CrewFigure, &mut CrewNav, &mut Transform)>,
) {
    *elapsed += time.delta_secs();
    let Some(layout) = &interior.layout else {
        return;
    };
    let on_shift = shift_parity(*elapsed, SHIFT_PERIOD);
    for (fig, mut nav, mut t) in &mut figures {
        let Some(m) = roster.by_id(&fig.0) else {
            continue;
        };
        let target = resolve_room(m, on_shift);
        let pos = t.translation.truncate();
        if nav.target != Some(target) {
            nav.target = Some(target);
            nav.path = route(layout, pos, target);
        }
        let Some(&next) = nav.path.first() else {
            continue;
        };
        let to = next - pos;
        let step = CREW_SPEED * time.delta_secs();
        if to.length() <= step.max(2.0) {
            t.translation.x = next.x;
            t.translation.y = next.y;
            nav.path.remove(0);
        } else {
            let d = to.normalize() * step;
            t.translation.x += d.x;
            t.translation.y += d.y;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member() -> CrewMember {
        CrewMember {
            id: "boris".into(),
            name: "Boris".into(),
            role: CrewRole::Engineer,
            duty_room: RoomKind::Reactor,
            current_room: RoomKind::Reactor,
            order: None,
        }
    }

    #[test]
    fn shift_toggles_duty_and_quarters() {
        assert_eq!(shift_room(RoomKind::Reactor, true), RoomKind::Reactor);
        assert_eq!(shift_room(RoomKind::Reactor, false), RoomKind::Quarters);
    }

    #[test]
    fn order_overrides_shift() {
        let mut m = member();
        m.order = Some(RoomKind::Bridge);
        // On shift he'd be at Reactor; the order pins him to the Bridge.
        assert_eq!(resolve_room(&m, true), RoomKind::Bridge);
        assert_eq!(resolve_room(&m, false), RoomKind::Bridge);
        // Clearing the order restores the shift cycle.
        m.order = None;
        assert_eq!(resolve_room(&m, false), RoomKind::Quarters);
    }

    #[test]
    fn route_walks_doors_not_walls() {
        use reachlock_core::generator::{Door, Room};
        // Hangar and Quarters both bud off the corridor: the only legal path
        // between them runs door → corridor → door.
        let layout = GeneratedLayout {
            rooms: vec![
                Room {
                    kind: RoomKind::Hangar,
                    x: 0,
                    y: 0,
                    width: 48,
                    height: 32,
                },
                Room {
                    kind: RoomKind::Corridor,
                    x: 0,
                    y: 32,
                    width: 128,
                    height: 16,
                },
                Room {
                    kind: RoomKind::Quarters,
                    x: 64,
                    y: 48,
                    width: 32,
                    height: 24,
                },
            ],
            doors: vec![
                Door {
                    from: 0,
                    to: 1,
                    x: 16,
                    y: 32,
                },
                Door {
                    from: 1,
                    to: 2,
                    x: 80,
                    y: 48,
                },
            ],
        };
        let path = route(&layout, Vec2::new(24.0, 16.0), RoomKind::Quarters);
        assert_eq!(
            path,
            vec![
                Vec2::new(16.0, 32.0), // hangar door
                Vec2::new(64.0, 40.0), // corridor center
                Vec2::new(80.0, 48.0), // quarters door
                Vec2::new(80.0, 60.0), // quarters center
            ]
        );
        // Already in the target room: path is just the room center.
        let stay = route(&layout, Vec2::new(24.0, 16.0), RoomKind::Hangar);
        assert_eq!(stay, vec![Vec2::new(24.0, 16.0)]);
    }

    #[test]
    fn parity_flips_each_period() {
        assert!(shift_parity(0.0, 10.0));
        assert!(shift_parity(9.9, 10.0));
        assert!(!shift_parity(10.0, 10.0));
        assert!(!shift_parity(19.9, 10.0));
        assert!(shift_parity(20.0, 10.0));
        // Degenerate period is safe.
        assert!(shift_parity(5.0, 0.0));
    }
}
