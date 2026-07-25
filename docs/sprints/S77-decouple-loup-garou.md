# S77 — Decouple the Loup-Garou

**Spec:** New (remove include_str!, data-driven crew, live interior decks, origin-based start, ship template catalog, purity scan) ·
**Wave D (character & open world)** · **Depends on:** — (standalone, no UI dependency)

## Outcome

The Loup-Garou and its seven named crew are no longer baked into engine code. The `include_str!` in `core/soul/runtime.rs` is removed — soul mutations load through the content index like every other content file. `CrewRoster` builds from authored data (a crew package) instead of `default_crew()`. `deck_of()` and `deck_zero_g()` resolve against the *live* `ShipInterior`, not `loup_garou_interior()`. The starting location comes from an origin package, not a magic literal in `main.rs:105`. Ship templates become a catalog under `mods/reachlock/hulls/` where the Loup-Garou is one entry among many. `make check-purity` widens its scan to cover `storylines/` and the rest of `mods/reachlock/` so the `include_str!` pattern cannot recur.

**Closes:** X5 (default_crew()), X6 (Critical — deck_of() resolves against wrong ship), X7 (Critical — include_str! in core), X8 (starting location hardcoded), B6 (purity scan misses storylines/), C12 (starting location magic literal)
**Part of Wave D — start-now, no dependencies**

## Context

- **X7 (Critical):** `core/soul/runtime.rs:388-391` contains `include_str!("../../../mods/reachlock/storylines/loup_garou_souls.ron")`. A content file is compiled into `reachlock-core` — a direct violation of iron rule #1 (core is pure, zero rendering/IO deps) and of the S22 engine-purity guard. The `make check-purity` scan missed it because it only scans `souls/ stations/ hulls/ systems/` but not `storylines/` (B6).
- **X6 (Critical — live bug):** `crew.rs:149,160` — `deck_of()` and `deck_zero_g()` call `core::generator::ship::loup_garou_interior()` regardless of which ship the player is flying. Crew pathing consults the authored ship even when the player has built a completely different interior via the S18 editor. This is not a future-coupling concern — it is a bug today.
- **X5 (High):** `crew.rs:68`, `CrewRoster::default_crew()` — six named crew members (`"tib"`, `"tove"`, `"keene"`, `"bardo"`, `"prudence"`, `"risc"`) inserted unconditionally at `main.rs:211`. There is no recruit, fire, or replace path.
- **X8 (Low) / C12 (Low):** Starting location is a magic literal at `main.rs:105-111`: `system_seed: 16843009` (Aethon), hardcoded in the app builder.
- **B6 (High):** `Makefile:57-60` — the `check-purity` target scans `souls/ stations/ hulls/ systems/` via `rg` for Bevy/IO imports. It does not scan `storylines/`, so the `include_str!` was invisible to the gate that was supposed to catch it.
- **The recommendation (CHARACTER-CREATION-PLAN.md §3.2):** demote the Loup-Garou to content. Keep the ship and seven crew as an *authored starting package* — one option among several, a lore artifact you can encounter. Do not delete the content; delete its *privileged position* in engine code.
- **Offline-first:** the content index reads from local disk. Authored packages, ship templates, and crew rosters all load from `mods/reachlock/`. The game works identically with no server.

## Freeze first

### `CrewRoster` loading from authored package format (`reachlock-client/src/systems/crew.rs`)

Replace `CrewRoster::default_crew()` with a `CrewRoster::load_from_content(content: &ContentIndex) -> Self`
that reads authored crew packages. The `CrewPackage` content schema:

```rust
/// A set of crew members that travel together. Authored by background
/// packages ("Loup-Garou veteran") or custom encounters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewPackage {
    pub id: String,
    pub name: String,
    pub description: String,
    pub members: Vec<CrewMemberEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewMemberEntry {
    pub soul_id: String,           // key into mods/reachlock/souls/
    pub role: CrewRole,
    pub duty_room: Option<String>, // override for the default duty room
    pub starting: bool,            // true = present from new game
}
```

`CrewRoster` gains a `pub members: Vec<CrewMember>` field (open-ended, not 6),
and `default_crew()` is deleted. The content index is queried for all
`CrewPackage` entries with `starting: true` to build the initial roster.

### `ShipInterior` → deck resolution (`reachlock-client/src/systems/crew.rs:149,160`)

`deck_of()` and `deck_zero_g()` change signature to take the live `ShipInterior`:

```rust
pub fn deck_of(interior: &ShipInterior, crew_member: &CrewMember) -> DeckId { … }
pub fn deck_zero_g(interior: &ShipInterior, crew_member: &CrewMember) -> usize { … }
```

