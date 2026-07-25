# S81 — Content Dispatch & The Contract Pipeline

**Spec:** New (content dispatch architecture) · **Wave B (the systems you already paid for)** · **Depends on:** S64 (green tree), S65 (editor trait fix — not strictly needed, but the dispatch layer touches 10+ files and green tree makes that safe)

**⚠ Highest-leverage sprint in the entire MASTER-PLAN.** This makes the game's signature mechanic real for the first time — the player writes rules their ship runs on, and those rules actually run.

## Outcome

A `ContentDispatcher` registry maps every `ContentPayload` variant to the consumer that loads it into the game world. No content file is loaded from disk and dropped on the floor. `content_index.themes` (`HashMap<String, Theme>`) is wired into `music.rs` so authored themes drive the procedural audio engine. `ContractRuntime` holds a **set** of contracts loaded from `ContentPayload::Contract` files, player-crafted contracts from the workshop, and imported contracts from the library — replacing the single hardcoded `auto_helm()`. The contract crafting workshop and library gain install buttons that push contracts into the runtime. A test asserts that every `ContentPayload` variant has a registered consumer, preventing the dark-system failure mode from recurring.

## Context

- MASTER-PLAN.md findings **D6, D7, D8, D9** and **P1**. D6 and D8 are critical — 10 of 15 content payload variants are loaded from disk and dropped on the floor, and the authored contract pipeline terminates in a HashMap nobody reads. D9 means the 898-line contract crafting workshop (S34) and 376-line contract library (S34) cannot install their result into the live runtime. The player can craft a contract, validate it, share it, and import someone else's — it can never execute. The ship runs `auto_helm()` forever.
- The content index (`content_index.rs:142`) calls `walk(root, &mut files)` which recursively parses every `ContentFile` envelope into a `Vec<ContentFile>`. Exactly one system consumes it: `soul::init_souls`. Everything else (careers, contracts, dialogue, dungeons, ecosystems, events, planet cultures, recipes, scripted encounters, themes, tropes) is parsed and dropped.
- `content_index.themes` is a `HashMap<String, Theme>` populated at `content_index.rs:133`. The only reference to `.themes` outside the loader is `#[allow(dead_code)]`. `music.rs` has no reference to `Theme` at all — the S48 authored music theme pipeline terminates in an unread HashMap.
- `ContractRuntime` (contract.rs:72) has exactly one field: `contract: Contract`, initialized to `auto_helm()` in `Default::default()`. Only two systems hold `ResMut<ContractRuntime>` — the evaluator itself (contract.rs:161) and network sync (network.rs:144). Neither replaces the contract.
- The contract crafting workshop (`contract_crafting.rs:163`) holds a `draft: Option<Contract>` and has no way to push it to the runtime. The library (`contract_library.rs:38`) holds a `entries: Vec<ContractLibraryEntry>` with no install path.

## Freeze first

### `ContentDispatcher` — the registry of consumers

New module at `reachlock-client/src/systems/dispatch.rs` (or inline in `content_index.rs`). Maps every `ContentPayload` variant to a closure that loads it into the appropriate system.

```rust
/// A consumer receives a list of ContentFiles for its variant and integrates
/// them into the game world. Returns a list of errors (file id + error
/// message) for files that failed to load. Non-fatal — a single bad file
/// doesn't block the rest.
pub type ContentConsumer = fn(&[ContentFile]) -> Vec<(String, String)>;

/// Registry of consumers, one per ContentPayload variant.
/// Every variant in `ContentPayload` must have exactly one entry.
/// Missing entries are caught by `consumer_coverage_test`.
pub struct ContentDispatcher {
    consumers: HashMap<PayloadVariantDiscriminant, ContentConsumer>,
}

impl ContentDispatcher {
    /// Register a consumer for a payload variant.
    pub fn register(payload: ContentPayloadVariant, consumer: ContentConsumer);

    /// Dispatch all files in `index` to their registered consumers.
    /// Returns aggregated errors from every consumer.
    pub fn dispatch_all(&self, index: &ContentIndex) -> Vec<(String, String)>;

    /// Returns the set of registered variant discriminants — for the
    /// completeness test.
    pub fn registered_variants(&self) -> HashSet<PayloadVariantDiscriminant>;
}
```

