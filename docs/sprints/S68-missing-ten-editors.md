# S68 — The Missing 10 Editors

**Spec:** New (editor completeness — browser, menu, trait surface) ·
**Wave B (editor shell v2) · Depends on:** S65 (editor data-loss closure, `ui(&mut Ui)` refactor, trait surface)

Closes findings: E6 (10 editors absent from Content Browser — `browser.rs:29-44` shows 14 of 26),
E7 (10 editors absent from `File → New` — `main.rs:728-792` shows 16 of 26),
E29 (10 editors lack `snapshot`/`preview_ui`/`delete_selected`/`apply_ai_json`/`touch` —
trait defaults at `app.rs:196-284`).

## Outcome

Every `ContentType` appears in the Content Browser's `FILE_TYPES`, in the
`File → New` menu, and implements the full `Editor` trait surface (including
`snapshot`/`preview_ui`/`delete_selected`/`apply_ai_json`/`touch`). The Dialogue
editor gets a graph canvas (modelled on `gate_network.rs`) for tree editing. The
Dungeon editor gets a grid canvas for room layout. The "editor completeness"
gate from MASTER-PLAN.md Part 4 (a table-driven test) is built and goes green,
against which no future sprint can regress.

## Context

- **E6 — Browser gap.** `browser.rs:29-44` hardcodes a `FILE_TYPES` array of 14
  entries. The 26-variant `ContentType` enum (defined at `app.rs:31`) has 10
  file-backed types that never appear in the browser tree: Career, Ecosystem,
  PlanetCulture, Theme, Trope, ScriptedEncounter, Dialogue, Dungeon, Event,
  Recipe. Authors cannot browse or open `.ron` files for any of these types.
- **E7 — File → New gap.** `main.rs:728-792` (the `File → New` submenu) embeds
  a flat list of type groups covering 16 of 26 variants. The same 10 types are
  absent — an author cannot create a new file of any of these types without
  hand-crafting a `.ron` in a text editor.
- **E29 — Incomplete trait surface.** Each of the 10 editors exists in
  `editors/*.rs` and implements the mandatory methods (`load`, `save`,
  `validate`, `ui`, `generate_from_seed`) plus `touch`, `mark_saved`, and
  `save_all`. But they rely on the **default** implementations (which are no-ops
  or stubs) for:
  - `snapshot()` / `restore_snapshot()` — no undo support
  - `preview_ui()` — no right-panel preview card
  - `delete_selected()` — cannot delete entries via keyboard
  - `selected_entry_name()` — Delete shortcut disabled
  - `apply_ai_json()` — AI generation returns "not wired yet" error
- **Soul editor** (1220 lines, `editors/soul.rs`) is the gold standard: full
  multi-entry `save_all` with path recording, RON-based snapshot undo, species-
  colored preview card, `apply_ai_json` that accepts both bare SoulFile and
  ContentFile envelope, entry selection and deletion. Every new editor should
  match this level.
- **Gate network editor** (`editors/gate_network.rs`, 583 lines) has the canvas
  pattern (pannable/zoomable canvas with draggable nodes, status-colored
  arrows, node selection via click, context menus on right-click). Dialogue's
  graph canvas and Dungeon's grid canvas model on this.
- S65 established the `Editor` trait with all default methods. The trait is now
  stable — this sprint overrides the defaults in the 10 editor files. No trait
  changes in this sprint.
- Existing editors (soul, hull_frame, station, etc.) are NOT touched by this
  sprint — they already meet the completeness criteria.

## Freeze first

### Editor completeness gate (`editors/mod.rs` or `app.rs` test module)

A single table-driven test that asserts three properties for every
`ContentType` variant:

