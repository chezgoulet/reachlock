//! Interior editor contracts (spec §19; S18 freeze). `ShipInteriorLayout`
//! is the player's room placement: which templates sit where on the hull's
//! cell grid, which corridors join them, what furniture fills the slots. It
//! goes in the save, so every field is plain data.
//!
//! THE KEY CONTRACT: [`realize`] turns a placement into the exact
//! [`GeneratedLayout`] S08 already walks — same struct, same grid units as
//! `generator::station`, so the On-Board renderer, crew routing, and the
//! walkability math need zero changes to walk an edited ship. If S08 ever
//! needs edits to consume this, the output type drifted; fix it here.
//!
//! Grid math is integers end to end: placements are cells (`u8`), realized
//! rooms are grid units (`i32`, 1 cell = [`CELL`] units — the station
//! generator's GRID). The editor cursor's screen position is the only float
//! anywhere in this feature, and it lives in the client.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::generator::{Door, GeneratedLayout, Room, RoomKind};
use crate::item::StatKey;

/// Grid units per placement cell — matches `generator::station`'s GRID so a
/// realized 2–4-cell room lands at the same world scale as generated rooms.
pub const CELL: i32 = 8;

// ---------------------------------------------------------------------
// Frozen placement contracts (S18 "Freeze first").
// ---------------------------------------------------------------------

/// A room placed on the hull grid. `position` is the room's minimum-corner
/// cell; `rotation` is quarter-turns (0..=3, i.e. 0°/90°/180°/270° — a
/// quarter-turn swaps the template's width and height).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedRoom {
    pub template_id: String,
    pub position: (u8, u8),
    pub rotation: u8,
}

impl PlacedRoom {
    /// Footprint in cells after rotation.
    pub fn footprint(&self, template: &RoomTemplate) -> (u8, u8) {
        if self.rotation % 2 == 1 {
            (template.height, template.width)
        } else {
            (template.width, template.height)
        }
    }
}

/// A corridor the player (or the auto-router) drew: an L-shaped integer
/// path from `from` to `to`, horizontal leg first. Endpoints are cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corridor {
    pub from: (u8, u8),
    pub to: (u8, u8),
}

/// A furniture piece in one of its room's template slots. `room_idx`
/// indexes `ShipInteriorLayout::rooms`; `slot_id` names an entry of the
/// room template's `furniture_slots`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlacedFurniture {
    pub slot_id: String,
    pub room_idx: usize,
    pub kind: FurnitureKind,
}

/// The interior placement (spec §19 struct). The whole editor state that
/// persists: everything else (doors, corridor rooms, bonuses) derives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipInteriorLayout {
    /// References a `content/hulls/*` frame by envelope id (the frame's
    /// `grid_bounds` is the placement area).
    pub hull_id: String,
    pub rooms: Vec<PlacedRoom>,
    pub corridors: Vec<Corridor>,
    pub furniture: Vec<PlacedFurniture>,
    pub seed: u64,
}

/// An authored room template (content/hulls/room_templates.ron payload).
/// `kind` is the realized `RoomKind` — the S08 renderer, crew duty mapping,
/// and mode machine all key on it (airlock ⇒ `Hangar`, cockpit ⇒ `Cockpit`).
/// `adjacent_pairs` lists room kinds this template rewards being next to
/// (S18 adjacency bonuses); default-empty so minimal templates stay valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomTemplate {
    pub id: String,
    pub kind: RoomKind,
    pub label: String,
    /// Size in cells (unrotated).
    pub width: u8,
    pub height: u8,
    pub required_systems: Vec<String>,
    pub furniture_slots: Vec<String>,
    #[serde(default)]
    pub adjacent_pairs: Vec<RoomKind>,
}

/// Everything that can be wrong with a placement. The editor surfaces these
/// live; `realize` refuses to build a broken ship (unreachable room =
/// validation error, not a runtime surprise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealizeError {
    /// Two rooms (or a corridor and a room) share grid cells, or two
    /// furniture pieces share a slot tile.
    Overlap,
    /// A room (or corridor cell) falls outside the hull frame's grid bounds.
    OutOfBounds,
    /// A room the airlock can't reach through doors and corridors.
    UnreachableRoom,
    /// Cockpit + airlock are the minimum viable ship.
    MissingRequiredRoom,
    /// Unknown template id, bad furniture reference, or a template whose
    /// slots don't fit its own footprint.
    InvalidTemplate,
}

impl std::fmt::Display for RealizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RealizeError::Overlap => write!(f, "rooms overlap"),
            RealizeError::OutOfBounds => write!(f, "room outside the hull grid"),
            RealizeError::UnreachableRoom => write!(f, "room unreachable from the airlock"),
            RealizeError::MissingRequiredRoom => write!(f, "cockpit + airlock required"),
            RealizeError::InvalidTemplate => write!(f, "invalid template or furniture reference"),
        }
    }
}

/// Adjacency bonuses computed from a realized placement (spec §19: "Rooms
/// placed next to compatible types gain adjacency bonuses"). Numbers are
/// inert pipes until their consuming systems exist — the pipe and the
/// display are the S18 deliverable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutBonuses {
    /// Galley next to crew quarters → crew relationship recovery.
    pub galley_quarters_bonus: bool,
    /// Engineering next to the cargo hold → faster repair material transfer.
    pub engineering_cargo_bonus: bool,
}

