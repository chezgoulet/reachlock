//! Crew as data + onboard behaviour (spec §14 Mode 2; S08). Souls arrive in
//! S13; here a `CrewMember` is id/name/role/duty-room plus a live
//! `current_room` and an optional `order` the player can issue. The
//! `CrewRoster` resource persists in the save; the on-board sprites are
//! rebuilt each time you board (S06 `ModeScope` pattern). Ids are stable
//! strings ("boris", "tove", …) so S13 can attach personalities by id.

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
    /// The room this member is actually in right now. Kept live: on-screen
    /// figures write it back when they arrive; off-screen members advance
    /// it through the abstract walker. The jump-cryo check and the fire
    /// loop read it, so it MUST be true (S16B).
    pub current_room: RoomKind,
    /// Which deck this member is on (0 = gravity, 1 = zero-g). Kept live
    /// alongside `current_room`.
    #[serde(default)]
    pub deck: usize,
    /// An order overrides the shift cycle until cleared (`None`).
    pub order: Option<RoomKind>,
    /// Seconds until the member's next off-screen movement leg completes.
    /// Transient — not part of the save.
    #[serde(skip, default)]
    pub offscreen_eta: f32,
}

/// The ship's crew. Persists in the save; the sprites don't.
#[derive(Resource, Default, Clone, Debug)]
pub struct CrewRoster {
    pub members: Vec<CrewMember>,
}

