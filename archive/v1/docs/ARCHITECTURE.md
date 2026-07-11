# REACHLOCK Architecture — The Three Layers

> Modding is not a post-launch feature. It is the architecture the entire game
> is built on. — [GAME-DESIGN.md §8](../GAME-DESIGN.md)

REACHLOCK is built in three strictly separated layers. **Each layer knows about
the layer below it. No layer knows about the layer above it.** Our own content —
the Compact, the Loup-Garou, Tib, the Veil — is a mod that loads through the
exact same path as any community mod. The engine has *zero* REACHLOCK content.

This document is the canonical mapping from those layers to directories on disk,
and the rules that `scripts/check_architecture.py` enforces in CI.

## The layers, mapped to directories

| Layer | Name | Lives in | Knows about |
|---|---|---|---|
| **1** | **Engine** (Ring 0) | `godot/scripts/`, `server/` | Generic systems only. Faction *objects*, soul *files*, hull *definitions* — never a specific faction, soul, or hull. |
| **2** | **Framework** (the contracts) | `godot/framework/` (schemas + [README](../godot/framework/README.md)), `scripts/validate_mod_data.py` | The schemas and conventions content is written against. Knows the *shape* of a faction, not its *identity*. |
| **3** | **Content** (Ring 1 + Ring 2) | `godot/mods/` | Everything specific: REACHLOCK's factions, ships, NPC souls, storylines, assets. Loads through the mod loader like any mod. |

- **Ring 0 — Engine:** the mode-switching framework, rendering/physics, flight
  model, NPC agent gateway, faction tick loop, economy engine, combat, UI, save/
  load, the mod loader. Ships as the compiled binary.
- **Ring 1 — Content (data):** ships, weapons, factions, missions, economy
  tables. Data files under `godot/mods/<mod>/`.
- **Ring 2 — Soul:** NPC personality, dialogue, decision weights — plain JSON/
  text. Also under `godot/mods/<mod>/`.

## The rules (enforced by CI)

`scripts/check_architecture.py` runs in CI and on `make check`. It fails the
build when the engine layer reaches *up* into content:

1. **No hardcoded content IDs in engine code.** Engine code under
   `godot/scripts/` may not contain the literal id of any entity a content mod
   `provides` (faction/ship/npc/location/storyline ids, e.g. `compact`,
   `loup_garou`, `tib`, `the_veil`). The denylist is derived **dynamically** from
   every `manifest.json` under `godot/mods/`, so it stays correct as content
   grows — you never edit the guard to add a faction.

2. **No direct content paths in engine code.** Engine code may not `load()` /
   `preload()` / reference a `res://mods/...` path. The engine reaches content
   only through the framework (the mod loader / data registry), never by reaching
   into a content directory by path.

If the engine needs to *do something* with content, it does it generically —
iterate the loaded factions, look one up by id supplied at runtime — never by
naming a specific REACHLOCK id in engine source.

At runtime this is concrete: `godot/scripts/framework/mod_loader.gd` loads
every mod in dependency order, and the `DataRegistry` autoload
(`godot/scripts/framework/data_registry.gd`) is the **only** window engine
systems read content through. Even "where does a new game start?" is a
content decision — the manifest `start` block — so the engine boots with zero
knowledge of what universe it's running.

## How to stay on the right side of the line

- Adding a faction/ship/NPC? It's a **data file under `godot/mods/`** plus, if
  needed, a new field in a **framework schema**. It is never a code change in
  `godot/scripts/`.
- Engine code needs a value that's currently a content id? That's a sign the
  value belongs in a **framework contract** (a manifest field, a schema default)
  that content fills in — push it down a layer.
- Need engine behavior content can hook? Add a **framework scripting hook**
  (`on_dock`, `on_jump`, `on_soul_mutation`, …) the content subscribes to —
  don't special-case the content in the engine.

## Boundary debt (resolved in Sprint 01)

The previous known boundary debt — `server/internal/factions/factions.go`
hardcoding the five REACHLOCK faction ids in its stub — is resolved. The
server now reads faction data through its own loader
(`server/internal/loader`) from `godot/mods/<mod>/factions/*.json` and the
HTTP handler set iterates whatever the loader returns. The architecture
guard's scope has expanded to `server/`; it catches hardcoded content ids
in Go source (regular and raw string literals) the same way it does in
GDScript. The denylist is still derived dynamically from every mod's
`manifest.json` plus the top-level `id` of every data file, so adding a
new faction to a mod does not require any engine-side change.
