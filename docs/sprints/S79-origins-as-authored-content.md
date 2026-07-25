# S79 — Origins as Authored Content

**Spec:** New (Origin content type, launch origins) ·
**Wave D (character & open world) · Depends on:** S78 (creation flow — the origin selection step), S81 (content dispatch — `OriginRegistry` consumer)

**Closes:** P5 (origins/backgrounds don't exist; careers have no distinctive rewards)

## Outcome

An `Origin` content type defines a character's starting conditions: career path + rank, faction standing deltas, credits, ship template, gear, crew, known systems, and opening log entries. Origins are authored `.ron` files loaded through the content dispatch layer and editable in the content suite (S68 pattern). Six to ten launch origins ship with the game, spanning all career path types (Military, Trade, Exploration, Science, Political, Criminal, Freelance) plus a "Loup-Garou veteran" that reconstructs the v1 starting state exactly. The origin selection step of S78 reads from this content to display background cards. Each launch origin has a test asserting it produces a playable opening state.

## Context

- **Origins don't exist (P5).** `CareerPath` has 7 path types with ranks, perks, and progression criteria — but no starting conditions package. A "Compact deserter" is a flavour concept with no mechanical footprint. `CrewRole` is a closed enum; `SaveFile` has no origin/background field. S75 added `origin_id: Option<String>` to `PlayerCharacter` — this sprint makes that field meaningful.
- **Career paths already exist** (`core/career/mod.rs`). `join_path`, `advance_rank`, `leave_path` with `CompletionReason` are fully implemented. `conflicting_paths` models career locks. An Origin references these directly — it's a starting conditions bundle, not a new progression system.
- **S75 frozen the wire shape.** `PlayerCharacter.origin_id` is a `String` identifying the origin content file. This sprint does not change the wire shape — it populates the content that the id references.
- **S78 needs origins.** The creation flow's origin step queries `OriginRegistry` for available origins and renders them as cards. Without this sprint, S78's origin step has nothing to show.
- **S81's content dispatch** provides the `OriginRegistry` resource and the consumer that loads `.ron` origin files into it. This sprint authors the files and builds the editor.
- **The Loup-Garou veteran origin** reconstructs exactly what v1 hardcoded: starting at Aethon (system seed `16843009`), captain Tib, the Loup-Garou hull, the six canonical crew (Tove, Keene, Bardo, Prudence, Risc, Boris), starting credits, Compact faction standing, and opening log entry. This origin is mechanically identical to today's `main.rs:105-111` + `default_crew()` — just expressed as data.
- **Offline-first:** origins are local `.ron` files in `mods/reachlock/origins/`. No server needed.

## Freeze first

### `Origin` struct definition

```rust
// reachlock-core/src/content/origin.rs
use reachlock_core::career::{CareerPathId, Rank};
use reachlock_core::faction::FactionStandingDelta;
use reachlock_core::item::ItemStack;
use reachlock_core::ship::ShipTemplateId;
use reachlock_core::soul::SoulFile;
use reachlock_core::util::rng::Seed;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Origin {
    /// Unique identifier (e.g. "compact_deserter", "loup_garou_veteran").
    pub id: String,
    /// Display name ("Compact Deserter").
    pub name: String,
    /// Flavour text shown on the card.
    pub description: String,
    /// Icon glyph or faction emblem id (for card rendering).
    pub icon: String,
    /// The career path and rank the character starts with.
    pub starting_career: CareerPathId,
    pub starting_rank: Rank,
    /// Faction standing adjustments applied on character creation.
    /// Positive *and* negative entries — faction doors close as well as open.
    pub faction_deltas: Vec<FactionStandingDelta>,
    /// Starting credits.
    pub starting_credits: u64,
    /// Ship template id (references a hull in the ship catalog).
    /// None = a default starter ship is granted.
    pub ship_template: Option<ShipTemplateId>,
    /// Seed override for the ship's procedural hull appearance.
    /// None = random seed on character creation.
    pub ship_seed: Option<Seed>,
    /// Starting inventory / gear.
    pub starting_gear: Vec<ItemStack>,
    /// Crew that come with this origin. Each entry can be:
    ///   - A reference to an authored soul file (by id)
    ///   - A procedural soul generation spec (seed + species)
    pub starting_crew: Vec<CrewAssignment>,
    /// System seeds for systems that are known from the start (revealed on galaxy map).
    pub known_systems: Vec<Seed>,
    /// Starting system seed (the player's location at game start).
    pub start_system: Seed,
    /// Starting station or location id within the start system.
    pub start_location: String,
    /// Opening log entries injected into the captain's log on character creation.
    pub opening_log_entries: Vec<LogEntryDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CrewAssignment {
    /// Reference to an authored soul file by id (loaded from content index).
    Authored { soul_id: String, role: String },
    /// Procedurally generated soul.
    Procedural { seed: Seed, species: String, role: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogEntryDraft {
    pub title: String,
    pub body: String,
    /// In-game time offset from start (in ticks). 0 = first entry.
    pub tick_offset: u64,
}
```

### Origin variant in `AssetType` + `ContentPayload`

```rust
// reachlock-core/src/content/envelope.rs

// Add to AssetType enum:
pub enum AssetType {
    …
    Origin,  // NEW
}

// Add to ContentPayload enum:
pub enum ContentPayload {
    …
    Origin(Origin),  // NEW
}

// Add mapping in ContentFile deserialization:
// AssetType::Origin => ContentPayload::Origin(origin)
```

### Envelope wire-shape test

```rust
// reachlock-core/src/content/envelope.rs — existing round-trip test extended:
#[test]
fn origin_envelope_round_trip() {
    let origin = Origin { … /* full example Loup-Garou veteran origin */ };
    let file = ContentFile {
        asset_type: AssetType::Origin,
        id: "loup_garou_veteran".into(),
        payload: ContentPayload::Origin(origin.clone()),
    };
    let ron = ron::ser::to_string(&file).unwrap();
    let restored: ContentFile = ron::de::from_str(&ron).unwrap();
    assert_eq!(file, restored);
}
```

The serialized `.ron` form is pinned — changing an `Origin` field is a protocol revision per iron rule #4.

### `OriginRegistry` resource (consumer target)

```rust
// reachlock-client/src/systems/dispatch.rs (extended from S81)
// or reachlock-core/src/content/origin.rs:
#[derive(Resource, Default)]
pub struct OriginRegistry {
    pub origins: HashMap<String, Origin>,
}

impl OriginRegistry {
    pub fn get(&self, id: &str) -> Option<&Origin>;
    pub fn all(&self) -> impl Iterator<Item = &Origin>;
}
```

The S81 content dispatcher's `Origin` consumer populates this registry. The S78 origin step reads from it.

## Deliverables

### 1. `Origin` type + envelope integration (`origin.rs`, `envelope.rs`)

- [ ] Define `Origin` struct in `reachlock-core/src/content/origin.rs` as frozen above.
- [ ] Add `Origin` to `AssetType` enum in `reachlock-core/src/content/envelope.rs`.
- [ ] Add `Origin(Origin)` to `ContentPayload` enum in `envelope.rs`.
- [ ] Add `AssetType::Origin` → `ContentPayload::Origin` deserialization mapping in the `ContentFile` deserialize impl.
- [ ] Add `Origin` to the content directory traversal: `origin::init_origins` or the generic dispatch consumer.
- [ ] Wire-shape test: round-trip a full `Origin` through `ContentFile` serde.
- [ ] Determinism entry: if origin selection affects any generator path, extend `determinism.rs` with a manifest entry (iron rule #3 — unlikely for static origin data, but check).

**Gate:** `cargo test -p reachlock-core content::envelope::origin_envelope_round_trip`. Wire shape test passes.

### 2. Content dispatch consumer (`dispatch.rs` or `content_index.rs`)

- [ ] In the S81 `ContentDispatcher`, register a consumer for `AssetType::Origin`:
      ```rust
      dispatcher.register(AssetType::Origin, |files| {
          let mut errors = vec![];
          let registry = …;  // ResMut<OriginRegistry>
          for file in files {
              if let ContentPayload::Origin(origin) = &file.payload {
                  registry.origins.insert(file.id.clone(), origin.clone());
              }
          }
          errors
      });
      ```
- [ ] `OriginRegistry` resource is inserted at startup (default empty).
- [ ] Test: place an origin `.ron` file in a temp `mods/` tree, run `load_content_index`, assert `OriginRegistry` contains the expected origin.

**Gate:** `cargo test dispatch::origin_consumer_loads`.

### 3. Origin editor (`reachlock-editor/src/editors/origin.rs`)

- [ ] New editor module `origin.rs` following the S68 pattern (full trait surface: `new`, `load`, `ui`, `save`, `save_all`, `touch`, `top_bar`, `preview_ui`, `snapshot`).
- [ ] Editor panels:
  - **Identity:** id, name, description, icon (text fields).
  - **Career:** dropdown selecting a `CareerPathId` (from career registry) + rank selector.
  - **Faction deltas:** table with add/remove rows. Each row: faction id dropdown, standing delta (i32, -100 to +100).
  - **Credits:** numeric field.
  - **Ship:** dropdown of ship template ids (from hull catalog) + optional seed text field.
  - **Gear:** item browser / add-stack widget. Each row: item id + count. Reuses existing item picker from the editor suite.
  - **Crew:** list of `CrewAssignment` entries. Each entry: type toggle (Authored/Procedural), soul id text field (authored) or seed+species fields (procedural), role text field.
  - **Known systems:** list of seed text fields (add/remove).
  - **Start location:** system seed field + location name text field.
  - **Log entries:** list of `LogEntryDraft` entries. Each: title, body (multiline text), tick_offset.
- [ ] `preview_ui` shows a rendered character creation summary card (reusing the S78 confirm-step card rendering logic if available, or a simplified version).
- [ ] Register the editor in `build_default_registry` (`app.rs`), `browser.rs` file type list, and `File → New` menu.
- [ ] Schema: add `origin` JSON schema for CLI validation.

**Gate:** `File → New → Origin` creates a blank origin. Load a `.ron` origin → all fields display. Edit → dirty flag. Save → file round-trips. Browser shows `.ron` files as Origin type.

### 4. Launch origin files — 6–10 `.ron` files

Author `.ron` files for each origin in `mods/reachlock/origins/`:

- [ ] **Loup-Garou veteran** — reconstructs v1 starting state exactly:
  - `starting_career`: Freelance (Tib's implied path), rank 1
  - `faction_deltas`: +10 Compact standing
  - `starting_credits`: 5000
  - `ship_template`: "loup_garou" hull
  - `ship_seed`: `16843009` (Aethon's seed — hull appearance matches)
  - `starting_crew`: 6 authored soul references (Tove, Keene, Bardo, Prudence, Risc, Boris)
  - `start_system`: seed `16843009` (Aethon)
  - `start_location`: "Aethon Station"
  - `opening_log_entries`: entry recapping the Loup-Garou's last job
- [ ] **Compact Militia** (Military path) — Compact citizen, naval training, light frigate
- [ ] **Free Trader** (Trade path) — Independent merchant, small cargo hauler, modest savings
- [ ] **Deep Scout** (Exploration path) — Frontier surveyor, long-range scout, deep-space survival gear
- [ ] **Lab Escapee** (Science path) — Research station survivor, science vessel, partial gate-network data
- [ ] **Colony Diplomat** (Political path) — Fringe colony attaché, diplomatic shuttle, faction connections
- [ ] **Ghost** (Criminal path) — Criminal record, stolen light fighter, contraband, zero faction standing
- [ ] **Freelancer** (Freelance path) — Generic start: default ship, modest gear, no faction ties, max flexibility
- [ ] **Outer Rim Castaway** (Survival-oriented Freelance variant) — Shipwrecked, one crew, barely any credits, high-risk start
- [ ] (Optional) **Corporate Asset** — Megacorp field agent, corporate corvette, high credits but negative standing with multiple factions

Each `.ron` file follows the `ContentFile` envelope format and includes a `ContentPayload::Origin` payload.

**Gate:** `make check` passes. Each origin file loads into `OriginRegistry` without error. `cargo run -p reachlock-cli -- validate mods/reachlock/origins/` reports all valid.

### 5. Playable opening state test

- [ ] A test (in `reachlock-client` or `reachlock-core`) that for each launch origin, asserts the conditions produce a valid game state:
  - Career path + rank are valid (`CareerRegistry` contains the path, rank is ≤ max for that path)
  - Faction deltas are within valid range (-100 to +100 per faction)
  - Credits ≥ 0
  - Ship template exists in hull catalog (or default starter is granted)
  - Crew assignments: authored souls exist in `SoulRegistry` or procedural spec is valid (seed ≤ 2^53, species is a known species)
  - Known systems: all seeds are valid
  - Start system seed is valid
  - Start location string is non-empty
  - Opening log entries have non-empty title + body
- [ ] Integration test: for each origin, simulate the "Launch" action from S78 — construct a `SaveFile`, apply origin conditions, assert the resulting game state is internally consistent.
- [ ] The Loup-Garou veteran origin specifically: `assert starting_carerr == Freelance`, `assert ship_template == "loup_garou"`, `assert crew.len() == 6`.

**Gate:** `cargo test origin::every_launch_origin_is_playable` passes for all 6+ origins.

### 6. Origin card rendering in S78

- [ ] Coordinate with S78 delivery: the character creation origin step reads from `OriginRegistry` and renders cards. This sprint ensures the registry is populated and the card data (name, description, icon, grants/conflicts) is accessible.
- [ ] If S78 ships before S79, the S78 origin step uses a hardcoded single origin. Once S79 lands, the step dynamically reads from the registry. Add a feature gate or runtime check: if `OriginRegistry` is empty, fall back to the hardcoded origin.

## Acceptance gates

```
# Origin type round-trips
cargo test -p reachlock-core content::envelope::origin_envelope_round_trip

# Content dispatch consumer loads origins
cargo test dispatch::origin_consumer_loads

# All launch origins are valid and playable
cargo test origin::every_launch_origin_is_playable
cargo test origin::loup_garou_veteran_reconstructs_v1

# Editor round-trip
cargo test -p reachlock-editor origin::editor_round_trip

make check
```

Manual:
1. Open editor → File → New → Origin → fill fields → Save → file written to disk → reopen → fields match
2. Launch game → New Game → Origin step → 8+ origin cards shown → select each → summary card updates
3. Select "Loup-Garou veteran" → Launch → game starts at Aethon with Loup-Garou, 6 crew, 5000 credits, +10 Compact standing
4. `cargo run -p reachlock-cli -- validate mods/reachlock/origins/` → no errors

## Non-goals

- Origin progression beyond starting conditions (career `advance_rank` is already separate)
- Origin-specific storylines or faction quests (future content, not a content type change)
- In-game origin change or respec UI (deferred)
- Balancing gameplay between origins (launch balance is iterative, not a sprint gate)
- Modding documentation or origin authoring guide (separate doc sprint)
- Per-origin achievement tracking or unlock conditions

## Gotchas

- The `Origin` struct references `ShipTemplateId`, `CareerPathId`, `FactionStandingDelta`, `ItemStack`, and `SoulFile` — all existing types in `reachlock-core`. No new dependencies needed, but the editor must have the corresponding registries available to populate dropdowns (ship templates, career paths, souls). The editor runs standalone — these registries must be populated from the content index, not assumed to exist.
- The Loup-Garou veteran origin must reconstruct v1 exactly. Verify: `start_system` seed `16843009` resolves to the same Aethon system; the ship template `loup_garou` loads from the hull catalog (S77 demoted it from a hardcoded generator function to a catalog entry); all 6 crew souls exist in `SoulRegistry` (authored by S59).
- `CrewAssignment::Authored { soul_id }` references a soul by id. The soul must be loaded before the origin is applied. The S81 dispatch layer loads souls before other content — but origins are loaded in the same phase. Ensure origin consumer runs AFTER soul consumer, or origins check soul existence at Launch time, not at load time. Recommend: origin loading parses the file into `OriginRegistry` immediately; soul existence is validated at Launch (the playable-state test).
- The editor's `preview_ui` for an Origin should show a rendered summary card. The easiest path is to reuse the `character_creation.rs` confirm-step card rendering (move it to a shared `origin_preview` function in `reachlock-core` or a shared client module). If S78 hasn't shipped, build a simple preview inline.
- The `Origin` wire shape is pinned (iron rule #4). Once shipped, adding a new field (e.g., `starting_contracts`) requires a protocol revision: update the test, note it in the commit. Get the field list right in this sprint — look at what S80 (crew) and future economy sprints will need. `starting_gear` covers items; `known_systems` covers map state; `opening_log_entries` covers narrative. If anything is missing (e.g., active contracts), add it now.
- The `icon` field is a string identifier, not an embedded image. The character creation step renders it by looking up an icon glyph from the UI theme (bevy_feathers tokens from S70). If the token system isn't ready, use a text label fallback.