The callers supply `save_file.ship.interior` (or equivalent) instead of
`loup_garou_interior()`. This is the live bug fix.

### Ship template catalog (`mods/reachlock/hulls/`)

```rust
/// A ship template for starting packages and NPC encounters.
/// The Loup-Garou is one entry among many.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipTemplate {
    pub id: String,                    // "loup_garou", "freelancer_mk2", etc.
    pub name: String,
    pub description: String,
    pub hull_id: String,               // key into hull registry
    pub interior: ShipInterior,
    pub default_system_seed: u64,      // starting system when this template is chosen
}
```

Move `loup_garou_interior()` output into `mods/reachlock/hulls/loup_garou.ron`.
The function `loup_garou_interior()` is deleted from core. The generator's
`ship.rs` no longer holds any authored ship data — it only holds procedural
generation logic.

## Deliverables

### 1. Remove `include_str!` from `core/soul/runtime.rs:388-391`

- [ ] Delete `include_str!("../../../mods/reachlock/storylines/loup_garou_souls.ron")` from `core/soul/runtime.rs`.
- [ ] Replace with a content-index lookup: soul mutations load via `content_index.get_soul(soul_id)` or the runtime's `resolve_soul(soul_id)` method that queries the loaded content.
- [ ] Update the soul runtime initialisation to accept a reference to the content index (or a `LoadedSouls` map pre-populated from the content index). The runtime should not compile if it still has any `include_str!` or hardcoded path.
- [ ] Verify no remaining `include_str!` in `reachlock-core` via `grep -r 'include_str!' reachlock-core/src/`. This is a test gate.
- [ ] Move the actual soul data from the `include_str!`'d RON into `mods/reachlock/souls/loup_garou_*.ron` files (one per crew member, matching the S59 format). If S59 already produced these files, link them — no duplication.

### 2. Extend `make check-purity` (Makefile + B6 fix)

- [ ] Update the `check-purity` target in `Makefile:57-60` to scan all content directories: `souls/ stations/ hulls/ systems/ storylines/ factions/ contracts/ dialogue/ ecosystems/ events/ themes/ tropes/ recipes/ dungeons/`. Use a single `rg` pass over the entire `mods/reachlock/` tree instead of enumerating subdirectories separately.
- [ ] Current command (approximate):
      ```makefile
      check-purity:
      	@echo "Checking mods/reachlock/ for Bevy imports (purity violation)..."
      	@! rg -n 'use bevy|use reachlock_client' mods/reachlock/ || \
      	  (echo "PURITY VIOLATION:"; false)
      ```
      Change to:
      ```makefile
      check-purity:
      	@echo "Checking mods/reachlock/ for Bevy/IO imports or include_str! in core..."
      	@! rg -n 'use bevy|use reachlock_client|include_str!' \
      	  mods/reachlock/ reachlock-core/src/ || \
      	  (echo "PURITY VIOLATION:"; false)
      ```
- [ ] Run `make check-purity` and verify it passes before any other changes in this sprint.
- [ ] Verify the scan catches the removed `include_str!` by temporarily re-adding it — the gate must fail.

### 3. `CrewRoster` — data-driven loading

- [ ] Define `CrewPackage` and `CrewMemberEntry` structs (Freeze first) in `reachlock-core/src/soul/types.rs` or a new `reachlock-core/src/crew.rs`. They must derive `Serialize`/`Deserialize`/`Clone`/`Debug`.
- [ ] Define `CrewMember` as a runtime type (with resolved references, not just IDs) in `reachlock-client/src/systems/crew.rs`:
      ```rust
      pub struct CrewMember {
          pub soul: SoulFile,           // fully resolved
          pub role: CrewRole,
          pub duty_room: Option<String>,
      }
      ```
- [ ] Implement `CrewRoster::load_from_content(content: &ContentIndex) -> Self` that:
      1. Queries all `ContentPayload::CrewPackage` entries from the loaded content index.
      2. Filters to those with `starting: true`.
      3. Resolves each `soul_id` to a `SoulFile` from the content index.
      4. Builds `CrewMember` instances.
