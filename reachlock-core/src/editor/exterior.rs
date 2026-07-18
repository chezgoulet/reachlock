//! Exterior editor contracts (spec §19; S17 freeze). `HullConfiguration` is
//! the ship's character sheet: which frame, what hangs on which hardpoint
//! slot, which engine, how much plating, and how it's painted. It goes in
//! the save and (later) the seed ledger diffs, so every field is plain data
//! — items are `ItemRef`s (seed + params, regenerated on demand), paint is
//! palette *slots* resolved at render (spec §19: "Colors are palette
//! references — the generator resolves them on render"), never raw RGB.
//!
//! `HullFrame` is the authored side (content/hulls/*_frame.ron): structural
//! constants — hardpoint slot positions, the engine mount, armor zones,
//! decal slots — that a configuration fills in. Composition and handling
//! derivation live here too so the client preview and flight mode render
//! and fly through the same functions (S17 gotcha: two renderers drift).

use serde::{Deserialize, Serialize};

use crate::generator::hull::{generate_hull_class, HullClass, HullHandling};
use crate::generator::{FixedVec2, GeneratedMesh};
use crate::item::{generate_item, GeneratedItem, ItemSeed, StatKey};
use crate::util::color::{generate_palette, ColorRgba8};
use crate::util::rng::Fixed;

// ---------------------------------------------------------------------
// Frozen configuration contracts (S17 "Freeze first").
// ---------------------------------------------------------------------

/// A reference to an item by its generation inputs, not its generated
/// output. `ItemSeed` already IS "seed + params" (S05 froze it), so this is
/// a transparent newtype: configs stay data, and `generate` re-derives the
/// exact same `GeneratedItem` everywhere.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemRef(pub ItemSeed);

impl ItemRef {
    pub fn generate(&self) -> GeneratedItem {
        generate_item(&self.0)
    }
}

/// Hardpoint size class (spec §19: "Player chooses weapon type, size class,
/// and position"). A slot's class caps what fits; visually it scales the
/// attachment geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    Small,
    Medium,
    Large,
}

impl SizeClass {
    /// Attachment half-extent in whole world units.
    fn half_extent(self) -> i64 {
        match self {
            SizeClass::Small => 3,
            SizeClass::Medium => 5,
            SizeClass::Large => 7,
        }
    }
}

/// A filled hardpoint: which frame slot, what's mounted there. Slots the
/// config doesn't list are empty — `hardpoints` holds *placed* items only
/// (spec §19: `hardpoints: Vec<Hardpoint> // Placed weapons, utilities`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hardpoint {
    pub slot_id: String,
    pub item: ItemRef,
    pub size_class: SizeClass,
}

/// A palette slot reference — which entry of the seed/faction palette a
/// paint layer resolves to. Storing the slot (not the RGB) is the point:
/// the same config repaints itself when the palette context changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaintSlot {
    Primary,
    Accent,
    Structure,
}

impl PaintSlot {
    pub const ALL: [PaintSlot; 3] = [PaintSlot::Primary, PaintSlot::Accent, PaintSlot::Structure];

    pub fn label(self) -> &'static str {
        match self {
            PaintSlot::Primary => "primary",
            PaintSlot::Accent => "accent",
            PaintSlot::Structure => "structure",
        }
    }
}

/// Layer-based paint (spec §19): primary/secondary/accent layers, each a
/// palette slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaintScheme {
    pub primary: PaintSlot,
    pub secondary: PaintSlot,
    pub accent: PaintSlot,
}

impl Default for PaintScheme {
    fn default() -> Self {
        PaintScheme {
            primary: PaintSlot::Primary,
            secondary: PaintSlot::Structure,
            accent: PaintSlot::Accent,
        }
    }
}

/// A decal placed in one of the frame's decal slots. `decal_id` names an
/// insignia (faction id, crew emblem, earned badge) — resolution to pixels
/// is the renderer's job, the config stores only the reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decal {
    pub slot_id: String,
    pub decal_id: String,
}

/// Armor plating over one of the frame's zones. `mass` is fixed-point
/// (1/1024) and feeds `handling` — plating mass slows turn, which is what
/// makes the editor gameplay, not dress-up. S19's visual damage model
/// consumes these segments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmorSegment {
    pub zone_id: String,
    pub mass: i64,
}

