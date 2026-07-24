# S60 — Storyline Framework

**Spec:** §10 (faction/storyline content), §21 (faction storylines) · **Wave 16 (Content Authoring) · Depends on:** S59

## Outcome

Five faction profiles and three storyline arcs exist as authored content files. The faction system in `reachlock-core` can load these files and evaluate chapters against the current universe state. Authors have a concrete example of how storylines work, and the first Predecessor dungeon exists as a playable authored encounter.

## Context

- The faction system (`core/src/faction.rs`) defines `Faction`, `FactionState`, `tariff`, `evaluate_storylines`, `load_storylines`. Storylines are lists of `Chapter` structs with triggers (TickAfter, ChapterComplete, PlayerReputation).
- `load_storylines()` is hardcoded — it returns a `Vec<Storyline>` constructed in code. This sprint replaces the hardcoded storylines with authored content files.
- The spec defines 5 major factions in ReachLock: Compact (human military alliance), ISC (Interstellar Commerce collective), Corporate Charters (mega-corps), The Reach (independent frontier alliance), Earth's Remnant (post-Earth isolationist state).
- The spec mentions three major story arcs: The Duskway Runs (Earth blockade), The Veil escalation, Alexander's long game.

## Freeze first

1. Faction profiles are `.json` files (matching spec §10 line 715). Storylines are `.ron` files using the `Storyline` struct from `core/src/faction.rs`.
2. The FIVE faction profiles form the canonical set — all future faction content references these five IDs.

## Deliverables

### 1. Faction profiles (`content/factions/*.json`)

- [ ] **`content/factions/compact.json`** — Compact: human military alliance. Doctrine: Diplomatic with militaristic undertones. Territory: core human systems. Produces: military ships, patrol services. Tariff: Regulated (foreign higher).
- [ ] **`content/factions/isc.json`** — ISC (Interstellar Commerce Collective): Trade guild. Doctrine: Economic. Territory: trade routes, hub stations. Produces: consumer goods, trade services. Tariff: Flat moderate rate.
- [ ] **`content/factions/corporate_charters.json`** — Corporate Charters: Mega-corporations. Doctrine: Exploitative. Territory: resource-rich frontier systems. Produces: manufactured goods, cybernetics. Tariff: Low to encourage trade, high on luxury imports.
- [ ] **`content/factions/reach_remnant.json`** — The Reach: Independent frontier alliance. Doctrine: Libertarian (minimal governance). Territory: outer frontier systems. Produces: raw materials, salvaged goods. Tariff: Minimal.
- [ ] **`content/factions/earth_remnant.json`** — Earth's Remnant: Post-Earth isolationist state. Doctrine: Theocratic/mystical. Territory: The Veil (blockaded zone around Earth). Produces: rare artifacts, salvage. Tariff: Prohibitive (blockade).
- [ ] Each profile includes: `id`, `name`, `doctrine`, `territory` (list of systems), `resources` (stock map of goods), `relationships` (starting affinity with other factions), `goals`, `color`.

### 2. Storyline arcs (`content/storylines/` directory, created if not exists)

- [ ] **`content/storylines/compact_arc.ron`** — The Compact's storyline: "The Armada Rebuilds." 5-8 chapters covering the Compact's recovery after a major defeat. Triggers: `TickAfter` for early chapters, `PlayerReputation { faction: "compact", trust: N }` for player-involvement chapters. Player can help or hinder the Compact's rearmament.
- [ ] **`content/storylines/veil_arc.ron`** — The Veil: the mysterious blockade around Earth. 5-8 chapters covering its origins, the Earth Remnant's true nature, and the player's choice to investigate or ignore it. Triggers: `PlayerInSystem { system_id: "veil_gate" }`, `HasItem { item_id: "predecessor_key" }`. Culminates in a choice: breach the Veil or maintain the isolation.
- [ ] **`content/storylines/alexander_long_game.ron`** — Alexander's conspiracy: a shadowy figure manipulating factions for unknown ends. 3-5 chapters, initially invisible (triggers only fire when the player has certain information). Triggers: `FlagSet { flag: "met_alexander" }`, `ChapterComplete { chapter: "veil_earth_briefing" }`. Player can align with, oppose, or ignore Alexander.