- [ ] Delete `CrewRoster::default_crew()` entirely.
- [ ] Remove the unconditional `init_souls` + `default_crew()` call at `main.rs:211`. Replace with `CrewRoster::load_from_content(&content_index)` called after content loading but before save restoration (maintaining the existing ordering: souls loaded before save restoration).
- [ ] Delete the `CrewRole` closed-5-variant assumption if any code outside `CrewRoster` iterates it — `load_from_content` supports any role string (or enum variant) that the content delivers. For now, keep `CrewRole` as-is (S80 opens it).
- [ ] Author the Loup-Garou crew package as `mods/reachlock/crews/loup_garou.ron` with `starting: true`, referencing the 7 soul files from S59.
- [ ] Unit test: load a `CrewPackage` from a RON string, assert all 7 members resolve with correct roles.
- [ ] Integration test: boot the client with `CrewPackage` populated, assert `CrewRoster` has 7 members.

### 4. `deck_of()` / `deck_zero_g()` — live `ShipInterior`

- [ ] Change `deck_of(crew_member: &CrewMember) -> DeckId` to `deck_of(interior: &ShipInterior, crew_member: &CrewMember) -> DeckId` in `reachlock-client/src/systems/crew.rs`.
- [ ] Change `deck_zero_g(crew_member: &CrewMember) -> usize` to `deck_zero_g(interior: &ShipInterior, crew_member: &CrewMember) -> usize`.
- [ ] Find all call sites (currently `crew.rs:149` and `crew.rs:160`, plus any in `pathfinding.rs` or `navigation.rs` that use deck resolution). Pass `save_file.ship.interior` (or the live `ShipInterior` resource) instead of calling `loup_garou_interior()`.
- [ ] If there is no guaranteed `ShipInterior` resource at the call site, require it as a system parameter (`Res<SaveFile>` or a dedicated `Res<ShipInterior>` that tracks the current ship's interior).
- [ ] Add a guard: if no interior is available (e.g., during startup before the save is loaded), return `DeckId::default()` (deck 0, the first deck) instead of falling back to the Loup-Garou interior. The game should not crash on crew pathing before the player has a ship.
- [ ] Test: create a `ShipInterior` with 3 decks named `["Engineering", "Habitation", "Bridge"]`, place a crew member on deck `"Habitation"`, call `deck_of` with that interior — assert the returned `DeckId` is deck index 1. Repeat with the Loup-Garou interior from the catalog — assert the same crew member resolves to a different deck if the interior is different (proving the bug fix).

### 5. Starting location from origin package (`main.rs:105-111`)

- [ ] Remove the hardcoded `system_seed: 16843009` literal at `main.rs:105-111` (Aethon).
- [ ] Define a `StartingLocation` data type or embed it in `CrewPackage` / a new `OriginPackage`:
      ```rust
      pub struct StartingLocation {
          pub system_seed: u64,
          pub station_id: Option<String>,
          pub landing_pad: Option<String>,
      }
      ```
- [ ] During new-game initialisation (S78), query the selected origin package for `starting_location`. If none is provided, fall back to a default (`system_seed: 16843009` — Aethon, the current hardcoded value — for backward compatibility).
- [ ] Update `AppState::InGame` initialisation to use the origin's starting location instead of the magic literal.
- [ ] Test: initialise a game with an origin that specifies `system_seed: 42` — assert the player spawns at system 42.

### 6. Ship template catalog

- [ ] Define `ShipTemplate` struct (Freeze first) in `reachlock-core/src/generator/ship.rs` or a new `reachlock-core/src/ship_template.rs`.
- [ ] Add `ContentPayload::ShipTemplate` variant to the content envelope enum.
- [ ] Create `mods/reachlock/hulls/loup_garou.ron` containing the Loup-Garou's hull, frame, room templates, and interior deck layout — everything currently produced by `loup_garou_interior()`.
- [ ] Delete `loup_garou_interior()` from `core/src/generator/ship.rs`. The generator's `ship.rs` now contains only procedural generation (`generate_hull`, `generate_interior`, etc.).
- [ ] Add a second template `mods/reachlock/hulls/freelancer_mk2.ron` with a different deck layout (3 decks instead of 2) to prove the catalog works.
- [ ] Implement `ShipTemplate::load(id: &str) -> Option<ShipTemplate>` via content index lookup.
- [ ] Test: load the Loup-Garou template, assert interior has correct deck count and room layout. Load the freelancer template, assert a different deck count.
- [ ] Test: boot with freelancer template as the starting ship, call `deck_of` with that interior — assert crew pathing resolves against the freelancer's decks, not the Loup-Garou's.

### 7. Content package for the Loup-Garou crew

- [ ] Create `mods/reachlock/crews/loup_garou.ron`:
      ```ron
      CrewPackage(
          id: "loup_garou",
          name: "Loup-Garou Veteran",
          description: "The canonical crew from the prototype — Tib, Tove, Keene, Bardo, Prudence, Risc, and Boris.",
          members: [
              (soul_id: "loup_garou_tib", role: Captain, starting: true),
              (soul_id: "loup_garou_tove", role: Pilot, starting: true),
              (soul_id: "loup_garou_keene", role: Engineer, starting: true),
              (soul_id: "loup_garou_bardo", role: Doctor, starting: true),
              (soul_id: "loup_garou_prudence", role: Gunner, starting: true),
              (soul_id: "loup_garou_risc", role: Scientist, starting: true),
              (soul_id: "loup_garou_boris", role: General, starting: true),
          ],
      )
      ```
- [ ] Register the crew package in the content index scaffold (or ensure `walk()` picks it up if `crews/` is under `mods/reachlock/`).

## Acceptance gates

```
cargo test -p reachlock-core crew::                 # CrewPackage round-trip, ShipTemplate load
cargo test -p reachlock-client crew::               # CrewRoster::load_from_content, deck_of live interior test
cargo test -p reachlock-client starting_location::   # origin-driven starting location test
grep -r 'include_str!' reachlock-core/src/          # must return nothing
make check-purity                                   # widened scan passes
make check
```

Manual: boot the client with a save from before S77 — the save still loads, crew
members are populated from the content index (fallback). Boot with the freelancer
template as starting ship — crew pathing resolves against freelancer decks, crew
stand on the correct deck. The Loup-Garou is still available as a starting package
option (S78) and as an authored ship you can encounter in the world.

## Non-goals

- Opening `CrewRole` beyond the fixed 5 variants (S80)
- Recruit/hire/fire/lose/death paths for crew (S80)
- Player character integration — the crew roster now builds from data, but the player's own character slot is still separate (S75/S78)
- The character creation flow itself (S78)
- Removing the Loup-Garou content — the ship and crew stay as authored packages; only their privileged compilation-into-core status is removed
- Any change to how `loup_garou_interior()` was generated — the output is preserved verbatim as a catalog entry

## Gotchas

- **S81 dependency ordering:** `soul::init_souls` runs between `load_content_index` and `load_save` in the startup chain (gotcha ledger, S81 entry). The `CrewRoster::load_from_content` call must occur *after* souls are loaded but *before* save restoration — exactly the slot where `default_crew()` currently runs. Maintain this ordering; do not move crew loading to a different stage.
- The `include_str!` path is `../../../mods/reachlock/storylines/loup_garou_souls.ron`. The file it references is in `storylines/`, not `souls/`. After removal, the soul data should live in `mods/reachlock/souls/` where the content index expects it. Move the data, don't just delete the `include_str!` — otherwise the seven crew souls vanish from the content index.
- `loup_garou_interior()` returns a `ShipInterior` with specific deck indices. Any code that depended on the deck ordering (e.g., "the captain is always on deck 0") will break when a different ship template is used. The S18 interior editor already allows arbitrary deck counts and names, so the code should already handle variable decks — but verify that no code assumes a 2-deck Loup-Garou layout.
- `check-purity` currently uses `rg` (ripgrep). If the Makefile's `check-purity` target has a different shell command, adapt accordingly. The key is to scan the entire `mods/reachlock/` tree and `reachlock-core/src/` for `include_str!` and Bevy imports.
- `deck_of()` is called from navigation/pathfinding systems that run every frame. Adding a `&ShipInterior` parameter means those systems need `Res<SaveFile>` or a dedicated `Res<CurrentShipInterior>`. Measure the cost of the lookup — if it's hot, cache the interior on `CrewRoster` or compute deck indices once per interior change rather than per frame.
- The `CrewPackage` `starting: bool` flag determines initial roster membership. If no package has `starting: true`, the player starts with zero crew — that's a valid degenerate state (S78's character creator must ensure at least one starting crew package is selected). Do not hardcode a fallback.
- `CrewRole` is currently a closed enum. The `CrewPackage` format references it by variant name. If `CrewRole` gains new variants later (S80), the package format stays compatible as long as the variant names are stable (RON deserialises by variant name). Mark this in a comment: "extend CrewRole by adding variants; existing packages continue to deserialise."
- The `ShipTemplate` catalog entry for the Loup-Garou must reproduce exactly what `loup_garou_interior()` returned. Capture the output of the function once before deleting it (e.g., by `println!("{:#?}", loup_garou_interior())` and copying the RON into the template file). Run the round-trip test to confirm the loaded template matches the original data structure.
- Branch: `sprint-v2/s77-decouple-loup-garou`, cut from `testing`.