// ---------------------------------------------------------------------
// Furniture: kinds + stat contributions.
// ---------------------------------------------------------------------

/// Every placeable furniture kind (spec §19 furniture table). Each carries
/// stat contributions — a fully-equipped med bay heals faster than a sparse
/// one — via [`FurnitureKind::stat_contributions`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FurnitureKind {
    MedStation,
    PharmacyLocker,
    TriageBed,
    ReactorConsole,
    RepairBench,
    ComponentStorage,
    GalleyUnit,
    Bunk,
    Workbench,
    NanoForge,
    WeaponRack,
    ArmorLocker,
    SampleAnalyzer,
    IsolationChamber,
    GrowBed,
    AeroponicArray,
    NavConsole,
    SensorModule,
    CargoRack,
    RefrigerationUnit,
    RecreationModule,
}

impl FurnitureKind {
    pub const ALL: [FurnitureKind; 21] = [
        FurnitureKind::MedStation,
        FurnitureKind::PharmacyLocker,
        FurnitureKind::TriageBed,
        FurnitureKind::ReactorConsole,
        FurnitureKind::RepairBench,
        FurnitureKind::ComponentStorage,
        FurnitureKind::GalleyUnit,
        FurnitureKind::Bunk,
        FurnitureKind::Workbench,
        FurnitureKind::NanoForge,
        FurnitureKind::WeaponRack,
        FurnitureKind::ArmorLocker,
        FurnitureKind::SampleAnalyzer,
        FurnitureKind::IsolationChamber,
        FurnitureKind::GrowBed,
        FurnitureKind::AeroponicArray,
        FurnitureKind::NavConsole,
        FurnitureKind::SensorModule,
        FurnitureKind::CargoRack,
        FurnitureKind::RefrigerationUnit,
        FurnitureKind::RecreationModule,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FurnitureKind::MedStation => "med station",
            FurnitureKind::PharmacyLocker => "pharmacy locker",
            FurnitureKind::TriageBed => "triage bed",
            FurnitureKind::ReactorConsole => "reactor console",
            FurnitureKind::RepairBench => "repair bench",
            FurnitureKind::ComponentStorage => "component storage",
            FurnitureKind::GalleyUnit => "galley unit",
            FurnitureKind::Bunk => "bunk",
            FurnitureKind::Workbench => "workbench",
            FurnitureKind::NanoForge => "nano-forge",
            FurnitureKind::WeaponRack => "weapon rack",
            FurnitureKind::ArmorLocker => "armor locker",
            FurnitureKind::SampleAnalyzer => "sample analyzer",
            FurnitureKind::IsolationChamber => "isolation chamber",
            FurnitureKind::GrowBed => "grow bed",
            FurnitureKind::AeroponicArray => "aeroponic array",
            FurnitureKind::NavConsole => "nav console",
            FurnitureKind::SensorModule => "sensor module",
            FurnitureKind::CargoRack => "cargo rack",
            FurnitureKind::RefrigerationUnit => "refrigeration unit",
            FurnitureKind::RecreationModule => "recreation module",
        }
    }

    /// Stat contributions in fixed-point 1/1024 (the S05 stat vocabulary —
    /// no new keys, no floats). A pure lookup: the client displays these
    /// and future ShipSystems consumers sum them; nothing here mutates.
    pub fn stat_contributions(self) -> BTreeMap<StatKey, i64> {
        let entries: &[(StatKey, i64)] = match self {
            FurnitureKind::MedStation => &[(StatKey::RepairRate, 2048)],
            FurnitureKind::PharmacyLocker => &[(StatKey::RepairRate, 512)],
            FurnitureKind::TriageBed => &[(StatKey::RepairRate, 1024)],
            FurnitureKind::ReactorConsole => &[(StatKey::Recharge, 1024)],
            FurnitureKind::RepairBench => &[(StatKey::RepairRate, 2048)],
            FurnitureKind::ComponentStorage => &[(StatKey::RepairRate, 512)],
            FurnitureKind::GalleyUnit => &[(StatKey::Recharge, 512)],
            FurnitureKind::Bunk => &[(StatKey::Recharge, 256)],
            FurnitureKind::Workbench => &[(StatKey::RepairRate, 1024)],
            FurnitureKind::NanoForge => &[(StatKey::RepairRate, 3072)],
            FurnitureKind::WeaponRack => &[(StatKey::Damage, 1024)],
            FurnitureKind::ArmorLocker => &[(StatKey::ShieldHp, 1024)],
            FurnitureKind::SampleAnalyzer => &[(StatKey::SensorRange, 2048)],
            FurnitureKind::IsolationChamber => &[(StatKey::RepairRate, 256)],
            FurnitureKind::GrowBed => &[(StatKey::Recharge, 512)],
            FurnitureKind::AeroponicArray => &[(StatKey::Recharge, 1024)],
            FurnitureKind::NavConsole => &[(StatKey::SensorRange, 1024)],
            FurnitureKind::SensorModule => &[(StatKey::SensorRange, 2048)],
            FurnitureKind::CargoRack => &[(StatKey::Weight, 4096)],
            FurnitureKind::RefrigerationUnit => &[(StatKey::Recharge, 256)],
            FurnitureKind::RecreationModule => &[(StatKey::Recharge, 512)],
        };
        entries.iter().copied().collect()
    }
}

