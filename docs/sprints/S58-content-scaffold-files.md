# S58 — Content Scaffold Files

**Spec:** §10 (file organization, content types) · **Wave 16 (Content Authoring) · Depends on:** S57

## Outcome

The `content/` directory mirrors the spec §10 layout exactly. Every content type has a subdirectory with a `.gitkeep` and one template `.ron` file showing the correct structure. Authors have a clear starting point for every kind of content they can create.

## Context

- The spec §10 files diagram (lines 919-951) shows a complete `content/` tree with 7 subdirectories and example files.
- The actual `content/` directory has been mentioned repeatedly as "zero authored content files exist." This sprint creates the structure and templates — the actual content files come in S59 and S60.
- Each template file is a minimal valid example of that content type, validated by the corresponding schema (S53).

## Deliverables

- [ ] `content/stations/` directory with `.gitkeep`
- [ ] `content/stations/scaffold_station.ron` — minimal station: one room (hangar), one NPC spawn, one contract reference. Comments in the file explain each field.
- [ ] `content/souls/` directory with `.gitkeep`
- [ ] `content/souls/scaffold_soul.ron` — minimal soul: name, portrait_id, voice_params, one backstory paragraph, one goal, one secret, one contract reference.
- [ ] `content/dungeons/` directory with `.gitkeep`
- [ ] `content/dungeons/scaffold_dungeon.ron` — minimal dungeon: 3 rooms (entrance, puzzle, treasure), one puzzle, one reward table.
- [ ] `content/factions/` directory with `.gitkeep`
- [ ] `content/factions/scaffold_faction.json` — minimal faction: id, name, doctrine, relationship with one other faction, one goal. JSON format (matching spec §10 faction profile type).
- [ ] `content/events/` directory with `.gitkeep`
- [ ] `content/events/scaffold_event.ron` — minimal event: one trigger condition, one narrative text block, one consequence.
- [ ] `content/schemas/` directory with `.gitkeep` — the actual schema files were created in S53. This directory in `content/` holds LOCAL copies of the schemas for authors who browse the content directory directly (the authoritative schemas live in `mods/reachlock/schemas/`). Each is a symlink or a copy.
- [ ] `content/gate_network/` directory with `.gitkeep`
- [ ] `content/gate_network/scaffold_gate.ron` — minimal gate network entry: gate_id, system_id, connections (two connected gates), jump difficulty.
- [ ] `content/README.md` — one-page guide: "Content files go here. Each subdirectory holds `.ron` or `.json` files. Validate with `reachlock-cli content validate <file>`. Preview with `reachlock-cli content preview <file>`. Publish with `reachlock-cli content publish <file>`."
- [ ] Repo-level `.gitignore` entry for `content/` — individual files are tracked but the directory itself is not gitignored. `*` is NOT in `.gitignore` — the scaffold files and auth content should be committed.

## Acceptance gates

```
reachlock-cli content validate content/stations/scaffold_station.ron
reachlock-cli content validate content/souls/scaffold_soul.ron
reachlock-cli content validate content/dungeons/scaffold_dungeon.ron
reachlock-cli content validate content/factions/scaffold_faction.json
reachlock-cli content validate content/events/scaffold_event.ron
reachlock-cli content validate content/gate_network/scaffold_gate.ron
# All pass — every scaffold is a valid content file
ls content/*/  # All 8 subdirectories exist
make check
```

## Non-goals

Authoring actual game content (S59, S60). The preview command may not exist yet at this point (S56) — validation is the acceptance mechanism until then.

## Gotchas

- The `content/schemas/` symlinks must be relative (e.g., `../../mods/reachlock/schemas/hull.schema.json`) so the content directory is relocatable. If the platform doesn't support symlinks (Windows), fall back to copies with a sync script.
- Comments in `.ron` files are dropped on round-trip through the editor. Each scaffold file includes a warning comment at the top: "This file contains comments that will be lost if edited through the ReachLock editor. Keep a copy of comments externally."
- The `content/README.md` should NOT be the only documentation — the spec §10 and the sprint briefs are the authoritative docs. The README is a quick-start guide.
- `content/` is in the workspace root, not inside any crate. It's not compiled. Content files are served by the server's content service (S57) and read by the CLI (S56).