/// The exterior configuration (spec §19 struct, with `engine` as an
/// `ItemRef` — engines are S05 `GeneratedItem`s, referenced by seed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HullConfiguration {
    /// References a `content/hulls/*` frame by envelope id.
    pub hull_id: String,
    /// Drives the base hull silhouette and the paint palette.
    pub seed: u64,
    pub hardpoints: Vec<Hardpoint>,
    pub engine: ItemRef,
    pub plating: Vec<ArmorSegment>,
    pub paint: PaintScheme,
    pub decals: Vec<Decal>,
}

// ---------------------------------------------------------------------
// The authored frame (content/hulls/*_frame.ron payload).
// ---------------------------------------------------------------------

/// A hardpoint slot position authored on a frame. `position` is in the same
/// fixed-point hull-local space `GeneratedMesh` vertices use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardpointSlot {
    pub id: String,
    pub position: FixedVec2,
    pub size_class: SizeClass,
}

/// A customizable plating zone on a frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmorZone {
    pub id: String,
}

/// An authored hull frame: the structural constants of one hull class
/// (spec §19: "fixed structural elements ... and customizable zones").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HullFrame {
    pub class: HullClass,
    pub slots: Vec<HardpointSlot>,
    pub engine_mount: FixedVec2,
    pub zones: Vec<ArmorZone>,
    pub decal_slots: Vec<String>,
    /// S18: interior placement area in cells (spec §19: "Hull class
    /// determines available grid area"). Defaulted so pre-S18 authored
    /// frames keep deserializing — an additive protocol revision (iron
    /// rule #4), noted in the S18 PR.
    #[serde(default = "default_grid_bounds")]
    pub grid_bounds: (u8, u8),
}

/// The corvette-class placement area — the fallback for frames authored
/// before `grid_bounds` existed.
fn default_grid_bounds() -> (u8, u8) {
    (16, 12)
}

impl HullFrame {
    /// The canonical built-in frame per class: the offline-first fallback
    /// when no authored frame is loaded, and the determinism manifest's
    /// fixture. Pure data, no RNG — authored frames override it.
    pub fn reference(class: HullClass) -> HullFrame {
        let v = |x: i64, y: i64| FixedVec2 {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
        };
        let slot = |id: &str, x: i64, y: i64, size_class: SizeClass| HardpointSlot {
            id: id.into(),
            position: v(x, y),
            size_class,
        };
        let zone = |id: &str| ArmorZone { id: id.into() };
        match class {
            HullClass::Shuttle => HullFrame {
                class,
                slots: vec![slot("chin", 20, 0, SizeClass::Small)],
                engine_mount: v(-24, 0),
                zones: vec![zone("nose"), zone("belly")],
                decal_slots: vec!["tail".into()],
                grid_bounds: (10, 8),
            },
            HullClass::Freighter => HullFrame {
                class,
                slots: vec![
                    slot("dorsal", 0, 40, SizeClass::Medium),
                    slot("ventral", 0, -40, SizeClass::Medium),
                ],
                engine_mount: v(-52, 0),
                zones: vec![
                    zone("bow"),
                    zone("cargo_port"),
                    zone("cargo_starboard"),
                    zone("stern"),
                ],
                decal_slots: vec!["bow".into(), "flank".into()],
                grid_bounds: (22, 16),
            },
            // Corvette is the player default; Station/Rock frames exist so
            // `reference` is total, but nothing edits them today.
            _ => HullFrame {
                class,
                slots: vec![
                    slot("nose", 32, 0, SizeClass::Small),
                    slot("wing_port", -8, 30, SizeClass::Small),
                    slot("wing_starboard", -8, -30, SizeClass::Medium),
                ],
                engine_mount: v(-36, 0),
                zones: vec![zone("nose"), zone("port"), zone("starboard")],
                decal_slots: vec!["nose".into(), "tail".into()],
                grid_bounds: default_grid_bounds(),
            },
        }
    }

    pub fn slot(&self, id: &str) -> Option<&HardpointSlot> {
        self.slots.iter().find(|s| s.id == id)
    }
}

// ---------------------------------------------------------------------
// Composition: HullConfiguration → mesh + resolved paint.
// ---------------------------------------------------------------------

/// Paint resolved through the seed palette — render-time colors, never
/// stored in the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedPaint {
    pub primary: ColorRgba8,
    pub secondary: ColorRgba8,
    pub accent: ColorRgba8,
}

