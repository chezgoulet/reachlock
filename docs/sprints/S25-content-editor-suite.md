# S25 — Content Editor Suite

**Spec:** §10 (content pipeline), §23 (modding framework) · **Wave 7 (tooling) ·
Depends on:** S01 (content pipeline), S04 (generators), S05 (items), core types

## Outcome

A standalone desktop application (`reachlock-editor`) built on Bevy + egui
that provides a full suite of visual content creation tools. Every content
type — hulls, stations, locations, souls, dialogue/contracts, factions,
economy goods, storylines, items, enemies — gets a purpose-built editor with
live preview and schema validation. Every editor supports a "Generate from
Seed → Preview → Lock/Unlock Fields → Edit → Save as Authored" workflow,
making procedural generation the starting point for hand-crafted content.

Content velocity is the business model. This tool makes creating content
drops as frictionless as possible.

## Context

- The content pipeline (S01) already validates `.ron` files against JSON
  schemas via `reachlock content validate`. The editor saves the same `.ron`
  format — no new schema layer, no format conversion.
- All generators (`generator/hull.rs`, `generator/station.rs`, etc.) are pure
  functions producing plain-data structs. The editor calls them directly for
  the seed workflow.
- `bevy_inspector_egui` is already in the workspace — egui is proven in this
  project. The editor uses `bevy_egui` directly for its UI; Bevy manages the
  native window and the 3D preview render pass.
- This sprint is self-contained: zero game-client dependencies, zero server
  dependencies. Only depends on `reachlock-core`.
- S17/S18 provide player-facing ship editors in-game. S25 provides the
  developer/modder-facing content creation tool. They serve different users
  but share some patterns (grid placement, seed preview).

## Freeze first

### Editor trait + registry (`src/app.rs`)

```rust
pub trait Editor {
    fn title(&self) -> &str;
    fn content_type(&self) -> ContentType;
    fn has_unsaved_changes(&self) -> bool;
    fn load(&mut self, path: &Path) -> Result<(), EditorError>;
    fn save(&self, path: &Path) -> Result<(), EditorError>;
    fn validate(&self) -> Vec<ContentValidationError>;
    fn ui(&mut self, ctx: &egui::Context, preview: &mut PreviewPanel);
    fn generate_from_seed(&mut self, seed: u64, rng: &mut SeededRng);
}

pub enum ContentType {
    HullFrame, Station, Location, Soul, Contract, Faction,
    EconomyGoods, Storyline, Item, EnemyArchetype,
}
```

### Preview panel (`src/preview.rs`)

```rust
pub struct PreviewPanel {
    pub mesh: Option<Handle<Mesh>>,
    pub texture: Option<Handle<Image>>,
    pub camera: OrbitCamera,
}

impl PreviewPanel {
    pub fn set_mesh(&mut self, mesh: Handle<Mesh>);
    pub fn set_texture(&mut self, texture: Handle<Image>);
    pub fn render_to_egui(&self) -> egui::Image;
    pub fn orbit_ui(&mut self, response: &egui::Response);
}
```

### Seed workflow (`src/seed_workflow.rs`)

```rust
pub struct SeedWorkflow {
    pub seed: u64,
    pub locked_fields: HashSet<String>,
    pub generated: Option<serde_json::Value>,
}

impl SeedWorkflow {
    pub fn ui(&mut self, ui: &mut egui::Ui, editor: &mut dyn Editor);
    pub fn reroll_unlocked(&mut self, editor: &mut dyn Editor);
}
```

### IO layer (`src/io.rs`)

```rust
pub fn read_ron<T: DeserializeOwned>(path: &Path) -> Result<T, EditorError>;
pub fn write_ron<T: Serialize>(path: &Path, value: &T) -> Result<(), EditorError>;
pub fn validate_against_schema(content_type: ContentType, value: &serde_json::Value) -> Vec<ContentValidationError>;
```

Wire tests: round-trip `read_ron → write_ron → read_ron` produces identical structs.
Schema validation produces the same errors as `reachlock content validate`.

### GUI registry entry

Each editor module exports a constructor:
```rust
pub fn create_editor() -> Box<dyn Editor>
```
registered in `app.rs`'s `EditorRegistry` by `ContentType`.

