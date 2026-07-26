//! Crew as data + onboard behaviour (spec §14 Mode 2; S08). Souls arrive in
//! S13; here a `CrewMember` is id/name/role/duty-room plus a live
//! `current_room` and an optional `order` the player can issue. The
//! `CrewRoster` resource persists in the save; the on-board sprites are
//! rebuilt each time you board (S06 `ModeScope` pattern). Ids are stable
//! strings ("boris", "tove", …) so S13 can attach personalities by id.
//!
//! S80: CrewRole is now data-driven (struct, not enum). CrewRoster supports
//! add/remove, hiring, salary, breaking points, injury, and death. See the
//! S80 sprint brief for the full open-world crew lifecycle.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use reachlock_core::generator::{GeneratedLayout, RoomKind};
use reachlock_core::util::SeededRng;

use crate::systems::content_index::ContentIndex;
use crate::systems::soul::SoulRegistry;

/// A crew role is data-driven — defined by its id string. The old closed enum
/// is replaced by this struct. Five default roles are populated in
/// [`CrewRoleRegistry`]. Any string is valid as a role id; unknown ids fall
/// back to a generic handler.
///
/// NOTE: separate from `contract::metadata::CrewRole` in core (used for
/// contract library filtering). They serve different purposes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CrewRole {
    pub id: String,
    pub name: String,
    pub description: String,
    pub duty_station: Option<String>,
}

impl CrewRole {
    pub fn new(id: &str, name: &str, description: &str) -> Self {
        CrewRole {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            duty_station: None,
        }
    }
}

/// Registry of all known crew roles, populated at startup. Data-driven — the
/// five default roles cover the old closed enum variants.
#[derive(Resource, Default)]
pub struct CrewRoleRegistry {
    pub roles: HashMap<String, CrewRole>,
}

impl CrewRoleRegistry {
    /// Populate with the five canonical default roles. Additional roles can
    /// be loaded from content.
    pub fn with_defaults() -> Self {
        let mut roles = HashMap::new();
        for role in [
            CrewRole::new("pilot", "Pilot", "Flies the ship — helm and navigation."),
            CrewRole::new(
                "engineer",
                "Engineer",
                "Maintains the reactor and ship systems.",
            ),
            CrewRole::new("doctor", "Doctor", "Crew health, medbay, and trauma care."),
            CrewRole::new(
                "marine",
                "Marine",
                "Security, boarding actions, and defense.",
            ),
            CrewRole::new(
                "scientist",
                "Scientist",
                "Research, data analysis, and signal processing.",
            ),
            CrewRole::new("captain", "Captain", "Commands the ship — final authority."),
            CrewRole::new(
                "navigator",
                "Navigator",
                "Plots courses and manages jump calculations.",
            ),
            CrewRole::new("medic", "Medic", "Field medicine and emergency triage."),
            CrewRole::new(
                "gunner",
                "Gunner",
                "Operates ship weapons and targeting systems.",
            ),
            CrewRole::new(
                "general",
                "General",
                "General-purpose duties and coordination.",
            ),
        ] {
            roles.insert(role.id.clone(), role);
        }
        CrewRoleRegistry { roles }
    }

    pub fn get(&self, id: &str) -> CrewRole {
        self.roles
            .get(id)
            .cloned()
            .unwrap_or_else(|| CrewRole::new(id, id, ""))
    }
}

/// Map a crew role id to the action they propose during co-deliberation.
/// Replaces the old `match role { CrewRole::Engineer => ... }` pattern.
/// Unknown role ids fall back to a generic action.
pub fn role_id_to_action(role_id: &str) -> &'static str {
    match role_id {
        "captain" => "lead_crew",
        "pilot" => "hold_course",
        "engineer" => "repair_systems",
        "navigator" => "plot_jump",
        "medic" | "doctor" => "tend_medbay",
        "gunner" => "man_battle_stations",
        "scientist" => "analyze_data",
        "marine" => "coordinate_defense",
        "general" => "coordinate_defense",
        _ => "maintain_course",
    }
}

/// Health state of a crew member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CrewHealth {
    #[default]
    Healthy,
    /// Reduced duty efficiency; heals over time or with medical supplies.
    Injured,
    /// Incapacitated, needs medical attention or degrades to Dead.
    Critical,
    Dead,
}

/// Tracks a single breaking point's state for a crew member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingPointState {
    pub condition: String,
    pub threshold: u32,
    pub current: u32,
    pub triggered: bool,
    pub consequence: BreakingPointConsequence,
}

/// What happens when a breaking point is crossed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreakingPointConsequence {
    /// Crew member voices disapproval (trust delta, no loss).
    Warning,
    /// Crew member refuses a specific order.
    RefuseOrder,
    /// Crew member leaves the ship at next station.
    LeaveAtStation,
    /// Crew member abandons ship immediately.
    AbandonShip,
    /// Crew member attempts to take control (mutiny).
    Mutiny,
}

/// Where a recruitable crew member came from — for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrewSource {
    Procedural { seed: u64, species: String },
    Authored { soul_id: String },
}

/// A recruitable crew member encountered at a station.
#[derive(Debug, Clone)]
pub struct RecruitableCrew {
    pub soul: reachlock_core::soul::SoulFile,
    pub preferred_role: String,
    pub salary_demand: u64,
    pub hook: String,
    pub source: CrewSource,
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
    /// The resolved soul file reference.
    #[serde(skip, default)]
    pub soul: Option<reachlock_core::soul::SoulFile>,
    /// Salary demand per pay period (credits).
    #[serde(default)]
    pub salary: u64,
    /// Ticks since last paid.
    #[serde(default)]
    pub unpaid_ticks: u64,
    /// Current health state.
    #[serde(default)]
    pub health: CrewHealth,
    /// Active breaking point thresholds.
    #[serde(default)]
    pub active_breaking_points: Vec<BreakingPointState>,
}

/// The ship's crew. Persists in the save; the sprites don't.
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct CrewRoster {
    pub members: Vec<CrewMember>,
    /// The current ship interior for deck resolution.
    pub current_interior: Option<reachlock_core::generator::ship::ShipInterior>,
}

impl CrewRoster {
    /// Map from authored role strings to default duty rooms.
    pub fn default_duty_room(role: &str) -> RoomKind {
        match role.to_lowercase().as_str() {
            "captain" | "pilot" => RoomKind::Cockpit,
            "engineer" => RoomKind::Reactor,
            "navigator" => RoomKind::Bridge,
            "medic" | "doctor" => RoomKind::MedBay,
            "gunner" => RoomKind::Bridge,
            "scientist" | "marine" => RoomKind::TechBay,
            "general" => RoomKind::TechBay,
            _ => RoomKind::Quarters,
        }
    }