/// Resolve a paint scheme against the palette the seed generates. The
/// flight scene and the editor preview both call this — same seed, same
/// coat of paint.
pub fn resolve_paint(scheme: &PaintScheme, seed: u64) -> ResolvedPaint {
    let palette = generate_palette(seed);
    let pick = |slot: PaintSlot| match slot {
        PaintSlot::Primary => palette.primary,
        PaintSlot::Accent => palette.accent,
        PaintSlot::Structure => palette.structure,
    };
    ResolvedPaint {
        primary: pick(scheme.primary),
        secondary: pick(scheme.secondary),
        accent: pick(scheme.accent),
    }
}

/// A composed exterior: the mesh with attachments welded on, plus the
/// resolved paint. This is the ONE composition path — the editor preview
/// and the flight-mode ship must both render exactly this (S17 gotcha).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedHull {
    pub mesh: GeneratedMesh,
    pub paint: ResolvedPaint,
}

/// Compose the exterior: base hull silhouette from `(seed, frame.class)`,
/// a diamond attachment per filled hardpoint slot (scaled by the slot's
/// size class), an engine nozzle at the frame's mount (scaled by the engine
/// item's thrust stat, so the engine choice is visible), and the paint
/// resolved through the seed palette. Deterministic in `(config, frame)`;
/// hardpoints referencing unknown slot ids are skipped.
pub fn compose_hull(config: &HullConfiguration, frame: &HullFrame) -> ComposedHull {
    let mut mesh = generate_hull_class(config.seed, frame.class);

    for hardpoint in &config.hardpoints {
        let Some(slot) = frame.slot(&hardpoint.slot_id) else {
            continue;
        };
        let size = hardpoint.size_class.min(slot.size_class);
        append_diamond(
            &mut mesh,
            slot.position,
            Fixed::from_int(size.half_extent()),
        );
    }

    // Engine nozzle: half-extent grows with the thrust stat (whole units,
    // integer division — no floats).
    let engine = config.engine.generate();
    let thrust_whole = engine.stats.0.get(&StatKey::Thrust).copied().unwrap_or(0) / 1024;
    let nozzle = Fixed::from_int(3 + thrust_whole / 24);
    append_diamond(&mut mesh, frame.engine_mount, nozzle);

    ComposedHull {
        mesh,
        paint: resolve_paint(&config.paint, config.seed),
    }
}