Where `PayloadVariantDiscriminant` is a unit enum matching `ContentPayload` variant for variant:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentPayloadVariant {
    Hull,
    Station,
    Contract,
    Ecosystem,
    Career,
    PlanetCulture,
    Theme,
    Trope,
    ScriptedEncounter,
    Dialogue,
    Dungeon,
    Event,
    Recipe,
    Soul,
    HullFrame,
    RoomTemplates,
}
```

(NOTE: Use a macro to derive this from `ContentPayload` so they can't drift. The discriminant enum pairs with `AssetType` — `ContentFile.asset_type` determines routing.)

### `ContractRuntime` — from one contract to a set

```rust
#[derive(Resource)]
pub struct ContractRuntime {
    /// All loaded contracts, keyed by id. Always contains at least one
    /// (the auto-helm default, or the first authored contract).
    pub contracts: HashMap<String, Contract>,
    /// The currently active contract id. Switched by the player, or by
    /// context (docking switches to a station approach contract).
    pub active_id: String,
    pub eval_timer: Timer,
    last_action: Option<String>,
    chain: SignatureChain,
    next_tick: u64,
    recent_uncovered: u8,
}

impl ContractRuntime {
    /// Install a contract: add or replace by id. Makes it the active
    /// contract. Called by:
    ///   - `ContentDispatcher` on loading authored contracts
    ///   - Workshop "Install" action
    ///   - Library "Install" action
    pub fn install(&mut self, contract: Contract);

    /// Switch to a different loaded contract by id. No-op if unknown.
    pub fn activate(&mut self, id: &str);

    /// List all installed contract ids.
    pub fn list(&self) -> impl Iterator<Item = &str>;
}
```

The active contract is the one `evaluate_contracts` uses. Player can switch with a new "Contract Selection" panel (or a dedicated HUD key).

### Content dispatch wire — `init_souls` replacement

The current `soul::init_souls` system (which filters `index.files` for `AssetType::Soul`) is replaced by the generic dispatch layer. Each consumer receives only its own variant's files:

| Variant | Consumer | Destination |
|---|---|---|
| `Soul` | Existing `soul::init_souls` | `SoulRegistry` |
| `Hull` | Existing `resolve` path in `setup.rs` | `GeneratedMesh` lookup by seed |
| `Station` | Existing interior resolution | Station lookups |
| `HullFrame` | Existing `frame_for` in `shipeditor` | Hull frame catalog |
| `RoomTemplates` | Existing template loading | Interior editor |
| **`Contract`** | **NEW** → `install_contracts` | `ContractRuntime::install` |
| **`Career`** | **NEW** → `load_careers` | Career registry (if any) |
| **`Theme`** | **NEW** → `install_themes` | `MusicEngine` theme list |
| **`Dialogue`** | **NEW** → `load_dialogues` | Dialogue registry |
| **`Dungeon`** | **NEW** → `load_dungeons` | Dungeon registry |
| **`Ecosystem`** | **NEW** → `load_ecosystems` | Ecosystem override registry |
| **`Event`** | **NEW** → `load_scripted_events` | Event registry |
| **`PlanetCulture`** | **NEW** → `load_cultures` | Culture override registry |
| **`Recipe`** | **NEW** → `load_recipes` | Recipe registry |
| **`ScriptedEncounter`** | **NEW** → `load_encounters` | Encounter registry |
| **`Trope`** | **NEW** → `load_tropes` | Trope registry |

### `MusicEngine` theme integration

Current `MusicEngine` (music.rs:50) drives params from game state. Add:

```rust
pub struct MusicEngine {
    pub active_theme: Option<Theme>,
    pub theme_library: HashMap<String, Theme>,
}
```

`ContentDispatcher`'s `install_themes` populates `theme_library`. `MusicEngine` picks a theme based on location/faction context. When no authored theme matches, falls back to the existing procedural generation.

### Workshop/Library install path

```rust
// In workshop_system or a new action handler:
fn install_from_workshop(
    mut runtime: ResMut<ContractRuntime>,
    mut state: ResMut<ContractWorkshopState>,
) {
    if let Some(contract) = state.draft.take() {
        runtime.install(contract);
        state.status = "Installed! Press I to open contract selector.".into();
    }
}

