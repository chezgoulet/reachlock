# The REACHLOCK Framework — Layer 2 Contracts

This directory is the **framework layer** from [docs/ARCHITECTURE.md](../../docs/ARCHITECTURE.md):
the schemas and conventions that content — ours and every community mod — is
written against. The engine implements these contracts; it never knows whose
content is flowing through them.

**These files are load-bearing.** Every soul file, hull, and faction anyone
authors conforms to them, so changing a schema is the single most expensive
kind of change in the project. Additive changes (new optional fields) are
cheap; renames and semantic changes require a `framework_version` bump and an
explicit migration. Design accordingly.

## The contracts

| Schema | Governs | Notes |
|---|---|---|
| [manifest.schema.json](schemas/manifest.schema.json) | `<mod>/manifest.json` | Identity, dependencies, `provides`, optional `start` block |
| [faction.schema.json](schemas/faction.schema.json) | `<mod>/factions/*.json` | Territory, resources, relationship stances, divisions, goals |
| [ship.schema.json](schemas/ship.schema.json) | `<mod>/ships/*.json` | Class, chassis archetype, stats, hardpoints, `flight` feel block |
| [npc.schema.json](schemas/npc.schema.json) | `<mod>/npcs/*.json` | Soul file v1 — authored *birth-state*: mind tier, memory seeds, emotional baseline, relationship graph |
| [location.schema.json](schemas/location.schema.json) | `<mod>/locations/*.json` | Stations/planets: services, NPCs present, economy links, docking |
| [dialogue.schema.json](schemas/dialogue.schema.json) | `<mod>/dialogues/*.json` | Authored/generated dialogue graphs; conditions are trigger-DSL, CI-parsed |
| [good.schema.json](schemas/good.schema.json) | `<mod>/goods/*.json` | Trade goods: base price, legality by faction |
| [save.schema.json](schemas/save.schema.json) | runtime saves | The *runtime* counterpart of authored data; storage engine is swappable |

Beyond entity schemas, three sibling contracts:

- **[The Soul Protocol](protocol/SOUL-PROTOCOL.md)** — the wire contract with
  the mind daemon (Pan). Golden fixtures under `protocol/fixtures/` are the
  conformance suite (`make protocol`); Pan round-trips the same fixtures in
  its own CI.
- **[The trigger DSL](../../scripts/trigger_dsl.py)** — the condition language
  of storyline cards and dialogue branches. The reference evaluator's
  self-test battery IS the semantics (`make dsl`); other implementations must
  match it.
- **[The memory interface](../../docs/MEMORY-INTERFACE.md)** (Ragamuffin
  binding) and **[the universe tick](../../docs/UNIVERSE-TICK.md)** — the
  memory-store subset we depend on, and the deterministic simulation clock.

Kinds without a schema yet (`storylines`, `economy_tables`) only need to
parse; their contracts land with the systems that consume them.

## Conventions every mod follows

- **A mod is a directory** under `mods/` containing a `manifest.json`. Each
  key in `provides` maps 1:1 to a subdirectory of entity JSON files
  (`provides.ships` ↔ `ships/`).
- **Ids are `snake_case`** (`^[a-z][a-z0-9_]*$`), globally namespaced by kind.
  Ids leak into the event trigger syntax (`faction.compact.trust < -50`), so
  they must stay machine-friendly forever.
- **Filename = entity id.** `ships/loup_garou.json` declares `"id": "loup_garou"`.
- **Load order is dependency order** (topological; cycles rejected). On id
  collisions, the last-loaded mod wins with a warning — that's how overhaul
  mods replace base content deliberately.
- **`start` block**: content tells the engine where a new game begins (mode,
  player ship, location). The engine never hardcodes an entity id; the
  last-loaded mod defining `start` wins.
- **Unknown keys are errors** — schemas are strict so typos surface in CI
  instead of silently doing nothing. Mod-specific custom data goes under the
  free-form `extra` object, which the engine never reads.

## Design decisions worth knowing

- **Authored data vs. runtime state.** Mod files are *templates*: the birth
  state of an NPC soul, the factory stats of a hull, a faction's posture at
  universe start. Everything that changes during play — memories, emotional
  state, live faction standings, hull damage — belongs to save data (single
  player) or the server (MMO), never to mod files. This is what makes soul
  files v0-simple today and lets the Pan bridge add memory/emotion contracts
  (v1) without breaking any authored content.
- **`flight` numbers are designer-facing units** (units/s, rad/s), consumed
  directly by the flight model with engine defaults for anything omitted. A
  hull with no flight block still flies.
- **Strict in CI, lenient at runtime.** `scripts/validate_mod_data.py` fails
  the build on any contract violation. The in-engine loader logs problems and
  keeps going — a player with a half-broken mod gets a report, not a crash.

## Runtime counterpart

The engine side lives at `godot/scripts/framework/`:
[mod_loader.gd](../scripts/framework/mod_loader.gd) discovers and loads mods;
[data_registry.gd](../scripts/framework/data_registry.gd) (autoload
`DataRegistry`) is the only window engine systems read content through.
