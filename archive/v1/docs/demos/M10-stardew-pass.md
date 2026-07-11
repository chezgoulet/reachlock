# M10 — "It looks like a place now" — the Stardew pass ✅ (2026-07-09)

Sprint 2 of the Duskway Run demo: the Sprint 1 spine, brought up to
pixel-art-and-interaction quality. The ship is furniture and light instead
of colored rectangles; the crew walk-cycles; the stations are consoles you
open, not lines you read; Sorrow Station is a place you walk — concourse,
the Interval, Doss's office, market row — and the bar is visibly wrecked
after the fight. NPC speech never stalls the scene again.

## Dialogue latency (the hard requirement)

Hybrid authored/LLM dialogue in `dialogue_runner.gd` + content:

- **`buffer_line`** on generated nodes (dialogue schema, additive): an
  authored beat lands **immediately**; the mind's line follows as a second
  beat when inference finishes. Nodes with choices stay interactive the
  whole time ("offered"); choiceless nodes bridge buffer → real line
  ("held"). Moving past a node **drops** its late line and supersedes the
  in-flight goal at the daemon (`SoulInstance.supersede()`, M7 semantics).
- **Thinking indicator** (`thinking_changed`): a pulsing `· · ·` in every
  dialogue host while a mind works — the game visibly breathes, never
  freezes. 15s ceiling + abandon fail-fast unchanged.
- **Routine questions never hit the LLM**: authored Q&A hubs —
  `tib_cockpit_hub` (the ship's name, the Compact, and the shape of what
  he won't say about Québec), `prudence_flight_hub` (the crossing, the
  personhood answer, her read on the crew), `bardo_galley_hub` (the work,
  the Predecessors, home). Every Sprint-1 generated node gained a
  buffer_line; the trees themselves are untouched.
- pan-daemon and the Soul Protocol: **untouched**, per the boundary.

## The art pass

`scripts/gen_pixel_art.py` (PIL, deterministic, committed) generates every
sprite through the existing AssetLibrary override convention — an artist
replaces PNGs file-by-file and nothing else moves:

- **11 character sheets** (7 crew + Doss, Grissom, Noor, Vex + the player):
  24x32, 4-direction, 4-frame walk cycles, per-character palettes from the
  brief (Tib's worn jacket, Doc's medcoat, droid visors for Prudence/Boris,
  Grissom's dock-worker build). Rendered by the new **CharacterSprite**
  node (walk animation, idle breathing, StandIn fallback when no sheet).
- **10 floor/wall tiles** (deck, grate, med, galley, quarters, cargo, bar
  planks, office, cryo) and **31 props** (consoles, drive core, cryo pods,
  bunks, bar counter + bottle shelf, tables intact AND broken, wrecked
  terminal, extinguisher residue, memorial walls, desk, shop counter...).
- Project-wide nearest-neighbor filtering: crisp pixels at every scale.

## Interiors

- **InteriorWorld** (new shared framework node): tiled floors by room
  kind, walls with door gaps, **real wall collision with door bridges**
  (walk is slide-along-walls, through doorways only), props from data with
  trigger-DSL `condition`s re-evaluated on state change, affordance glow
  ring on the nearest interactable.
- **Loup-Garou furnished** (27 props in hull JSON): wraparound viewport +
  pilot console + nav screen in the cockpit, drive core glow in
  engineering, pods in cryo/med bay, bunks and lockers, galley table with
  Tove's charts, mining rig in the hold.
- **Stations are minigames** (`station_minigames.gd`): engineering opens
  the **power routing grid** — weapons/shields/engines shares that persist
  (`player.ship.power`, save schema) and the next flight actually flies
  (speed, fire rate, damage soak); scanner opens a **sweeping radar scope**
  reading the current system's real data; cargo opens the manifest;
  weapons opens the gunnery check with live numbers. The pilot's seat IS
  the launch. Esc closes any panel (also fixes the Sprint-1 trap where the
  pilot seat froze you forever).
- **StationInterior** (new): locations with an `interior` block (location
  schema, additive) mount a walkable station instead of the dock panel.
  **Sorrow Station authored**: dock concourse (airlock berth, the 847-name
  memorial wall AND the second small one), the Interval (bar counter,
  bottle shelf, stools, tables — swapped for fallen stools, broken tables,
  a dead sparking terminal and extinguisher fog decals once
  `bar_fight_done` sets), Doss's dock office, market row (outfitting
  counter = UpgradeShop, market counter = MarketBoard, chatter terminal =
  news feed). Prudence and Grissom stand in the bar; Doss in her office;
  the fight scene auto-plays exactly where it happens.

## Space flight feel

- **Red alert**: flashing vignette + klaxon on ambush and on arriving
  inside a hostile picket line.
- **Crew callouts** on the intercom, chosen by ROLE from whoever is
  actually aboard (engine never names a character): contact calls, hull
  status at 75/50/25%, stealth signature report, "drive can spool — the
  pods are prepped" after the ambush.
- **Tracers** both ways (player slugs, patrol return fire, visible
  misses), explosion/impact audio.
- **Radar minimap** with heading, station/pirate/patrol blips, and
  **hostile detection rings drawn to scale — stealth purchases visibly
  shrink them**.
- **Cordon countdown ring** on the MissionHud: drains green → amber →
  pulsing red alongside the digits.
- **Earth approach**: a 240-unit planet with an atmosphere rim fills the
  sky behind the landing beacon at any `kind: planet` location.
- **Sorrow reads as a port**: pulsing running lights, two ships at berth.
- **Cryo transit**: the pod bank is drawn — one pod per sleeper, sealing
  one by one with cryo-frost as the script reads names, reopening on the
  far side — plus hull vibration at the jump threshold and wake.

## Verified

- **84/84 GUT tests**, `make check` green, dsl-bridge green, headless import clean.
- New contract tests (+18): dialogue buffer semantics (7), InteriorWorld
  walkability + conditional props (4), power routing (4), CharacterSprite
  facing/fallback/sheet-coverage (3). AssetLibrary now decodes images from
  bytes — export-safe, and the engine's imported-image warning is gone.
- Headless smokes of every scene against the live stack.

## Known gaps (tracked, non-blocking)

- Weapons station is a systems-check panel, not a firing minigame — live
  gunnery still happens in flight (by design this sprint).
- Comms station not built (no comms station in the hull data yet).
- The bar fight plays as the scripted scene inside the real bar room with
  before/after set-dressing — swing-by-swing fight animation is an art
  pass beyond placeholders.
- Docking approach uses the existing station model + lights; no dedicated
  approach cinematic.
- Character sheets are generated placeholder pixel art — deliberately
  replaceable PNG-by-PNG via AssetLibrary.
