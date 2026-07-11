# REACHLOCK: The Open Hatch — Fable Brief (draft handoff, rev 2)

> Status: DRAFT — written by the previous Fable session as a handoff,
> revised after Christopher's review. He edits, then issues. Untracked
> until issued.

## The outcome, in one sentence

A stranger downloads one file, double-clicks it, sees a crew drawn like
people worth knowing, plays ninety minutes — **and can invite a friend
onto the same deck and talk to the crew with their own voice.**

## Where the repo stands

The Duskway Run demo is complete and has survived three quality passes:
playable crew with real stats, a two-deck zero-G ship with damage control,
an isolated dialogue surface with typewriter masking and mind-status
lamps, and a beat-by-beat second-playthrough pass — named rocks, real
cargo loss, the held cryo beat, a bar fight that echoes, five run verbs,
endings with numbers and names. 136 tests, architecture guard, dsl-bridge,
all green. Read `docs/demos/M9..M12-*.md` first; they are the campaign's
memory.

Four things are now fully unaddressed, and three of them hinge on
contracts that get expensive to change later: **voice**, **woven
deterministic dialogue**, **multiplayer**, and **the art direction**.
This sprint's shape follows the Sprint-01 pattern that worked: freeze the
contracts first, then build slices as far as the budget runs.

---

## Priority 0 — freeze the contracts (Phase A; nothing else starts until these are written)

These are the irreversible decisions. Each becomes a versioned schema/doc
in `godot/framework/` with fixtures, exactly like the Soul and Sim
protocols. A thin working slice proves each one; depth comes later.

### A1. Ear Protocol v0 — voice in

The player speaks into a microphone, naturally or in character. Design it
as an INPUT METHOD, not a new dialogue system — speech must enter the
game at exactly the two doors that already exist:

1. **Choice matching**: transcribed speech is semantically matched against
   the offered dialogue choices (embeddings — ragamuffin already serves
   them). Close enough → that choice fires, deterministically, exactly as
   if clicked. Say *"I'd do it again — she's crew"* and the STEP-IN choice
   lands.
2. **Free speech**: no match, live mind → `perceive_utterance` (the path
   that already exists), buffer-line latency mask and all. No match, no
   daemon → the panel shows what it heard and the choices stay up.

Contract: `EAR-PROTOCOL.md` + fixtures — a loopback STT sidecar
(`server/cmd/reachlock-eard` wrapping whisper.cpp, port 4070x, NDJSON like
its siblings): push-to-talk key (suggest V) → audio chunk → {transcript,
confidence}. **Optional and silent like pan**: no daemon, no model, no
mic permission → the voice button simply doesn't exist. The transcript
enters the game as text — meaning multiplayer, logging, and tests all see
voice as ordinary input. That one property is the whole design; protect
it.

### A2. Weave contract v0 — inference branches that change the world, deterministically

The direction: authored trees stay the backbone; inference + RAG grow
**branches from what's written** — and those branches may carry real
world effects. The contract that makes this safe (and multiplayer-ready):

- New dialogue node kind `woven`: carries a `prompt_hint`, RAG grounding
  refs (compendium/lore vault queries via MemoryStore — ragamuffin is
  read-only from here, untouched), and a **`may` allowlist**: which
  mutation ops, which flags, which factions, what magnitude caps this
  node is permitted to produce.
- The mind proposes `{line, choices[], mutations[]}` as structured data;
  the engine **validates against the allowlist and clamps or drops**
  anything outside it. A woven branch can move faction standing 3 points
  because the author said it may — never 40 because a model felt like it.
- The proposal is **resolved once, then persisted in the save** as
  ordinary dialogue data: replays, reloads, and every multiplayer client
  see the same branch. Generation happens at one authority; the world
  only ever consumes data. This is the property that makes A3 possible.
- Known gap to note, not fight: pan v0's LLM provider emits Express +
  Conclude only (no tool invokes, M7 note). Design the contract for the
  full shape; implement v0 with line + choice text generation and
  author-supplied effect menus; the invoke-carrying version is a
  pan-budgeted sprint.

### A3. Ship-Share v0 — LAN multiplayer contract

The demo's multiplayer is **co-op crew on one boat** — the thing the
whole architecture has been pointing at: ShipOperation's station
occupancy (occupy/vacate/controls) was built for more than one pair of
hands. LAN over Godot's high-level ENet; players connect by IP, with
Tailscale/ZeroTier as the documented remote path — zero NAT code.

Contract: `SHIP-SHARE.md` — host-authoritative, and write down what that
means: souls, sim, missions, weave resolution, and RNG live on the host;
clients send intents (movement, station controls, dialogue choices,
transcripts) and receive state. The save is the host's. Character select
becomes seat-claiming in a lobby: each player IS a crew member; claimed
crew stop being NPCs (the single-player substitution logic already
exists — generalize it). Version-stamp the protocol from day one and
refuse mismatches loudly.

Slice to prove it: two players walk the ship and Sorrow together, see
each other move and talk, and one flies while the other works the power
grid mid-cordon. That slice IS the FTL × Among Us pitch, live.

## Priority 1 — the face and the hatch (what strangers meet first)

### The art direction pass (spec from Christopher, references attached to the issuing message)