```rust
#[test]
fn editor_completeness_gate() {
    // 1. Every ContentType (except previewers) is in FILE_TYPES
    let file_types = crate::browser::FILE_TYPES;
    for ct in ContentType::all() {
        match ct {
            ContentType::ItemBrowser | ContentType::SpriteViewer => continue,
            _ => {}
        }
        assert!(
            file_types.contains(ct),
            "{ct:?} is missing from browser FILE_TYPES"
        );
    }

    // 2. Every ContentType (except previewers) is reachable via File → New.
    //    Check the registry, which is the single source for File → New.
    for ct in ContentType::all() {
        match ct {
            ContentType::ItemBrowser | ContentType::SpriteViewer => continue,
            _ => {}
        }
        assert!(
            registry.create(*ct).is_some(),
            "{ct:?} has no registered editor — not reachable from File → New"
        );
    }

    // 3. Every non-previewer editor implements the full trait surface.
    for ct in ContentType::all() {
        match ct {
            ContentType::ItemBrowser | ContentType::SpriteViewer => continue,
            _ => {}
        }
        let mut editor = registry.create(*ct).unwrap();
        // snapshot() returns Some (undo supported)
        assert!(editor.snapshot().is_some(), "{ct:?} snapshot() returned None");
        // snapshot round-trips through restore_snapshot
        let snap = editor.snapshot().unwrap();
        assert!(editor.restore_snapshot(&snap).is_ok(),
            "{ct:?} snapshot did not round-trip through restore_snapshot");
        // preview_ui does not panic (render test)
        // apply_ai_json returns a non-default error or Ok
        // delete_selected returns false (no selection in a fresh editor) — ok
        // selected_entry_name returns None (single entry in fresh editor) — ok
        // touch() marks dirty
        let _ = editor.touch();
        assert!(editor.has_unsaved_changes(), "{ct:?} touch() did not mark dirty");
        editor.mark_saved();
        assert!(!editor.has_unsaved_changes(), "{ct:?} mark_saved() did not clear dirty");
    }
}
```

The test lives in `app.rs` (alongside the existing
`every_content_type_is_registered` test at line 437). It is non-optional: every
commit must pass it.

### Dialogue graph canvas

```
DialogueEditor.ui() replaces the current CentralPanel label dump with a
visual node graph editor (modelled on gate_network.rs):

- Nodes are DialogueNodes (NpcLine, PlayerChoice, ConditionGate, End)
  rendered as boxes with a header color per node type
- Edges connect node outputs to node inputs; drag from output port to
  input port creates a new choice/edge
- Click selects a node; double-click opens an inline text editor for the
  node's text field
- Right-click context menu: Add Child, Delete Node, Set as Start
- Pan: middle-mouse drag. Zoom: scroll wheel.
- The underlying data stays a Dialogue struct — the canvas is a view
  into it, not a separate representation.
```

### Dungeon grid canvas

```
DungeonEditor.ui() replaces the current CentralPanel label dump with a
grid-based room layout editor:

- A grid of tiles (default 20×20) where rooms are placed as rectangles
- Each DungeonRoom is a resizable rectangle on the grid, coloured by
  tag (e.g. entrance = green, combat = red, puzzle = purple)
- Click selects a room; click+drag moves it; drag edges/corners to resize
- Right-click context menu: Add Room, Delete Room, Edit Tags,
  Add Connector (draw a line between connector points)
- A room inspector panel (side or bottom) shows the selected room's
  properties (id, x, y, width, height, connectors, tags)
- Rooms snap to tile boundaries (granularity: 1 tile = 1 unit)
- Pan: middle-mouse drag. Zoom: scroll wheel.
- The underlying data stays a Dungeon struct.
```

## Deliverables

### 1. Browser registration (`reachlock-editor/src/browser.rs`)

- [ ] Extend `FILE_TYPES` from 14 to 24 entries: add Career, Ecosystem,
      PlanetCulture, Theme, Trope, ScriptedEncounter, Dialogue, Dungeon,
      Event, Recipe. Keep the two previewers (ItemBrowser, SpriteViewer)
      excluded — they persist nothing.
- [ ] Ensure `classify_hull_file` logic still works (the shared `hulls/`
      dir is unchanged).
- [ ] Update the doc comment on `FILE_TYPES` to reflect that all file-backed
      content types are present.

### 2. File → New menu (`reachlock-editor/src/main.rs`)

- [ ] Add a new submenu (or extend existing ones) in the `File → New` cascade
      at `main.rs:728-792` so every file-backed `ContentType` has a clickable
      entry.
- [ ] Suggested grouping:
      - **Living Galaxy** (or **Content**): Career, Ecosystem, PlanetCulture,
        Theme, Trope, ScriptedEncounter
      - **Scripting**: Dialogue, Dungeon, Event, Recipe
      - (Existing groups: Systems, Ships, Characters, World, Economy — keep
        these unchanged; each group's membership is a UX choice, but every
        type must appear in exactly one group.)
