# S82 — Narrative Systems Light-Up

**Spec:** New (narrative execution) · **Wave B (the systems you already paid for)** · **Depends on:** S81 (content dispatch & contract pipeline)

**Closes findings:** D1 (dilemma generator dark), D3 (scripted encounters dark), D4 (storyline generator dark)

## Outcome

Dilemmas surface at decision points. `generate_dilemma` (S36) finally gets called from the client — a system decides when to present one (jump gate, station arrival, combat aftermath) and displays it as a text-based choice panel. Scripted encounters execute end-to-end: `evaluate_scripted_encounter`, `advance_scene`, `apply_consequences` wired through the S81 `ContentPayload::ScriptedEncounter` consumer into a runtime that walks scenes on player input. Storylines drive faction arcs — `generate_storyline` produces chapters, a client system checks faction state and advances the arc, surfacing the next chapter to the HUD. The S40 trope engine goes from authored templates in a registry to live `instantiate_trope` calls on system entry, pulling from game state. Every generator touched gets a determinism golden.

## Context

- **D1 (Dilemma generator dark — S36):** `core/generator/dilemma.rs` has `generate_dilemma`, all 16 `DilemmaType` variants, choice + consequence structs — zero calls outside its own tests. The function is deterministic (seed + game state → `Option<Dilemma>`), validated by unit tests, but no client system ever invokes it and no UI presents the result.
- **D3 (Scripted encounters dark — S41):** `core/generator/scripted_encounter.rs` has `evaluate_scripted_encounter`, `advance_scene`, `apply_consequences`. CLI validation runs. No consumer ever loads an encounter from disk, evaluates it against live state, walks scenes, or applies consequences. S81's `ScriptedEncounter` consumer creates an `EncounterRegistry` HashMap — it stores data; it doesn't execute it.
- **D4 (Storyline generator dark — S60):** `core/generator/storyline.rs` has `generate_storyline`, chapter generation with prologue/development/resolution templates. Content payload exists. No system ever calls it against faction state, and no faction-arc driver advances a storyline on reputation or milestone triggers.
- **S81** wires the `Trope`, `ScriptedEncounter`, and `Storyline` payload variants into registries (`TropeRegistry`, `EncounterRegistry`, etc.) as typed HashMaps. Those registries are occupied storage — this sprint adds the readers.
- **ActivePanel pattern** (established by dialogue, market, onboard consoles): a `Res<ActivePanel>` enum drives which panel renders in the HUD text area. New `ActivePanel::Dilemma` and `ActivePanel::Encounter` variants slot into this existing pattern.

## Freeze first

These types are already defined in `reachlock-core` and must NOT be changed by this sprint (this sprint reads them):

```rust
// core/src/generator/dilemma.rs — frozen as-is
pub struct Dilemma { … }
pub enum DilemmaType { … }
pub struct DilemmaSetup { … }
pub struct DilemmaChoice { … }
pub struct DilemmaConsequence { … }
pub fn generate_dilemma(seed, is_frontier, relationship_count, faction_diversity) -> Option<Dilemma>

// core/src/generator/scripted_encounter.rs — frozen as-is
pub struct ScriptedEncounter { … }
pub enum EncounterTrigger { … }
pub struct EncounterScene { … }
pub struct EncounterChoice { … }
pub struct EncounterEvaluation { … }
pub fn evaluate_scripted_encounter(encounter, game_state) -> Option<EncounterEvaluation>
pub fn advance_scene(encounter, current_scene_id, choice_index, game_state) -> Option<EncounterEvaluation>
pub fn apply_consequences(consequences, game_state) -> BTreeMap<String, String>

// core/src/generator/storyline.rs — frozen as-is
pub struct StoryChapter { … }
pub fn generate_storyline(seed, chapter_count) -> Vec<StoryChapter>

// core/src/generator/trope.rs — frozen as-is
pub struct TropeTemplate { … }
pub enum TropeType { … }
pub struct TropeInstance { … }
pub fn instantiate_trope(template, seed, game_state, location) -> TropeInstance
```

These registries (created by S81) are consumed by this sprint — they are storage-only and must not change their public interface:

- `EncounterRegistry(HashMap<String, ScriptedEncounter>)` — `Resource`
- `TropeRegistry(HashMap<String, TropeTemplate>)` — `Resource`
- `StorylineRegistry` — if S81 created one; otherwise a new minimal `Resource` holding faction-arc entries

## Deliverables