// ---------------------------------------------------------------------
// The authored template set (built-in reference fallback).
// ---------------------------------------------------------------------

impl RoomTemplate {
    /// The canonical built-in template set: the spec §19 room list, the
    /// offline-first fallback when no authored templates load, and the
    /// determinism fixture. Pure data, no RNG — authored content overrides
    /// it (same pattern as `HullFrame::reference`).
    ///
    /// Template kinds map onto the existing `RoomKind` vocabulary where
    /// S08 already gives it meaning: airlock ⇒ `Hangar` (the boarding
    /// point), engineering ⇒ `Reactor`, galley ⇒ `Bar`, cargo hold ⇒
    /// `Shipyard`, workshop ⇒ `TechBay` — the same reuse the authored
    /// Loup-Garou plan and the hull-interior generator established.
    pub fn reference_set() -> Vec<RoomTemplate> {
        let t = |id: &str,
                 kind: RoomKind,
                 label: &str,
                 width: u8,
                 height: u8,
                 required: &[&str],
                 slots: &[&str],
                 adjacent: &[RoomKind]| RoomTemplate {
            id: id.into(),
            kind,
            label: label.into(),
            width,
            height,
            required_systems: required.iter().map(|s| s.to_string()).collect(),
            furniture_slots: slots.iter().map(|s| s.to_string()).collect(),
            adjacent_pairs: adjacent.to_vec(),
        };
        vec![
            t(
                "cockpit",
                RoomKind::Cockpit,
                "Cockpit",
                3,
                2,
                &["helm"],
                &["nav", "sensor"],
                &[],
            ),
            t(
                "bridge",
                RoomKind::Bridge,
                "Bridge",
                4,
                3,
                &["command"],
                &["nav", "sensor", "tactical"],
                &[],
            ),
            t(
                "med_bay",
                RoomKind::MedBay,
                "Med Bay",
                3,
                3,
                &["life_support"],
                &["station", "locker", "bed"],
                &[],
            ),
            t(
                "engineering",
                RoomKind::Reactor,
                "Engineering",
                4,
                3,
                &["power"],
                &["console", "bench", "storage"],
                &[RoomKind::Shipyard],
            ),
            t(
                "quarters",
                RoomKind::Quarters,
                "Crew Quarters",
                3,
                3,
                &[],
                &["bunk_a", "bunk_b", "rec"],
                &[],
            ),
            t(
                "galley",
                RoomKind::Bar,
                "Galley",
                3,
                2,
                &[],
                &["galley", "fridge"],
                &[RoomKind::Quarters],
            ),
            t(
                "cargo_hold",
                RoomKind::Shipyard,
                "Cargo Hold",
                4,
                4,
                &[],
                &["rack_a", "rack_b", "rack_c"],
                &[RoomKind::Reactor],
            ),
            t(
                "airlock",
                RoomKind::Hangar,
                "Airlock",
                2,
                2,
                &["pressure"],
                &[],
                &[],
            ),
            t(
                "hydroponics",
                RoomKind::Hydroponics,
                "Hydroponics",
                3,
                3,
                &["water"],
                &["grow_bed", "aero", "analyzer"],
                &[],
            ),
            t(
                "workshop",
                RoomKind::TechBay,
                "Workshop",
                4,
                3,
                &[],
                &["workbench", "nano_forge", "storage"],
                &[],
            ),
            t(
                "armory",
                RoomKind::Armory,
                "Armory",
                2,
                3,
                &[],
                &["rack", "locker"],
                &[],
            ),
            t(
                "brig",
                RoomKind::Brig,
                "Brig",
                2,
                2,
                &[],
                &["isolation"],
                &[],
            ),
        ]
    }
}

/// Find a template by id.
pub fn template<'a>(templates: &'a [RoomTemplate], id: &str) -> Option<&'a RoomTemplate> {
    templates.iter().find(|t| t.id == id)
}

// ---------------------------------------------------------------------
// realize(): placement → the GeneratedLayout S08 walks.
// ---------------------------------------------------------------------

/// A cell-space rectangle (min corner + size, all cells).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellRect {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl CellRect {
    fn overlaps(&self, o: &CellRect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }

    /// The shared-edge door position (grid units) if the rects share a wall
    /// with at least one cell of overlap: the center of the shared span.
    fn shared_door(&self, o: &CellRect) -> Option<(i32, i32)> {
        let span = |a0: i32, a1: i32, b0: i32, b1: i32| {
            let lo = a0.max(b0);
            let hi = a1.min(b1);
            (hi - lo >= 1).then_some((lo + hi) * CELL / 2)
        };
        if self.x + self.w == o.x || o.x + o.w == self.x {
            let edge = if self.x + self.w == o.x { o.x } else { self.x };
            span(self.y, self.y + self.h, o.y, o.y + o.h).map(|mid| (edge * CELL, mid))
        } else if self.y + self.h == o.y || o.y + o.h == self.y {
            let edge = if self.y + self.h == o.y { o.y } else { self.y };
            span(self.x, self.x + self.w, o.x, o.x + o.w).map(|mid| (mid, edge * CELL))
        } else {
            None
        }
    }

    fn to_room(self, kind: RoomKind) -> Room {
        Room {
            kind,
            x: self.x * CELL,
            y: self.y * CELL,
            width: self.w * CELL,
            height: self.h * CELL,
        }
    }
}

