# S52 — Generator Golden Entries

**Spec:** §5 (determinism guarantee) · **Wave 12 (Determinism Closure) · Depends on:** nothing

## Outcome

Every generator function in `reachlock-core` has a corresponding entry in the determinism manifest (`core/src/determinism.rs`). The manifest is bumped to a version that covers all 26+ generators. `cargo test determinism` passes on x86_64, and the cross-platform CI gate is green.

## Context

- Iron rule #3: "New generator or generator change ⇒ extend `core/src/determinism.rs` and recapture goldens deliberately."
- Current manifest version: 16. It covers: hull, station, hull_interior, planet, music, ui_panel, noise, palette, system_full, system_sparse, item_kinetic, hull_config, ship_interior, economy_catalog, economy_state, faction_state, faction_tariff, faction_storylines, combat_encounters, combat_damage, deep_space_seed, combat_humanoid, sprite_*, dilemma, ecosystem, ecosystem_event, trope_instantiation, mission, music_intent, music_themed, planet_extended, culture.
- Missing generators (code exists, no golden entry):
  - `generator/soul.rs` (S13) — `generate_soul(seed, params) -> Soul`
  - `generator/transit.rs` (S09) — `generate_transit_events(seed, system) -> Vec<TransitEvent>`
  - `generator/scripted_encounter.rs` (S41) — `generate_scripted_encounter(seed, template) -> ScriptedEncounter`
  - `generator/storyline.rs` — `generate_storyline_chapters(seed, faction) -> Vec<Chapter>`
  - `generator/enemy.rs` — `generate_enemy(seed, archetype) -> EnemyVessel`
  - `generator/ship.rs` — `generate_ship(seed, params) -> ShipConfig`
  - `generator/location.rs` (S07) — `generate_location(seed, biome) -> Location`
  - `generator/sfx.rs` (S49) — `generate_sfx(seed, kind) -> SfxParams`
  - `generator/contract.rs` — `generate_contract(seed, template) -> Contract`
  - `career/mod.rs` (S42) — `generate_career_paths(seed, player_state) -> Vec<CareerPath>`
  - `career/piracy.rs` (S43) — `generate_piracy_state(seed, context) -> PiracyState`
  - `agency/log_generation.rs` (S37) — `generate_log_entry(seed, context) -> LogEntry`
  - `contract/theater.rs` (S38) — `generate_deliberation_scene(seed, context) -> TheaterScene`

## Freeze first

1. Each new entry follows the existing pattern: iterate `CANONICAL_SEEDS` (6 seeds), call the generator with representative params, hash the output with `hash_serde()`, push an `Entry { generator: "<name>", seed, checksum }`.
2. The entry's `generator` string must be unique and descriptive — use the same naming convention as existing entries (snake_case, namespaced by domain).

## Deliverables

- [ ] Add golden entries for `soul` — call `generate_soul` with a representative `SoulParams` (standard personality distribution, one backstory template, one contract reference). Hash the full `Soul`.
- [ ] Add golden entries for `transit` — call `generate_transit_events` with a generated system, Biome::Frontier. Hash the `Vec<TransitEvent>`. Test at least one edge case (biome with anomaly chance modifiers).
- [ ] Add golden entries for `scripted_encounter` — call with a known `TropeTemplate` and representative `EncounterContext`. Hash the `ScriptedEncounter`.
- [ ] Add golden entries for `storyline` — call `generate_storyline_chapters` for a known faction. Hash the `Vec<Chapter>`.
- [ ] Add golden entries for `enemy` — call `generate_enemy` for at least two archetypes (Raider, Military). Hash the `EnemyVessel`.
- [ ] Add golden entries for `ship` — call `generate_ship` with representative params (small freighter, combat runner). Hash the `ShipConfig`.
- [ ] Add golden entries for `location` — call `generate_location` for two biome types (station interior, planet surface). Hash the `Location`.
- [ ] Add golden entries for `sfx` — call `generate_sfx` for all 10 SFX kinds. Hash the `SfxParams` for each kind.
- [ ] Add golden entries for `contract` — call with a known contract template. Hash the `Contract`.
- [ ] Add golden entries for `career_paths` — call `generate_career_paths` with representative player state (some progress, some empty). Hash the `Vec<CareerPath>`.
- [ ] Add golden entries for `piracy_state` — call with representative context (low notoriety, medium notoriety). Hash the `PiracyState`.
- [ ] Add golden entries for `log_entry` — call `generate_log_entry` with a representative context (recent transit, combat, dialogue). Hash the `LogEntry`.
- [ ] Add golden entries for `theater_scene` — call `generate_deliberation_scene` with a representative conflict context. Hash the `TheaterScene`.
- [ ] **Bump manifest version** to ~29 (v17: new entries). Add a version comment noting each new entry set.
- [ ] **Recapture goldens** — run `cargo run -p reachlock-cli -- determinism capture` and commit the updated manifest.
- [ ] **Verify cross-platform** — confirm the manifest is bit-identical on x86_64 (CI will catch aarch64/wasm32 on the next merge).

## Acceptance gates

```
cargo test -p reachlock-core determinism::
# deterministic — same seed produces same checksum every run
cargo run -p reachlock-cli -- determinism check
# passes — manifest matches current binary output
make check
```

## Non-goals

Adding new generator code. This sprint is exclusively about adding determinism entries for EXISTING generators. If a generator doesn't exist yet, it's not this sprint's problem.

## Gotchas

- Every new entry increases the test runtime. Target <50ms per generator per seed. If a generator is slow (e.g., full transit simulation), sample one seed instead of all 6.
- Some generators may depend on input types that are themselves generated (e.g., transit events require a `GeneratedSystem`). For these, generate the input from a fixed seed inside the determinism function — same as the existing `combat_encounters` entry at line 483-491.
- The `hash_serde` function uses the `Hasher` (FNV-1a) at the top of `determinism.rs`. Verify it produces consistent output for the types being hashed. If a type doesn't implement `serde::Serialize`, add the derive or wrap it.
- After recapturing goldens, run `cargo test` to confirm no existing entries changed (they shouldn't, since we only ADDED entries).