impl CrewRoster {
    /// The canonical starting crew: the Loup-Garou's complement minus Tib,
    /// who is the player avatar (docs/LORE.md §V). Stable ids so S13 can
    /// attach souls, and so `pixel::crew_look` finds each member's look.
    /// Duty rooms map lore spaces onto the generated hull: Reactor =
    /// engineering (Tove), Bridge = cockpit (Prudence, Risc at ops),
    /// Quarters = med bay side (Doc Keene), Bar = galley (Bardo plays
    /// there), Hangar = EVA prep (Boris).
    pub fn default_crew() -> Self {
        let member = |id: &str, name: &str, role, duty_room| CrewMember {
            id: id.into(),
            name: name.into(),
            role,
            duty_room,
            current_room: duty_room,
            deck: deck_of(duty_room),
            order: None,
            offscreen_eta: 0.0,
        };
        Self {
            members: vec![
                member("tove", "Tove", CrewRole::Engineer, RoomKind::Reactor),
                member("prudence", "Prudence", CrewRole::Pilot, RoomKind::Cockpit),
                member("risc", "Risc", CrewRole::Gunner, RoomKind::Bridge),
                member("keene", "Doc Keene", CrewRole::Medic, RoomKind::MedBay),
                member("bardo", "Bardo", CrewRole::Navigator, RoomKind::Bar),
                member("boris", "Boris", CrewRole::Engineer, RoomKind::TechBay),
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

/// Rooms the player can order a crew member to. Index → digit key (1–9,
/// then 0) in the order panel. Covers every room of the authored ship
/// (S16B closes the S09c watch-list gap); a room absent from the current
/// hull simply routes nowhere.
pub const ORDER_ROOMS: [RoomKind; 10] = [
    RoomKind::Quarters,
    RoomKind::Bridge,
    RoomKind::Reactor,
    RoomKind::Bar,
    RoomKind::Market,
    RoomKind::Cockpit,
    RoomKind::TechBay,
    RoomKind::Scanner,
    RoomKind::MedBay,
    RoomKind::Cryo,
];

/// Which deck of the Loup-Garou a room kind lives on (0 = gravity deck,
/// 1 = zero-g deck). Rooms absent from the authored ship default to the
/// gravity deck. Pure — unit-tested.
pub fn deck_of(kind: RoomKind) -> usize {
    let ship = reachlock_core::generator::ship::loup_garou_interior();
    for (index, deck) in ship.decks.iter().enumerate() {
        if deck.layout.rooms.iter().any(|r| r.kind == kind) {
            return index;
        }
    }
    0
}

/// Whether a deck index runs zero-g on the authored ship.
pub fn deck_zero_g(index: usize) -> bool {
    let ship = reachlock_core::generator::ship::loup_garou_interior();
    ship.decks.get(index).map(|d| d.zero_g).unwrap_or(false)
}

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

/// Crew walking speed, world px per second (~4 tiles/s), before the
/// body-kind × gravity factor.
const CREW_SPEED: f32 = 64.0;

/// Movement speed factor by body kind × deck gravity (docs/SHIPS.md §5):
/// robots are built heavy — fastest movers in zero-g, slow under gravity;
/// humans need mag boots in zero-g; androids are baseline everywhere.
/// Pure — unit-tested; shared by the avatar and the crew.
pub fn move_factor(body: crate::pixel::BodyKind, zero_g: bool) -> f32 {
    use crate::pixel::BodyKind;
    match (body, zero_g) {
        (BodyKind::Robot, true) => 1.6,
        (BodyKind::Robot, false) => 0.5,
        (BodyKind::Human, true) => 0.7,
        _ => 1.0,
    }
}

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

/// Door-honest route from `from` to the first room of `kind`. Pure —
/// unit-tested. See [`route_indexed`] for the BFS itself.
pub fn route(layout: &GeneratedLayout, from: Vec2, kind: RoomKind) -> Vec<Vec2> {
    let Some(start) = room_at(layout, from) else {
        return Vec::new();
    };
    let Some(goal) = layout.rooms.iter().position(|r| r.kind == kind) else {
        return Vec::new();
    };
    route_indexed(layout, start, goal)
}

/// Door-honest route to an exact point (S16B: the inter-deck ladder is a
/// position, not a room kind — and room kinds can repeat, e.g. Quarters ×2,
/// so the containing room is found by index). The final waypoint is the
/// point itself.
pub fn route_to_point(layout: &GeneratedLayout, from: Vec2, point: Vec2) -> Vec<Vec2> {
    let Some(start) = room_at(layout, from) else {
        return Vec::new();
    };
    let Some(goal) = room_at(layout, point) else {
        return Vec::new();
    };
    let mut path = route_indexed(layout, start, goal);
    if path.is_empty() {
        return path;
    }
    // Swap the goal-room center for the exact point.
    path.pop();
    path.push(point);
    path
}

/// BFS over the door graph from room index to room index, emitting each door
/// crossing as a waypoint and the target room center last. Crew walk
/// corridors like the FTL crew they are — no lerping through walls.
fn route_indexed(layout: &GeneratedLayout, start: usize, goal: usize) -> Vec<Vec2> {
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

/// Seconds one abstract off-screen movement leg takes at baseline speed —
/// roughly an on-screen walk across half a deck, so the jump clock is fair
/// in both directions (the S16B gotcha). Scaled by body kind × gravity.
const OFFSCREEN_LEG_SECS: f32 = 8.0;

/// Drive the crew on the shift cycle (orders override), everywhere:
///
/// - Members on the ACTIVE deck walk visibly along door-honest routes. A
///   member whose target is on the other deck routes to the ladder and
///   climbs (their sprite is despawned by `interior::sync_crew_deck_presence`
///   once `deck` flips). Arriving anywhere writes `current_room` back — the
///   jump-cryo check and the fire loop read it, so it must be live.
/// - Members WITHOUT a sprite (other deck, or no interior scene at all —
///   e.g. the player is at the helm) move abstractly: one timed leg per
///   ladder climb or room change, at speeds matching their body kind and
///   deck gravity. Crew keep living their lives when you aren't looking.
#[allow(clippy::type_complexity)]
pub fn crew_shift_system(
    time: Res<Time>,
    interior: Res<crate::systems::interior::CurrentInterior>,
    active_deck: Res<crate::systems::interior::ActiveDeck>,
    mode: Option<Res<State<crate::states::GameMode>>>,
    mut roster: ResMut<CrewRoster>,
    mut elapsed: Local<f32>,
    mut figures: Query<(&CrewFigure, &mut CrewNav, &mut Transform)>,
) {
    let dt = time.delta_secs();
    *elapsed += dt;
    let on_shift = shift_parity(*elapsed, SHIFT_PERIOD);
    let on_board =
        mode.is_some_and(|m| **m == crate::states::GameMode::OnBoard) && interior.layout.is_some();

    // ── on-screen: visible walking on the active deck ──
    let mut sprited: Vec<String> = Vec::new();
    if on_board {
        let layout = interior.layout.as_ref().expect("checked above");
        for (fig, mut nav, mut t) in &mut figures {
            sprited.push(fig.0.clone());
            let Some(m) = roster.by_id_mut(&fig.0) else {
                continue;
            };
            if m.deck != active_deck.index {
                continue; // climbed away; the presence sync will despawn it
            }
            let target = resolve_room(m, on_shift);
            let cross_deck = deck_of(target) != m.deck;
            let pos = t.translation.truncate();
            if nav.target != Some(target) {
                nav.target = Some(target);
                nav.path = if cross_deck {
                    // The way to the other deck is the ladder.
                    match interior.ladder {
                        Some(ladder) => route_to_point(layout, pos, ladder),
                        None => Vec::new(),
                    }
                } else {
                    route(layout, pos, target)
                };
            }
            let Some(&next) = nav.path.first() else {
                continue;
            };
            let to = next - pos;
            // Boris flies across the zero-g deck and trudges under gravity;
            // humans are the reverse (docs/SHIPS.md §5).
            let factor = move_factor(crate::pixel::crew_look(&fig.0).body, interior.zero_g);
            let step = CREW_SPEED * factor * dt;
            if to.length() <= step.max(2.0) {
                t.translation.x = next.x;
                t.translation.y = next.y;
                nav.path.remove(0);
                if nav.path.is_empty() {
                    if cross_deck {
                        // At the ladder: climb. One abstract leg covers the
                        // far side; the presence sync removes the sprite.
                        m.deck = deck_of(target);
                        m.offscreen_eta = OFFSCREEN_LEG_SECS
                            / move_factor(crate::pixel::crew_look(&m.id).body, deck_zero_g(m.deck));
                    } else {
                        m.current_room = target;
                    }
                }
            } else {
                let d = to.normalize() * step;
                t.translation.x += d.x;
                t.translation.y += d.y;
            }
        }
    }

    // ── off-screen: abstract legs for everyone without a sprite ──
    for m in roster.members.iter_mut() {
        if sprited.contains(&m.id) {
            continue;
        }
        let target = resolve_room(m, on_shift);
        if m.current_room == target && deck_of(target) == m.deck {
            m.offscreen_eta = 0.0;
            continue;
        }
        if m.offscreen_eta <= 0.0 {
            m.offscreen_eta = OFFSCREEN_LEG_SECS
                / move_factor(crate::pixel::crew_look(&m.id).body, deck_zero_g(m.deck));
        }
        m.offscreen_eta -= dt;
        if m.offscreen_eta <= 0.0 {
            if deck_of(target) != m.deck {
                m.deck = deck_of(target); // climbed the ladder, unseen
            } else {
                m.current_room = target; // walked into the room, unseen
            }
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
            deck: deck_of(RoomKind::Reactor),
            order: None,
            offscreen_eta: 0.0,
        }
    }

    #[test]
    fn deck_of_matches_the_authored_ship() {
        // Lower/gravity deck (docs/SHIPS.md §6).
        for kind in [
            RoomKind::Bridge,
            RoomKind::Reactor,
            RoomKind::MedBay,
            RoomKind::Cryo,
            RoomKind::Quarters,
            RoomKind::Bar,
        ] {
            assert_eq!(deck_of(kind), 0, "{kind:?} is Downstairs");
            assert!(!deck_zero_g(deck_of(kind)));
        }
        // Upper/zero-g deck.
        for kind in [RoomKind::Cockpit, RoomKind::TechBay] {
            assert_eq!(deck_of(kind), 1, "{kind:?} is Upstairs");
            assert!(deck_zero_g(deck_of(kind)));
        }
    }

    #[test]
    fn order_rooms_cover_the_whole_ship() {
        // The S09c watch-list gap: every authored ship room is orderable.
        for kind in [
            RoomKind::Cockpit,
            RoomKind::TechBay,
            RoomKind::Scanner,
            RoomKind::MedBay,
            RoomKind::Cryo,
        ] {
            assert!(ORDER_ROOMS.contains(&kind), "{kind:?} missing");
        }
        assert!(ORDER_ROOMS.len() <= 10, "must fit digit keys 1-9,0");
    }

    #[test]
    fn move_factor_matches_the_gravity_table() {
        use crate::pixel::BodyKind;
        // Robots: fastest in zero-g, slow under gravity.
        assert!(move_factor(BodyKind::Robot, true) > 1.0);
        assert!(move_factor(BodyKind::Robot, false) < 1.0);
        // Humans: mag-boot slow in zero-g, baseline under gravity.
        assert!(move_factor(BodyKind::Human, true) < 1.0);
        assert_eq!(move_factor(BodyKind::Human, false), 1.0);
        // Androids: baseline everywhere.
        assert_eq!(move_factor(BodyKind::Android, true), 1.0);
        assert_eq!(move_factor(BodyKind::Android, false), 1.0);
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

        // route_to_point ends on the exact point (S16B: the ladder is a
        // position, not a room kind).
        let ladder = Vec2::new(70.0, 60.0); // inside Quarters
        let to_point = route_to_point(&layout, Vec2::new(24.0, 16.0), ladder);
        assert_eq!(to_point.last(), Some(&ladder));
        assert_eq!(to_point.len(), 4, "same door-honest path, point-terminated");
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