- [ ] Remove the dead `ui.close_menu()` after pick logic — already handled
      by the existing pattern.

### 3. Career editor (`reachlock-editor/src/editors/career.rs`)

- [ ] Implement `snapshot()`: serialize `self.career` to RON.
- [ ] Implement `restore_snapshot()`: deserialize and replace `self.career`,
      clear `has_changes` if the restored state is clean.
- [ ] Implement `selected_entry_name()`: return `None` (single-entry editor —
      no multi-entry selection to delete).
- [ ] Implement `delete_selected()`: return `false` (single-entry).
- [ ] Implement `preview_ui()`: show career name, path type, rank count.
- [ ] Implement `apply_ai_json()`: deserialize a `CareerPath` or a
      ContentFile envelope; set `self.career`; call `self.touch()`.
- [ ] Verify existing `save_all` works for single-entry.

### 4. Ecosystem editor (`reachlock-editor/src/editors/ecosystem.rs`)

- [ ] Same set as career: `snapshot`, `restore_snapshot`, `preview_ui`,
      `delete_selected`, `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show planet seed, biome count, complexity.

### 5. PlanetCulture editor (`reachlock-editor/src/editors/planet_culture.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show cultural ID, language, social structure.

### 6. Theme editor (`reachlock-editor/src/editors/theme.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show theme ID, scale, note count, BPM range.

### 7. Trope editor (`reachlock-editor/src/editors/trope.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show trope ID, type, slot count, branch count.

### 8. ScriptedEncounter editor (`reachlock-editor/src/editors/scripted_encounter.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show encounter ID, type, scene count, trigger.

### 9. Dialogue editor — graph canvas (`reachlock-editor/src/editors/dialogue.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show start node, total node count, node types summary.
- [ ] **Graph canvas** (see Freeze first contract): replace the current
      `ui()` label dump with a pannable/zoomable visual node graph editor.
- [ ] Node types rendered with distinct header colours:
      - `NpcLine` / `PlayerChoice` / `ConditionGate` / `End`
- [ ] Drag-to-connect edges between node ports.
- [ ] Double-click node text to edit inline.
- [ ] Right-click context menu: Add Child, Delete Node, Set as Start.
- [ ] Coordinate state stored internally (not serialised — the `Dialogue`
      struct is the canonical representation; node positions are ephemeral
      and reset on load).

### 10. Dungeon editor — grid canvas (`reachlock-editor/src/editors/dungeon.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show dungeon ID, room count, puzzle count, enemy count.
- [ ] **Grid canvas** (see Freeze first contract): replace the current `ui()`
      label dump with a tiled grid view for room layout.
- [ ] Rooms rendered as coloured rectangles on the grid, movable by drag,
      resizable by edge/corner drag.
- [ ] Room inspector panel (side or bottom) shows selected room properties.
- [ ] Rooms snap to tile boundaries.
- [ ] Right-click context menu: Add Room, Delete Room, Edit Tags,
      Add Connector.

### 11. Event editor (`reachlock-editor/src/editors/event.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show event ID, stage count, trigger summary.

### 12. Recipe editor (`reachlock-editor/src/editors/recipe.rs`)

- [ ] Same set: `snapshot`, `restore_snapshot`, `preview_ui`, `delete_selected`,
      `selected_entry_name`, `apply_ai_json`.
- [ ] `preview_ui()`: show recipe ID, ingredient count, output item+quantity,
      workbench type.

### 13. Editor completeness gate (`reachlock-editor/src/app.rs`)

- [ ] Add the table-driven test from Freeze first to the `app.rs` test module.
- [ ] Remove or update the existing `every_content_type_is_registered` test
      (it is a subset of the new gate; the new gate supersedes it).

## Acceptance gates

```
# Completeness gate — the primary gate
cargo test -p reachlock-editor editor_completeness_gate

# All existing tests still pass
cargo test -p reachlock-editor

# Browser shows all 24 file-backed types
# File → New shows every type in a clickable submenu
make check
```

