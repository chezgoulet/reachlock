# Sprint 02 — The Living Universe: Handoff, Part 2 (finish the sprint)

You are picking up REACHLOCK Sprint 02 midway. Milestones M5–M8 have all shipped and are
live-verified. Your job is the remainder: **P4 StationDock, P7 PlanetScene, P8
ReputationPanel + FactionAction, and the stand-in sprite art pass** — plus authoring more
dialogue content (a known gap). Go as far as you can. Method: contracts first, additive
only, demo note per deliverable, all standing gates green before every push.

## State of the world (as of 2026-07-04)

Three repos, one game. All pushed, all CI green:

- **reachlock** `/home/c/git/chezgoulet/reachlock` @ `testing` (HEAD 11be93d) — your main workspace.
- **pan** `/home/c/git/chezgoulet/pan` @ `testing` (HEAD f16fd15) — the mind daemon. Likely
  untouched by this handoff; if you change it, the cross-repo harness in both repos' CI must pass.
- **ragamuffin** worktree `/home/c/git/chezgoulet/ragamuffin-raga` @ `sprint01/raga` (HEAD 6ee287c) —
  PRODUCTION service (powers the human's AI assistants); its API evolves **additively only**.
  The main checkout at `/home/c/git/chezgoulet/ragamuffin` stays on `main` — never touch it.
  Promoting ragamuffin testing→main is a human decision; never do it.

Shipped this sprint, all on reachlock `testing` with demo notes in `docs/demos/`:

- **M5** (aae3583) validation debt: DSL conformance bridge in CI, deep review
  (`docs/reviews/2026-07-03-sprint02-deep-review.md`), vault hygiene (REACHLOCK_VAULT_PREFIX,
  isolated `dev_stack.sh test` stack), loopback+auth by default.
- **M6** (4878e43) the universe moves: Sim Protocol v0 (`docs/SIM-PROTOCOL.md`, 15 fixtures at
  `godot/framework/protocol/sim/fixtures/`), `reachlock-simd` daemon (`server/internal/simd`,
  port 40708), `SimGateway` autoload (1 tick/s, batch advance on undock, snapshot in every
  `advanced` so saves are mid-motion), `MarketBoard` + `EventFeed` framework scenes.
- **M7** (cf88fb4 + pan f16fd15 + raga d84d92e) talking feels real: pan async perceive
  (supersession at the enact boundary), history channel (multi-turn coherence),
  conversation→facts→next-session recall verified live.
- **M8** (8f8b8af) two souls aboard: `CrewRoster` autoload (P6) + `ShipInterior` scene (P5),
  Tib@cockpit + Tove@cargo_hold, shared news reactions, relationship graph persisted in the save.
- **Playtest fixes** (11be93d + raga 6ee287c): dev_stack now probes the chat model and
  auto-falls-back gemma4:e4b→gemma3:4b on OOM; landed talk falls back to unscripted LLM exchange
  when no authored dialogue guard passes; ragamuffin vault-provisioning race (was destroying new
  vaults) serialized and non-destructive; space_flight physics teardown guard.

## Read before building (in order)

1. This file, then `docs/SPRINT-02-the-living-universe.md` (the sprint's north star; note the
   P4/P7/P8 contracts live HERE, below — not in that file).
2. `docs/SYSTEM-INTEGRATION.md` — the system architecture narrative + how to run the harness.
3. `docs/demos/M5…M8-*.md` — what verifiably works today, in transcript form. Match this format.
4. `GAME-DESIGN.md` sections 2 (Three Modes), 3 (Foundational Systems), 5 (Factional Engine),
   7 (Soul System), 8 (Modding-First).
5. `godot/framework/schemas/` — the modder's API. `godot/framework/protocol/SOUL-PROTOCOL.md`,
   `docs/SIM-PROTOCOL.md`, `docs/MEMORY-INTERFACE.md`, `docs/UNIVERSE-TICK.md` — frozen contracts.
6. Existing framework scenes as your pattern: `godot/scripts/framework/market_board.gd`,
   `event_feed.gd`, `ship_interior.gd`, `crew_roster.gd`, `sim_gateway.gd`; landed mode at
   `godot/scripts/landed/landed.gd`.

## Philosophy — Build Tools, Not Features

Every system produces two things: a **framework primitive** in `godot/scripts/framework/` (+
`godot/scenes/framework/`) and a **content instance** in `godot/mods/reachlock/`. The rule: if a
modder cannot produce a new instance by writing only data files (and at most a one-line
`extends <Primitive>`), the primitive is incomplete. Schema changes are additive with a version
bump + migration note. Every visual asset is a framework default a mod can override by placing
files in its own asset directory.

## Remaining scope — the contracts (verbatim from the sprint brief)

### P4 — StationDock (framework scene template)

Technical contract: A StationDock.tscn scene template with:
- Named NPC slots (modder assigns which NPCs are present)
- Service points (market, ship services, mission board, bar) configurable from location data
- Configurable background (station interior style differs by faction/class)
- Animated NPC stand-in sprites placed at their assigned locations
- Docking bay visual (not just gray — a recognizable space with the player's ship visible)

The scene is populated from the location's JSON data. A modder adds a station by writing a
location.json — no engine code.

Narrative contract: When you dock, you are in a place. The station has a character — a cramped
frontier outpost feels different from a polished Corp Charter hub. The NPCs present are not text
buttons; they are characters you can see, walk up to, and talk to.

Content instance: Sorrow Station — a neutral frontier station with Tib at the bar, the market
counter, and a news panel. Faction: independent. Economy: mining and salvage. Atmosphere: worn
but functional.

*(Handoff note: "mission board" is a visual service point only — the mission system itself is
not in this sprint. Render the point; wire nothing behind it. The existing MarketBoard/EventFeed
scenes should mount at their service points rather than being rebuilt.)*

### P7 — PlanetScene (framework scene template)

Technical contract: A PlanetScene.tscn template for the Landed (planetside) mode. Provides:
- A tile-based ground layer driven by location biome data (color, terrain type, sky)
- A top-down or isometric camera mode (distinct from Space flight camera)
- Points of interest rendered from location data (settlements, ruins, resource nodes, landing pads)
- Mode transition hooks (Space → Atmosphere Flyover → Landed surface, and reverse)
- NPC placement on the surface (visible characters at POIs)

The scene is populated from location JSON data. The ground layer, biome colors, POI positions,
and NPC placements are all data-driven. A modder adds a planet surface by writing a location.json
with biome data and POI coordinates.

Constraint: This sprint builds the data-driven hand-crafted path. Procedural generation — Perlin
noise terrain, POI scattering, etc. — is explicitly deferred. The PlanetScene loads explicit
data, not algorithms.

Narrative contract: A planet is not a waypoint on a map. It is a place you land on, walk around
on, and find things on. The ground you walk on and the sky above you are set by the location
data, not hardcoded.

Content instance: Aethon — a trade hub planet with a landing pad, a market district, and the
ruins of a Predecessor structure visible in the distance. Surface biome: arid temperate. Sky:
hazy orange from the system's star.

### P8 — ReputationPanel + FactionAction (framework UI + schema)

Technical contract: A ReputationPanel.tscn UI component and a FactionAction data schema that
together make the faction engine visible and interactive.

ReputationPanel displays the player's standing with each known faction across multiple axes
(trust, contribution, notoriety). Integrated with MarketBoard — prices are modified by standing,
some goods are restricted by rep level, some services are only available to trusted operators.

FactionAction schema defines what player actions count as faction inputs:

```json
{
  "action_id": "trade_with_rival",
  "description": "Trading with a faction's rival damages standing",
  "faction_delta": { "trust": -5, "contribution": 0 },
  "rival_faction_delta": { "trust": 2, "contribution": 1 },
  "trigger": "on_trade_completed"
}
```

A modder adds new faction-relevant actions by writing a FactionAction JSON file. No engine code.

Narrative contract: Your choices have political weight. The faction system is not a reputation
bar — it is a web of relationships you navigate through every interaction.

Content instance: Trading ore with Sorrow Station's market (independent) slightly increases
standing with independent-aligned factions and slightly decreases standing with the Compact. The
player sees this reflected in prices and available services on subsequent visits.

### Stand-in sprite art pass

Consistent stand-in style across characters, stations, planets: late-90s SNES RPG visual
language (Final Fantasy, Link to the Past, Stardew) — flat 2D, solid colors, minimal shading,
top-down or side-view as context demands. Not pixel art, not painterly. NPCs: distinctive
silhouette + one primary color each (the npc `color` field already exists — build on it).
Stations: tiled background + visually distinct service points. Ships: color-coded room zones
(ShipInterior already does zone colors — extend, don't rebuild). Every asset is a framework
default with a mod override path (engine loads mod asset if present, framework default if not —
build that loader convention if it doesn't exist yet).

### Dialogue content (known gap, flag it in your demo notes)

Exactly ONE authored dialogue exists (`tib_dock_debrief`). Author several more for Tib and Tove
(station and aboard, varied guards using the trigger DSL) as content instances proving P4.
Write them in-voice from GAME-DESIGN.md; note in the demo doc that dialogue is content the human
may rewrite.

## Standing verification gates (every push)

- `make check` (architecture guard, mod validation, protocol conformance) and `make harness`
- `cargo test` in pan (only if you touch pan) — PATH needs `~/.cargo/bin`
- `cd server && go build ./... && go test ./...` (reachlock); `go test ./... -short` (ragamuffin, if touched)
- Headless Godot import + boot clean (flatpak Godot)
- Architecture guard: Ring 0 has ZERO content ids — test files use neutral ids
  (`faction_a`, `good_ore`, `station_a`); `arch-allow` comment is the escape hatch
- DSL bridge green
- A deliverable without its demo note in `docs/demos/` did not happen

## Environment and operational gotchas (hard-won — believe them)

- Dev stack: `./scripts/dev_stack.sh` (play: pan 40707 + ragamuffin 8000 auth'd + simd 40708);
  `./scripts/dev_stack.sh test` for automated runs — export what it prints, ESPECIALLY
  `REACHLOCK_VAULT_PREFIX=test-`. Automation must NEVER write play vaults (port 8000).
- Ollama: gemma4:e4b needs 9.8 GiB and often doesn't fit with the editor up; the stack probes
  and falls back to gemma3:4b automatically. `PAN_LLM_MODEL` / `REACHLOCK_LLM_FALLBACK_MODEL` override.
- The human's playtest save lives at
  `~/.var/app/org.godotengine.Godot/data/godot/app_userdata/REACHLOCK/saves/slot0.json`
  (flatpak Godot). BACK IT UP before any headless run that could touch saves; restore after.
- Never edit protocol fixtures to make an implementation pass. Soul fixtures are byte-identical
  across reachlock and pan (the harness enforces this); sim fixtures live once in reachlock.
- Godot JSON numbers are float64: wire integers must stay ≤2^53; the Go side already normalizes
  floats like `-43.0` on decode — keep new sim fields inside that envelope.
- `pkill -f <pattern>` that matches your own shell command kills your shell (exit 144).
  Use `pkill -x` or kill by pid.
- Shell cwd may reset between commands — always `cd` with absolute paths, per command.
- `docs/SPRINT-02-the-living-slice.md` is the human's untracked draft (in `.git/info/exclude`).
  Never commit it. Never use `git add -A` blindly; add files by name.
- Ollama `think:false` is honored only on the native `/api/chat` endpoint, not the
  OpenAI-compat one (reasoning models return empty content there).

## Out of scope — do not start

Hull/interior editors; procedural planet generation; full combat for Landed/On Board; MMO
netcode, subscriptions, credits; Predecessor dungeons; production chains/crafting; the deferred
crew (Bardo, Doc Keene, Prudence, Risc, Boris); a mission SYSTEM (P4's mission board is visual
only); promoting ragamuffin testing→main and merging any repo's testing→main (human decisions).

## Definition of done

P4, P7, P8, and the sprite pass shipped on `testing` with demo notes; all gates green in CI on
every repo you touched; playtest save intact; play stack left running (`./scripts/dev_stack.sh`)
so the human can `make godot` and walk Sorrow Station, land on Aethon, and check their faction
standing without any setup.
