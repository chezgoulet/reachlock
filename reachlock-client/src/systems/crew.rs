//! Crew as data + onboard behaviour (spec §14 Mode 2; S08). Souls arrive in
//! S13; here a `CrewMember` is id/name/role/duty-room plus a live
//! `current_room` and an optional `order` the player can issue. The
//! `CrewRoster` resource persists in the save; the on-board sprites are
//! rebuilt each time you board (S06 `ModeScope` pattern). Ids are stable
//! strings ("boris", "tib", …) so S13 can attach personalities by id.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use reachlock_core::generator::RoomKind;

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

/// Seconds per shift half-cycle (duty ↔ quarters).
const SHIFT_PERIOD: f32 = 8.0;

/// Move crew sprites between their resolved rooms on the shift cycle (or to an
/// order's room, which overrides). Straight-line lerp — no A* yet, per brief.
/// Runs in interior modes; reads `CurrentInterior` for room centers.
#[allow(clippy::type_complexity)]
pub fn crew_shift_system(
    time: Res<Time>,
    interior: Res<crate::systems::interior::CurrentInterior>,
    roster: Res<CrewRoster>,
    mut elapsed: Local<f32>,
    mut figures: Query<(&CrewFigure, &mut Transform)>,
) {
    *elapsed += time.delta_secs();
    let Some(layout) = &interior.layout else {
        return;
    };
    let on_shift = shift_parity(*elapsed, SHIFT_PERIOD);
    for (fig, mut t) in &mut figures {
        let Some(m) = roster.by_id(&fig.0) else {
            continue;
        };
        let target = resolve_room(m, on_shift);
        let Some(center) = crate::systems::interior::room_center(layout, target) else {
            continue;
        };
        let cur = t.translation.truncate();
        let k = (time.delta_secs() * 1.5).min(1.0);
        let next = cur.lerp(center, k);
        t.translation.x = next.x;
        t.translation.y = next.y;
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