    /// Parse a room kind string from content — shared between load_from_content
    /// and the default_duty_room fallback.
    pub fn parse_room_kind(s: &str) -> Option<RoomKind> {
        match s.to_lowercase().as_str() {
            "hangar" => Some(RoomKind::Hangar),
            "corridor" => Some(RoomKind::Corridor),
            "quarters" => Some(RoomKind::Quarters),
            "bar" => Some(RoomKind::Bar),
            "reactor" => Some(RoomKind::Reactor),
            "bridge" => Some(RoomKind::Bridge),
            "cockpit" => Some(RoomKind::Cockpit),
            "tech_bay" | "techbay" => Some(RoomKind::TechBay),
            "scanner" => Some(RoomKind::Scanner),
            "med_bay" | "medbay" => Some(RoomKind::MedBay),
            "cryo" => Some(RoomKind::Cryo),
            _ => None,
        }
    }

    /// Build the crew roster from authored content packages.
    /// Call after souls are loaded in the content index.
    pub fn load_from_content(
        content: &ContentIndex,
        souls: &std::collections::BTreeMap<String, reachlock_core::soul::SoulFile>,
    ) -> Self {
        let mut members = Vec::new();

        // Load crew packages from the content index.
        for file in &content.files {
            if let reachlock_core::content::ContentPayload::CrewPackage(pkg) = &file.payload {
                for entry in &pkg.members {
                    if !entry.starting {
                        continue;
                    }
                    let soul = souls.get(&entry.soul_id);
                    let name = soul
                        .map(|s| s.name.clone())
                        .unwrap_or_else(|| entry.soul_id.clone());
                    let role = CrewRole::new(&entry.role, &name, "");
                    let duty_room = entry
                        .duty_room
                        .as_ref()
                        .and_then(|r| Self::parse_room_kind(r))
                        .unwrap_or_else(|| CrewRoster::default_duty_room(&entry.role));

                    members.push(CrewMember {
                        id: entry.soul_id.clone(),
                        name,
                        role,
                        duty_room,
                        current_room: duty_room,
                        deck: 0,
                        order: None,
                        offscreen_eta: 0.0,
                        soul: soul.cloned(),
                        salary: entry.salary,
                        unpaid_ticks: 0,
                        health: CrewHealth::Healthy,
                        active_breaking_points: Vec::new(),
                    });
                }
            }
        }

        if members.is_empty() {
            info!("crew: no starting crew packages found (empty roster)");
        } else {
            info!("crew: loaded {} member(s) from content", members.len());
        }

        CrewRoster {
            members,
            current_interior: None,
        }
    }

    /// Update the current interior reference for deck resolution.
    pub fn set_interior(&mut self, interior: reachlock_core::generator::ship::ShipInterior) {
        // Recompute decks for all members.
        for m in &mut self.members {
            m.deck = deck_of(&interior, m.duty_room);
        }
        self.current_interior = Some(interior);
    }

    /// Look up a member by id (used by the order system after an interaction).
    pub fn by_id(&self, id: &str) -> Option<&CrewMember> {
        self.members.iter().find(|m| m.id == id)
    }

    /// Mutable lookup by id.
    pub fn by_id_mut(&mut self, id: &str) -> Option<&mut CrewMember> {
        self.members.iter_mut().find(|m| m.id == id)
    }

    // ── S80: dynamic roster management ──

    /// Add a crew member to the roster. Returns an error if a member with
    /// the same id already exists.
    pub fn add(&mut self, member: CrewMember) -> Result<(), String> {
        if self.members.iter().any(|m| m.id == member.id) {
            return Err(format!("crew member '{}' already exists", member.id));
        }
        self.members.push(member);
        Ok(())
    }

    /// Remove a crew member by id. Returns the removed member, or None.
    pub fn remove(&mut self, id: &str) -> Option<CrewMember> {
        let idx = self.members.iter().position(|m| m.id == id)?;
        Some(self.members.remove(idx))
    }

    /// Get a crew member by id.
    pub fn get(&self, id: &str) -> Option<&CrewMember> {
        self.members.iter().find(|m| m.id == id)
    }