### 3. First Predecessor dungeon (`content/dungeons/predecessor_vault_alpha.ron`)

- [ ] **Layout** — 7 rooms: Entrance → Hall of Echoes (atmosphere) → Puzzle Chamber (glyph sequence) → Guardian Chamber (combat encounter) → Archive (lore discovery) → Vault Core (choice: take the artifact or study it) → Escape route.
- [ ] **Puzzles** — glyph sequence puzzle in Puzzle Chamber. Player must match a sequence of glyph symbols (visual: "triangle, circle, square, spiral..."). Failure triggers a Guardian Chamber encounter.
- [ ] **Encounters** — Guardian Chamber has 2 Predecessor sentinels (unique enemy archetype). Archive has environmental hazard (radiation leak — requires quick decision).
- [ ] **Rewards** — Vault Core offers: Predecessor artifact (`item_id: "predecessor_core"`), lore fragment ("The Builders' Lament" — a text entry for the Captain's Log), faction reputation with any faction of player's choice.
- [ ] **Narrative flavor** — each room has atmospheric description text. The dungeon tells a story: "The Builders sealed this vault for a reason. What they sealed — and why — is the dungeon's central mystery."

### 4. Load from files

- [ ] **Replace hardcoded `load_storylines()`** in `core/src/faction.rs` — instead of returning a hardcoded `Vec<Storyline>`, read from `content/storylines/*.ron` at startup. Fall back to hardcoded defaults if files don't exist (for offline play without content).
- [ ] **Faction profile loader** — add `load_faction_profiles()` in `core/src/faction.rs` that reads from `content/factions/*.json`. Fall back to hardcoded defaults.
- [ ] **Dungeon loader** — add `load_dungeon(id)` in `core/src/generator/dungeon.rs` that reads from `content/dungeons/*.ron`. The generator still produces procedural dungeons; authored dungeons override by ID.

## Acceptance gates

```
reachlock-cli content validate content/factions/compact.json
reachlock-cli content validate content/factions/isc.json
reachlock-cli content validate content/factions/corporate_charters.json
reachlock-cli content validate content/factions/reach_remnant.json
reachlock-cli content validate content/factions/earth_remnant.json
reachlock-cli content validate content/storylines/compact_arc.ron
reachlock-cli content validate content/storylines/veil_arc.ron
reachlock-cli content validate content/storylines/alexander_long_game.ron
reachlock-cli content validate content/dungeons/predecessor_vault_alpha.ron
# All pass

cargo test -p reachlock-core faction::storylines::
# Storylines load from files, chapters evaluate correctly
# PlayerReputation trigger fires at correct trust threshold
# ChapterComplete chaining works (arc1 → arc2)

make check
```

Manual: start a game → faction system loads from files → check faction relationships in-game → observe storyline chapter progression as conditions are met.

## Non-goals

Full branching dialogue for the dungeon (scripted encounter S41 handles that). Voice-acted NPC lines (S62). Dynamic faction warfare simulation (faction territory changes are event-driven, not simulated). All 50+ spec dungeons (this is the first — more are post-launch content).

## Gotchas

- The `load_storylines()` fallback to hardcoded defaults is critical for offline mode. If the content files don't exist (fresh clone, no `content/` directory), the game must still work. Use `include_str!` for the hardcoded fallback to avoid file I/O at runtime in core.
- Storyline chapter IDs must be globally unique (not just per-arc). Prefix with the arc name: `compact_arc_01`, `compact_arc_02`, `veil_arc_intro`, etc. The trigger system references chapters by ID.
- The Predecessor dungeon references a `predecessor_core` item that may not exist in the item generator. Add it as a minimal item entry in the generator (S05's item system) or create an authored item file. Either way, the item ID must resolve.
- Faction profiles establish the baseline relationship web. The starting affinity values must be coherent: Compact and ISC neutral (trade partners but wary), Corporate Charters distrusted by all, The Reach suspicious of everyone, Earth's Remnant isolated. The exact values should produce interesting gameplay at start (no maxed relationships, no impossible wars).