/// Weld a small diamond (4 vertices, 2 triangles) onto the mesh at `at`.
fn append_diamond(mesh: &mut GeneratedMesh, at: FixedVec2, half: Fixed) {
    let base = mesh.vertices.len() as u32;
    let (x, y, h) = (at.x.0, at.y.0, half.0);
    mesh.vertices.extend([
        FixedVec2 {
            x: Fixed(x + h),
            y: Fixed(y),
        },
        FixedVec2 {
            x: Fixed(x),
            y: Fixed(y + h),
        },
        FixedVec2 {
            x: Fixed(x - h),
            y: Fixed(y),
        },
        FixedVec2 {
            x: Fixed(x),
            y: Fixed(y - h),
        },
    ]);
    mesh.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

// ---------------------------------------------------------------------
// Handling derivation: the config flies differently than the bare class.
// ---------------------------------------------------------------------

/// Derive flight handling from a configuration (S17 deliverable; S09 froze
/// `HullHandling`). The class baseline comes from `HullHandling::for_class`
/// — then the engine model sets thrust and burn, and plating mass slows
/// turn. All integer math on fixed-point values.
pub fn handling(config: &HullConfiguration, frame: &HullFrame) -> HullHandling {
    let mut h = HullHandling::for_class(config.seed, frame.class);

    // Engine: thrust/turn stats are fixed-point whole-unit bands (S05).
    let engine = config.engine.generate();
    let stat = |key: StatKey| engine.stats.0.get(&key).copied().unwrap_or(0) / 1024;
    let thrust_whole = stat(StatKey::Thrust);
    h.thrust += thrust_whole * 8;
    h.turn_rate += stat(StatKey::Turn) * 4;
    h.mass += stat(StatKey::Weight) * 10;
    // A bigger engine drinks more (never less than the class baseline).
    h.fuel_burn += thrust_whole / 12;

    // Plating: mass is fixed-point 1/1024 per segment.
    let plating_whole: i64 = config.plating.iter().map(|s| s.mass / 1024).sum();
    h.mass += plating_whole * 20;
    h.turn_rate = (h.turn_rate - plating_whole * 2).max(20);

    h
}

#[cfg(test)]
mod hull_config {
    use super::*;
    use crate::item::{EquipmentKind, ItemType};

    fn engine(seed: u64, tier: u8) -> ItemRef {
        ItemRef(ItemSeed {
            seed,
            item_type: ItemType::Equipment(EquipmentKind::Engine),
            tier,
            faction: "compact".into(),
            biome: "frontier".into(),
        })
    }

    fn config() -> HullConfiguration {
        HullConfiguration {
            hull_id: "frame_corvette".into(),
            seed: 42,
            hardpoints: vec![Hardpoint {
                slot_id: "nose".into(),
                item: ItemRef(ItemSeed {
                    seed: 7,
                    item_type: ItemType::from_token("kinetic_cannon").unwrap(),
                    tier: 3,
                    faction: "compact".into(),
                    biome: "frontier".into(),
                }),
                size_class: SizeClass::Small,
            }],
            engine: engine(11, 4),
            plating: vec![ArmorSegment {
                zone_id: "nose".into(),
                mass: 8 * 1024,
            }],
            paint: PaintScheme::default(),
            decals: vec![Decal {
                slot_id: "tail".into(),
                decal_id: "compact".into(),
            }],
        }
    }

    /// Iron rule #4: the serialized form is pinned — `HullConfiguration`
    /// goes in the save and the seed ledger diffs. If this JSON changes,
    /// that's a save-format revision: update deliberately and note it.
    #[test]
    fn wire_shape_is_pinned() {
        let json = serde_json::to_string(&config()).unwrap();
        assert_eq!(
            json,
            r#"{"hull_id":"frame_corvette","seed":42,"hardpoints":[{"slot_id":"nose","item":{"seed":7,"item_type":{"equipment":{"weapon":{"kinetic":"cannon"}}},"tier":3,"faction":"compact","biome":"frontier"},"size_class":"small"}],"engine":{"seed":11,"item_type":{"equipment":"engine"},"tier":4,"faction":"compact","biome":"frontier"},"plating":[{"zone_id":"nose","mass":8192}],"paint":{"primary":"primary","secondary":"structure","accent":"accent"},"decals":[{"slot_id":"tail","decal_id":"compact"}]}"#
        );
        let back: HullConfiguration = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config());
    }

    /// RON is the authoring/save format — the round trip must hold there
    /// too (enum variant syntax is where typos live).
    #[test]
    fn ron_round_trip() {
        let text = ron::to_string(&config()).unwrap();
        let back: HullConfiguration = ron::from_str(&text).unwrap();
        assert_eq!(back, config());
    }

    #[test]
    fn frame_wire_shape_round_trips() {
        let frame = HullFrame::reference(HullClass::Corvette);
        let text = ron::to_string(&frame).unwrap();
        let back: HullFrame = ron::from_str(&text).unwrap();
        assert_eq!(back, frame);
        // Class serializes snake_case — the authored files depend on it.
        assert!(text.contains("corvette"), "got: {text}");
    }

    // --- composition ---

    #[test]
    fn compose_is_deterministic() {
        let frame = HullFrame::reference(HullClass::Corvette);
        assert_eq!(
            compose_hull(&config(), &frame),
            compose_hull(&config(), &frame)
        );
    }

    #[test]
    fn hardpoints_and_engine_add_geometry() {
        let frame = HullFrame::reference(HullClass::Corvette);
        let mut bare = config();
        bare.hardpoints.clear();
        let base = compose_hull(&bare, &frame);
        let full = compose_hull(&config(), &frame);
        // One filled hardpoint = one diamond = 4 vertices / 6 indices more
        // than the bare config (which still carries the engine nozzle).
        assert_eq!(full.mesh.vertices.len(), base.mesh.vertices.len() + 4);
        assert_eq!(full.mesh.indices.len(), base.mesh.indices.len() + 6);
        // And the bare config still has the nozzle over the raw class hull.
        let raw = generate_hull_class(42, HullClass::Corvette);
        assert_eq!(base.mesh.vertices.len(), raw.vertices.len() + 4);
    }

    #[test]
    fn unknown_slot_is_skipped_not_a_panic() {
        let frame = HullFrame::reference(HullClass::Corvette);
        let mut cfg = config();
        cfg.hardpoints[0].slot_id = "no_such_slot".into();
        let composed = compose_hull(&cfg, &frame);
        let mut bare = config();
        bare.hardpoints.clear();
        assert_eq!(composed.mesh, compose_hull(&bare, &frame).mesh);
    }

    #[test]
    fn attachment_lands_on_its_slot() {
        let frame = HullFrame::reference(HullClass::Corvette);
        let composed = compose_hull(&config(), &frame);
        let slot = frame.slot("nose").unwrap();
        // The first appended vertex after the base hull is the diamond's
        // +x point at the slot position.
        let base_len = generate_hull_class(42, HullClass::Corvette).vertices.len();
        let v = composed.mesh.vertices[base_len];
        assert_eq!(v.y.0, slot.position.y.0);
        assert!(v.x.0 > slot.position.x.0);
    }

    #[test]
    fn paint_resolves_through_the_seed_palette_not_raw_rgb() {
        let seed = 42;
        let palette = generate_palette(seed);
        let resolved = resolve_paint(&PaintScheme::default(), seed);
        assert_eq!(resolved.primary, palette.primary);
        assert_eq!(resolved.secondary, palette.structure);
        assert_eq!(resolved.accent, palette.accent);
        // Repainting = picking different slots, same palette.
        let flipped = PaintScheme {
            primary: PaintSlot::Structure,
            secondary: PaintSlot::Primary,
            accent: PaintSlot::Accent,
        };
        assert_eq!(resolve_paint(&flipped, seed).primary, palette.structure);
    }

    // --- handling: the direction of every tradeoff ---

    #[test]
    fn handling_is_deterministic() {
        let frame = HullFrame::reference(HullClass::Corvette);
        assert_eq!(handling(&config(), &frame), handling(&config(), &frame));
    }

    #[test]
    fn heavier_plating_adds_mass_and_slows_turn() {
        let frame = HullFrame::reference(HullClass::Corvette);
        let light = config();
        let mut heavy = config();
        heavy.plating = vec![
            ArmorSegment {
                zone_id: "nose".into(),
                mass: 40 * 1024,
            },
            ArmorSegment {
                zone_id: "port".into(),
                mass: 40 * 1024,
            },
        ];
        let hl = handling(&light, &frame);
        let hh = handling(&heavy, &frame);
        assert!(hh.mass > hl.mass, "plating adds mass");
        assert!(hh.turn_rate < hl.turn_rate, "plating slows turn");
        // Engine untouched: thrust and burn stay put.
        assert_eq!(hh.thrust, hl.thrust);
        assert_eq!(hh.fuel_burn, hl.fuel_burn);
    }

    #[test]
    fn bigger_engine_raises_thrust_and_burn() {
        let frame = HullFrame::reference(HullClass::Corvette);
        let small = config(); // tier-4 engine
        let mut big = config();
        big.engine = engine(11, 10);
        let hs = handling(&small, &frame);
        let hb = handling(&big, &frame);
        // Tier bands don't overlap 4→10 (growth 10/tier): strictly more
        // thrust, and the burn scales with it.
        assert!(hb.thrust > hs.thrust, "bigger engine, more thrust");
        assert!(hb.fuel_burn > hs.fuel_burn, "bigger engine, more burn");
        assert!(hb.mass > hs.mass, "bigger engine weighs more");
    }

    #[test]
    fn engine_choice_changes_the_composed_nozzle() {
        let frame = HullFrame::reference(HullClass::Corvette);
        let mut big = config();
        big.engine = engine(11, 10);
        assert_ne!(
            compose_hull(&config(), &frame).mesh,
            compose_hull(&big, &frame).mesh,
            "a tier-10 engine's nozzle reads bigger than a tier-4's"
        );
    }

    #[test]
    fn turn_rate_never_collapses_below_floor() {
        let frame = HullFrame::reference(HullClass::Freighter);
        let mut cfg = config();
        cfg.hull_id = "frame_freighter".into();
        cfg.plating = (0..12)
            .map(|i| ArmorSegment {
                zone_id: format!("z{i}"),
                mass: 100 * 1024,
            })
            .collect();
        assert!(handling(&cfg, &frame).turn_rate >= 20);
    }
}