### 1. Dilemma trigger system (`client/src/systems/dilemma_system.rs`)

- [ ] New system that runs after every game-mode transition that qualifies as a "decision point": jump gate activation, station arrival (docking), combat aftermath, and a manual trigger (debug key or `InputAction::Dilemma` placeholder).
- [ ] Decision-point detection: listen for `GameMode` transitions (e.g., `EnteredJump`, `Docked`, `CombatEnded` events or state-change signals). On each qualifying transition, roll `generate_dilemma(seed, is_frontier, …)` against the current system context.
- [ ] When `generate_dilemma` returns `Some(dilemma)`, set `ActivePanel::Dilemma(dilemma)` (or insert an `ActiveDilemma` resource) to present it. If `None`, no interruption.
- [ ] Decision-point calibration: dilemmas fire at most once per 2-3 minutes of play (cooldown timer), matching the spec's ~1 per 2-3 hours of play baseline.
- [ ] Wire `ActivePanel::Dilemma` variant into `interaction.rs` `ActivePanel` enum and the HUD panel router (`hud.rs`).
- [ ] Register the system in `main.rs` startup/shared schedule.

### 2. Dilemma UI (`client/src/systems/dilemma_ui.rs` or inline in `dilemma_system.rs`)

- [ ] Text-based panel (matching the existing dialogue/market text-render pattern) that shows:
  - Dilemma title and narrative (from `DilemmaSetup`)
  - Urgency indicator (Immediate/Pressing/Looming/Background)
  - Numbered list of choices, each showing label + description
  - Consequences preview (greyed text below each choice)
- [ ] Keyboard navigation: number keys `1`-`9` select a choice; `Enter` confirms; `Esc` returns to the previous scene or closes the panel.
- [ ] On choice confirmation, apply `DilemmaChoice.consequences` to game state:
  - `CrewTrustChanged` → modify crew trust values
  - `FactionReputationChanged` → modify faction reputation
  - `ResourceGained`/`ResourceLost` → modify ship resources
  - `CrewMemberQuits` → remove crew member from roster
  - `NewMissionUnlocked` → add mission to mission board
  - `StoryArcProgressed` → advance storyline
  - `Nothing` → no change
- [ ] After applying consequences, display a brief outcome text (2-3 lines) then close the panel after a timer or key press.
- [ ] Dilemma is one-shot per decision point — completing a dilemma clears the trigger cooldown for the next eligible point.

### 3. Scripted encounter executor (`client/src/systems/encounter_executor.rs`)