## Deliverables

### 1. Crate scaffold

- [ ] `reachlock-editor/Cargo.toml`: depends on `reachlock-core`, `bevy` (core +
      render + wgpu), `bevy_egui`, `egui`, `ron`, `serde_json`
- [ ] `reachlock-editor/src/main.rs`: Bevy app with `DefaultPlugins`,
      `EguiPlugin`, editor systems, window title "ReachLock Content Editor"
- [ ] Workspace `Cargo.toml`: add `reachlock-editor` member
- [ ] `cargo run -p reachlock-editor` launches a native window

### 2. Application shell (`src/app.rs`)

- [ ] Menu bar: File (New, Open, Save, Save As, Close), Edit (Undo stubbed,
      Redo stubbed), View (editors list, validation panel toggle)
- [ ] Editor tab bar: open editors as tabs, unsaved indicator (*), close button
- [ ] Status line: file path, validation error count, last save timestamp
- [ ] Validation panel: dockable bottom panel listing errors per-editor with
      file + field + message

### 3. IO layer (`src/io.rs`)

- [ ] `read_ron` / `write_ron` with `PrettyConfig` matching existing file style
- [ ] `validate_against_schema` reads `content/schemas/*.schema.json`, validates
      the editor's current value, returns structured errors
- [ ] Save hook: on `Ctrl+S` or menu, validate before writing, refuse to save
      with errors (offer "save anyway" override)
- [ ] Unsaved-changes tracking per editor tab

### 4. Preview panel (`src/preview.rs`)

- [ ] Bevy `Camera3d` rendering to a `RenderTarget` → `egui::Image`
- [ ] Orbit camera: drag to rotate, scroll to zoom, middle-drag to pan
- [ ] `set_mesh()` / `set_texture()` API for editors
- [ ] Auto-fit: camera positions to frame the loaded asset on first display
- [ ] Reset view button

### 5. Seed workflow (`src/seed_workflow.rs`)

- [ ] Seed input: text field with validation (≤ 2^53), hex/decimal toggle
- [ ] Generate button: calls `editor.generate_from_seed()`, refreshes preview
- [ ] Lock/unlock toggle per field: locked fields survive re-rolls
- [ ] "Save as Authored" button: writes to a new `.ron` file with
      `AssetSource::HandCrafted` and `priority: authoritative`
- [ ] Re-roll unlocked button: regenerates seed, keeps locked fields

### 6. Content browser (`src/browser.rs`)

- [ ] File tree of `content/` directory (recursive), grouped by subdirectory
      (hulls/, stations/, souls/, factions/, economy/, storylines/, locations/)
- [ ] Filter by `ContentType` dropdown
- [ ] Double-click to open in the appropriate editor
- [ ] New file: select ContentType → prompts for filename → opens empty editor
      with a "Generate from Seed" banner if the type supports it

### 7. Hull frame editor (`src/editors/hull.rs`)

- [ ] Class picker: Shuttle, Freighter, Corvette, Station, Rock
- [ ] Hardpoint slot grid: overlay on hull wireframe, click to select/remove,
      size class picker per slot (Small, Medium, Large, Capital)
- [ ] Engine mount placement: drag anchor points on the hull
- [ ] Zone definitions: paint grid cells into zones (plating, paint, decal slots)
- [ ] 3D preview: orbits the wireframe with slot markers
- [ ] Seed workflow: `generate_hull_class(seed, class)` → editable wireframe
- [ ] Save: `content/hulls/<name>.ron`

### 8. Station / Location grid editor (`src/editors/station.rs`)

