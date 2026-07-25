# S108 — ContentIndex Per-Type HashMap Indexing

**Wave: UX-Hardening · Depends on:** S01 (Content pipeline)

## Outcome

`ContentIndex` is extended with per-type `HashMap`s for O(1) lookup of stations, souls, items, factions, and other content types by ID. The existing `find_station_by_seed()` O(n) scan becomes an O(1) map lookup. All content loaders are updated to populate the maps during indexing.

## Context

The current `ContentIndex` stores all content in a flat `Vec<ContentFile>`:

```rust
pub struct ContentIndex {
    pub files: Vec<ContentFile>,
    // ... existing maps for hostile_archetypes, hostile_locations, charted_systems, gate_network, themes
}
```

Lookup by seed is O(n):
```rust
pub fn find_station_by_seed(&self, seed: u64) -> Option<&ContentFile> {
    self.files.iter().find(|f| f.asset_type == ...Station && f.seed == seed)
}
```

This is fine for 100-200 files but doesn't scale to modded content (1000+ files). More importantly, there is no lookup by ID (the `id` field from the `ContentFile` envelope), which is the natural key for stations, souls, factions, and items.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/content_index.rs` | ContentIndex struct + loader — add maps |
| `reachlock-client/src/systems/dispatch.rs` | Content dispatch — repopulate maps after dispatch |
| `reachlock-client/src/systems/soul.rs` | Soul lookup — can use new map |
| `reachlock-client/src/systems/shipeditor/mod.rs` | `frame_for()` — can use new map |

## Freeze first

### New ContentIndex fields

```rust
pub struct ContentIndex {
    pub files: Vec<ContentFile>,
    pub mod_manifests: HashMap<String, ModManifest>,
    pub load_order: Vec<String>,
    
    // Existing per-type maps
    pub hostile_archetypes: HashMap<String, HostileArchetype>,
    pub hostile_locations: HashMap<String, HostileLocation>,
    pub charted_systems: HashMap<String, ChartedSystem>,
    pub gate_network: Option<GateNetwork>,
    pub themes: HashMap<String, Theme>,
    
    // NEW: per-type indexed maps (keyed by ContentFile.id)
    pub stations: HashMap<String, ContentFile>,
    pub souls: HashMap<String, ContentFile>,
    pub factions: HashMap<String, ContentFile>,
    pub items: HashMap<String, ContentFile>,
    pub contracts: HashMap<String, ContentFile>,
    pub hull_frames: HashMap<String, ContentFile>,
    pub hull_meshes: HashMap<String, ContentFile>,
    pub room_templates: HashMap<String, ContentFile>,
    pub origins: HashMap<String, ContentFile>,
    pub careers: HashMap<String, ContentFile>,
    pub ecosystems: HashMap<String, ContentFile>,
    pub planet_cultures: HashMap<String, ContentFile>,
    pub tropes: HashMap<String, ContentFile>,
    pub scripted_encounters: HashMap<String, ContentFile>,
    pub dialogues: HashMap<String, ContentFile>,
    pub dungeons: HashMap<String, ContentFile>,
    pub events: HashMap<String, ContentFile>,
    pub recipes: HashMap<String, ContentFile>,
    // ... one map per AssetType variant
    
    // Content load error log (from S107)
    pub errors: Vec<ContentErrorNotification>,
}
```

### Index building function

```rust
/// Populate all per-type maps from the flat `files` vec.
/// Called after content loading completes (in `load_content_index`).
fn build_index_maps(files: &[ContentFile]) -> IndexMaps {
    // Build struct with all maps populated
}
```

Alternative: make a single `HashMap<AssetType, HashMap<String, ContentFile>>` to avoid 20+ named fields:
```rust
pub struct ContentIndex {
    pub files: Vec<ContentFile>,
    /// files indexed by type, then by id. O(1) lookup by (AssetType, id).
    pub by_type: HashMap<AssetType, HashMap<String, ContentFile>>,
    /// files indexed by seed (for seed-based lookup).
    pub by_seed: HashMap<u64, Vec<ContentFile>>,
}
```

The nested-map approach is simpler and automatically covers every `AssetType` variant.

## Deliverables

### 1. Add index maps to ContentIndex

- [ ] Add `by_type: HashMap<AssetType, HashMap<String, ContentFile>>` field
- [ ] Add `by_seed: HashMap<u64, Vec<ContentFile>>` field (seeds can collide)
- [ ] Implement `build_index()` function called at the end of `load_content_index`

### 2. Replace O(n) lookups with O(1) map lookups

- [ ] `find_station_by_seed()`: use `by_seed[&seed].iter().find(|f| f.asset_type == Station)` — much smaller search space
- [ ] `frame_for()` in `shipeditor/mod.rs`: use `by_type[&HullFrame][&hull_id]` instead of iterating all files
- [ ] Any other linear scans over `self.files` should use the maps

### 3. Update dispatch to repopulate maps

- [ ] After `dispatch_content` adds/modifies content files, call `build_index()` to repopulate the maps
- [ ] OR: `dispatch_content` modifies the maps directly when inserting new files

### 4. Deprecate `find_station_by_seed`

- [ ] Keep the method but mark it `#[deprecated]` with a note to use the map
- [ ] Add a new method `station_by_seed(&self, seed: u64) -> Option<&ContentFile>` that uses the map

### 5. Add coverage test

- [ ] Test: after loading content, every `AssetType` variant either has entries in `by_type` or is empty (not missing)
- [ ] Test: `by_type` entries match `files.iter().filter(|f| f.asset_type == t).count()`

## Acceptance gates

```bash
cargo test -p reachlock-client content_index
cargo clippy -p reachlock-client -- -D warnings

# Verify: no regressions
# Stations, souls, factions still load and are reachable

make check
```

## Non-goals

- Content hot-reload (still requires restart)
- Multi-key indexing (just id and seed)
- Index persistence (maps are rebuilt on every startup)
- Removing the `files` Vec (it's still the source of truth)

## Gotchas

- **Seeds can collide across content types.** A station and a soul could share seed `42`. The `by_seed` map uses `Vec<ContentFile>` to handle collisions. Filter by `asset_type` after the map lookup.
- **`AssetType` must implement `Hash + Eq`.** Verify it derives these. The `ContentFile` envelope has an `asset_type: AssetType` field — use that as the outer map key.
- **Nested HashMap may have empty inner maps.** Don't panic on `by_type.get(&Station)` returning `None` — return an empty result instead.
- **Index building is O(n).** It's called once at startup. For 200 files, it's negligible.
- **The `by_type` map uses `ContentFile.id` as the inner key.** Ensure every `ContentFile` has a non-empty `id` before inserting. Warn on empty IDs and skip them.