Manual: launch `cargo run -p reachlock-editor` → Content Browser shows Career,
Ecosystem, PlanetCulture, Theme, Trope, ScriptedEncounter, Dialogue, Dungeon,
Event, Recipe sections (may be empty directories) → File → New shows each of
those types in a submenu → open each editor type → right-panel shows a
preview card → Ctrl+Z undoes a change → Ctrl+Shift+Z redoes → Delete key
shows confirmation (single-entry editors say nothing to delete) → AI bar
generates content for each type (requires an AI provider configured).

## Non-goals

- New editors for types that already have full trait coverage (soul, hull,
  station, etc.)
- Validation beyond the existing `validate()` method (S69 adds cross-reference
  validation)
- Cross-reference index, go-to-definition, rename-with-referrers (S69)
- Diff-before-save, comment preservation warnings (S69)
- Server-side content dispatch (S81)
- Editor for `ItemBrowser` or `SpriteViewer` (previewers — not file-backed)
- Changes to the `Editor` trait itself (stable after S65)
- Multi-entry save_all for editors that are currently single-entry (they are
  single-entry by design, mirroring the data model)

## Gotchas

- **Multi-entry vs single-entry save pattern.** SoulEditor uses multi-entry
  `save_all` with per-entry path recording (`soul.rs:1097-1127`). The 10
  editors here are single-entry — their `save_all` delegates to `save` and
  records the path. This is correct. Do NOT blindly copy soul's multi-entry
  loop; single-entry editors should use the simpler pattern seen in
  `ecosystem.rs:52-62` or `theme.rs:49-58`.
- **Snapshot serialization.** `snapshot()` returns a `ron::to_string` of the
  editor's data. Single-entry editors snapshot the single struct. Make sure
  the core types (CareerPath, Ecosystem, PlanetCulture, Theme, Trope,
  ScriptedEncounter, Dialogue, Dungeon, Event, Recipe) all implement
  `Serialize`/`Deserialize`. If a core type's field uses a type that doesn't
  derive serde, add the derive. Core types must compile to wasm32 — serde is
  already present on all gameplay types.
- **apply_ai_json envelope handling.** AI models may emit either a bare payload
  or a `ContentFile` envelope. Model on `soul.rs:1039-1065` which tries bare
  deserialization first, then falls back to
  `ai::extract_inner_from_envelope`. Each editor's `apply_ai_json` should
  follow this two‑attempt pattern.
- **Canvas coordinate storage.** Graph node positions (Dialogue) and room grid
  positions (Dungeon) are ephemeral view state — they must NOT be serialised
  into the `Dialogue`/`Dungeon` data struct. Store them in the editor struct
  as a `HashMap<String, Pos2>` or similar, reset on load. The gate network
  editor at `gate_network.rs:55-70` shows the pattern.
- **Dialogue graph layout.** First render of a loaded dialogue places nodes
  in a simple vertical or horizontal auto-layout (no crossing minimisation —
  the author rearranges manually). Use a basic topological sort from the
  start node and stack them left-to-right.
- **Dungeon grid origin.** Place (0,0) at the top-left of the grid canvas, not
  bottom-left. Room coordinates stored in the Dungeon struct are in tile units;
  the grid canvas renders them at `(x * TILE_SIZE, y * TILE_SIZE)`.
- **Existing editors are untouched.** Do NOT modify `soul.rs`,
  `gate_network.rs`, `hull_frame.rs`, `station.rs`, `location.rs`,
  `contract.rs`, `faction.rs`, `economy.rs`, `storyline.rs`, `item.rs`,
  `enemy.rs`, `charted_system.rs`, `hull_mesh.rs`, `room_templates.rs`,
  `character_sprite.rs`, or `item_browser.rs`. This sprint only touches the
  10 listed editors, `browser.rs`, `main.rs`, and the test in `app.rs`.
- **The `FILE_TYPES` constant is not the registry.** Registry registration
  (via `build_default_registry` in `app.rs:302`) is a separate concern from
  `FILE_TYPES` in `browser.rs`. Both must be updated. The registry already
  covers all 26 types (`app.rs:304-377`). If it doesn't, fix that too.
- **RON round-trip drops comments.** The gotcha from the index applies: if
  any of the 10 content types has hand-written comments in its authored `.ron`
  files, a save through the editor will lose them. This is existing behaviour,
  not new. Document in the commit message if this is a concern for any type.