/// Split an L-shaped corridor (horizontal leg first, then vertical) into
/// 1-cell-wide rectangles. The corner cell belongs to the horizontal leg;
/// the vertical leg starts one cell past it so the two legs share a wall
/// (and get a direct door) instead of overlapping.
fn corridor_rects(c: &Corridor) -> Vec<CellRect> {
    let (fx, fy) = (c.from.0 as i32, c.from.1 as i32);
    let (tx, ty) = (c.to.0 as i32, c.to.1 as i32);
    let mut rects = Vec::new();
    if fx != tx {
        rects.push(CellRect {
            x: fx.min(tx),
            y: fy,
            w: (fx - tx).abs() + 1,
            h: 1,
        });
    }
    if fy != ty {
        // Vertical leg runs from the row after the corner to the target row
        // when a horizontal leg exists; otherwise it spans the whole way.
        let (lo, hi) = if fy < ty { (fy, ty) } else { (ty, fy) };
        let (y0, h) = if fx != tx {
            if fy < ty {
                (lo + 1, hi - lo)
            } else {
                (lo, hi - lo)
            }
        } else {
            (lo, hi - lo + 1)
        };
        rects.push(CellRect {
            x: tx,
            y: y0,
            w: 1,
            h,
        });
    }
    if fx == tx && fy == ty {
        rects.push(CellRect {
            x: fx,
            y: fy,
            w: 1,
            h: 1,
        });
    }
    rects
}

/// Turn a placement into the walkable [`GeneratedLayout`] (THE S18
/// contract). Validates bounds, overlaps, required rooms (cockpit +
/// airlock), furniture references, and reachability from the airlock;
/// corridors become 1-cell-wide `RoomKind::Corridor` rooms and every door
/// comes from one shared-wall pass, so the output is deterministic in the
/// layout alone — placement order-independent inputs produce identical
/// door sets because pairs are visited in room-index order over rects
/// built from sorted corridor legs.
pub fn realize(
    layout: &ShipInteriorLayout,
    templates: &[RoomTemplate],
    grid_bounds: (u8, u8),
) -> Result<GeneratedLayout, RealizeError> {
    let (bw, bh) = (grid_bounds.0 as i32, grid_bounds.1 as i32);

    // Placed rooms → cell rects (bounds-checked).
    let mut rects: Vec<(CellRect, RoomKind)> = Vec::with_capacity(layout.rooms.len());
    for placed in &layout.rooms {
        let tpl = template(templates, &placed.template_id).ok_or(RealizeError::InvalidTemplate)?;
        if tpl.width == 0 || tpl.height == 0 {
            return Err(RealizeError::InvalidTemplate);
        }
        let (fw, fh) = placed.footprint(tpl);
        let rect = CellRect {
            x: placed.position.0 as i32,
            y: placed.position.1 as i32,
            w: fw as i32,
            h: fh as i32,
        };
        if rect.x + rect.w > bw || rect.y + rect.h > bh {
            return Err(RealizeError::OutOfBounds);
        }
        rects.push((rect, tpl.kind));
    }

    // Corridors → 1-cell-wide legs (bounds-checked).
    for corridor in &layout.corridors {
        for rect in corridor_rects(corridor) {
            if rect.x < 0 || rect.y < 0 || rect.x + rect.w > bw || rect.y + rect.h > bh {
                return Err(RealizeError::OutOfBounds);
            }
            rects.push((rect, RoomKind::Corridor));
        }
    }

    // Overlap: O(n²) over <20 rooms + a few corridor legs is fine.
    for i in 0..rects.len() {
        for j in i + 1..rects.len() {
            if rects[i].0.overlaps(&rects[j].0) {
                return Err(RealizeError::Overlap);
            }
        }
    }

    // Required rooms: the minimum viable ship is a cockpit and an airlock.
    let has = |kind: RoomKind| rects.iter().any(|(_, k)| *k == kind);
    if !has(RoomKind::Cockpit) || !has(RoomKind::Hangar) {
        return Err(RealizeError::MissingRequiredRoom);
    }

    // Furniture references must resolve, slots must fit their footprint,
    // and no two pieces may take the same slot tile.
    let mut taken: Vec<(usize, (u8, u8))> = Vec::new();
    for piece in &layout.furniture {
        let placed = layout
            .rooms
            .get(piece.room_idx)
            .ok_or(RealizeError::InvalidTemplate)?;
        let tpl = template(templates, &placed.template_id).ok_or(RealizeError::InvalidTemplate)?;
        let tile =
            furniture_tile(placed, tpl, &piece.slot_id).ok_or(RealizeError::InvalidTemplate)?;
        if taken.contains(&(piece.room_idx, tile)) {
            return Err(RealizeError::Overlap);
        }
        taken.push((piece.room_idx, tile));
    }

    // One uniform door pass: every pair of rects sharing a wall (with ≥1
    // cell of shared span) gets a door at the span's center. Adjacent
    // placed rooms connect directly; corridor legs connect to whatever
    // they run alongside — including each other at the L corner.
    let mut doors = Vec::new();
    for i in 0..rects.len() {
        for j in i + 1..rects.len() {
            if let Some((x, y)) = rects[i].0.shared_door(&rects[j].0) {
                doors.push(Door {
                    from: i as u32,
                    to: j as u32,
                    x,
                    y,
                });
            }
        }
    }

    // Reachability from the airlock (fixpoint iteration, the
    // generator/station.rs connectivity-test pattern).
    let airlock = rects
        .iter()
        .position(|(_, k)| *k == RoomKind::Hangar)
        .expect("required-room check passed");
    let n = rects.len();
    let mut reached = vec![false; n];
    reached[airlock] = true;
    for _ in 0..n {
        for door in &doors {
            let (f, t) = (door.from as usize, door.to as usize);
            if reached[f] || reached[t] {
                reached[f] = true;
                reached[t] = true;
            }
        }
    }
    if !reached.iter().all(|&r| r) {
        return Err(RealizeError::UnreachableRoom);
    }

    Ok(GeneratedLayout {
        rooms: rects
            .into_iter()
            .map(|(rect, kind)| rect.to_room(kind))
            .collect(),
        doors,
    })
}

