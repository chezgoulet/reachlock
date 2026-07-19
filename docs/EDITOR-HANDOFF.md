# ReachLock Content Editor Suite — Fable Handoff

This document is the complete, self-contained specification for Claude Code
(Fable) to build the ReachLock content editor suite. Feed it into a fresh
Claude Code session with:

    claude "Read docs/EDITOR-HANDOFF.md and execute Phase 1 in order."

## Overview

The content editor is an egui/eframe 0.31 desktop application at
`reachlock-editor/`. It has a working shell: tab system, file menus, content
browser sidebar, seed workflow panel. Two editors are fully implemented
(hull, soul). Eight are 59-line stubs returning `Err("not yet implemented")`.
Six more have RON files + schemas but no editor at all. Four more are new
domains needing both data model and editor. Two are procedural previewers.

**Total: 22 editors + 1 AI system across 4 phases.**

## Quick Reference

- **Editor trait:** `reachlock-editor/src/app.rs:64-73`
- **IO helpers:** `reachlock-editor/src/io.rs` — `read_ron::<T>(path)`, `write_ron(path, &value)`
- **Reference editors:** `src/editors/hull.rs` (277 lines), `src/editors/soul.rs` (305 lines)
- **Core data types:** `reachlock-core/src/` — content/, generator/, soul/, item/, combat/, faction/, economy/, galaxy/, editor/, contract/
- **RON examples:** `mods/reachlock/` — stations/, souls/, systems/, hulls/, combat/, factions/, economy/, storylines/, gate_network/
- **JSON schemas:** `mods/reachlock/schemas/` — 11 schema files
- **Verification:** `cargo build -p reachlock-editor` must pass after every editor

## Architecture

Every editor implements the `Editor` trait:

```rust
pub trait Editor {
    fn title(&self) -> &str;                               // tab label
    fn content_type(&self) -> ContentType;                  // filesystem directory
    fn has_unsaved_changes(&self) -> bool;                  // asterisk on tab
    fn load(&mut self, path: &Path) -> Result<(), String>;  // crate::io::read_ron
    fn save(&self, path: &Path) -> Result<(), String>;      // crate::io::write_ron
    fn validate(&self) -> Vec<String>;                      // inline field errors
    fn ui(&mut self, ctx: &egui::Context);                  // the editor layout
    fn generate_from_seed(&mut self, seed: u64);            // SeededRng fill
}
```

## Universal UX Pattern

Every editor follows the RPG Maker database editor layout:

1. **Top toolbar** (`TopBottomPanel::top`):
   - "Generate from Seed" button (procedurally fills all fields)
   - "New" button (blank slate with defaults)
   - Filename display
   - Modified indicator (`*`)

2. **Left panel** (`SidePanel::left`, resizable, default 200px):
   - Searchable, scrollable list of entries by name/id
   - Click to select, right-click menu (Duplicate, Delete)
   - Add/Remove entry buttons in toolbar

3. **Center panel** (`CentralPanel`):
   - Property form for the selected entry
   - `CollapsingHeader` sections with descriptive headers
   - `Grid` layout for label:value pairs
   - Inline validation errors (red text next to offending field)

4. **Widget mapping per field type:**
   - Enums → `ComboBox` with `.selected_text()`
   - Integers → `DragValue` with `.clamp_range()`
   - Floats → `DragValue` with `.speed(0.01)`
   - Short strings → `TextEdit::singleline`
   - Long strings → `TextEdit::multiline` (min 3 visible rows)
   - Booleans → `Checkbox`
   - Colors → 4 `DragValue<u8>` (0..=255) for RGBA, with a color preview swatch

## The Procedural-Then-Edit Workflow

Every editor supports both creation paths:

1. **Generate from Seed** — fills all fields procedurally using `SeededRng`.
   Then the author edits any field manually. The `generate_from_seed()` method
   is the entry point.

2. **Create Wholecloth** — starts with sensible defaults (empty strings, zero
   numbers, first enum variant). The author builds everything from scratch.

Both paths produce the same saveable RON output. The editor doesn't care how
the data arrived.

## Species System

The canonical species enum has **5 variants** (not 3):

```rust
pub enum Species {
    Human,      // includes cybernetically enhanced humans
    Android,    // any synthetic humanoid
    Robot,      // industrial/non-humanoid synthetics
    Voidborn,   // space-dwelling creatures, Predecessor lore, special events
    Xenotype,   // planetary ecosystem creatures, planet-bound life
}
```

The generator strings "Augmented" and "Synthetic" fold into `Human` (they are
body-modification states, not separate species). The core `Species` enum in
`reachlock-core/src/soul/types.rs` and the client `BodyKind` in
`reachlock-client/src/pixel.rs` both use these 5 variants.

---

## Phase 0 — Prerequisites (Human Must Complete Before Fable)