// In library_system:
fn install_from_library(
    mut runtime: ResMut<ContractRuntime>,
    state: Res<ContractLibraryState>,
) {
    if let Some(entry) = state.entries.get(state.sel) {
        runtime.install(entry.contract.clone());
    }
}
```

## Deliverables

### 1. ContentDispatcher registry (`dispatch.rs` or `content_index.rs`)

- [ ] Define `ContentPayloadVariant` discriminant enum (derive from `ContentPayload` via macro or manual sync with coverage test).
- [ ] Define `ContentConsumer` type alias and `ContentDispatcher` struct with `register`, `dispatch_all`, `registered_variants`.
- [ ] Register consumers for all 16 `ContentPayload` variants. Existing consumers (soul, hull, station, hull_frame, room_templates) keep their current behaviour — just routed through the dispatcher.
- [ ] Wire `dispatch_all` into `load_content_index` (after Phase 4 walk, call `dispatcher.dispatch_all(&index)` instead of relying on individual systems to re-scan `index.files`).
- [ ] `dispatch_all` returns aggregated errors. Log each error at `warn!` level. One bad file doesn't block others.

**Gate:** The dispatcher builds. `make check` passes.

### 2. Consumer completeness test

- [ ] Table-driven test: for every `ContentPayloadVariant` variant, assert there is a registered consumer in the default `ContentDispatcher`.
- [ ] If a new variant is added to `ContentPayload` without a corresponding consumer, the test fails.
- [ ] Test iterates `registered_variants()` and compares against the full set of variants (obtained via strum::EnumIter or manual const array).

**Gate:** `cargo test dispatch::consumer_coverage_test` passes and covers all 16 variants.

### 3. `ContractRuntime` — set of contracts (`contract.rs`)

- [ ] Change `contract: Contract` to `contracts: HashMap<String, Contract>` + `active_id: String`.
- [ ] `Default::default()` inserts `auto_helm()` with id `"auto-helm"` and sets `active_id = "auto-helm"`.
- [ ] `install(contract)` inserts into map and sets `active_id` to that contract's id. If the id already exists, it's replaced.
- [ ] `activate(id)` sets `active_id` to the given id. No-op if id not found.
- [ ] Update `evaluate_contracts` to read `runtime.contracts.get(&runtime.active_id)` instead of `&runtime.contract`.
- [ ] Update `resolve_response` and `resolve_failed` to reference the active contract.
- [ ] Add a minimal "Contract Selector" panel (text-based for now) that lists installed contracts and lets the player switch active contract with a key (e.g., Tab cycles through them, or a dedicated `NextContract`/`PrevContract` InputAction).

**Gate:** Load a save with three authored contract files on disk → three contracts in runtime → player can switch between them → each evaluates differently. `auto_helm()` is the default but disappears once any authored contract is installed (or stays as a fallback).

### 4. Install path from workshop + library

- [ ] Workshop: add `Install` action (Enter on the SIM tab or a dedicated key). Calls `runtime.install(state.draft.take().unwrap())`.
- [ ] After install, workshop creates a fresh draft (same as opening the panel).
- [ ] Library: add `Install` action (Enter on a detail view or `I` key in list). Calls `runtime.install(entry.contract.clone())`.
- [ ] Both show a status message ("Installed! Switch to it with …").

**Gate:** Craft a contract → simulate → install → contract appears in runtime selector. Close workshop, open library, find a contract → install → both contracts available.

### 5. Wire `content.themes` into `MusicEngine` (`music.rs`)

- [ ] Remove `#[allow(dead_code)]` from `ContentIndex.themes`.
- [ ] Add `theme_library: HashMap<String, Theme>` to `MusicEngine` resource.
- [ ] Consumer `install_themes` copies themes from content index into `MusicEngine.theme_library`.
- [ ] `MusicEngine` selects an authored theme when the player enters a system with a matching theme id. The `MusicIntent` generator (`reachlock_core::generator::music`) already has a `theme: Option<Theme>` field — wire it.
- [ ] Fallback: when no authored theme matches, fall back to fully procedural generation (existing behaviour).
- [ ] Location/context → theme lookup: faction space stations override with that faction's theme; frontier systems with no authored theme use procedural.

**Gate:** Author a theme file (`content/themes/compact_march.ron`) → launch the game → dock at a Compact station → theme plays. Remove the file → procedural fallback plays.

### 6. Wire the remaining 9 variant consumers (stub-level)

Each of these consumers starts as a thin loader that:
- Parses the `ContentPayload` variant
- Stores it in a new `Resource` (a `HashMap<String, T>`)
- Can be extended by the relevant narrative/world sprint (S82, S83, S84)

Specific consumers:

- [ ] **Career consumer**: loads `CareerPath` into a `CareerRegistry` resource. Need a new `Resource` `CareerDatabase(HashMap<String, CareerPath>)`. (Can be empty — S79 fills real origins.)
- [ ] **Dialogue consumer**: loads `Dialogue` into a `DialogueRegistry` resource.
- [ ] **Dungeon consumer**: loads `Dungeon` into a `DungeonRegistry` resource.
- [ ] **Ecosystem consumer**: loads `Ecosystem` into an `EcosystemOverrideRegistry`.
- [ ] **Event consumer**: loads `Event` into an `EventRegistry`.
- [ ] **PlanetCulture consumer**: loads `PlanetCulture` into a `CultureOverrideRegistry`.
- [ ] **Recipe consumer**: loads `Recipe` into a `RecipeRegistry`.
- [ ] **ScriptedEncounter consumer**: loads `ScriptedEncounter` into an `EncounterRegistry`.
- [ ] **Trope consumer**: loads `TropeTemplate` into a `TropeRegistry`.

