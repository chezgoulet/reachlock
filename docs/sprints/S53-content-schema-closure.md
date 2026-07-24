# S53 — Content Schema Closure

**Spec:** §10 (content pipeline) · **Wave 12 (Determinism Closure) · Depends on:** nothing

## Outcome

Every content type defined in the spec has a dedicated JSON Schema file in `mods/reachlock/schemas/`. No asset type falls back to a placeholder schema. `reachlock-cli content validate` routes each `AssetType` to its correct schema.

## Context

- `reachlock-cli/src/content.rs:339-340` explicitly maps `AssetType::Trope` and `AssetType::ScriptedEncounter` to `ECOSYSTEM_SCHEMA` with the comment "placeholder — needs dedicated schema."
- The spec §10 defines 9 content types (hull, station, planet, soul, dialogue/contract, location, dungeon, faction, event) plus game-specific types like trope, scripted_encounter, recipes.
- Existing schemas: hull, station, soul, ecosystem (which doubled as general-purpose). Missing schemas: trope, scripted_encounter, dungeon, event, recipe.

## Freeze first

1. JSON Schema (draft-07) for each new content type. Located in `mods/reachlock/schemas/`.
2. Each schema validates the serialized form of the corresponding core struct — the same struct the generator produces and the editor saves.

## Deliverables

- [ ] **`mods/reachlock/schemas/trope.schema.json`** — validates `TropeTemplate` (id, trope_type, title_template, narrative_template, slots, branches, base_frequency, location_types, min/max_threat_level, dilemma_chance). Each slot validates slot_name, slot_kind (enum: Text, PlanetName, FactionName, etc.), constraints.
- [ ] **`mods/reachlock/schemas/scripted_encounter.schema.json`** — validates `ScriptedEncounter` (id, scenes, triggers, choices, conditions). Each scene has narrative text, NPC lines, player choices, exit conditions.
- [ ] **`mods/reachlock/schemas/dungeon.schema.json`** — validates `DungeonLayout` (rooms, connections, encounters, puzzles, rewards). Room graph validation (all connectors reference valid rooms).
- [ ] **`mods/reachlock/schemas/event.schema.json`** — validates `ScriptedEvent` (event_type, trigger_conditions, narrative_template, consequences, expiration). Trigger conditions support AND/OR nesting.
- [ ] **`mods/reachlock/schemas/recipe.schema.json`** — validates crafting recipes (inputs → output, skill_requirement, workbench_type, duration_ticks, category).
- [ ] **Update `reachlock-cli/src/content.rs`** — replace the placeholder entries for `AssetType::Trope` and `AssetType::ScriptedEncounter` with their dedicated schemas. Add `AssetType::Dungeon`, `AssetType::Event`, and `AssetType::Recipe` mapping if they're not already present.
- [ ] **Wire in editors** — update `reachlock-editor` to use the correct schema for validation when creating/editing each content type.
- [ ] **Tests** — for each new schema, a test validates a known-good fixture file and rejects a known-bad file. Fixtures live alongside the schema.

## Acceptance gates

```
reachlock-cli content validate mods/reachlock/schemas/fixtures/trope_valid.ron
# passes — validates against trope.schema.json

reachlock-cli content validate mods/reachlock/schemas/fixtures/trope_invalid.ron
# fails — schema validation error

cargo test -p reachlock-cli
# schema validation tests pass

make check
```

## Non-goals

Authoring content files (S58-S60). The CLI `content preview` and `content publish` commands (S56).

## Gotchas

- JSON Schema draft-07 — rust `jsonschema` crate supports draft-07 only (not 2019-09 or 2020-12). Pin schemas to draft-07.
- Schemas for generator output types must match the serde serialization format of the corresponding Rust struct. Check that `#[serde(rename = "...")]` or `#[serde(skip_serializing_if)]` attributes don't create invisible schema mismatches.
- The `ECOSYSTEM_SCHEMA` that was used as placeholder may have drifted from what the generator actually produces. When removing the placeholder, verify that the correct mapping is `AssetType::Trope → TROPE_SCHEMA`, not a silent fallback.
- Existing `.ron` content files in the workspace (if any) that use the wrong schema path must be updated. Do a workspace-wide grep for `ecosystem.schema.json` in `.ron` files.