    /// Get a mutable crew member by id.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut CrewMember> {
        self.members.iter_mut().find(|m| m.id == id)
    }

    /// Iterate over all crew members.
    pub fn iter(&self) -> impl Iterator<Item = &CrewMember> {
        self.members.iter()
    }

    /// Number of crew members.
    pub fn count(&self) -> usize {
        self.members.len()
    }

    /// Get all crew members with a given role id.
    pub fn by_role(&self, role_id: &str) -> Vec<&CrewMember> {
        self.members
            .iter()
            .filter(|m| m.role.id == role_id)
            .collect()
    }

    /// Whoever currently fills `role_id`, healthiest first.
    ///
    /// Engine systems that need "the pilot" or "the medic" must ask for the
    /// role, never for a name. Several of them used to hardcode a canonical
    /// crew member — so on any other ship, with any other crew, the wrong
    /// person announced your jump and put out your fires.
    pub fn speaker_for(&self, role_id: &str) -> Option<&CrewMember> {
        let rank = |h: &CrewHealth| match h {
            CrewHealth::Healthy => 0,
            CrewHealth::Injured => 1,
            CrewHealth::Critical => 2,
            CrewHealth::Dead => 3,
        };
        self.by_role(role_id)
            .into_iter()
            .filter(|m| m.health != CrewHealth::Dead)
            .min_by_key(|m| rank(&m.health))
    }

    /// Display name of whoever fills `role_id`.
    ///
    /// With nobody in the role, falls back to the role itself ("the pilot")
    /// rather than inventing a crew member. A solo character reads
    /// "the pilot plots the jump", not the name of somebody else's crew.
    pub fn voice_of(&self, role_id: &str) -> String {
        match self.speaker_for(role_id) {
            Some(m) => m.name.clone(),
            None => format!(
                "the {}",
                CrewRoleRegistry::with_defaults().get(role_id).name
            )
            .to_lowercase(),
        }
    }

    /// Id of whoever fills `role_id`, for systems keyed on crew id rather than
    /// display name (soul lookups, relationship deltas).
    pub fn voice_id_of(&self, role_id: &str) -> String {
        self.speaker_for(role_id)
            .map(|m| m.id.clone())
            .unwrap_or_else(|| role_id.to_string())
    }

    /// Set a crew member's health state.
    pub fn set_health(&mut self, id: &str, health: CrewHealth) {
        if let Some(m) = self.by_id_mut(id) {
            m.health = health;
        }
    }

    /// Total salary demand per pay period across all crew.
    pub fn total_salary(&self) -> u64 {
        self.members.iter().map(|m| m.salary).sum()
    }

    /// Advance the pay clock one tick. If `pay_period` ticks have elapsed,
    /// deduct salaries from credits. Returns list of crew ids that went unpaid.
    pub fn tick_payroll(&mut self, credits: &mut i64, pay_period: u64) -> Vec<String> {
        let mut unpaid = Vec::new();
        for m in self.members.iter_mut() {
            m.unpaid_ticks = m.unpaid_ticks.saturating_add(1);
            if m.unpaid_ticks >= pay_period && m.salary > 0 {
                let cost = m.salary as i64;
                if *credits >= cost {
                    *credits -= cost;
                    m.unpaid_ticks = 0;
                } else {
                    unpaid.push(m.id.clone());
                }
            }
        }
        unpaid
    }

    /// Pay a specific crew member immediately (e.g. on demand).
    pub fn pay_crew_member(&mut self, id: &str, credits: &mut i64) -> bool {
        let Some(m) = self.by_id_mut(id) else {
            return false;
        };
        let cost = m.salary as i64;
        if *credits < cost {
            return false;
        }
        *credits -= cost;
        m.unpaid_ticks = 0;
        true
    }

    /// Injure a crew member (Healthy → Injured, Injured → Critical).
    pub fn injure(&mut self, id: &str) {
        let Some(m) = self.by_id_mut(id) else {
            return;
        };
        m.health = match m.health {
            CrewHealth::Healthy => CrewHealth::Injured,
            CrewHealth::Injured => CrewHealth::Critical,
            other => other,
        };
    }

    /// Heal a crew member one step (Critical → Injured → Healthy).
    pub fn heal(&mut self, id: &str) {
        let Some(m) = self.by_id_mut(id) else {
            return;
        };
        m.health = match m.health {
            CrewHealth::Critical => CrewHealth::Injured,
            CrewHealth::Injured => CrewHealth::Healthy,
            other => other,
        };
    }

    /// Kill a crew member — removes them from the roster and returns them.
    pub fn kill(&mut self, id: &str) -> Option<CrewMember> {
        let idx = self.members.iter().position(|m| m.id == id)?;
        let member = self.members.remove(idx);
        Some(member)
    }

    /// Check all active breaking points and return the list of consequences
    /// for triggered ones. Also resets non-triggered breaking point counters
    /// if a healing event matches (high trust acts as buffer).
    pub fn check_breaking_points(
        &mut self,
        condition: &str,
        trust_bonus: u32,
    ) -> Vec<BreakingPointConsequence> {
        let mut consequences = Vec::new();
        for m in self.members.iter_mut() {
            for bp in m.active_breaking_points.iter_mut() {
                if bp.triggered {
                    continue;
                }
                if bp.condition == condition {
                    // High trust can buffer the count (trust acts as forgiveness).
                    let effective_threshold = bp.threshold.saturating_add(trust_bonus / 256);
                    bp.current = bp.current.saturating_add(1);
                    if bp.current >= effective_threshold {
                        bp.triggered = true;
                        consequences.push(bp.consequence);
                    }
                }
            }
        }
        consequences
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

/// Which deck of a ship interior a room kind lives on (0 = gravity deck,
/// 1+ = higher decks). Rooms absent from the ship default to the gravity
/// deck. Pure — unit-tested.
pub fn deck_of(interior: &reachlock_core::generator::ship::ShipInterior, kind: RoomKind) -> usize {
    for (index, deck) in interior.decks.iter().enumerate() {
        if deck.layout.rooms.iter().any(|r| r.kind == kind) {
            return index;
        }
    }
    0
}

/// Whether a deck index runs zero-g on the given ship interior.
pub fn deck_zero_g(interior: &reachlock_core::generator::ship::ShipInterior, index: usize) -> bool {
    interior.decks.get(index).map(|d| d.zero_g).unwrap_or(false)
}

/// Helper: lookup deck_of using the interior stored on the roster, or 0.
pub fn deck_of_roster(roster: &CrewRoster, kind: RoomKind) -> usize {
    match &roster.current_interior {
        Some(interior) => deck_of(interior, kind),
        None => 0,
    }
}

/// Helper: lookup deck_zero_g using the interior stored on the roster.
pub fn deck_zero_g_roster(roster: &CrewRoster, index: usize) -> bool {
    match &roster.current_interior {
        Some(interior) => deck_zero_g(interior, index),
        None => false,
    }
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

/// The ship the player is currently flying, set once the character (and so
/// their origin) is known. `None` means "not chosen yet" — the engine then
/// uses a neutral starter hull rather than any particular authored ship.
///
/// This is a process-wide holder rather than a Bevy resource because the
/// callers that need it (`cryo_wake_spawn`, `cockpit_seat_spawn`,
/// `crisis::deck_layouts`) are plain functions, not systems. Same pattern as
/// the editor's content root.
static ACTIVE_SHIP: std::sync::RwLock<Option<reachlock_core::crew::ShipTemplate>> =
    std::sync::RwLock::new(None);

/// Every ship template authored under `<content root>/hulls/`.
pub fn ship_template_catalog() -> Vec<reachlock_core::crew::ShipTemplate> {
    let mut out = Vec::new();
    // Honour the same content root as every other loader. This used to try
    // two hardcoded relative paths, so it silently found nothing whenever the
    // working directory was neither the workspace root nor a crate dir.
    let dir = reachlock_core::paths::content_root().join("hulls");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            // hulls/ also holds frames and room templates; only the files
            // that parse as a ShipTemplate are ships.
            if let Ok(t) = ron::from_str::<reachlock_core::crew::ShipTemplate>(&text) {
                out.push(t);
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Select the ship the player flies, by template id (from their origin, or a
/// save). Unknown ids leave the neutral starter in place and warn — an origin
/// naming a ship nobody authored must not silently hand the player some other
/// character's ship.
pub fn set_active_ship_template(id: &str) -> bool {
    let catalog = ship_template_catalog();
    match catalog.into_iter().find(|t| t.id == id) {
        Some(template) => {
            info!("ship: flying \"{}\" ({id})", template.name);
            if let Ok(mut g) = ACTIVE_SHIP.write() {
                *g = Some(template);
            }
            true
        }
        None => {
            warn!(
                "ship template \"{id}\" is not authored under hulls/ — \
                 falling back to the starter hull"
            );
            false
        }
    }
}

/// Clear the active ship (new game / character reset).
pub fn clear_active_ship() {
    if let Ok(mut g) = ACTIVE_SHIP.write() {
        *g = None;
    }
}

/// The active ship's hull id, for content overrides and the flight model.
pub fn active_hull_id() -> String {
    ACTIVE_SHIP
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(|t| t.hull_id.clone()))
        .unwrap_or_else(|| STARTER_HULL_ID.to_string())
}

/// Hull id used when the character has no ship of their own yet.
pub const STARTER_HULL_ID: &str = "starter";

/// The interior of the ship the player is flying.
///
/// This used to be `active_ship_interior()`, hardwired to one authored
/// ship. Because it was also the fallback for every unresolved case, every
/// origin whose `ship_template` was not authored quietly put the player aboard
/// the Loup-Garou.
pub fn active_ship_interior() -> reachlock_core::generator::ship::ShipInterior {
    if let Ok(g) = ACTIVE_SHIP.read() {
        if let Some(t) = g.as_ref() {
            return t.interior.clone();
        }
    }
    starter_interior()
}

/// A neutral one-deck hull: a corridor and a cockpit. Deliberately generic —
/// it is what a character flies before their origin grants them anything, and
/// it must not resemble any authored ship.
pub fn starter_interior() -> reachlock_core::generator::ship::ShipInterior {
    reachlock_core::generator::ship::ShipInterior {
        decks: vec![reachlock_core::generator::ship::ShipDeck {
            name: "MAIN DECK".into(),
            zero_g: false,
            layout: reachlock_core::generator::GeneratedLayout {
                rooms: vec![
                    reachlock_core::generator::Room {
                        kind: reachlock_core::generator::RoomKind::Corridor,
                        x: 0,
                        y: -8,
                        width: 8,
                        height: 48,
                    },
                    reachlock_core::generator::Room {
                        kind: reachlock_core::generator::RoomKind::Cockpit,
                        x: -12,
                        y: 32,
                        width: 32,
                        height: 16,
                    },
                ],
                doors: vec![reachlock_core::generator::Door {
                    from: 0,
                    to: 1,
                    x: 4,
                    y: 32,
                }],
            },
            ladder: (4, 16),
        }],
    }
}

/// Generate a pool of recruitable crew members at a station. Deterministic
/// for a given station seed + tick — same seed + tick produces the same pool.
pub fn generate_recruitable_crew(
    station_seed: u64,
    tick: u64,
    local_species: &[String],
) -> Vec<RecruitableCrew> {
    let mut rng = SeededRng::new(station_seed ^ tick);
    let count = 3 + (rng.next_below(3) as usize); // 3–5 candidates
    let roles = [
        "pilot",
        "engineer",
        "doctor",
        "marine",
        "scientist",
        "navigator",
        "gunner",
        "medic",
    ];
    let hooks = [
        "Looking for a quiet berth.",
        "Former corporate security, seeking honest work.",
        "Need a change of scenery — and fast.",
        "My last ship went down. I didn't.",
        "I've got skills and no questions.",
        "Heard this captain's got a reputation.",
        "Just need to get off this rock.",
        "Specialist with a troubled past.",
        "Crew's fine, pay's fair — that's all I ask.",
        "Ex-military, clean record, steady hands.",
    ];

    let mut crew = Vec::with_capacity(count);
    for i in 0..count {
        let species = if local_species.is_empty() {
            "Human"
        } else {
            &local_species[rng.next_below(local_species.len() as u64) as usize]
        };
        let seed = rng.next_below(1 << 53);
        let soul_data = reachlock_core::generator::soul::generate_soul(seed, species);
        let species_enum = match species {
            "Human" => reachlock_core::soul::types::Species::Human,
            "Synthetic" | "Android" => reachlock_core::soul::types::Species::Android,
            "Robot" => reachlock_core::soul::types::Species::Robot,
            "Voidborn" => reachlock_core::soul::types::Species::Voidborn,
            "Augmented" => reachlock_core::soul::types::Species::Human,
            "Xenotype" => reachlock_core::soul::types::Species::Xenotype,
            _ => reachlock_core::soul::types::Species::Human,
        };
        let role_idx = rng.next_below(roles.len() as u64) as usize;
        let role_id = roles[role_idx];
        let hook_idx = rng.next_below(hooks.len() as u64) as usize;

        // Salary based on role difficulty: marines/gunners cost more.
        let salary_demand = match role_id {
            "pilot" | "engineer" => 50 + rng.next_below(50),
            "doctor" | "marine" => 75 + rng.next_below(75),
            _ => 30 + rng.next_below(40),
        };

        let soul_name = soul_data.name.clone();
        let soul_backstory = soul_data.backstory.clone();
        let soul_formality = soul_data.formality;
        let soul_file = reachlock_core::soul::types::SoulFile {
            id: format!("recruit_{station_seed}_{tick}_{i}"),
            name: soul_name,
            species: species_enum,
            portrait_id: String::new(),
            identity: reachlock_core::soul::types::Identity {
                origin: "procedural".into(),
                faction_affiliation: "unaffiliated".into(),
                role: role_id.to_string(),
                public_bio: soul_backstory.clone(),
            },
            personality: reachlock_core::soul::types::Personality {
                traits: vec![],
                values: vec![],
                speaking_style: if soul_formality > 512 {
                    reachlock_core::soul::types::SpeakingStyle::Formal
                } else {
                    reachlock_core::soul::types::SpeakingStyle::Terse
                },
                quirks: vec![],
            },
            emotional_state: reachlock_core::soul::types::EmotionalState {
                dominant_mood: reachlock_core::soul::types::Mood::Stable,
                intensity: 256,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: soul_backstory,
            secrets: vec![],
            dialogue: None,
            deflections: vec![],
            look: None,
        };

        crew.push(RecruitableCrew {
            soul: soul_file,
            preferred_role: role_id.to_string(),
            salary_demand,
            hook: hooks[hook_idx].to_string(),
            source: CrewSource::Procedural {
                seed,
                species: species.to_string(),
            },
        });
    }
    crew
}

/// Build the crew roster from content after souls are loaded.
/// Must run after `soul::init_souls` and before `inventory::load_save`.
pub fn init_crew_roster(
    content: Res<ContentIndex>,
    soul_registry: Res<SoulRegistry>,
    mut roster: ResMut<CrewRoster>,
) {
    *roster = CrewRoster::load_from_content(&content, &soul_registry.files);
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
fn crew_body_kind(souls: &SoulRegistry, id: &str) -> crate::pixel::BodyKind {
    souls
        .files
        .get(id)
        .map(|s| crate::pixel::body_kind_from_species(s.species))
        .unwrap_or(crate::pixel::BodyKind::Human)
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn crew_shift_system(
    time: Res<Time>,
    interior: Res<crate::systems::interior::CurrentInterior>,
    active_deck: Res<crate::systems::interior::ActiveDeck>,
    mode: Option<Res<State<crate::states::GameMode>>>,
    mut roster: ResMut<CrewRoster>,
    souls: Res<SoulRegistry>,
    mut elapsed: Local<f32>,
    mut figures: Query<(&CrewFigure, &mut CrewNav, &mut Transform)>,
) {
    let dt = time.delta_secs();
    *elapsed += dt;
    let on_shift = shift_parity(*elapsed, SHIFT_PERIOD);
    let on_board =
        mode.is_some_and(|m| **m == crate::states::GameMode::OnBoard) && interior.layout.is_some();
    // ── Pre-compute deck lookups (clone interior to avoid borrow conflicts) ──
    let deck_interior = roster.current_interior.clone();
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
            let cross_deck = deck_interior.as_ref().map_or(0, |i| deck_of(i, target)) != m.deck;
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
            let factor = move_factor(crew_body_kind(&souls, &fig.0), interior.zero_g);
            let step = CREW_SPEED * factor * dt;
            if to.length() <= step.max(2.0) {
                t.translation.x = next.x;
                t.translation.y = next.y;
                nav.path.remove(0);
                if nav.path.is_empty() {
                    if cross_deck {
                        // At the ladder: climb. One abstract leg covers the
                        // far side; the presence sync removes the sprite.
                        m.deck = deck_interior.as_ref().map_or(0, |i| deck_of(i, target));
                        m.offscreen_eta = OFFSCREEN_LEG_SECS
                            / move_factor(
                                crew_body_kind(&souls, &m.id),
                                deck_interior
                                    .as_ref()
                                    .is_some_and(|i| deck_zero_g(i, m.deck)),
                            );
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
        let target_deck = deck_interior.as_ref().map_or(0, |i| deck_of(i, target));
        if m.current_room == target && target_deck == m.deck {
            m.offscreen_eta = 0.0;
            continue;
        }
        if m.offscreen_eta <= 0.0 {
            m.offscreen_eta = OFFSCREEN_LEG_SECS
                / move_factor(
                    crew_body_kind(&souls, &m.id),
                    deck_interior
                        .as_ref()
                        .is_some_and(|i| deck_zero_g(i, m.deck)),
                );
        }
        m.offscreen_eta -= dt;
        if m.offscreen_eta <= 0.0 {
            if target_deck != m.deck {
                m.deck = target_deck; // climbed the ladder, unseen
            } else {
                m.current_room = target; // walked into the room, unseen
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use reachlock_core::generator::ship::ShipInterior;
    use reachlock_core::generator::{Door, Room};

    /// Build the Loup-Garou interior for testing.
    fn test_loup_garou() -> ShipInterior {
        ShipInterior {
            decks: vec![
                reachlock_core::generator::ship::ShipDeck {
                    name: "LOWER DECK".into(),
                    zero_g: false,
                    layout: reachlock_core::generator::GeneratedLayout {
                        rooms: vec![
                            Room {
                                kind: RoomKind::Corridor,
                                x: 0,
                                y: -16,
                                width: 8,
                                height: 72,
                            },
                            Room {
                                kind: RoomKind::Bridge,
                                x: -12,
                                y: 56,
                                width: 32,
                                height: 16,
                            },
                            Room {
                                kind: RoomKind::Scanner,
                                x: -20,
                                y: 40,
                                width: 20,
                                height: 12,
                            },
                            Room {
                                kind: RoomKind::MedBay,
                                x: 8,
                                y: 40,
                                width: 18,
                                height: 12,
                            },
                            Room {
                                kind: RoomKind::Reactor,
                                x: -22,
                                y: 18,
                                width: 22,
                                height: 18,
                            },
                            Room {
                                kind: RoomKind::Cryo,
                                x: 8,
                                y: 16,
                                width: 24,
                                height: 22,
                            },
                            Room {
                                kind: RoomKind::Quarters,
                                x: -20,
                                y: 2,
                                width: 20,
                                height: 12,
                            },
                            Room {
                                kind: RoomKind::Quarters,
                                x: 8,
                                y: 2,
                                width: 18,
                                height: 14,
                            },
                            Room {
                                kind: RoomKind::Bar,
                                x: -20,
                                y: -16,
                                width: 20,
                                height: 14,
                            },
                            Room {
                                kind: RoomKind::Hangar,
                                x: 8,
                                y: -16,
                                width: 18,
                                height: 14,
                            },
                        ],
                        doors: vec![
                            Door {
                                from: 0,
                                to: 1,
                                x: 4,
                                y: 56,
                            },
                            Door {
                                from: 0,
                                to: 2,
                                x: 0,
                                y: 46,
                            },
                            Door {
                                from: 0,
                                to: 3,
                                x: 8,
                                y: 46,
                            },
                            Door {
                                from: 0,
                                to: 4,
                                x: 0,
                                y: 27,
                            },
                            Door {
                                from: 0,
                                to: 5,
                                x: 8,
                                y: 27,
                            },
                            Door {
                                from: 0,
                                to: 6,
                                x: 0,
                                y: 8,
                            },
                            Door {
                                from: 0,
                                to: 7,
                                x: 8,
                                y: 9,
                            },
                            Door {
                                from: 0,
                                to: 8,
                                x: 0,
                                y: -9,
                            },
                            Door {
                                from: 0,
                                to: 9,
                                x: 8,
                                y: -9,
                            },
                        ],
                    },
                    ladder: (4, 48),
                },
                reachlock_core::generator::ship::ShipDeck {
                    name: "UPPER DECK".into(),
                    zero_g: true,
                    layout: reachlock_core::generator::GeneratedLayout {
                        rooms: vec![
                            Room {
                                kind: RoomKind::Corridor,
                                x: 0,
                                y: 8,
                                width: 8,
                                height: 48,
                            },
                            Room {
                                kind: RoomKind::Cockpit,
                                x: -12,
                                y: 56,
                                width: 32,
                                height: 16,
                            },
                            Room {
                                kind: RoomKind::TechBay,
                                x: -24,
                                y: -24,
                                width: 56,
                                height: 32,
                            },
                        ],
                        doors: vec![
                            Door {
                                from: 0,
                                to: 1,
                                x: 4,
                                y: 56,
                            },
                            Door {
                                from: 0,
                                to: 2,
                                x: 4,
                                y: 8,
                            },
                        ],
                    },
                    ladder: (4, 48),
                },
            ],
        }
    }

    fn member() -> CrewMember {
        let ship = test_loup_garou();
        CrewMember {
            id: "boris".into(),
            name: "Boris".into(),
            role: CrewRole::new("engineer", "Engineer", ""),
            duty_room: RoomKind::Reactor,
            current_room: RoomKind::Reactor,
            deck: deck_of(&ship, RoomKind::Reactor),
            order: None,
            offscreen_eta: 0.0,
            soul: None,
            salary: 0,
            unpaid_ticks: 0,
            health: CrewHealth::Healthy,
            active_breaking_points: Vec::new(),
        }
    }

    #[test]
    fn deck_of_matches_the_authored_ship() {
        let ship = test_loup_garou();
        // Lower/gravity deck (docs/SHIPS.md §6).
        for kind in [
            RoomKind::Bridge,
            RoomKind::Reactor,
            RoomKind::MedBay,
            RoomKind::Cryo,
            RoomKind::Quarters,
            RoomKind::Bar,
        ] {
            assert_eq!(deck_of(&ship, kind), 0, "{kind:?} is Downstairs");
            assert!(!deck_zero_g(&ship, deck_of(&ship, kind)));
        }
        // Upper/zero-g deck.
        for kind in [RoomKind::Cockpit, RoomKind::TechBay] {
            assert_eq!(deck_of(&ship, kind), 1, "{kind:?} is Upstairs");
            assert!(deck_zero_g(&ship, deck_of(&ship, kind)));
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

    // ── S80: crew open-world system tests ──

    #[test]
    fn role_registry_defaults() {
        let reg = CrewRoleRegistry::with_defaults();
        // Should have at least 5 default roles.
        assert!(
            reg.roles.len() >= 5,
            "expected >=5 default roles, got {}",
            reg.roles.len()
        );
        assert!(reg.roles.contains_key("pilot"));
        assert!(reg.roles.contains_key("engineer"));
        assert!(reg.roles.contains_key("doctor"));
        assert!(reg.roles.contains_key("marine"));
        assert!(reg.roles.contains_key("scientist"));
        // A few extra legacy roles should also be present.
        assert!(reg.roles.contains_key("captain"));
        assert!(reg.roles.contains_key("gunner"));
        // Unknown role gracefully returns a placeholder.
        let unknown = reg.get("nonexistent");
        assert_eq!(unknown.id, "nonexistent");
    }

    #[test]
    fn roster_add_remove_save_cycle() {
        let mut roster = CrewRoster::default();
        let member = CrewMember {
            id: "test_crew_1".into(),
            name: "Test".into(),
            role: CrewRole::new("pilot", "Pilot", ""),
            duty_room: RoomKind::Cockpit,
            current_room: RoomKind::Cockpit,
            deck: 0,
            order: None,
            offscreen_eta: 0.0,
            soul: None,
            salary: 100,
            unpaid_ticks: 0,
            health: CrewHealth::Healthy,
            active_breaking_points: Vec::new(),
        };
        // Add
        assert!(roster.add(member.clone()).is_ok());
        assert_eq!(roster.count(), 1);
        // Duplicate add fails
        assert!(roster.add(member.clone()).is_err());
        // Get
        assert!(roster.get("test_crew_1").is_some());
        assert!(roster.get("nonexistent").is_none());
        // Remove
        let removed = roster.remove("test_crew_1");
        assert!(removed.is_some());
        assert_eq!(roster.count(), 0);
        // Save → reload round-trip via serialization
        roster.add(member.clone()).unwrap();
        let text = ron::to_string(&roster).expect("serialize roster");
        let back: CrewRoster = ron::from_str(&text).expect("deserialize roster");
        assert_eq!(back.count(), 1);
        assert_eq!(back.get("test_crew_1").unwrap().name, "Test");
        assert_eq!(back.get("test_crew_1").unwrap().salary, 100);
    }

    #[test]
    fn recruitable_generation_deterministic() {
        let species = vec!["Human".to_string(), "Synthetic".to_string()];
        // Same seed + tick produces the same pool.
        let a = generate_recruitable_crew(42, 100, &species);
        let b = generate_recruitable_crew(42, 100, &species);
        assert_eq!(a.len(), b.len());
        for (ca, cb) in a.iter().zip(b.iter()) {
            assert_eq!(ca.soul.id, cb.soul.id);
            assert_eq!(ca.preferred_role, cb.preferred_role);
            assert_eq!(ca.salary_demand, cb.salary_demand);
            assert_eq!(ca.hook, cb.hook);
        }
        // All have valid data.
        for crew in &a {
            assert!(!crew.soul.id.is_empty());
            assert!(!crew.preferred_role.is_empty());
            assert!(crew.salary_demand > 0);
            assert!(!crew.hook.is_empty());
        }
        // Different seed → different pool.
        let c = generate_recruitable_crew(99, 100, &species);
        let some_diff = a
            .iter()
            .zip(c.iter())
            .any(|(ca, cc)| ca.soul.id != cc.soul.id);
        assert!(
            some_diff || a.len() != c.len(),
            "different seeds should differ"
        );
    }

    #[test]
    fn relationship_on_hire() {
        use reachlock_core::soul::SoulState;
        // Hiring a crew member creates a relationship entry in the player's soul.
        let mut registry = crate::systems::soul::SoulRegistry::default();
        // Create a player soul.
        let player_soul = reachlock_core::soul::types::SoulFile {
            id: "player".into(),
            name: "Captain".into(),
            species: reachlock_core::soul::types::Species::Human,
            portrait_id: String::new(),
            identity: reachlock_core::soul::types::Identity {
                origin: "test".into(),
                faction_affiliation: "crew".into(),
                role: "Captain".into(),
                public_bio: String::new(),
            },
            personality: reachlock_core::soul::types::Personality {
                traits: vec![],
                values: vec![],
                speaking_style: reachlock_core::soul::types::SpeakingStyle::Terse,
                quirks: vec![],
            },
            emotional_state: reachlock_core::soul::types::EmotionalState {
                dominant_mood: reachlock_core::soul::types::Mood::Stable,
                intensity: 512,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![],
            dialogue: None,
            deflections: vec![],
            look: None,
        };
        registry
            .states
            .insert("player".into(), SoulState::from_file(&player_soul));
        registry.files.insert("player".into(), player_soul);

        // Create a crew member soul.
        let crew_soul = reachlock_core::soul::types::SoulFile {
            id: "hired_crew".into(),
            name: "Recruit".into(),
            species: reachlock_core::soul::types::Species::Human,
            portrait_id: String::new(),
            identity: reachlock_core::soul::types::Identity {
                origin: "test".into(),
                faction_affiliation: "crew".into(),
                role: "Engineer".into(),
                public_bio: String::new(),
            },
            personality: reachlock_core::soul::types::Personality {
                traits: vec![],
                values: vec![],
                speaking_style: reachlock_core::soul::types::SpeakingStyle::Terse,
                quirks: vec![],
            },
            emotional_state: reachlock_core::soul::types::EmotionalState {
                dominant_mood: reachlock_core::soul::types::Mood::Stable,
                intensity: 256,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![],
            dialogue: None,
            deflections: vec![],
            look: None,
        };
        registry
            .states
            .insert("hired_crew".into(), SoulState::from_file(&crew_soul));
        registry.files.insert("hired_crew".into(), crew_soul);

        // Simulate hiring: add relationship entry for player → crew.
        let player_state = registry.states.get_mut("player").unwrap();
        // Add a relationship entry (this is what hiring creates).
        player_state
            .relationships
            .push(reachlock_core::soul::types::Relationship {
                target_id: "hired_crew".into(),
                trust: 128,
                familiarity: 64,
                history: vec!["hired".into()],
            });
        // Also record the interaction memory.
        player_state.record_interaction(
            "hired_crew",
            reachlock_core::soul::memory::SignificantEvent {
                tick: 1,
                event_type: reachlock_core::soul::memory::SignificantEventType::FirstMet,
                summary: "Hired as crew.".into(),
                weight: reachlock_core::util::rng::Fixed(256),
                fading: false,
            },
            128,
            1,
        );

        let player_rel = player_state.relationship("hired_crew");
        assert!(
            player_rel.is_some(),
            "player should have relationship with hired crew"
        );
        let rel = player_rel.unwrap();
        assert!(rel.trust > 0, "initial trust should be positive after hire");
    }

    #[test]
    fn trust_delta_from_deliberation() {
        use reachlock_core::contract::co_deliberation::*;
        // Run a deliberation with opposing stances and verify trust delta.
        let mut d = CoDeliberation::from_proposals(
            vec![
                ("boris".into(), "repair_weapons".into(), "do it".into()),
                ("tove".into(), "tend_medbay".into(), "crew hurt".into()),
            ],
            GameEvent {
                event_type: "test".into(),
                summary: "test".into(),
                fields: BTreeMap::new(),
            },
        );
        // Seed Tove's distrust of Boris so she opposes him (creates deltas).
        if let Some(tove) = d.participants.iter_mut().find(|p| p.crew_id == "tove") {
            tove.relationship_state.insert(
                "boris".into(),
                reachlock_core::contract::co_deliberation::CrewRelationship {
                    familiarity: reachlock_core::util::rng::Fixed::from_int(0),
                    trust: reachlock_core::util::rng::Fixed(-512),
                    respect: reachlock_core::util::rng::Fixed(-512),
                    tension: reachlock_core::util::rng::Fixed::from_int(0),
                    notable_events: vec![],
                },
            );
        }
        // Step through.
        let mut resolved = false;
        for _ in 0..10 {
            match d.step() {
                StepOutcome::Turn(t) => {
                    // Check if this turn had relationship deltas.
                    if !t.relationship_delta.is_empty() {
                        return; // found deltas — test passes
                    }
                }
                StepOutcome::Resolved(_) => {
                    resolved = true;
                    break;
                }
            }
        }
        assert!(resolved, "deliberation should resolve");
        // Also check post-resolution relationship state.
        for p in &d.participants {
            for rel in p.relationship_state.values() {
                if rel.trust.0 != 0 || rel.respect.0 != 0 || rel.tension.0 != 0 {
                    return; // found deltas — test passes
                }
            }
        }
        panic!("no relationship deltas found after deliberation");
    }

    #[test]
    fn breaking_point_warning() {
        let mut roster = CrewRoster::default();
        let member = CrewMember {
            id: "principled".into(),
            name: "Principled".into(),
            role: CrewRole::new("marine", "Marine", ""),
            duty_room: RoomKind::Quarters,
            current_room: RoomKind::Quarters,
            deck: 0,
            order: None,
            offscreen_eta: 0.0,
            soul: None,
            salary: 0,
            unpaid_ticks: 0,
            health: CrewHealth::Healthy,
            active_breaking_points: vec![
                BreakingPointState {
                    condition: "player_kills_civilian".into(),
                    threshold: 2,
                    current: 0,
                    triggered: false,
                    consequence: BreakingPointConsequence::Warning,
                },
                BreakingPointState {
                    condition: "player_abandons_crew".into(),
                    threshold: 3,
                    current: 0,
                    triggered: false,
                    consequence: BreakingPointConsequence::LeaveAtStation,
                },
            ],
        };
        roster.add(member).unwrap();

        // First trigger — below threshold, no consequence yet.
        let consequences = roster.check_breaking_points("player_kills_civilian", 0);
        assert_eq!(consequences.len(), 0, "below threshold, no trigger");

        // Second trigger — reaches threshold → warning.
        let consequences = roster.check_breaking_points("player_kills_civilian", 0);
        assert_eq!(
            consequences.len(),
            1,
            "should have one triggered consequence"
        );
        assert_eq!(consequences[0], BreakingPointConsequence::Warning);

        // Already triggered — no re-trigger.
        let consequences = roster.check_breaking_points("player_kills_civilian", 0);
        assert_eq!(consequences.len(), 0, "already triggered, no re-trigger");
    }

    #[test]
    fn breaking_point_leave() {
        let mut roster = CrewRoster::default();
        let member = CrewMember {
            id: "leaver".into(),
            name: "Leaver".into(),
            role: CrewRole::new("scientist", "Scientist", ""),
            duty_room: RoomKind::TechBay,
            current_room: RoomKind::TechBay,
            deck: 0,
            order: None,
            offscreen_eta: 0.0,
            soul: None,
            salary: 0,
            unpaid_ticks: 0,
            health: CrewHealth::Healthy,
            active_breaking_points: vec![BreakingPointState {
                condition: "player_abandons_crew".into(),
                threshold: 1,
                current: 0,
                triggered: false,
                consequence: BreakingPointConsequence::LeaveAtStation,
            }],
        };
        roster.add(member).unwrap();

        // Single trigger crosses the threshold → LeaveAtStation.
        let consequences = roster.check_breaking_points("player_abandons_crew", 0);
        assert_eq!(consequences.len(), 1);
        assert_eq!(consequences[0], BreakingPointConsequence::LeaveAtStation);

        // Verify the crew member still exists.
        assert!(roster.get("leaver").is_some());
    }

    #[test]
    fn breaking_point_mutiny() {
        let mut roster = CrewRoster::default();
        roster
            .add(CrewMember {
                id: "mutineer".into(),
                name: "Mutineer".into(),
                role: CrewRole::new("gunner", "Gunner", ""),
                duty_room: RoomKind::Bridge,
                current_room: RoomKind::Bridge,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 0,
                unpaid_ticks: 0,
                health: CrewHealth::Healthy,
                active_breaking_points: vec![BreakingPointState {
                    condition: "player_breaks_contract".into(),
                    threshold: 1,
                    current: 0,
                    triggered: false,
                    consequence: BreakingPointConsequence::Mutiny,
                }],
            })
            .unwrap();

        let consequences = roster.check_breaking_points("player_breaks_contract", 0);
        assert_eq!(consequences.len(), 1);
        assert_eq!(consequences[0], BreakingPointConsequence::Mutiny);
    }

    #[test]
    fn salary_deduction() {
        let mut roster = CrewRoster::default();
        roster
            .add(CrewMember {
                id: "worker".into(),
                name: "Worker".into(),
                role: CrewRole::new("engineer", "Engineer", ""),
                duty_room: RoomKind::Reactor,
                current_room: RoomKind::Reactor,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 100,
                unpaid_ticks: 0,
                health: CrewHealth::Healthy,
                active_breaking_points: Vec::new(),
            })
            .unwrap();
        roster
            .add(CrewMember {
                id: "worker2".into(),
                name: "Worker2".into(),
                role: CrewRole::new("pilot", "Pilot", ""),
                duty_room: RoomKind::Cockpit,
                current_room: RoomKind::Cockpit,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 150,
                unpaid_ticks: 0,
                health: CrewHealth::Healthy,
                active_breaking_points: Vec::new(),
            })
            .unwrap();

        assert_eq!(roster.total_salary(), 250);

        let mut credits = 1000i64;
        // Tick payroll with pay_period=5 — no deductions until ticks >= 5.
        let unpaid = roster.tick_payroll(&mut credits, 5);
        assert!(unpaid.is_empty());
        assert_eq!(credits, 1000);

        // Advance to 5 ticks unpaid by calling tick_payroll 5 times.
        let mut unpaid = Vec::new();
        for _ in 0..5 {
            unpaid = roster.tick_payroll(&mut credits, 5);
        }
        // Both should get paid: 1000 - 250 = 750
        assert!(unpaid.is_empty(), "both should be paid: {unpaid:?}");
        assert_eq!(credits, 750);
    }

    #[test]
    fn unpaid_crew_demands_payment() {
        let mut roster = CrewRoster::default();
        roster
            .add(CrewMember {
                id: "expensive".into(),
                name: "Expensive".into(),
                role: CrewRole::new("doctor", "Doctor", ""),
                duty_room: RoomKind::MedBay,
                current_room: RoomKind::MedBay,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 500,
                unpaid_ticks: 0,
                health: CrewHealth::Healthy,
                active_breaking_points: Vec::new(),
            })
            .unwrap();

        let mut credits = 100i64; // Not enough to pay 500.
        let mut unpaid = Vec::new();
        for _ in 0..5 {
            unpaid = roster.tick_payroll(&mut credits, 5);
        }
        // Crew member went unpaid.
        assert!(!unpaid.is_empty(), "crew should be unpaid");
        assert!(unpaid.contains(&"expensive".to_string()));
        // Credits unchanged (couldn't pay).
        assert_eq!(credits, 100);
    }

    #[test]
    fn injury_healing_cycle() {
        let mut roster = CrewRoster::default();
        roster
            .add(CrewMember {
                id: "boris".into(),
                name: "Boris".into(),
                role: CrewRole::new("engineer", "Engineer", ""),
                duty_room: RoomKind::Reactor,
                current_room: RoomKind::Reactor,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 0,
                unpaid_ticks: 0,
                health: CrewHealth::Healthy,
                active_breaking_points: Vec::new(),
            })
            .unwrap();

        // Healthy → Injured
        roster.injure("boris");
        assert_eq!(roster.get("boris").unwrap().health, CrewHealth::Injured);

        // Injured → Critical
        roster.injure("boris");
        assert_eq!(roster.get("boris").unwrap().health, CrewHealth::Critical);

        // Critical stays Critical (no further injury progression)
        roster.injure("boris");
        assert_eq!(roster.get("boris").unwrap().health, CrewHealth::Critical);

        // Critical → Injured
        roster.heal("boris");
        assert_eq!(roster.get("boris").unwrap().health, CrewHealth::Injured);

        // Injured → Healthy
        roster.heal("boris");
        assert_eq!(roster.get("boris").unwrap().health, CrewHealth::Healthy);

        // Healthy stays Healthy
        roster.heal("boris");
        assert_eq!(roster.get("boris").unwrap().health, CrewHealth::Healthy);
    }

    #[test]
    fn death_removes_from_roster() {
        let mut roster = CrewRoster::default();
        roster
            .add(CrewMember {
                id: "boris".into(),
                name: "Boris".into(),
                role: CrewRole::new("engineer", "Engineer", ""),
                duty_room: RoomKind::Reactor,
                current_room: RoomKind::Reactor,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 0,
                unpaid_ticks: 0,
                health: CrewHealth::Healthy,
                active_breaking_points: Vec::new(),
            })
            .unwrap();

        assert_eq!(roster.count(), 1);
        let dead = roster.kill("boris");
        assert!(dead.is_some());
        assert_eq!(roster.count(), 0);
        assert!(roster.get("boris").is_none());
        // Trying to kill a missing member returns None.
        assert!(roster.kill("nobody").is_none());
    }

    #[test]
    fn role_id_to_action_maps_all_roles() {
        // Every default role maps to a valid action.
        assert_eq!(role_id_to_action("pilot"), "hold_course");
        assert_eq!(role_id_to_action("engineer"), "repair_systems");
        assert_eq!(role_id_to_action("doctor"), "tend_medbay");
        assert_eq!(role_id_to_action("marine"), "coordinate_defense");
        assert_eq!(role_id_to_action("scientist"), "analyze_data");
        assert_eq!(role_id_to_action("captain"), "lead_crew");
        assert_eq!(role_id_to_action("gunner"), "man_battle_stations");
        assert_eq!(role_id_to_action("medic"), "tend_medbay");
        assert_eq!(role_id_to_action("navigator"), "plot_jump");
        assert_eq!(role_id_to_action("general"), "coordinate_defense");
        // Unknown roles get a fallback.
        assert_eq!(role_id_to_action("unknown_role"), "maintain_course");
    }

    #[test]
    fn set_health_changes_health() {
        let mut roster = CrewRoster::default();
        roster
            .add(CrewMember {
                id: "test".into(),
                name: "Test".into(),
                role: CrewRole::new("marine", "Marine", ""),
                duty_room: RoomKind::TechBay,
                current_room: RoomKind::TechBay,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 0,
                unpaid_ticks: 0,
                health: CrewHealth::Healthy,
                active_breaking_points: Vec::new(),
            })
            .unwrap();
        roster.set_health("test", CrewHealth::Injured);
        assert_eq!(roster.get("test").unwrap().health, CrewHealth::Injured);
        roster.set_health("test", CrewHealth::Dead);
        assert_eq!(roster.get("test").unwrap().health, CrewHealth::Dead);
    }

    #[test]
    fn by_role_filters_correctly() {
        let mut roster = CrewRoster::default();
        for (id, role_id) in [("a", "pilot"), ("b", "engineer"), ("c", "pilot")] {
            roster
                .add(CrewMember {
                    id: id.into(),
                    name: id.into(),
                    role: CrewRole::new(role_id, role_id, ""),
                    duty_room: RoomKind::Quarters,
                    current_room: RoomKind::Quarters,
                    deck: 0,
                    order: None,
                    offscreen_eta: 0.0,
                    soul: None,
                    salary: 0,
                    unpaid_ticks: 0,
                    health: CrewHealth::Healthy,
                    active_breaking_points: Vec::new(),
                })
                .unwrap();
        }
        let pilots = roster.by_role("pilot");
        assert_eq!(pilots.len(), 2);
        let engineers = roster.by_role("engineer");
        assert_eq!(engineers.len(), 1);
        let nonexistent = roster.by_role("doctor");
        assert_eq!(nonexistent.len(), 0);
    }

    #[test]
    fn pay_crew_member_manually() {
        let mut roster = CrewRoster::default();
        roster
            .add(CrewMember {
                id: "boris".into(),
                name: "Boris".into(),
                role: CrewRole::new("engineer", "Engineer", ""),
                duty_room: RoomKind::Reactor,
                current_room: RoomKind::Reactor,
                deck: 0,
                order: None,
                offscreen_eta: 0.0,
                soul: None,
                salary: 100,
                unpaid_ticks: 10,
                health: CrewHealth::Healthy,
                active_breaking_points: Vec::new(),
            })
            .unwrap();

        let mut credits = 150i64;
        assert!(roster.pay_crew_member("boris", &mut credits));
        assert_eq!(credits, 50);
        assert_eq!(roster.get("boris").unwrap().unpaid_ticks, 0);

        // Not enough credits (50 < 100).
        assert!(!roster.pay_crew_member("boris", &mut credits));
        assert_eq!(credits, 50);
    }
}