Each registry is registered in the dispatcher. Each has a test asserting it loads at least one file. The registries are minimal — storing data for future consumers in S82/S83/S84.

**Gate:** `cargo test dispatch::all_consumers_round_trip` — a test that places one file of each type in a temp `mods/` tree, runs `load_content_index`, and asserts each registry is non-empty.

## Acceptance gates

```
# Consumer coverage (prevents dark systems)
cargo test dispatch::consumer_coverage_test

# Content dispatch: all 16 variants load
cargo test dispatch::all_consumers_round_trip

# Contract switch test
cargo test contract::switch_contract
cargo test contract::install_from_crafting
cargo test contract::install_from_library

# Theme wiring
cargo test music::authored_theme_plays
cargo test music::procedural_fallback

make check
```

Manual:
1. Place three contract `.ron` files in `mods/reachlock/contracts/` → launch → open contract selector → cycle through all three → each one's rules execute
2. Open workshop → craft a contract → install → selector shows it → switch to it → auto-helm is replaced
3. Open library → browse → install → both available
4. Place a theme file → dock at matching station → hear themed music → undock → music returns to procedural
5. Delete the theme file → music is always procedural (no crash)

## Non-goals

- Full narrative system integration (dilemmas, scripted encounters, storylines) — that's S82
- Captain's Log integration — S83
- Living world ecosystem surfacing — S84
- Contract exchange (P2P sharing via server) — S86
- Originating conditions (starting careers, player identity) — S75/S78
- Full UI for contract selector beyond text-based panel. The contract selector is a minimal list that follows the existing `ActivePanel` pattern (text entity, keyboard nav). Polish is S70 (client UI framework).

## Gotchas

- The `ContentDispatcher` must run AFTER `load_content_index` finishes, not inside it. The resource insertion at `content_index.rs:153` is the signal that the index is ready. Register a startup system that runs after `load_content_index` and calls `dispatcher.dispatch_all`.
- `Soul` is already consumed by `soul::init_souls` which runs in `Startup` at `main.rs:218`. That system filters `index.files` for `AssetType::Soul`. After this sprint, the soul consumer is registered in the dispatcher and `init_souls` is replaced with the generic dispatch call. Verify nothing depends on `init_souls` running before `load_save` — the ordering in `main.rs:218-221` chains `load_content_index` → `init_souls` → `load_save`. The dispatch layer must maintain this ordering: souls loaded before save restoration.
- `ContractRuntime` changes from a single `Contract` to a `HashMap<String, Contract>`. All code that accesses `runtime.contract` must be updated to `runtime.contracts.get(&runtime.active_id)`. The compiler catches direct field access — but check for pattern matches like `let contract = &runtime.contract;` which silently works (it borrows the whole struct).
- The theme wiring changes `MusicEngine` to consult `theme_library` on context changes. The current music system is driven by `MusicParams` which is updated every frame. Add a once-per-context-change trigger (system change, station dock) to select a theme, then apply its params as a base overlay.
- The 9 new registries (Career, Dialogue, etc.) are mostly empty until the narrative sprints (S82-84). Their creation and registration in this sprint is deliberately minimal — the dispatcher just needs them to exist. Each registry is a `HashMap<String, T>` stored as a `Resource`. The coverage test checks that the dispatcher routes to them. No gameplay code reads them yet.
- The discriminant enum (`ContentPayloadVariant`) must stay in sync with `ContentPayload`. Manual sync with a test that asserts every `ContentPayload` variant maps to a `ContentPayloadVariant`. Better: use a macro that generates both from one source, or add a method `ContentPayload::variant(&self) -> ContentPayloadVariant` that matches by variant name.

**Gotcha-adjacent (add to 00-INDEX.md ledger):**
- `content_index.rs:214` skips specific directories (`combat`, `locations`, `systems`, `gate_network`, `themes`) in the `walk` function, because those dirs are loaded by `load_typed_into` and would be double-parsed as `ContentFile` envelopes. This is correct — the typed loaders handle those. After this sprint, the typed loaders are replaced by the dispatcher for most types, but the walk still skips those dirs (they're not `ContentFile` envelopes, they're raw typed files). The skip list does not need to change.
