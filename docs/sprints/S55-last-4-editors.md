# S55 — Last 4 Editors

**Spec:** §10 (content pipeline), S25 editor architecture · **Wave 14 (Editor & CLI) · Depends on:** S54

## Outcome

Every content type defined in the spec has a dedicated editor in `reachlock-editor/src/editors/`. Four new editors: dungeon (Predecessor ruins), event (scripted timeline), dialogue (branching tree), recipe (crafting). Each follows the existing editor pattern — `ContentType::Dungeon` → `dungeon::DungeonEditor` with `fn ui()`, `fn load()`, `fn save()`, and `fn new()`. Total editor count: 26.

## Context

- Current editors (22): hull, station, planet, music, ui_panel, noise, palette, system, item, hull_config, ship_interior, sector, combat_encounter, soul, faction, contract, ecosystem, planet_culture, theme, trope, scripted_encounter, career.
- Missing editors: dungeon (Predecessor ruins — spec §10 line 714), event (scripted timeline — spec §10 line 716), dialogue (branching conversation trees), recipe (crafting — spec §10 line 1592).
- Each missing type already has a core struct, a generator, and a schema. The editor is the final missing piece.

## Freeze first

1. Follow the existing editor architecture: one file per editor, one `ContentType` variant, fields in `ContentFile` envelope, `fn ui(&mut self, ctx: &EditorContext)` for the egui rendering, `fn save(&self) -> ContentFile` for serialization.
2. Editors are native-only (exempt from WASM build in `make check` — see gotcha ledger).

## Deliverables

### 1. Dungeon Editor (`editor/dungeon.rs`)

- [ ] **Room graph editor** — 2D grid where author places rooms as rectangles. Rooms have: id, position, size, connectors (directional edges to other rooms), tags (entrance, boss, puzzle, treasure, empty).
- [ ] **Puzzle editor** — per-room puzzle assignment. Puzzle types: lever sequence, symbol matching, navigation maze, combat encounter, exploration find. Each puzzle has parameters (sequence length, symbol count, enemy spawn points).
- [ ] **Reward table editor** — per-room reward drop table. Items by ID, credits, faction standing, lore fragment pointer.
- [ ] **Enemy encounter table editor** — per-room enemy spawns. Enemy archetype picker, count, patrol routes (waypoints on the grid).
- [ ] **Validation** — room graph must be connected (all rooms reachable from entrance). Connectors must reference valid room IDs.

### 2. Event Editor (`editor/event.rs`)

- [ ] **Timeline editor** — vertical timeline of event stages. Each stage has: trigger condition (AND/OR tree of predicates), narrative text (trope templates allowed), NPC dialogue lines with speaker assignment, consequences (state changes, faction reputation, item rewards).
- [ ] **Trigger condition tree editor** — visual tree of conditions. Leaf conditions: `PlayerReputation { faction, min_standing }`, `TickAfter { tick_count }`, `ChapterComplete { chapter_id }`, `HasItem { item_id, count }`, `PlayerInSystem { system_id }`, `FactionState { faction, status }`.
- [ ] **Consequence editor** — what happens when the event fires: `AddReputation { faction, delta }`, `AddItem { item_id, count }`, `AdvanceChapter { chapter_id }`, `EcosystemEvent { event }`, `SpawnEncounter { encounter_id }`, `SetFlag { flag_name }`.
- [ ] **Expiration editor** — optional expiration: after X ticks, after Y other events, at a specific date.

### 3. Dialogue Editor (`editor/dialogue.rs`)

- [ ] **Branching tree editor** — visual node graph of dialogue nodes. Root node is "NPC says X." Child nodes are player response options. Player options have: display text, condition (optional: requires item/reputation/flag), consequence on select, link to next NPC node.
- [ ] **Node types** — `NarratorLine` (unvoiced narration), `NpcLine` (voiced NPC speech), `PlayerChoice` (player selects response), `Branch` (condition-based auto-advance), `End` (dialogue terminates).
- [ ] **Variable interpolation** — in dialogue text, `{player_name}`, `{ship_name}`, `{station_name}`, `{faction}` are filled at runtime. Editor preview shows placeholder values.
- [ ] **Voice recording field** — each `NpcLine` node has a field for the voice clip path (for future TTS override). Currently unused but captured in the content file.
- [ ] **Import/export** — dialogue trees can be exported to a plain-text script format for non-technical writers, and re-imported.

### 4. Recipe Editor (`editor/recipe.rs`)

- [ ] **Ingredient grid** — list of required items with quantity per slot. Each slot has: item_id (from autocomplete), quantity (integer), optional (checkmark — may be omitted, but yields lower quality).
- [ ] **Output config** — output item_id, output quantity, quality range (min/max based on skill), durability range (for equipment).
- [ ] **Skill requirement** — minimum skill level per skill category (engineering, chemistry, biology, programming).
- [ ] **Workbench type** — pick from: ship_workshop, station_fabricator, med_bay, computer_terminal, cargo_bay_converted.
- [ ] **Duration** — base duration in ticks. Modified by skill level (higher skill = faster).

### 5. Wiring

- [ ] Add `ContentType::Dungeon`, `ContentType::Event`, `ContentType::Dialogue`, `ContentType::Recipe` to the `ContentType` enum in `core/src/content/envelope.rs`.
- [ ] Add the four editor modules to `editors/mod.rs`.
- [ ] Add `ContentType::* → editor_fn` routing in the editor dispatch.
- [ ] Add entries to `ContentFile` envelope with schema path.

## Acceptance gates

```
cargo run -p reachlock-editor
# All 4 new editors open, create a new entry, save, reload
cargo test -p reachlock-core content::
# ContentType enum covers all 4 new types
cargo test -p reachlock-editor
# Editor tests pass (load, edit cycle, save round-trip)
make check
```

## Non-goals

Authoring actual content files (S58-S60). The CLI `content preview` and `content publish` commands (S56). Full 3D dungeon preview inside the editor — rooms are 2D grid representations only.

## Gotchas

- Dialogue editor node graphs are notoriously complex to implement in egui (no built-in graph widget). Use a simple nested-list layout: NPC node → player options indented below → next NPC nodes nested under those. Visual graph layout (force-directed) is a future enhancement.
- The `ContentFile` envelope may need expansion if dialogue trees exceed the current `serde_json::Value` payload size limit. Test with a 200-node dialogue tree to verify round-trip.
- Each new editor follows the naming convention: lowercase snake_case filename, CamelCase struct name with `Editor` suffix.
- All editors must handle the "dirty" state: `fn is_dirty(&self) -> bool` and show `*` in the tab title when unsaved changes exist.