Terraria-school retro-SNES character sprites, adapted to a universe where
**no aliens have been discovered** — it's humans, scattered into the
galaxy's wild corners, dressed by their biome and their work:

- **Frame**: grow the character frame from 24×32 to ~32×48 (drawn at 2×),
  Terraria proportions — big readable head (~⅓ of height), expressive
  hair/headgear as the identity silhouette, layered body. Keep the sheet
  contract (4 directions × 4 walk frames, AssetLibrary override,
  `CharacterSprite.FRAME` is the one engine constant to touch). Every
  sprite stays replaceable PNG-by-PNG.
- **Rendering language**: dark outline, 3-step shading ramps, saturated
  palettes, walk cycles with visible leg/arm swing and hair bounce —
  study the attached Terraria NPC sheets for the level of costume detail
  per character.
- **Wardrobe is worldbuilding**: a `wardrobe` vocabulary in npc data the
  generator consumes — Reach miners in sun-faded canvas, tool belts,
  sealed boots; station dwellers in layered synthetics that never see
  weather; Earth-remnant in old-world fabric; Corp Charter crisp and
  uniform; droids as chassis with humanizing touches (Prudence's
  presentation is authored — honor it). No rubber-forehead anything:
  every face is human or machine.
- Regenerate all sheets + the character-select portraits; the select
  screen is where the new art earns its keep first.

### The hatch (unchanged from rev 1 — still the multiplier)

Title screen (New Game / Continue / Settings / Quit — nobody deletes JSON
by hand again), pause menu (mind the Esc-vs-panel ordering), settings
that persist outside the save (volumes, fullscreen, typewriter speed,
keybind card — now including push-to-talk), **offline-first confidence**
(no daemons → no error spam, campaign completes; add offline variants to
the journey tests), export presets + CI artifacts for Linux/Windows with
a version stamp, exported-artifact headless smoke, and `docs/PLAYING.md`
for players — including the two-machine LAN quickstart and the "minds
and voice are optional; here's what the full stack adds" section.

## Priority 2 — the ship breathes (unchanged from rev 1)

Routines from data (`routine` blocks: room, prop, activity label,
trigger-DSL gates), crew walking their day with the deck-aware movement
that already exists, co-located crew bantering through the DialoguePanel
bark surface seeded by the persisted relationship graph, story-reactive
moments (post-ambush med bay, the sleepless night before the cordon).
Routines yield to auto scenes, timers, and damage control. In
multiplayer, NPCs breathe on the host and everyone sees the same life.

## Priority 3 — stretch

The sound of the boat (CC0 beds: title, drift, red-alert layer, Sorrow
bar, cordon pulse; silence kept for the cryo crossing; room tone per room
kind; bus routing so the settings sliders mean something; attribution
file).

## Hard requirements

- The dialogue latency contract is untouchable — voice adds STT time on
  top, so the mask matters MORE: the panel shows "…listening" and the
  transcript immediately, before any mind responds.
- Multiplayer never blocks single-player: no lobby screens in the solo
  path, no regression to boot time, hosting is one button.
- Tests: ≥ 170 total, green twice consecutively. New contracts get
  fixture tests like Soul/Sim protocols; weave clamping gets adversarial
  tests (a proposal that exceeds its allowlist must be provably
  neutered); seat-claiming and choice-matching get contract tests that
  run without mic or network hardware.
- `make check` + dsl-bridge green; guard clean (it reads comments and
  matches content ids as words — "reach" in prose fails).
- Single commit on `testing` per landed pillar is acceptable this sprint
  (contracts, art, hatch, multiplayer slice may land separately);
  player-experience messages.
- Back up the play save before headless smokes; restore or delete
  deliberately.

## Boundaries

Do not modify: pan-daemon, ragamuffin, the Soul Protocol schema, the four
storyline JSONs, the authored dialogue trees (extend, never rewrite).
reachlock-simd may gain a SIBLING (`reachlock-eard`) but not changes.
Mission and dialogue schemas may gain fields. Where an outcome needs
pan-side work (weave invokes, streaming STT), write the contract, note
the gap, move on.

## Workflow ledger (lessons already paid for — do not pay twice)

- Headless import parse-check after any batch of .gd edits; GDScript
  cannot infer types from Dictionary-member expressions — annotate.
- GUT counts engine *warnings* as failures.
- DataRegistry dictionaries are shared — `.duplicate()` before mutating.
- `_find_dialogue_for` picks the first passing dialogue in alphabetical
  id order — mutually exclusive guards or self-gating seen-flags, and
  choose ids with the sort in mind.
- Headless hooks: `REACHLOCK_FORCE_MODE`, `REACHLOCK_AUTOSAVE_AFTER`,
  `REACHLOCK_CHARACTER=<npc id>`.
- Stack: `./scripts/dev_stack.sh`, pan 40707 / simd 40708; the box is
  memory-marginal for gemma4 with the editor open (auto-fallback exists).
  Whisper will share that budget — size the model accordingly (base/small).
- `pkill -f` with a string matching your own command kills your shell.
- Tile regeneration is deterministic (crc32); the character generator is
  the file to grow for the art pass (`scripts/gen_pixel_art.py`).

## The closer

The scope is the full vision. The budget priority is: **contracts first —
they are the decisions we cannot cheaply unmake; then the face and the
hatch; then two people on one deck.** Fable decides how far it gets.