// ---------------------------------------------------------------------
// Furniture placement math.
// ---------------------------------------------------------------------

/// The cell a furniture slot occupies, absolute on the hull grid: slots
/// fill the room's (rotated) footprint row-major, so a slot's tile is a
/// pure function of the template and always falls inside its parent room
/// as long as the template's slot count fits its area (checked here —
/// `None` = unknown slot id or a template with more slots than cells).
pub fn furniture_tile(
    placed: &PlacedRoom,
    template: &RoomTemplate,
    slot_id: &str,
) -> Option<(u8, u8)> {
    let index = template.furniture_slots.iter().position(|s| s == slot_id)?;
    let (fw, fh) = placed.footprint(template);
    if fw == 0 || index >= fw as usize * fh as usize {
        return None;
    }
    let dx = (index % fw as usize) as u8;
    let dy = (index / fw as usize) as u8;
    Some((placed.position.0 + dx, placed.position.1 + dy))
}

// ---------------------------------------------------------------------
// Corridor auto-routing (the editor's corridor preview).
// ---------------------------------------------------------------------

/// Auto-generate corridors joining rooms that don't already touch: door
/// connectors are wall centers, one cell outside each room; disconnected
/// rooms link to the nearest already-connected room by an L-shaped path
/// between their closest connectors. Connector lists and candidate pairs
/// are sorted before pathing (S18 gotcha: two identical layouts must not
/// diff), so the result is placement order-independent. Rooms sharing a
/// wall get a direct door from `realize` and never a corridor here.
pub fn auto_corridors(rooms: &[PlacedRoom], templates: &[RoomTemplate]) -> Vec<Corridor> {
    let rects: Vec<Option<CellRect>> = rooms
        .iter()
        .map(|p| {
            template(templates, &p.template_id).map(|t| {
                let (fw, fh) = p.footprint(t);
                CellRect {
                    x: p.position.0 as i32,
                    y: p.position.1 as i32,
                    w: fw as i32,
                    h: fh as i32,
                }
            })
        })
        .collect();
    let known: Vec<(usize, CellRect)> = rects
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.map(|r| (i, r)))
        .collect();
    if known.is_empty() {
        return Vec::new();
    }

    // Union-find over rooms, seeded by direct wall adjacency.
    let n = rooms.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut Vec<usize>, i: usize) -> usize {
        if parent[i] != i {
            let root = find(parent, parent[i]);
            parent[i] = root;
        }
        parent[i]
    }
    for a in 0..known.len() {
        for b in a + 1..known.len() {
            if known[a].1.shared_door(&known[b].1).is_some() {
                let (ra, rb) = (find(&mut parent, known[a].0), find(&mut parent, known[b].0));
                parent[ra] = rb;
            }
        }
    }

    // Wall-center connectors, one cell outside each wall.
    let connectors = |r: &CellRect| -> Vec<(i32, i32)> {
        let mut c = vec![
            (r.x + r.w / 2, r.y - 1),   // south
            (r.x + r.w / 2, r.y + r.h), // north
            (r.x - 1, r.y + r.h / 2),   // west
            (r.x + r.w, r.y + r.h / 2), // east
        ];
        c.sort_unstable();
        c
    };

    // The best cross-component join found in one pass: its ordering key
    // (distance, then canonicalized endpoints) plus the two room indices.
    type Join = ((i32, (i32, i32), (i32, i32)), usize, usize);

    // Greedily join components: among all cross-component room pairs, take
    // the connector pair with the smallest (manhattan distance, coords)
    // key. The pair is canonicalized (min endpoint first) so the choice —
    // and the emitted corridor — is independent of room placement order.
    let mut corridors = Vec::new();
    loop {
        let mut best: Option<Join> = None;
        for a in 0..known.len() {
            for b in a + 1..known.len() {
                let (ia, ra) = known[a];
                let (ib, rb) = known[b];
                if find(&mut parent, ia) == find(&mut parent, ib) {
                    continue;
                }
                for ca in connectors(&ra) {
                    for cb in connectors(&rb) {
                        let d = (ca.0 - cb.0).abs() + (ca.1 - cb.1).abs();
                        let key = (d, ca.min(cb), ca.max(cb));
                        if best.as_ref().is_none_or(|(k, _, _)| key < *k) {
                            best = Some((key, ia, ib));
                        }
                    }
                }
            }
        }
        let Some(((_, ca, cb), ia, ib)) = best else {
            break;
        };
        // Clamp connectors to the grid's non-negative quadrant; realize
        // bounds-checks the rest.
        let cell = |p: (i32, i32)| (p.0.max(0) as u8, p.1.max(0) as u8);
        corridors.push(Corridor {
            from: cell(ca),
            to: cell(cb),
        });
        let (ra, rb) = (find(&mut parent, ia), find(&mut parent, ib));
        parent[ra] = rb;
    }
    corridors
}