- [ ] System that consumes `EncounterRegistry` (from S81) and checks encounter triggers against current game state on each state transition that could fire.
- [ ] Trigger evaluation: for each loaded encounter, compare `EncounterTrigger` against live state. `OnSystemEntry { system_id }` fires when the player enters that system. `OnStationDock { station_id }` fires on docking. `OnFactionReputation { faction, threshold, direction }` fires when reputation crosses the threshold.
- [ ] When a trigger fires, call `evaluate_scripted_encounter(encounter, game_state)` — if prerequisites are met, present the first scene as `ActivePanel::Encounter(EncounterState { encounter_id, current_scene_id, active: true })`.
- [ ] Scene-walk system: on player choice in an encounter scene, call `advance_scene(encounter, current_scene_id, choice_index, game_state)` to get the next scene. Update `EncounterState.current_scene_id` and re-render.
- [ ] Consequence application: when a choice carries `immediate_consequences`, call `apply_consequences(consequences, game_state)` and merge the returned state into the live game state.
- [ ] `EncounterState` resource tracks active encounter + scene. When the outcome scene has no further choices (or the encounter's scene list is exhausted), apply `on_complete` outcomes and clear the active state.
- [ ] Encounter cooldown: non-repeatable encounters deactivate after completion. Repeatable encounters respect `cooldown_ticks`.
- [ ] Wire `ActivePanel::Encounter` variant into `ActivePanel` and the HUD panel router.

### 4. Storyline / faction-arc driver (`client/src/systems/storyline_driver.rs`)

- [ ] System that runs on faction-reputation changes and milestone triggers (universe tick, mission completion, dilemma choice with `StoryArcProgressed`).
- [ ] On trigger, call `generate_storyline(seed, chapter_count)` for the relevant faction arc. The seed is derived from the faction id + current arc chapter index so each chapter is deterministic from the same state.
- [ ] Track active storylines per faction via `StorylineState` resource: `HashMap<FactionId, StorylineProgress>` where `StorylineProgress { chapter_index, chapters, last_triggered_tick }`.
- [ ] When a new chapter is available, push a notification to the HUD ticker ("Storyline advanced: {faction} — {chapter_title}"). The chapter text is viewable from a new "Story Log" option in the pause menu or a dedicated HUD panel.
- [ ] Storyline state persists in the save file (so chapter progress survives a reload). Add `storylines: HashMap<String, StorylineProgress>` to the save data.
- [ ] Wire faction-arc data sources: faction reputation (from S11's `Res<FactionReputation>` or equivalent), system discovery state, player level/career rank.

### 5. Trope dispatcher & instantiation system (`client/src/systems/trope_dispatcher.rs`, extending S81's `TropeRegistry` consumer)

- [ ] Post-S81 consumer: the existing S81 `load_tropes` consumer populates `TropeRegistry`. This sprint adds the **reader** that picks a trope on system entry/exploration events.
- [ ] Trope selection system: on `SystemEntry` (or when the player reaches a new `LocationType`), roll against `TropeTemplate.base_frequency` for each eligible template whose `location_types` includes the current location and whose threat range matches the system threat level.
- [ ] When a trope is selected, call `instantiate_trope(template, seed, game_state, location)`. The `game_state` is populated from the current world state (factions in system, discovered items, crew roles, etc.).
- [ ] Present the `TropeInstance` as a lightweight narrative popup (non-blocking, dismissable with `Space` or `Enter`) — matching the spec's "procedural seasoning" tone. The popup shows title, filled narrative, and available branches.
- [ ] Branch resolution: player picks a `TropeBranch` → apply `TropeConsequence` effects (reputation, credits, ship damage, cargo, etc.) → mark `TropeInstance.resolved = true`.
- [ ] If `dilemma_chance` roll succeeds on resolution, trigger the dilemma system with a seed derived from the trope's seed.
- [ ] Trope frequency throttle: a fixed cooldown (minimum distance between tropes, e.g. 3 system entries) prevents the "trope every jump" effect.

### 6. Determinism goldens for dilemma generation (extends `core/src/determinism.rs`)

- [ ] Add `dilemma` entries to `determinism::manifest()`: for each canonical seed, hash the output of `generate_dilemma(seed, true, 5, 3)` (frontier) and `generate_dilemma(seed, false, 5, 3)` (safe). Use `hash_serde` since `Dilemma` derives `Serialize`.
- [ ] Name the entries `"dilemma_frontier"` and `"dilemma_safe"`.
- [ ] Recapture golden manifests (`make check` regenerates goldens if they've drifted) — commit the updated manifest files.

### 7. Wire new systems into client startup (`main.rs`)

- [ ] Add `ActivePanel::Dilemma` and `ActivePanel::Encounter` variants to `interaction.rs`.
- [ ] Register `dilemma_system`, `encounter_executor`, `storyline_driver`, `trope_dispatcher` in the shared Bevy schedule (or appropriate stage after game state is ready).
- [ ] Register new resources (`EncounterState`, `DilemmaCooldown`, `StorylineState`, `TropeCooldown`) as init-resources.
- [ ] HUD text routing: add `ActivePanel::Dilemma` and `ActivePanel::Encounter` arms to the `panel_text` function in `hud.rs` that render the dilemma or encounter text.
- [ ] Verify `make check` passes — all new types are reachable, nothing breaks existing systems.

## Acceptance gates

```
# Core determinism — new dilemma goldens
cargo test -p reachlock-core determinism::manifest     # dilemma_frontier + dilemma_safe entries present
cargo test -p reachlock-core generator::dilemma::tests  # existing tests still pass

# Dilemma system
cargo test -p reachlock-client dilemma_system::tests    # trigger logic, cooldown, consequence application

# Scripted encounter executor
cargo test -p reachlock-client encounter_executor::tests  # trigger eval, scene walk, consequence apply

# Storyline driver
cargo test -p reachlock-client storyline_driver::tests    # chapter generation, faction arc tracking

# Trope dispatcher
cargo test -p reachlock-client trope_dispatcher::tests    # selection, instantiation, branch resolution

# Integration — S81 consumer registration completeness
cargo test dispatch::consumer_coverage_test              # no regressions from S81

make check
```

Manual:
1. Fly to a jump gate → engage jump → a dilemma panel appears (1-in-3 or so). Choose an option → consequences appear → panel closes. Jump again a minute later → another dilemma may appear.
2. Author a simple scripted encounter mod file (one scene, one choice) → place it in `mods/` → enter the trigger system → encounter fires, shows narrative, accepts choice, shows outcome.
3. Change faction reputation past a storyline threshold → ticker shows "Storyline advanced" → open story log → chapter text is visible.
4. Jump into a frontier system → a trope narrative popup appears ("You find the {ship_name}…") → pick a branch → consequence applies.
5. No crash on startup with empty `mods/` directory — all systems gracefully handle empty registries.
6. Save game mid-encounter → reload → encounter is still active (or properly deactivated if it was non-repeatable).

## Non-goals

- LLM integration for dilemmas — dilemmas are procedural (S36 pure function). LLM-driven dilemma deliberation is a future sprint (S38 deliberation theater).
- Full UI beyond text-based panels — dilemma/encounter panels follow the existing ActivePanel text-render pattern. UI polish (styled panels, animated choices) belongs in S70 (client UI framework).
- Multi-faction storyline weaving — storylines are per-faction arcs. Cross-faction integration is a future concern.
- Persistence of individual trope instances — tropes are ephemeral "seasoning" that fire and resolve. They do not go into the save file. Only storyline state persists.
- Dungeon integration — the dungeon content type exists but is not called from any narrative system. That's a future sprint.
- The `generate_dilemma` function itself is not modified — no new dilemma types, no LLM integration, no authored dilemma files. This sprint calls the existing function and presents its output.
- Faction arc authorship — faction profiles and story arcs are content files from S58/S60. This sprint reads them; it does not define the content format.

## Gotchas

- **Determinism goldens are required for dilemma generation** (iron rule #3). This sprint adds two new entries (`dilemma_frontier`, `dilemma_safe`) to `determinism::manifest()` in `core/src/determinism.rs`. If the dilemma generator's vocabulary tables (`NAMES`, `ASSETS`, `ISSUES`, etc.) are ever changed, the golden manifests must be recaptured deliberately. The commit message for any such change must say so.
- Dilemma trigger cooldown must use real game time (ticks or seconds), not frame count. A dilemma that fires on every `Update` due to a cooldown bug would be unplayable. Use a `Timer` resource with a fixed duration (2-3 minutes of play, not wall-clock time).
- `evaluate_scripted_encounter` and `advance_scene` return `Option<EncounterEvaluation>` — the system must handle `None` gracefully (encounter prerequisites not met, scene id not found → skip without crash).
- The `game_state` parameter for `evaluate_scripted_encounter` and `apply_consequences` is a `BTreeMap<String, String>`. The encounter executor must maintain a live game-state map derived from current world state (reputation, credits, items, discovered systems, etc.). This map is rebuilt each tick from Bevy resources — or cached and invalidated on state change. Keep it as a `BTreeMap` (deterministic ordering), not `HashMap`.
- `ActivePanel::Dilemma` variants are `Dilemma` structs, which are `Clone` but not `Copy`. The `ActivePanel` enum currently derives `PartialEq, Eq` — `Dilemma` derives `PartialEq` so no change needed, but verify the `Eq` derive on `Dilemma` (it does derive `Eq` as of S36).
- Trope presentation is a narrative popup, not a blocking panel. The player can dismiss it at any time. Trope branches that are not taken are simply skipped — no consequence accrues. This is intentional (tropes are seasoning, not critical path).
- The `EncounterState` resource must implement `Clone` or be storable in save data. Encounters that are mid-scene when the player saves should survive a reload — either the active encounter is serialized, or it's marked completed and the player restarts from the trigger evaluation.
- Scripted encounter cooldowns use `cooldown_ticks` — a universe tick counter, not wall-clock time. The encounter executor must have access to the universe tick resource (or a monotonic tick counter).
- The storyline driver reads faction state. If faction state doesn't exist yet (offline mode with no faction system initialized), the system is a no-op — it doesn't crash.
- Storyline chapters use `generate_storyline(seed, chapter_count)` which is a pure function — it does NOT read game state. The "storyline advances" mechanic is purely about choosing which chapter index to show next. The driver decides the index based on the trigger condition (reputation threshold crossed, milestone reached). The seed is faction_id XOR chapter_index so each chapter is deterministic per faction.
- Add `Dilemma`, `Encounter`, `Trope`, `Storyline` entries to the S81 consumer completeness test if they were not covered — the dispatch coverage test from S81 must include consumers that this sprint adds real readers for. The `Trope` and `ScriptedEncounter` consumers were stubs in S81; this sprint upgrades them to real readers.