- [ ] Room grid: drag templates from a palette onto a scrollable grid
      (same pattern as S18's interior editor)
- [ ] Template palette: room type dropdown, size display, furniture slot count
- [ ] Door connectors: click room A door → click room B door → auto-path
      corridor (L-shaped, deterministic)
- [ ] NPC spawn markers: place on grid, assign soul ID from dropdown
      (reads `content/souls/` directory)
- [ ] Encounter markers: place, assign enemy archetype, set trigger condition
      (on_enter, on_interact, on_timer)
- [ ] Connectivity validation: highlights unreachable rooms in red
- [ ] 2D wireframe preview: top-down view with room labels
- [ ] Seed workflow: `generate_station(seed, StationKind, Biome, size)` →
      editable grid
- [ ] Save: `content/stations/<name>.ron`

### 9. Soul editor (`src/editors/soul.rs`)

- [ ] Text fields: name, species, backstory (multi-line)
- [ ] Species dropdown: Human, Synthetic, Voidborn, Augmented, Xenotype
- [ ] Speaking style: sliders for formality, verbosity, humor, aggression,
      technical_depth, fatalism — each 0.0–1.0
- [ ] Mood memory editor: list of memory entries (text + mood weight slider)
- [ ] Portrait preview: `generate_portrait(seed, species, style_params)` →
      rendered as 2D texture. Re-roll button changes only portrait_seed.
- [ ] Seed workflow: generates a full soul from seed, all fields editable
- [ ] Save: `content/souls/<name>.ron`

### 10. Dialogue / Contract editor (`src/editors/dialogue.rs`)

- [ ] Rule table: add/remove/reorder rules. Each row: trigger dropdown (Timer,
      Event, StateChange, Manual), condition builder (field + op + value),
      action picker (wake_crew, maintain_course, stand_down, repair_nearest,
      fire_weapons, etc.), priority spinner
- [ ] LLM fallback config: enabled toggle, timeout_ms, max_tokens,
      fallback_action dropdown, system_prompt text area, persona dropdown
      (soul IDs from content/souls/)
- [ ] Dry-run simulator: select a game state preset → shows which rule fires
      and whether deliberation would be invoked. Highlights the active rule
      path in green, deliberation edges in amber.
- [ ] Trigger condition builder: composite conditions (AND/OR/NOT), preview
      as readable text
- [ ] Save: `content/contracts/<name>.ron`

### 11. Faction editor (`src/editors/faction.rs`)

- [ ] Basic info: id, name, territory (SystemClaim list — add/remove system_id +
      claim_type)
- [ ] Relation matrix: grid of faction pairs, status dropdown per pair
      (Allied, Friendly, Neutral, Hostile, War), treaty optional config
- [ ] Doctrine dropdown: Military, Economic, Diplomatic, Expansionist
- [ ] Internal divisions table: add/remove, name, influence slider (0.0–1.0),
      agenda dropdown (Hawkish, Dovish, Mercantile, Isolationist),
      player_standing
- [ ] Goals list: add/remove with description text
- [ ] Save: `content/factions/<name>.ron`

### 12. Economy goods editor (`src/editors/economy.rs`)

- [ ] Goods table: id, display_name, category dropdown (RawMaterial,
      Manufactured, Luxury, Contraband, Fuel, Medical, Tech, Food),
      base_price, weight, rarity dropdown
- [ ] Production chain editor: per-good, multi-select from station types
      (MiningStation, Refinery, IndustrialOutpost, OrbitalWorkshop, etc.)
- [ ] Consumption chain editor: per-good, multi-select from station types
- [ ] Price curve preview: simple chart showing base_price * demand/supply
      ratio at equilibrium
- [ ] Save: `content/economy/<name>.ron`

### 13. Storyline editor (`src/editors/storyline.rs`)

- [ ] Chapter list: add/remove/reorder, chapter id + narration text
- [ ] Trigger condition builder (shared widget with dialogue editor):
      TickCondition, PlayerReputation, ChapterComplete, AND/OR composites
- [ ] Chapter events: add/remove, event type dropdown (FactionMove,
      DiplomaticShift, ContentRelease, MissionUnlock, NPCUpdate),
      parameters per type
- [ ] Chapter flow diagram: simple tree view showing chapter dependencies
      and unlock paths
- [ ] Save: `content/storylines/<name>.ron`

### 14. Item editor (`src/editors/item.rs`)

- [ ] Family picker: ItemFamily dropdown (18 variants)
- [ ] Tier selector: 1–10
- [ ] Stat band table per stat key (Damage, Range, FireRate, ShieldHp, etc.):
      min/max sliders for each tier. Preview shows the stat range at
      current tier.
- [ ] Rarity weights: per-tier probability table (Common through Legendary)
- [ ] Name template: adjective + material + base pattern preview
- [ ] Icon preview: `generate_item(ItemSeed {..})` → render 2D texture
- [ ] Seed workflow: roll from seed, edit stat bands, save as authored item
- [ ] Save: `content/items/<name>.ron`

### 15. Enemy archetype editor (`src/editors/enemy.rs`)

- [ ] Stat sheet: hull, shield, speed, turn_rate — fixed-point sliders
- [ ] Behavior weights: patrol_weight, engage_weight, evade_threshold,
      retreat_threshold, reinforce_range — all sliders
- [ ] Attack config: weapon family picker, damage mult, fire_rate mult,
      range_mod
- [ ] Telegraph config: windup_ticks, active_ticks, recovery_ticks,
      telegraph_flash_color (palette picker)
- [ ] Combat sim: runs a quick dummy fight against default stats, shows
      average damage per tick and time-to-kill
- [ ] Save: `content/hostiles/<name>.ron`

### 16. Location / dungeon editor (`src/editors/location.rs`)

- [ ] Room grid (reuses station editor grid widget): room definitions with
      room kind, size, connections
- [ ] Encounter table: per-room, add/remove enemy archetypes with spawn count
      and trigger
- [ ] Boss config: room assignment, enemy archetype, phase definitions
      (HP threshold → behavior change, new attack), phase count add/remove
- [ ] Reward cache: per-room or per-boss, item list (item ID + quantity)
- [ ] Keycard / locked door: door → keycard pair editor (keycard in room A
      unlocks door to room B)
- [ ] Save: `content/locations/<name>.ron`

### 17. Round-trip determinism

- [ ] For every editor that supports seed workflow: a test generates content
      from seed, saves to `.ron`, loads, verifies struct equality
- [ ] Determinism manifest: add `editor_roundtrip` golden entries for each
      content type (manifest version bump)
- [ ] Test: `cargo test -p reachlock-editor roundtrip::`

## Acceptance gates

```
cargo run -p reachlock-editor                     # launches, no panic
cargo test -p reachlock-editor                    # all round-trip and validation tests
reachlock content validate <files-saved-by-editor> # passes for every editor
make check                                         # fmt + clippy (editor exempts WASM)
```

Manual: open each editor, generate from seed, modify 3 fields, save. Re-open
the saved file. Verify it loads with same values. Run the game — verify the
authored content appears in-game.

## Non-goals

- WASM build (editor is a native desktop tool — add `exclude = ["reachlock-editor"]`
  to the WASM build step)
- In-game integration (S17/S18 are player-facing; this is for devs)
- Mod distribution GUI (S22 packages `.reachmod` files — this editor produces
  `.ron` files that S22's mod manager will load)
- Multi-user collaboration, version history, diff/merge
- Undo/redo (stubbed in menu bar, Phase 4 polish)
- PNG/WAV asset import from external tools (Phase 4)
- The bridge layer reuse: the editor links `reachlock-core` directly and
  duplicates the ~30 lines of mesh conversion it needs. No dependency on
  `reachlock-client`.

## Gotchas

- Bevy `RenderTarget` → egui texture: the egui `Image` handle must survive
  tab switches — use a `Handle<Image>` cached in the `PreviewPanel` resource,
  not a per-frame allocation.
- `.ron` round-trip fidelity: `ron::ser::PrettyConfig` must match the
  existing content file style (indent, struct pretty, etc.). Test with a
  diff against existing `content/*.ron` files after save.
- The editor loads `content/schemas/*.schema.json` at runtime from the
  filesystem. The schemas must be distributed alongside the binary or found
  relative to the working directory. Use `env!("CARGO_MANIFEST_DIR")/../content/schemas/`
  for development; document the production path.
- Large meshes (stations with many rooms) need the preview camera on its own
  render layer to avoid frame drops while editing text fields.
- `generate_from_seed` must be deterministic (iron rule 3) — the preview must
  match what the game's generator produces from the same seed. The editor
  calls the same core generator functions, so this is inherent — but verify
  with the round-trip test.
- The editor is native-only (`target: native`). `make check` currently includes
  `cargo build --target wasm32-unknown-unknown` — exclude `reachlock-editor`
  from the WASM build step in the Makefile.