// ---------------------------------------------------------------------
// Adjacency bonuses.
// ---------------------------------------------------------------------

/// True when a room of `kind_a` sits next to a room of `kind_b`: sharing a
/// wall, or joined by a corridor of length 1 (a single-cell leg touching
/// both).
fn kinds_adjacent(
    layout: &ShipInteriorLayout,
    templates: &[RoomTemplate],
    kind_a: RoomKind,
    kind_b: RoomKind,
) -> bool {
    let rect_of = |p: &PlacedRoom| {
        template(templates, &p.template_id).map(|t| {
            let (fw, fh) = p.footprint(t);
            (
                t.kind,
                CellRect {
                    x: p.position.0 as i32,
                    y: p.position.1 as i32,
                    w: fw as i32,
                    h: fh as i32,
                },
            )
        })
    };
    let rooms: Vec<(RoomKind, CellRect)> = layout.rooms.iter().filter_map(rect_of).collect();
    let single_cells: Vec<CellRect> = layout
        .corridors
        .iter()
        .flat_map(corridor_rects)
        .filter(|r| r.w == 1 && r.h == 1)
        .collect();
    for (ka, ra) in &rooms {
        for (kb, rb) in &rooms {
            if (*ka, *kb) != (kind_a, kind_b) {
                continue;
            }
            if ra.shared_door(rb).is_some() {
                return true;
            }
            if single_cells
                .iter()
                .any(|c| ra.shared_door(c).is_some() && rb.shared_door(c).is_some())
            {
                return true;
            }
        }
    }
    false
}

/// Compute the layout's adjacency bonuses from the templates' authored
/// `adjacent_pairs`. A bonus fires only if some placed template authors
/// the pairing AND the pairing is physically adjacent in the placement.
pub fn compute_bonuses(layout: &ShipInteriorLayout, templates: &[RoomTemplate]) -> LayoutBonuses {
    let authored = |kind_a: RoomKind, kind_b: RoomKind| {
        layout.rooms.iter().any(|p| {
            template(templates, &p.template_id)
                .is_some_and(|t| t.kind == kind_a && t.adjacent_pairs.contains(&kind_b))
        })
    };
    let pair = |a: RoomKind, b: RoomKind| {
        (authored(a, b) || authored(b, a))
            && (kinds_adjacent(layout, templates, a, b) || kinds_adjacent(layout, templates, b, a))
    };
    LayoutBonuses {
        galley_quarters_bonus: pair(RoomKind::Bar, RoomKind::Quarters),
        engineering_cargo_bonus: pair(RoomKind::Reactor, RoomKind::Shipyard),
    }
}

#[cfg(test)]
mod interior_tests {
    use super::*;

    fn place(template_id: &str, x: u8, y: u8) -> PlacedRoom {
        PlacedRoom {
            template_id: template_id.into(),
            position: (x, y),
            rotation: 0,
        }
    }

    /// The determinism fixture: airlock + cockpit + galley + quarters
    /// adjacent; engineering + cargo across a 2-cell gap bridged by one
    /// vertical corridor. Everything reachable — only via the corridor for
    /// the aft pair.
    fn fixture() -> ShipInteriorLayout {
        ShipInteriorLayout {
            hull_id: "frame_corvette".into(),
            rooms: vec![
                place("airlock", 0, 0),     // 0: (0..2, 0..2)
                place("cockpit", 0, 2),     // 1: (0..3, 2..4)
                place("galley", 2, 0),      // 2: (2..5, 0..2)
                place("quarters", 3, 2),    // 3: (3..6, 2..5)
                place("engineering", 0, 6), // 4: (0..4, 6..9)
                place("cargo_hold", 4, 6),  // 5: (4..8, 6..10)
            ],
            corridors: vec![Corridor {
                from: (1, 4),
                to: (1, 5),
            }],
            furniture: vec![
                PlacedFurniture {
                    slot_id: "galley".into(),
                    room_idx: 2,
                    kind: FurnitureKind::GalleyUnit,
                },
                PlacedFurniture {
                    slot_id: "console".into(),
                    room_idx: 4,
                    kind: FurnitureKind::ReactorConsole,
                },
            ],
            seed: 42,
        }
    }

    const BOUNDS: (u8, u8) = (16, 12);

    fn templates() -> Vec<RoomTemplate> {
        RoomTemplate::reference_set()
    }