Fable cannot modify `reachlock-core` or `reachlock-client` (they are outside
the editor crate's scope). These changes must be done before Phase 1:

### 0.1 Expand core Species enum

**File:** `reachlock-core/src/soul/types.rs:12-21`

Change from 3 variants to 5:
```rust
pub enum Species {
    Human,
    Android,
    Robot,
    Voidborn,
    Xenotype,
}
```

Update the doc comment. The existing `as_str()` methods are in downstream
modules — update them to include `"voidborn"` and `"xenotype"`.

### 0.2 Expand client BodyKind enum

**File:** `reachlock-client/src/pixel.rs:273-278`

Change from 3 variants to 5. Add `paint_voidborn()` and `paint_xenotype()`
functions (or extend `paint_character()` with species-aware proportions from
the sprite generator). Update `Look::seeded()` to potentially roll
Voidborn/Xenotype civilian NPCs (~5% chance each).

### 0.3 Update generator species mapping

**File:** `reachlock-core/src/generator/sprite.rs`
**File:** `reachlock-core/src/generator/soul.rs`

Wire the 5-species enum into the generators. The sprite generator already has
5 body-proportion sets — map them to enum variants instead of loose strings.
The soul generator already has 5 name tables and backstory tables — same
mapping.

### 0.4 Update agency and cryo

**File:** `reachlock-core/src/agency.rs:378-386`
Add agency behavior: Voidborn → unpredictable (randomizes responses).
Xenotype → instinct-driven (follows ecosystem triggers, ignores complex orders).

**File:** `reachlock-client/src/systems/cryojump.rs:85`
Update species checks: Voidborn don't need cryo pods (space-dwelling).
Xenotypes need specialized environment pods.

### 0.5 Update authored .ron files

All existing `.ron` files use `species: human`, `species: android`, or
`species: robot` — no changes needed since those variants still exist.
No new `.ron` files need to be created for Voidborn/Xenotype unless authors
want to create them.

---

## Phase 1 — 14 File-Based Editors (Strict Dependency Order)

Each editor reads/writes `.ron` files under `mods/reachlock/<directory>/`.
Every editor registers itself in `src/editors/mod.rs` via `register_all()` and
in `src/app.rs` via `build_default_registry()` using a factory function
`pub fn create_editor() -> Box<dyn Editor>`.

### Registration Checklist Per Editor

- [ ] Create `src/editors/<name>.rs`
- [ ] Add `pub mod <name>;` to `src/editors/mod.rs`
- [ ] Implement `pub fn create_editor() -> Box<dyn Editor>` at bottom of file
- [ ] Register in `src/editors/mod.rs::register_all()`
- [ ] Register in `src/app.rs::build_default_registry()` if new ContentType
- [ ] Verify `cargo build -p reachlock-editor` passes
- [ ] Verify `cargo run -p reachlock-editor` launches and editor is accessible

### Dependency Graph

```
HullFrame
    ├─→ HullMesh
    │       └─→ Station
    └─→ RoomTemplates
            └─→ Location
                    │
EnemyArchetype ─────┘
                    │
ChartedSystem ──────┤
                    │
EconomyGoods ───────┤
                    │
Faction ──→ Storyline ──→ Soul
                    │         │
Contract ───────────┘         │
                    │         │
Item ───────────────┘         │
                    │         │
GateNetwork ←───────┴─────────┘
```

---

### 1. Hull Frame Editor

**File:** `src/editors/hull_frame.rs` (new)
**Data type:** `HullFrame` in `reachlock-core/src/editor/exterior.rs`
**RON examples:** `mods/reachlock/hulls/corvette_frame.ron`, `freighter_frame.ron`, `shuttle_frame.ron`
**Schema:** `mods/reachlock/schemas/hull_frame.schema.json`
**Dependencies:** None

**Fields per frame:**
- `class`: ComboBox — Shuttle, Freighter, Corvette, Station, Rock
- `slots`: list of `HardpointSlot`. Per slot: `id` (String), `size_class` (ComboBox: Small/Medium/Large), `position` (two DragValue<f32>, converts to/from Fixed ×1024)
- `engine_mount`: two DragValue<f32> for (x, y)
- `zones`: list of `ArmorZone`. Per zone: `id` (String), `position` (x, y DragValues)
- `decal_slots`: list of String fields (add/remove buttons, each row is a text field)
- `grid_bounds`: two DragValue<u8> for (width, height), range 4..=32

**generate_from_seed:** picks a random class, generates 2-6 slots with random positions, 2-4 zones, 1-3 decal slots, grid bounds proportional to class (Shuttle 8×6, Corvette 16×12, Freighter 20×16, Station 32×24, Rock 12×8).

**Left panel:** list of frames by class name. Right panel: property form for selected.

---

### 2. Enemy Archetype Editor

**File:** `src/editors/enemy.rs` (replace existing stub)
**Data type:** `HostileArchetype` in `reachlock-core/src/combat/humanoid.rs`
**RON examples:** `mods/reachlock/combat/raider_melee.ron`, `raider_gunner.ron`, `raider_boss.ron`, `security_bot.ron`
**Schema:** `mods/reachlock/schemas/hostile.schema.json`
**Dependencies:** None

**Fields:**
- `id`, `display_name`: singleline text
- `hp`, `speed`, `chase_radius`, `disengage_radius`, `flee_hp_frac`: DragValues with appropriate ranges
- `light_attack` / `heavy_attack`: CollapsingHeader sections. Each contains `startup_ticks`, `active_ticks`, `recovery_ticks` (DragValue<u32>), `damage` (DragValue<i64> fixed-point), `range` (DragValue<i64>)
- `block`: CollapsingHeader — `active_ticks`, `cooldown_ticks`, `parry_ticks`
- `dodge`: CollapsingHeader — `i_frame_ticks`, `recovery_ticks`, `distance`

**generate_from_seed:** randomizes all numeric fields within sensible bands. Seed parity determines light/fast vs heavy/slow archetype. Raider archetypes use HP 4000-16000, speed 64-256. Bot archetypes use HP 8000-32000, speed 32-128.

**Left panel:** list by display_name. Right panel: CollapsingHeader sections for each attack/defense group.

---

### 3. Charted System Editor

**File:** `src/editors/charted_system.rs` (new)
**Data type:** `ChartedSystem` in `reachlock-core/src/galaxy/mod.rs`
**RON examples:** `mods/reachlock/systems/*.ron` (8 files: aethon, cadence, sorrow, verne, earth, the_veil, fringe_a, fringe_b)
**Schema:** `mods/reachlock/schemas/charted_system.schema.json`
**Dependencies:** None

**Fields:**
- `id`, `display_name`: singleline text
- `position`: three DragValue<i64> for (x, y, z) — GalaxyCoord, range -32768..=32767
- `biome`: ComboBox — Core, Frontier, Nebula, Derelict, DeepSpace
- `seed`: DragValue<u64> (range 0..=2^53-1)
- `description`: multiline text, minimum 4 rows

**generate_from_seed:** picks a random biome, generates position within ±2000 range per axis, fills description with biome-appropriate template text.

**Left panel:** list of systems by display_name, sorted alphabetically.

---

### 4. Hull Mesh Editor

**File:** `src/editors/hull_mesh.rs` (new)
**Data type:** `ContentFile` with `ContentPayload::Hull` wrapping `GeneratedMesh`
**RON examples:** `mods/reachlock/hulls/loup_garou.ron`
**Schema:** `mods/reachlock/schemas/hull.schema.json`
**Dependencies:** HullFrame (for reference composition preview)

**Fields:**
- Standard envelope: `id`, `display_name`, `seed`, `universe` (text — usually "all"), `priority` (ComboBox: Procedural/Curated/Event/Authoritative)
- Payload — GeneratedMesh: vertices shown as a read-only table (columns: index, x, y). Indices shown as read-only triangle list (columns: index, v0, v1, v2). "Regenerate" button calls `generate_hull_class(seed, class)` to replace the mesh procedurally. "Compose Preview" button runs `compose_hull()` against the reference frame and shows a summary.

**generate_from_seed:** calls `generate_hull_class(seed, class)` with a randomly picked HullClass.

**Left panel:** list of hulls by id. One entry loaded from disk.

---

### 5. Room Templates Editor

**File:** `src/editors/room_templates.rs` (new)
**Data type:** `ContentFile` with `ContentPayload::RoomTemplates` wrapping `Vec<RoomTemplate>`
**RON examples:** `mods/reachlock/hulls/room_templates.ron`
**Schema:** `mods/reachlock/schemas/room_template.schema.json`
**Dependencies:** None

**Fields per template:**
- `id`, `label`: singleline text
- `kind`: ComboBox — all 16 RoomKind variants (Hangar, Corridor, Quarters, Bar, Market, Shipyard, Reactor, Bridge, Cockpit, TechBay, Scanner, MedBay, Cryo, Hydroponics, Armory, Brig)
- `width`, `height`: DragValue<u8>, range 1..=16
- `required_systems`: list of String fields (add/remove buttons)
- `furniture_slots`: list of String fields (add/remove)
- `adjacent_pairs`: list of (String, String) pairs. Each pair row has two text fields + a remove button. "Add Pair" button.

**generate_from_seed:** calls `RoomTemplate::reference_set()` — no procedural generation. The 12 canon templates are the defaults.

**Left panel:** list of templates by label. Right panel: property form for selected. Toolbar: Add Template, Remove Template buttons.

---

### 6. Economy Goods Editor

**File:** `src/editors/economy.rs` (replace existing stub)
**Data type:** `GoodsCatalog` in `reachlock-core/src/economy.rs`
**RON examples:** `mods/reachlock/economy/goods.ron`, `examples/mods/duskway_pack/economy/trade_tweak.ron`
**Schema:** None (flat struct, no published schema — create one if desired)
**Dependencies:** None

**Fields per good:**
- `id`: singleline text (acts as key in BTreeMap)
- `name`: singleline text
- `base_price`: DragValue<i64>, min 0
- `mass`: DragValue<i64>, min 0
- `category`: ComboBox — Consumable, Fuel, Material, Manufactured, Medical, Luxury, Contraband
- `contraband`: Checkbox

**generate_from_seed:** not applicable — goods are hand-authored economics. The "Generate" button seeds the 12 canon goods from `goods.ron` as defaults.

**Left panel:** list of goods by name. Right panel: property form. Toolbar: Add Good, Remove Good.

---

### 7. Faction Editor

**File:** `src/editors/faction.rs` (replace existing stub)
**Data type:** `FactionCatalog` in `reachlock-core/src/faction.rs`
**RON examples:** `mods/reachlock/factions/canon.ron`
**Schema:** `mods/reachlock/schemas/faction.schema.json`
**Dependencies:** None

**Fields per faction:**
- `id`: singleline text (the FactionId newtype — stored as "compact" etc., no parentheses in editor)
- `name`: singleline text
- `color`: 4 DragValue<u8> (R, G, B, A) with a 32×32 color swatch preview using `ui.colored_label()`
- `doctrine`: ComboBox — Military, Economic, Diplomatic, Expansionist
- `tariff_policy`: ComboBox switching between variants:
  - Regulated → shows foreign_mult + own_mult DragValues
  - Flat → shows mult DragValue
  - Dynamic → label "adjusts with demand"
  - None → label "no tariffs"
- `produces`: list of ComboBoxes (GoodCategory variants), add/remove buttons
- `territory`: list of SystemClaim. Per claim: `system_id` text (match charted system IDs), `control` DragValue<u8> (0..=100)
- `internal_divisions`: list of InternalDivision. Per division: `id` text, `name` text, `influence` DragValue<f32> (0.0..=1.0), `agenda` ComboBox (Hawkish/Dovish/Mercantile/Isolationist), `player_standing` DragValue<i8> (-100..=100)
- `relationships`: sub-table. Columns: Target Faction, Status ComboBox (Allied/Friendly/Neutral/Hostile/War), Treaty text, War Goal text. "Add Relationship" button. Each row is a faction from the catalog not yet showing in the relationship map.

**generate_from_seed:** creates 1 faction with randomized doctrine, tariff policy, produces 1-2 goods, random color, 0-2 territory claims.

**Left panel:** list of factions by name with colored dot indicator. Right panel: property form for selected.

---

### 8. Station Editor

**File:** `src/editors/station.rs` (replace existing stub)
**Data type:** `ContentFile` with `ContentPayload::Station { exterior, layout, npc_spawns }`
**RON examples:** `mods/reachlock/stations/sorrow_station.ron`, `examples/mods/duskway_pack/stations/duskway_hub.ron`
**Schema:** `mods/reachlock/schemas/station.schema.json`
**Dependencies:** HullFrame (for exterior mesh reference), RoomKind (for layout rooms)

**Fields:**
- Standard envelope: `id`, `display_name`, `seed`, `universe`, `priority`
- **Exterior section** (CollapsingHeader): read-only mesh info (vertex count, index count, bounding box). "Regenerate Exterior" button calls the station generator.
- **Layout section** (CollapsingHeader): rooms list. Per room: `kind` (ComboBox of RoomKind), `x`, `y`, `width`, `height` (DragValue<i32>). Doors list: per door: `from` (ComboBox of room indices), `to` (ComboBox), `x`, `y` (DragValue<i32>). "Add Room" and "Add Door" buttons.
- **NPC Spawns section** (CollapsingHeader): list. Per spawn: `room_index` (DragValue<usize>, range 0..=room_count-1), `name` (singleline text), `dialogue` list (add/remove String fields, one per line the NPC says). "Add NPC" button.

**generate_from_seed:** generates full station: exterior mesh, spine-corridor layout with 4-8 rooms, 1-2 NPC spawns with dialogue.

**Left panel:** list of stations by display_name.

---

### 9. Location Editor

**File:** `src/editors/location.rs` (replace existing stub)
**Data type:** `HostileLocation` in `reachlock-core/src/combat/location.rs`
**RON examples:** `mods/reachlock/locations/derelict_hold.ron`
**Schema:** None (flat struct)
**Dependencies:** EnemyArchetype (for spawn archetype references)

**Fields:**
- `id`, `display_name`: singleline text
- `rooms` section (CollapsingHeader): list of HostileRoom. Per room:
  - `id`: text
  - `width`, `height`: DragValue<u32>, range 4..=64
  - `kind`: text field (freeform: "empty", "corridor", "arena", "boss", "reward")
  - `spawns` sub-list: per spawn: `archetype` text (matches enemy ID), `pos` two DragValue<i64>, `patrol` list of (i64, i64) waypoint pairs (add/remove). "Add Spawn" button.
  - `props` sub-list: per prop: `kind` text, `pos` two DragValue<i64>. "Add Prop" button.
  - "Add Room" button at bottom
- `connections` section: list of (String, String) pairs. Each row: two text fields (room IDs) + remove button. "Add Connection" button.
- `keycard` section: Checkbox to enable. When enabled: `door` two text fields (room pair), `key_name` text.

**generate_from_seed:** generates 3-6 rooms with random dimensions, spawns pulling from known enemy IDs, 2-5 connections, optional keycard.

**Left panel:** list of locations by display_name.

---

### 10. Item Editor

**File:** `src/editors/item.rs` (replace existing stub)
**Data type:** `ItemSeed` → `GeneratedItem` in `reachlock-core/src/item/types.rs`
**RON examples:** None (procedural only — this editor creates the first authored items)
**Schema:** None
**Dependencies:** ItemType hierarchy (5 levels, 52 leaf types)

**This is the most structurally complex editor.** The ItemType has a 5-level hierarchy:

```
ItemType
├─ Equipment
│  ├─ Weapon
│  │  ├─ Energy → Laser, Plasma, Tachyon
│  │  ├─ Kinetic → Cannon, Railgun, Autocannon
│  │  ├─ Missile → Torpedo, Standard, Decoy
│  │  ├─ Melee → Blade, Baton, ArcWelder
│  │  └─ Boarding → BreachingCharge, SuppressionTool
│  ├─ Armor, Shield, Engine, Sensor, MiningTool, RepairTool
│  ├─ Cybernetic, Augmentation, Spacesuit
├─ Consumable → Medkit, RepairPack, Ammunition, FuelCell, BatteryPack,
│               Booster, Grenade, Mine, DeployableCover, DataShard
├─ Component → Hardpoint, HullPlating, ArmorSegment, PowerPlant,
│              Capacitor, JumpDriveComponent, CraftingMaterial, RefinedOre
├─ Implant → NeuralLace, DroidInterface, MemoryUpgrade, FactionSpecific
└─ Cosmetic → Costume, Hat, ShipPaint, Decal, CrewOutfit, PortraitFrame, InteriorDecoration
```

**UX — cascading ComboBox chain:**
Row 1: ComboBox for top-level (Equipment / Consumable / Component / Implant / Cosmetic)
Row 2: appears based on Row 1 selection (Weapon / Armor / ... / etc.)
Row 3: appears based on Row 2 selection (if Weapon was chosen: Energy / Kinetic / Missile / Melee / Boarding)
Row 4: appears based on Row 3 (leaf type: Laser / Plasma / Tachyon / etc.)

**Form below the type picker:**
- `seed`: DragValue<u64>
- `tier`: DragValue<u8> (1..=10)
- `faction`: singleline text (default "compact")
- `biome`: singleline text (default "frontier")
- "Generate Preview" button: calls `ItemSeed.generate()` and displays the GeneratedItem

**Preview display:**
- `display_name` (bold, large text)
- `description` (multiline, read-only)
- Rarity badge label (Common/Uncommon/Rare/Epic/Legendary)
- Stats table: rows for each StatKey → value. StatKeys include: Damage, Range, FireRate, ShieldHp, Recharge, Thrust, Turn, SensorRange, MiningRate, RepairRate, Weight
- Icon info: "24×24 pixel icon (Circuitry motif)" or equivalent

**generate_from_seed:** picks a random ItemType path and tier, generates seed, shows preview.

**Left panel:** list of saved ItemSeeds by id (generated item display_name). Save format is the ItemSeed, not the GeneratedItem (items are seed-deterministic).

---

### 11. Contract Editor

**File:** `src/editors/contract.rs` — register under ContentType::Contract but keep the filename `dialogue.rs` for backward compatibility, or rename to `contract.rs` and update the registry
**Data type:** `Contract` in `reachlock-core/src/contract/types.rs`
**RON examples:** None (procedural only)
**Schema:** `mods/reachlock/schemas/contract.schema.json`
**Dependencies:** Condition tree widget (shared with Storyline and Soul editors)

**Fields:**
- `id`, `label`: singleline text
- `trigger` section (CollapsingHeader): ComboBox switching between:
  - Timer: shows `interval_secs` DragValue<u32> + `repeat` Checkbox
  - Event: shows `event_type` singleline text
  - StateChange: shows `field` text, `op` ComboBox (Lt/Le/Eq/Ne/Ge/Gt), `value` DragValue<i64>
  - Manual: shows "(fires when triggered explicitly)"
- `rules` section: list of Rule. Per rule:
  - `priority`: DragValue<u8> (0..=255)
  - `condition`: **Condition Tree Widget** (see below)
  - `action`: `kind` text field + `params` BTreeMap. Params displayed as key:value rows (two text fields per row, "Add Param" button)
- `llm_authority` section: Checkbox to enable (Optional). When enabled:
  - `fallback_on_timeout`: Checkbox
  - `timeout_ms`: DragValue<u32> (min 100, default 5000)
  - `max_tokens`: DragValue<u32> (min 1, default 256)
  - `system_prompt`: multiline text
  - `fallback_action`: same Action editor as above (kind text + params table)

**Condition Tree Widget** (shared component, used by Contract, Storyline, Soul):

```
[+] Always                                              [×]
[+] Compare  field:[________]  op:[ComboBox]  value:[__] [×]
[-] All
    [+] Compare  field:[________]  op:[ComboBox]  value:[__] [×]
    [+] Not
        [+] Any
            [+] Compare  field:[________]  op:[__]  value:[__] [×]
            [+] Compare  field:[________]  op:[__]  value:[__] [×]
            [+ Add Child]
    [+ Add Child]
[+ Add Root Condition]
```

Rules:
- `All` and `Any` nodes have children (list of Condition). Show "[+ Add Child]" button.
- `Not` node has exactly one child. No add/remove child buttons.
- `Compare` and `Always` are leaf nodes (no children).
- `[+]` / `[-]` toggles expand/collapse.
- `[×]` on any node removes it (and its children, with confirmation for non-leaf nodes).
- `[+ Add Root Condition]` at the very bottom adds a new top-level node.
- Indentation depth signals nesting level. Use `ui.add_space(20.0 * depth)` or frame grouping.
- Compare node: field text field, op ComboBox (Lt, Le, Eq, Ne, Ge, Gt), value DragValue. All inline on one row.

**generate_from_seed:** creates a simple contract with 1-2 rules using Compare conditions, Timer or Manual trigger.

**Left panel:** list of contracts by label.

---

### 12. Storyline Editor

**File:** `src/editors/storyline.rs` (replace existing stub)
**Data type:** `Storyline` in `reachlock-core/src/faction.rs`
**RON examples:** `mods/reachlock/storylines/compact_arc.ron`, `loup_garou_souls.ron`
**Schema:** `mods/reachlock/schemas/storyline.schema.json`
**Dependencies:** Faction (faction ID reference), ChapterTrigger tree widget

**Fields:**
- `faction`: singleline text (matches a FactionId, e.g. "compact")
- `chapters` section: list of Chapter. Per chapter:
  - `id`: singleline text
  - `trigger`: Checkbox "Has Trigger". When enabled, shows the **ChapterTrigger Tree Widget** (see below)
  - `narration`: multiline text (the story text that fires when this chapter triggers)
  - `events`: list of String fields (add/remove). These are event IDs released when the chapter fires.

**ChapterTrigger Tree Widget** (similar to Condition Tree but with different node types):

```
[ChapterTrigger ComboBox ▼]                              [×]
    TickAfter: [____] (u64)

[ChapterTrigger ComboBox ▼]                              [×]
    All:
        [+ ChapterTrigger ComboBox ▼]                    [×]
            ChapterComplete: [______________]
        [+ ChapterTrigger ComboBox ▼]                    [×]
            PlayerReputation  faction:[____]  trust:[___]
        [+ Add Child]
```

Rules:
- Leaf nodes: `TickAfter(u64)` shows DragValue. `ChapterComplete(String)` shows text field. `PlayerReputation { faction, trust }` shows faction text + DragValue<i64>.
- Container nodes: `All(Vec<ChapterTrigger>)` and `Any(Vec<ChapterTrigger>)` show children list with "[+ Add Child]" button.
- `[×]` removes the node. On root trigger, sets `trigger` back to `None` (uncheck the checkbox).
- ComboBox at each node selects the variant type for that node.

**generate_from_seed:** creates a 3-chapter storyline with TickAfter triggers in sequence (after 5, 15, 30 ticks) and templated narration.

**Left panel:** list of storylines by faction:chapter_id.

---

### 13. Soul / NPC Editor

**File:** `src/editors/soul.rs` (enhance existing — it already has 305 lines with working load/save)
**Data type:** `SoulFile` in `reachlock-core/src/soul/types.rs`
**RON examples:** `mods/reachlock/souls/boris.ron`, `tib.ron`, `doss.ron`
**Schema:** `mods/reachlock/schemas/soul.schema.json`
**Dependencies:** Condition tree widget, dialogue tree widget

The existing editor already handles Identity, Personality, Emotional State, and basic Relationships. **Extend it with the following new sections:**

**Changes to existing:**

- **Species ComboBox:** now shows all 5: Human, Android, Robot, Voidborn, Xenotype
- **Species-specific fields** appear below species selection:
  - Human: "Cybernetic Grade" label (informational — editable in BodyMod editor)
  - Android: "Chassis Model" text field + "Firmware Version" text field
  - Robot: "Unit Class" ComboBox (Industrial/Service/Security/Exploration) + "Intelligence Tier" DragValue<u8> (1..=5)
  - Voidborn: "Void Adaptation" text field (bioluminescent, pressure-resistant, etc.) + "Origin Region" (Deep Space, Nebula Birth, Predecessor Ruin)
  - Xenotype: "Planet of Origin" text field + "Ecosystem Role" ComboBox (Predator, Prey, Scavenger, Symbiont, Apex, Decomposer) + "Environment" ComboBox (Aquatic, Arboreal, Subterranean, Aerial, Plains)

**New sections to add:**

**Tab 4 — Dialogue Graph** (replace the placeholder "Relationships" list):
- Checkbox "Has Dialogue Graph". When enabled, shows the **Dialogue Tree Widget**:

```
[Node 1 ▼] (Start Node)
  NPC: "Hello, traveler." [multiline text, edit inline]
  [Condition: (optional) ▼] [Condition Tree Widget if enabled]
  Player Responses:
    > [______________] → Node [ComboBox: 2]  [Edit] [×]
    > [______________] → Node [ComboBox: 3]  [Edit] [×]
    [+ Add Response]

[Node 2 ▼]
  NPC: "I'm a simple trader." [multiline]
  Player Responses:
    > [______________] → Node [ComboBox: 4]  [Edit] [×]
    [+ Add Response]

[+ Add Node]
```

Implementation notes:
- Each node is a `CollapsingHeader` showing "Node N: (first line of NPC text)"
- Expanded: NPC text as multiline TextEdit, optional Condition Tree Widget (checkbox to enable), Player response list
- Each response: text field for what the player says, node dropdown for where it leads, "Edit" button selects that node, "×" removes the response
- "[+ Add Response]" button at bottom of each node's response list
- "[+ Add Node]" at the very bottom creates a new node
- Node 1 is always the start node (first line of dialogue)
- The dropdown shows node numbers + first 20 chars of NPC text

**Tab 5 — Secrets & Breaking Points:**
- `secrets` list: per secret: `id` text, `reveal_condition` (Condition Tree Widget), `flavor_text` multiline. "Add Secret" button.
- `breaking_points` list: per breaking point: `id` text, `trigger` (Condition Tree Widget), `description` multiline. "Add Breaking Point" button.
- `deflections` list: per deflection: `trigger` (Condition Tree Widget), `quote` multiline. "Add Deflection" button.

**Tab 6 — Memory & Relationships:**
- `memory_tree` list (read-only if loaded from save, editable in editor): per memory: `event_type` text, `timestamp` DragValue<u64>, `emotional_weight` DragValue<i64>
- `relationship_graph` list: per relationship: `target_id` text, `trust` DragValue (0..=1024), `familiarity` DragValue (0..=1024), `history` list (read-only). "Add Relationship" button.
- `goals` list: per goal: `id` text, `description` text, `priority` DragValue. "Add Goal" button.

**Tab 7 — Contracts:**
- `contracts` list: Contract ID text fields, "Add Contract Reference" button.

**generate_from_seed:** generates a complete soul with random species (weighted: 40% Human, 20% Android, 15% Robot, 15% Voidborn, 10% Xenotype), random personality traits, emotional state, and 1-2 relationships.

**Left panel:** list of souls by name with species icon indicator (colored dot: peach=Human, blue=Android, grey=Robot, violet=Voidborn, green=Xenotype).

---

### 14. Gate Network Editor

**File:** `src/editors/gate_network.rs` (new)
**Data type:** `GateNetwork` in `reachlock-core/src/galaxy/gate.rs`
**RON examples:** `mods/reachlock/gate_network/core_region.ron`
**Schema:** `mods/reachlock/schemas/gate_network.schema.json`
**Dependencies:** SystemId (charted system IDs), FactionId (for controlled_by)

**Full visual graph editor.** This is fundamentally different from the other editors — it's a 2D canvas with interactive nodes and directed edges.

**Layout:**

**Left panel (250px):** Gate list in text form. Rows: from → to, status badge (colored), controlled_by if set. Click to select. Delete key removes selected gate. "Add Gate" button at top.

**Center panel — The Canvas:**
- Systems rendered as circular nodes (radius ~40px) with system name label below
- Directed gates rendered as arrows between nodes with arrowheads at destination
- Node color by biome:
  - Core: gold (#DAA520)
  - Frontier: green (#3CB371)
  - Nebula: purple (#9370DB)
  - Derelict: grey (#808080)
  - DeepSpace: dark (#2F4F4F)
  - Unknown (not in charted systems): white (#CCCCCC)
- Gate arrow color by status:
  - Active: green (#4CAF50)
  - Blockaded: red (#F44336)
  - Restricted: orange (#FF9800)
  - Contested: yellow (#FFEB3B)
  - Destroyed: dashed dark grey (#424242)
- Gate arrow has a small label showing status
- Nodes are draggable — click and drag to reposition (store positions in a HashMap<String, Pos2>)
- Click a gate arrow (or its label) to cycle status: Active → Restricted → Blockaded → Contested → Destroyed → Active
- Right-click a gate arrow opens a context menu to edit `controlled_by` text
- Canvas supports pan (middle-click or right-click drag) and scroll-wheel zoom (0.25x to 4.0x)

**Toolbar above canvas:**
- "Add System" button → dropdown of charted system IDs (or free text for new)
- "Add Gate" button → two ComboBoxes for from/to system, creates Active gate
- "Delete Selected" → removes selected gate from the network
- "Auto-Layout" button → simple grid layout: place nodes in rows of 4, evenly spaced
- Zoom level indicator: "100%"

**Implementation approach:**
- Use `egui::Frame::canvas(&ui.style())` for the drawing area
- Use `ui.painter()` for custom rendering:
  - `painter.circle_filled(center, radius, color)` for nodes
  - `painter.text(center, Align2::CENTER_BOTTOM, label, font_id, text_color)` for labels
  - `painter.line_segment([from, to], stroke)` for gate arrows
  - `painter.arrow(to - dir * 6.0, dir, arrow_length, stroke)` for arrowheads
- Use `ui.interact(rect, id, Sense::drag())` for node dragging
- Hit testing: check if click position is within node radius (distance < 40px) or near a gate line (point-to-segment distance < 8px)
- Store node positions alongside gate data in the editor struct: `node_positions: HashMap<String, egui::Pos2>`

**generate_from_seed:** not applicable — gate networks are purely authored.

**Left panel:** text-based gate list (supplementary to the visual editor).

---

## Phase 2 — 2 Procedural Previewers

These tools explore generated content without persisting files. They're read-only
previewers with seed exploration, useful for content authors who want to browse
what the generators produce before authoring overrides.

---

### 15. Item Browser

**File:** `src/editors/item_browser.rs` (new)
**Data type:** none persisted — reads ItemSeed → GeneratedItem live
**Dependencies:** ItemType hierarchy, generate_item()

**Purpose:** Explore all 52 item types visually without authoring them.

**Layout:**
- Left sidebar (250px): list of 18 ItemFamily variants with expand/collapse triangles.
  Each family shows the count of subtypes (e.g. "Weapon (14)", "Armor (1)").
  Click to select a family. Selected family highlighted.
- Top toolbar: Tier slider DragValue<u8> (1..=10). "Re-roll Seeds" button.
- Center: grid of 8 item cards (4 columns × 2 rows) generated from seeds 0-7 for the selected family + tier.
  Each card shows: generated name (bold), rarity badge, 3-4 key stats, colored rarity border.
  Click a card → detail panel slides in.
- Right panel (300px, resizable): detail for selected item.
  - Full `display_name` (large text)
  - `description` (multiline)
  - Complete stat table: every StatKey with value, sorted by value descending
  - `ItemType` breadcrumb trail (Equipment → Weapon → Kinetic → Cannon)
  - `ItemSeed` display: seed, tier, faction, biome
  - Rarity badge with color (Common=grey, Uncommon=green, Rare=blue, Epic=purple, Legendary=gold)
  - Icon placeholder: colored 64×64 square with circuitry motif indicator
  - "Pin Seed" button (saves ItemSeed to disk as authored item, opens in Item Editor)

---

### 16. Character Sprite Viewer

**File:** `src/editors/character_sprite.rs` (new)
**Data type:** none persisted — calls `generate_character_sprite()` live
**Dependencies:** Species enum (5 variants), hair styles (7), sprite generator

**Purpose:** Preview and pin character looks. Standalone tool.

**Layout:**
- Left sidebar (250px): controls
  - Species: ComboBox — Human, Android, Robot, Voidborn, Xenotype
    (Robot shows simplified controls — no hair, no skin tone; chassis color + visor color instead)
  - Hair style: left/right cycle buttons (◀ ▶) with label showing current style:
    Short, Buzz, Long, Locs, Bun, Crest, Bald
    (Disabled for Robot — shows "N/A")
  - Hair color: 3 DragValue<u8> sliders (R, G, B) with a 20×20 color preview swatch
  - Skin color: 3 DragValue<u8> sliders with swatch
  - Shirt color: 3 DragValue<u8> sliders with swatch
  - Pants color: 3 DragValue<u8> sliders with swatch
  - Jacket: Checkbox to enable, then 3 DragValue<u8> sliders with swatch
  - "Randomize" button: seeds all colors randomly
  - "Pin Seed" button: saves current look as `CharacterLook.ron` — includes species, hair style, all colors, seed
- Center: large 32×48 pixel preview displayed at 4x nearest-neighbor scale (128×192 display size).
  Black border around the preview. Updates live on any slider or control change.
- Right: 4-direction × 2-frame walk cycle preview. 4 rows labeled Down/Up/Left/Right,
  each showing 2 small frames (standing + mid-stride, 2x scale). All frames update live.
- Bottom: seed display + "Re-roll Seed" button

**Implementation note:** The pixel painting code lives in `reachlock-client/src/pixel.rs`
(`paint_character`, `paint_robot`). Since the editor cannot depend on the client crate,
replicate the minimal pixel painting logic in the editor or call `generate_character_sprite()`
from core and display the generated texture layers. For Phase 2, use `generate_character_sprite()`
and render each layer (body, outfit, hair) as stacked colored rectangles in the preview.

---

## Phase 3 — 4 New-Domain Editors (After Data Model Design)

These editors depend on NEW data types that need to be designed in
`reachlock-core/src/` before Fable can build the editors. This phase is
described for completeness but requires a separate data-model design pass
followed by a second Fable handoff.

### Prerequisites for Phase 3

The following types must be defined in core before Fable starts:

```rust
// BodyMod system
pub enum BodyModKind {
    Cybernetic(HumanSlot),
    Augment(AndroidSlot),
    Enhancement(RobotSlot),
    Mutation(VoidbornSlot),
    Adaptation(XenotypeSlot),
}

pub enum HumanSlot { Head, Eyes, Torso, LeftArm, RightArm, LeftLeg, RightLeg }
pub enum AndroidSlot { Processor, Memory, SensorArray, PowerCell, Chassis, MotiveSystem }
pub enum RobotSlot { Chassis, LeftArm, RightArm, SensorHead, PowerCore }
pub enum VoidbornSlot { BioluminescentOrgan, PressureGland, TelepathyNode, VoidSight, GravitySense, DimensionalPocket }
pub enum XenotypeSlot { Camouflage, VenomGland, Carapace, ThermalSense, DigestiveTract, RespirationOrgan }

// Ship room upgrade system (per S45 spec)
pub enum RoomUpgradeKind { /* per-room-type upgrade variants */ }
pub struct RoomUpgradeStats {
    pub stat_bonuses: BTreeMap<StatKey, i64>,
    pub power_draw: i64,
    pub install_ticks: u32,
}
```

---

### 17-21. BodyMod Editor (5 species tabs, one editor)

**File:** `src/editors/body_mod.rs` (new)
**Data type:** New `BodyMod` struct (to be designed)
**RON examples:** None (new domain)
**Schema:** None (new domain)
**Dependencies:** Species-slot enums, StatKey mappings

**Tabs at top:** Human | Android | Robot | Voidborn | Xenotype

Each tab shows:

**Left panel:** list of mods for that species. Columns: Slot, Name, Tier, Installed checkbox.

**Right panel — for selected mod:**
- `id`: singleline text
- `display_name`: singleline text
- `slot`: ComboBox of species-specific slots (7 human, 6 android, 5 robot, 6 voidborn, 6 xenotype)
- `tier`: DragValue<u8> (1..=10)
- `stat_bonuses`: table of StatKey ComboBox + value DragValue<i64> pairs. "Add Stat Bonus" button.
- `visual_seed`: DragValue<u64> — drives procedural visual representation
- `description`: multiline text
- Visual preview: species-specific silhouette with the mod highlighted on the relevant body part

**generate_from_seed:** fills a random slot and tier, rolls stat bonuses appropriate to the species and slot type.

---

### 22. Widget Editor (Ship Room Upgrades)

**File:** `src/editors/widget.rs` (new)
**Data type:** New `RoomUpgrade` struct (per S45 spec)
**RON examples:** None (new domain)
**Schema:** None (new domain)
**Dependencies:** RoomKind, StatKey, RoomUpgradeKind

**Left panel:** list of widgets by display_name. Filter by room kind at top.

**Right panel — for selected widget:**
- `id`, `display_name`: singleline text
- `room_kind`: ComboBox — which room type(s) this widget fits in (single or multi-select)
- `slot_type`: ComboBox — Primary, Secondary, Utility
- `tier`: DragValue<u8> — Basic(1-2), Improved(3-4), Advanced(5-6), Experimental(7-8), Predecessor(9-10)
- `stats` section: `stat_bonuses` table (StatKey + value DragValue), `power_draw` DragValue<i64>, `install_ticks` DragValue<u32>
- `visual_seed`: DragValue<u64>
- **Visual preview:** 32×32 pixel sprite rendered at 4x scale (128×128 display).
  Generated from the visual_seed using procedural icon motif + palette.
- **Placement preview:** a grid showing a representative room template (e.g., Cockpit 6×4)
  with the widget's icon placed in a highlighted slot cell. This shows authors
  exactly how large the widget appears in the ship interior.

**generate_from_seed:** picks a random room kind + slot type + tier, fills stats proportionate to tier, generates a visual seed.

---

## Verification

After each editor:
```bash
cargo build -p reachlock-editor       # must compile with zero errors
```

After each phase:
```bash
make check                             # fmt + clippy + tests + WASM build
```

Manual verification (after each editor, launch and test):
```bash
cargo run -p reachlock-editor
# 1. Open the editor from File menu or content browser
# 2. Verify all ComboBoxes cycle through all options
# 3. Modify a field → asterisk (*) appears on tab
# 4. Save As → verify .ron file written to mods/
# 5. Close editor tab, reopen → load from saved file succeeds
# 6. "Generate from Seed" → all fields populate, no empty required fields
# 7. Clear a required field → red validation error appears inline
# 8. "New" button → blank slate with defaults, no crash
```

## Browser Upgrade

The `src/browser.rs` ContentBrowser currently shows 10 flat buttons. Upgrade it
to show a file tree reading from `mods/reachlock/`:

- Use `std::fs::read_dir` to scan each ContentType's directory
- Show a collapsible tree: ContentType name → list of .ron files
- Double-click a file to open it in the appropriate editor
- Right-click a file to delete (with confirmation dialog)
- "New" button per content type creates a blank editor

## What NOT to Touch

These files are outside the editor crate's scope and should NOT be modified:
- `reachlock-core/src/` — core data types (except new types for Phase 3)
- `reachlock-client/src/` — game client code
- `reachlock-server/src/` — server code
- `mods/reachlock/*.ron` — authored content files (read only, write via save)
- `Makefile`, `AGENTS.md`, `docs/sprints/` — project infrastructure
- `src/main.rs` (the app shell) — unless adding new ContentType variants or
  menu items for entirely new editor categories

## Content Types Map

| ContentType | Directory | Editor file |
|-------------|-----------|-------------|
| HullFrame | huds/ | hull_frame.rs |
| Station | stations/ | station.rs |
| Location | locations/ | location.rs |
| Soul | sould/ | soul.rs |
| Contract | contracts/ | dialogue.rs (or contract.rs) |
| Faction | factions/ | faction.rs |
| EconomyGoods | economy/ | economy.rs |
| Storyline | storylines/ | storyline.rs |
| Item | items/ | item.rs |
| EnemyArchetype | enemies/ | enemy.rs |
| ChartedSystem | systems/ | charted_system.rs |
| HullMesh | huds/ | hull_mesh.rs |
| RoomTemplates | huds/ | room_templates.rs |
| GateNetwork | gate_network/ | gate_network.rs |

---

## Phase 2.5 — AI-Assisted Content Creation

**After completing Phase 1 editors and the Phase 2 previewers, implement this.**
It depends on the editors existing because it populates their fields.

### Concept

A global AI prompt bar at the top of the editor window. The author types
a natural language description ("a swampy planet with thriving ecosystem
and lots of resources for discovery") and the editor calls any
OpenAI-compatible API to generate a complete, schema-valid content asset.
The generated fields populate the active editor tab, ready for human tuning.

### Architecture

```
User prompt → Editor assembles system prompt + JSON schema + user text
→ POST /v1/chat/completions → LLM returns JSON
→ Editor validates JSON against schema → Populates editor fields
```

The editor calls any OpenAI-compatible chat completions endpoint. Ollama
exposes this at `http://localhost:11434/v1` by default — local-first,
offline, zero cost. Cloud APIs (OpenAI, Anthropic via proxy) work by
changing the URL and key. No code branches needed.

### Dependencies to Add

The editor's `Cargo.toml` already has `jsonschema` and `serde_json`. Add:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

### New Files

- `src/ai.rs` — AiConfig, generate_content(), prompt builders per content type
- `src/settings_window.rs` — AI settings popup (or inline in main.rs)

### Settings

Stored in `save/editor-settings.ron` alongside editor preferences:

| Setting | Default | Description |
|---------|---------|-------------|
| `ai_api_base_url` | `http://localhost:11434/v1` | Ollama's OpenAI-compatible endpoint |
| `ai_api_key` | `ollama` | Ollama accepts any string; cloud APIs need real keys |
| `ai_model` | `llama3.2:3b` | Fast, lightweight, local |
| `ai_max_tokens` | `4096` | Largest content type (SoulFile) is ~3KB of JSON |

### AI Settings Popup

A modal window opened from the gear (⚙) icon on the AI bar:

```
┌─ AI Settings ────────────────────────────────────┐
│                                                  │
│  API Base URL: [http://localhost:11434/v1     ]  │
│  API Key:      [ollama                        ]  │
│  Model:        [llama3.2:3b                   ]  │
│  Max Tokens:   [4096                      ▲▼  ]  │
│                                                  │
│  [Test Connection]  [Save]  [Cancel]             │
│                                                  │
│  Status: ✓ Connected — llama3.2:3b (3.8B params) │
│                                                  │
└──────────────────────────────────────────────────┘
```

**"Test Connection"** sends a GET to `{base_url}/models` (Ollama-compatible)
or a minimal chat completion. Shows ✓ with model name and param count on
success, or ✗ with error message on failure. Parses the model list to
populate a ComboBox for model selection (cache the list).

Settings are saved immediately on "Save" and persist across editor restarts.

### UI — Global AI Bar

```
┌──────────────────────────────────────────────────────────────────┐
│ 🤖 [Describe what you want to create...                  ] [⚙] [▶] │
└──────────────────────────────────────────────────────────────────┘
```

- **Position:** below the menu bar, above the tab bar. Always visible.
- **Text field:** expands to 3 visible lines on focus. Enter sends,
  Shift+Enter inserts newline. Placeholder text: "Describe what you want
  to create..."
- **⚙ gear icon:** opens the AI Settings popup.
- **▶ send button:** fires generation for the active editor tab's content
  type. Disabled (greyed out) if no editor tab is open or the text field
  is empty.
- **Loading state:** during generation, the send button becomes a spinner
  animation, the text field is disabled, and the status bar shows
  "Generating... (may take 5-15 seconds)".
- **On success:** status bar shows "AI generated — review and tune."
  All editor fields populate. Modified indicator (*) appears on the tab.
  Text field clears.
- **On failure:** status bar shows the error in red. Example messages:
  - "Connection refused — is Ollama running?"
  - "Response failed schema validation: 'id' is required"
  - "API error 401 — invalid API key"
  A "Retry" link and a "[Use procedural instead]" button appear next
  to the error.

### Prompt Construction

For each content type, the editor assembles a system prompt from three parts:

**Part 1 — Role instruction:**

> You are a content generation assistant for the ReachLock spacefaring
> game. Output ONLY valid JSON matching the schema below. Do not include
> markdown code fences or explanatory text. Output raw JSON only.

**Part 2 — Content type context** (one paragraph per type, describing what
it means in the game world):

| ContentType | Context paragraph |
|-------------|------------------|
| ChartedSystem | A star system in the ReachLock galaxy. Systems are connected by a gate network. Each has a 3D position, a biome flavor, and a descriptive paragraph visible in the galaxy map. |
| Soul | An NPC character. Species: Human (cybernetically enhanced), Android (synthetic humanoid), Robot (non-humanoid machine), Voidborn (space-dwelling, mystical, Predecessor-lore), Xenotype (planet-bound ecosystem creature). Each has personality traits, an emotional state, memories, relationships, secrets, goals, and optional branching dialogue. |
| Faction | A political faction. Has a doctrine (Military/Economic/Diplomatic/Expansionist), tariff policy, territory claims, internal divisions with agendas, relationships with other factions, and goods it produces. |
| HullFrame | A ship frame defining where hardpoints, armor zones, decals, and the engine mount go. Classes: Shuttle (small, fast), Corvette (balanced), Freighter (large, slow), Station (immobile), Rock (asteroid). |
| EnemyArchetype | A landed-combat enemy. Has HP, speed, light and heavy attack windows (startup/active/recovery ticks, damage, range), block window, dodge window, chase/disengage radii, and flee threshold. |
| Station | A space station with an exterior hull mesh, an interior layout of rooms connected by doors, and NPC spawns with dialogue lines. |
| Location | A hostile interior location (derelict ship, bunker, space station). Contains rooms with enemy spawns, props, connections between rooms, and an optional keycard gate. |
| Economy | A catalog of trade goods. Each good has a name, category (Consumable, Fuel, Material, Manufactured, Medical, Luxury, Contraband), base price, mass, and contraband flag. |
| Contract | An automated contract evaluated by the game engine. Has a trigger (Timer, Event, StateChange, or Manual), prioritized rules with conditions (Always, Compare, All, Any, Not), actions, and optional LLM fallback authority. |
| Storyline | A faction's narrative arc. Contains chapters with triggers (TickAfter, ChapterComplete, PlayerReputation, All, Any) and narration text that fires when triggered. |
| Item | A generated equipment item. Has a type hierarchy (Equipment→Weapon→Kinetic→Cannon), tier (1-10), seed, faction/biome origin, and generates stats like Damage, Range, FireRate, ShieldHp, etc. |
| HullMesh | A hand-crafted ship hull: a polygon mesh with vertices and triangle indices. |
| RoomTemplates | A set of room templates for ship interiors. Each template has a kind (Cockpit, MedBay, Reactor, etc.), dimensions, required systems, furniture slots, and adjacency bonus pairs. |
| GateNetwork | A directed graph of star systems connected by gates. Each gate has a from/to system, a status (Active, Blockaded, Restricted, Contested, Destroyed), and an optional controlling faction. |

**Part 3 — JSON schema:** Read from `mods/reachlock/schemas/<type>.schema.json`
at editor startup. Inline the schema's key requirements in compact form:
required fields, types, enum values, numeric ranges. Strip `$schema`,
`title`, and `description` meta fields — keep only the structural constraints.

Example assembled system prompt for ChartedSystem:

```
You are a content generation assistant for the ReachLock spacefaring
game. Output ONLY valid JSON matching the schema below. Do not include
markdown code fences or explanatory text. Output raw JSON only.

A star system in the ReachLock galaxy. Systems are connected by a gate
network. Each has a 3D position, a biome flavor, and a descriptive
paragraph visible in the galaxy map.

Schema:
- id: string (required, snake_case identifier)
- display_name: string (required)
- position: { x: integer, y: integer, z: integer } (required)
- biome: "core" | "frontier" | "nebula" | "derelict" | "deep_space" (required)
- seed: integer, 0 to 9007199254740991 (required)
- description: string (required)
```

The user's natural language text becomes the `user` message. The full
payload sent to the API:

```json
{
  "model": "llama3.2:3b",
  "messages": [
    { "role": "system", "content": "<assembled system prompt + schema>" },
    { "role": "user", "content": "<user's natural language description>" }
  ],
  "max_tokens": 4096,
  "temperature": 0.7
}
```

### Response Handling Pipeline

```
                  ┌─────────────────┐
                  │ LLM Response    │
                  └────────┬────────┘
                           │
                  ┌────────▼────────┐
                  │ Extract JSON    │
                  │ from response   │──No JSON found──▶ "Response contained no JSON" error
                  └────────┬────────┘
                           │
                  ┌────────▼────────┐
                  │ Validate against│
                  │ JSON schema     │──Validation errors──▶ Show specific field errors
                  └────────┬────────┘
                           │
                  ┌────────▼────────┐
                  │ Map JSON fields │
                  │ to editor state │──Type mismatch──▶ Show specific field error
                  └────────┬────────┘
                           │
                  ┌────────▼────────┐
                  │ Populate editor │
                  │ Mark modified   │
                  │ Status: success  │
                  └─────────────────┘
```

**JSON extraction strategies** (try in order, stop at first success):
1. Parse entire response content as JSON via `serde_json::from_str`
2. Extract from ````json ... ```` markdown fences (regex: `json\s*\n(.*?)\n\s*\`\`\``)
3. Find the first `{` and matching `}` (character-level bracket matching)
4. Find the first `[` and matching `]` (for array return types like RoomTemplates)
5. If all fail: return error with the raw response content for debugging

**Schema validation:** The `jsonschema` crate is already in the editor's
Cargo.toml. The `io.rs::validate_content()` function already reads schema
files, compiles validators, and validates JSON values. Extract this logic
into a shared helper (`src/schema.rs`) used by both the existing validator
and the AI pipeline.

On validation failure, parse the `jsonschema` error iterator and display
each error inline: `"field 'biome': value 'jungle' is not one of [core,
frontier, nebula, derelict, deep_space]"`.

**Mapping JSON to editor state:** The validated JSON is deserialized into
the target Rust struct. If deserialization fails (type mismatch), show the
serde error. On success, assign to the editor's data field and set
`has_changes = true`.

### Schema Caching

At editor startup, read all JSON schemas from `mods/reachlock/schemas/`
into a `HashMap<ContentType, CompiledSchema>`. The `CompiledSchema` struct holds:
- The raw schema JSON string (for the LLM prompt)
- The compiled `jsonschema::JSONSchema` validator (for response validation)

This cache is shared by `validate_content()` and `generate_content()`.

### The generate_content() Function

```rust
// src/ai.rs

pub struct AiConfig {
    pub api_base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

pub struct GenerationResult {
    pub json_value: serde_json::Value,
    pub warnings: Vec<String>,  // non-fatal issues (e.g., extra fields)
}

pub async fn generate_content(
    config: &AiConfig,
    content_type: ContentType,
    schema: &CompiledSchema,
    user_prompt: &str,
) -> Result<GenerationResult, GenerationError> {
    // 1. Build system prompt from content type + schema
    // 2. POST to {config.api_base_url}/chat/completions
    // 3. Extract JSON from response
    // 4. Validate against schema
    // 5. Return validated JSON value or error
}

pub enum GenerationError {
    HttpError(String),
    NoJsonFound(String),
    SchemaValidationFailed(Vec<String>),
    DeserializationFailed(String),
}
```

### Fallback Chain

The editor always offers three creation paths:

1. **AI generation** (the AI bar) — prompt-driven, requires API config
2. **Procedural generation** ("Generate from Seed" button) — always works,
   no network, uses SeededRng
3. **Manual creation** ("New" button) — blank slate with defaults

The "Generate from Seed" button remains in every editor toolbar regardless
of AI availability. The AI bar is additive. If AI generation fails, the
error message includes a clickable "[Use procedural instead]" link.

### Verification

```
# Prerequisite: install and run Ollama
ollama pull llama3.2:3b
ollama serve

# Build and launch
cargo run -p reachlock-editor

# Manual tests:
# 1. Open Charted System editor, type "a swampy frontier world with
#    abundant alien flora and ancient ruins", click send
#    → Verify fields populate: id is snake_case, biome is "frontier",
#      position has 3 integers, description is 2+ sentences
# 2. Modify the description field → verify it saves
# 3. Open AI Settings, change model to "nonexistent-model"
#    → Click Test Connection → verify ✗ error shown
#    → Click send → verify graceful error, "Use procedural instead" appears
# 4. Open Soul editor, type "a grizzled voidborn smuggler with a
#    mysterious past"
#    → Verify species is "voidborn", personality traits filled, backstory
#      generated, emotional state set
# 5. Open Faction editor, type "a ruthless mercantile cartel that
#    controls the outer rim trade routes"
#    → Verify doctrine is Economic, tariff_policy has Flat/Dynamic params,
#      territory has system claims, produces has goods

# Automated: add tests to ensure all schemas parse and compile validators
cargo test -p reachlock-editor
```

### What the AI Bar Does NOT Do

- It does not replace manual editing — it's a starting point for human
  creative direction
- It does not auto-save — the human must review and explicitly Save
- It does not generate visuals (sprites, meshes, icons) — it generates
  the structured data; visuals come from the procedural generator via
  the `visual_seed` field
- It does not require internet — Ollama runs locally and offline
- It does not store prompts or responses server-side — everything is local

### Registration Checklist

- [ ] Create `src/ai.rs` with AiConfig, generate_content(), prompt builders
- [ ] Create `src/schema.rs` with CompiledSchema cache (extracted from io.rs)
- [ ] Add `reqwest` and `tokio` to `Cargo.toml`
- [ ] Add AI bar rendering in `main.rs::update()` between menu bar and tab bar
- [ ] Add AI Settings popup window (modal)
- [ ] Add settings persistence to `save/editor-settings.ron` (expand existing)
- [ ] Wire "Test Connection" button in settings popup
- [ ] Verify `cargo build -p reachlock-editor` passes
- [ ] Verify `cargo run -p reachlock-editor` launches with AI bar visible