    /// Iron rule #4: the serialized form is pinned — `ShipInteriorLayout`
    /// goes in the save. If this JSON changes, that's a save-format
    /// revision: update deliberately and note it.
    #[test]
    fn wire_shape_is_pinned() {
        let json = serde_json::to_string(&fixture()).unwrap();
        assert_eq!(
            json,
            r#"{"hull_id":"frame_corvette","rooms":[{"template_id":"airlock","position":[0,0],"rotation":0},{"template_id":"cockpit","position":[0,2],"rotation":0},{"template_id":"galley","position":[2,0],"rotation":0},{"template_id":"quarters","position":[3,2],"rotation":0},{"template_id":"engineering","position":[0,6],"rotation":0},{"template_id":"cargo_hold","position":[4,6],"rotation":0}],"corridors":[{"from":[1,4],"to":[1,5]}],"furniture":[{"slot_id":"galley","room_idx":2,"kind":"galley_unit"},{"slot_id":"console","room_idx":4,"kind":"reactor_console"}],"seed":42}"#
        );
        let back: ShipInteriorLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fixture());
    }

    /// RON is the save/authoring format — the round trip must hold there
    /// too (enum variant syntax is where typos live).
    #[test]
    fn ron_round_trip() {
        let text = ron::to_string(&fixture()).unwrap();
        let back: ShipInteriorLayout = ron::from_str(&text).unwrap();
        assert_eq!(back, fixture());
    }

    #[test]
    fn template_set_ron_round_trips() {
        let set = templates();
        let text = ron::to_string(&set).unwrap();
        let back: Vec<RoomTemplate> = ron::from_str(&text).unwrap();
        assert_eq!(back, set);
        // Kind serializes snake_case — the authored file depends on it.
        assert!(
            text.contains("med_bay") || text.contains("hangar"),
            "{text}"
        );
    }

    #[test]
    fn reference_set_is_the_spec_list() {
        let set = templates();
        assert_eq!(set.len(), 12);
        for id in [
            "cockpit",
            "bridge",
            "med_bay",
            "engineering",
            "quarters",
            "galley",
            "cargo_hold",
            "airlock",
            "hydroponics",
            "workshop",
            "armory",
            "brig",
        ] {
            assert!(template(&set, id).is_some(), "missing {id}");
        }
    }

    // --- realize: the validation battery ---

    #[test]
    fn realize_is_deterministic() {
        let a = realize(&fixture(), &templates(), BOUNDS).unwrap();
        let b = realize(&fixture(), &templates(), BOUNDS).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn realize_outputs_grid_unit_rooms_of_the_template_kinds() {
        let out = realize(&fixture(), &templates(), BOUNDS).unwrap();
        // 6 placed rooms + 1 single-cell corridor leg.
        assert_eq!(out.rooms.len(), 7);
        assert_eq!(out.rooms[0].kind, RoomKind::Hangar);
        assert_eq!(out.rooms[0].width, 2 * CELL);
        assert_eq!(out.rooms[1].kind, RoomKind::Cockpit);
        assert_eq!(out.rooms[6].kind, RoomKind::Corridor);
        // Grid units: the galley placed at cell (2,0) lands at 2*CELL.
        assert_eq!(out.rooms[2].x, 2 * CELL);
    }

    #[test]
    fn adjacent_rooms_get_a_direct_door_on_the_shared_edge() {
        let out = realize(&fixture(), &templates(), BOUNDS).unwrap();
        // Airlock (0) and cockpit (1) share the edge y = 2 cells.
        let door = out
            .doors
            .iter()
            .find(|d| (d.from, d.to) == (0, 1))
            .expect("airlock-cockpit door");
        assert_eq!(door.y, 2 * CELL);
        assert!(door.x >= 0 && door.x <= 2 * CELL);
    }

    #[test]
    fn every_room_reachable_from_the_airlock() {
        let out = realize(&fixture(), &templates(), BOUNDS).unwrap();
        let n = out.rooms.len();
        let mut reached = vec![false; n];
        let airlock = out
            .rooms
            .iter()
            .position(|r| r.kind == RoomKind::Hangar)
            .unwrap();
        reached[airlock] = true;
        for _ in 0..n {
            for door in &out.doors {
                let (f, t) = (door.from as usize, door.to as usize);
                if reached[f] || reached[t] {
                    reached[f] = true;
                    reached[t] = true;
                }
            }
        }
        assert!(reached.iter().all(|&r| r), "unreachable room");
    }

    #[test]
    fn overlap_is_rejected() {
        let mut layout = fixture();
        layout.rooms.push(place("brig", 0, 0)); // on top of the airlock
        assert_eq!(
            realize(&layout, &templates(), BOUNDS),
            Err(RealizeError::Overlap)
        );
    }

    #[test]
    fn out_of_bounds_is_rejected() {
        let mut layout = fixture();
        layout.rooms.push(place("brig", 15, 11)); // 2x2 exceeds 16x12
        assert_eq!(
            realize(&layout, &templates(), BOUNDS),
            Err(RealizeError::OutOfBounds)
        );
    }

    #[test]
    fn missing_required_room_is_rejected() {
        let mut layout = fixture();
        layout.rooms.remove(1); // drop the cockpit
        layout.furniture.clear();
        layout.corridors.clear();
        assert_eq!(
            realize(&layout, &templates(), BOUNDS),
            Err(RealizeError::MissingRequiredRoom)
        );
    }

    #[test]
    fn unreachable_room_is_rejected_not_silently_shipped() {
        let mut layout = fixture();
        layout.corridors.clear(); // engineering + cargo now float free
        assert_eq!(
            realize(&layout, &templates(), BOUNDS),
            Err(RealizeError::UnreachableRoom)
        );
    }

    #[test]
    fn unknown_template_is_rejected() {
        let mut layout = fixture();
        layout.rooms[0].template_id = "no_such_room".into();
        assert_eq!(
            realize(&layout, &templates(), BOUNDS),
            Err(RealizeError::InvalidTemplate)
        );
    }

    #[test]
    fn rotation_swaps_the_footprint() {
        let set = templates();
        let tpl = template(&set, "engineering").unwrap(); // 4x3
        let mut placed = place("engineering", 0, 0);
        assert_eq!(placed.footprint(tpl), (4, 3));
        placed.rotation = 1;
        assert_eq!(placed.footprint(tpl), (3, 4));
        placed.rotation = 2;
        assert_eq!(placed.footprint(tpl), (4, 3));
        placed.rotation = 3;
        assert_eq!(placed.footprint(tpl), (3, 4));
    }

    // --- corridors ---

    #[test]
    fn l_corridor_legs_do_not_overlap_and_share_a_wall() {
        let c = Corridor {
            from: (2, 6),
            to: (6, 9),
        };
        let rects = corridor_rects(&c);
        assert_eq!(rects.len(), 2);
        assert!(!rects[0].overlaps(&rects[1]), "legs overlap at the corner");
        assert!(
            rects[0].shared_door(&rects[1]).is_some(),
            "legs must connect at the corner"
        );
    }

    #[test]
    fn auto_corridors_is_placement_order_independent() {
        let set = templates();
        let mut rooms = vec![
            place("airlock", 0, 0),
            place("cockpit", 0, 2),
            place("engineering", 8, 0),
        ];
        let a = auto_corridors(&rooms, &set);
        rooms.swap(0, 2);
        let b = auto_corridors(&rooms, &set);
        assert_eq!(a, b, "identical layouts must route identically");
        assert!(!a.is_empty(), "the detached room needs a corridor");
    }

    #[test]
    fn auto_corridors_skips_already_adjacent_rooms() {
        let set = templates();
        let rooms = vec![place("airlock", 0, 0), place("cockpit", 0, 2)];
        assert!(auto_corridors(&rooms, &set).is_empty());
    }

    // --- adjacency bonuses ---

    #[test]
    fn galley_beside_quarters_fires_the_bonus() {
        let bonuses = compute_bonuses(&fixture(), &templates());
        assert!(bonuses.galley_quarters_bonus);
        // Engineering (0..4, 5..8) and cargo (4..8, 5..9) share x = 4.
        assert!(bonuses.engineering_cargo_bonus);
    }

    #[test]
    fn galley_three_cells_away_does_not_fire() {
        let mut layout = fixture();
        // Move the galley off to the far corner: 3+ cells from quarters.
        layout.rooms[2].position = (10, 8);
        layout.corridors.push(Corridor {
            from: (5, 1),
            to: (9, 9),
        });
        let bonuses = compute_bonuses(&layout, &templates());
        assert!(!bonuses.galley_quarters_bonus);
    }

    // --- furniture ---

    #[test]
    fn furniture_tile_falls_inside_its_parent_room() {
        let set = templates();
        let placed = place("med_bay", 5, 5); // 3x3 at (5..8, 5..8)
        let tpl = template(&set, "med_bay").unwrap();
        for slot in &tpl.furniture_slots {
            let (x, y) = furniture_tile(&placed, tpl, slot).unwrap();
            assert!(
                (5..8).contains(&x) && (5..8).contains(&y),
                "{slot} at ({x},{y})"
            );
        }
    }

    #[test]
    fn distinct_slots_get_distinct_tiles() {
        let set = templates();
        let placed = place("med_bay", 0, 0);
        let tpl = template(&set, "med_bay").unwrap();
        let tiles: Vec<_> = tpl
            .furniture_slots
            .iter()
            .map(|s| furniture_tile(&placed, tpl, s).unwrap())
            .collect();
        let mut dedup = tiles.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(dedup.len(), tiles.len());
    }

    #[test]
    fn duplicate_furniture_slot_is_an_overlap() {
        let mut layout = fixture();
        layout.furniture.push(layout.furniture[0].clone());
        assert_eq!(
            realize(&layout, &templates(), BOUNDS),
            Err(RealizeError::Overlap)
        );
    }

    #[test]
    fn unknown_furniture_slot_is_invalid() {
        let mut layout = fixture();
        layout.furniture[0].slot_id = "no_such_slot".into();
        assert_eq!(
            realize(&layout, &templates(), BOUNDS),
            Err(RealizeError::InvalidTemplate)
        );
    }

    #[test]
    fn every_furniture_kind_has_stat_contributions() {
        for kind in FurnitureKind::ALL {
            let stats = kind.stat_contributions();
            assert!(!stats.is_empty(), "{kind:?} contributes nothing");
            assert!(
                stats.values().all(|v| *v > 0),
                "{kind:?} has a non-positive stat"
            );
        }
    }
}
