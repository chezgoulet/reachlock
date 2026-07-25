# ReachLock — Character Creation & Open-World Identity

**Date:** 2026-07-24 · **Companion to:** `docs/UX-AUDIT-AND-PLAN.md`
**Premise:** the Loup-Garou and its crew were a development stand-in. ReachLock
is an open world where players create a character and play as they choose.

---

## 0. The one-paragraph answer

Roughly **six sprints**, one of which (the flow itself) is blocked on the client
UI framework from S70. The visual half of a character creator already exists in
core and is currently only used by an editor previewer. The data half — a
`SoulFile` rich enough to model a person — exists and is currently only used by
NPCs. The real work is **(a)** giving the player an identity that the engine
knows about at all, **(b)** unifying two parallel appearance systems that don't
talk to each other, and **(c)** surgically removing the Loup-Garou and its six
named crew from load-bearing engine code, including a content file that is
`include_str!`'d into `reachlock-core` at compile time.

---

## 1. What you already have (more than expected)

| Asset | Location | Why it matters |
|---|---|---|
| **Parametric character appearance generator** | `core/generator/sprite.rs:22-517` | `CharacterLookConfig` already exposes species, 7 hair styles, and hair/skin/shirt/pants/jacket/chassis/visor colors, each *optionally pinned or seed-derived*. This is exactly a character creator's backend, already written and deterministic. |
| **A working appearance-editing UI** | `editor/editors/character_sprite.rs` (479 lines) | The `SpriteViewer` is a live look explorer over that generator — sliders, species switch, seed reroll, walk-cycle preview. It is a character-creator prototype that happens to live in the editor. |
| **A complete person data model** | `core/soul/types.rs` — `SoulFile` | species, identity, personality, speaking style, emotional baseline, memory tree, relationship graph, goals, breaking points, secrets, backstory, dialogue graph, contract ids. Frozen and wire-tested. |
| **Procedural soul generation** | `core/generator/soul.rs:128` — `generate_soul(seed, species)` | Generates whole personalities. The raw material for recruitable crew and station NPCs. |
| **Career progression** | `core/career/mod.rs` — `PlayerCareer`, `CareerPath`, `join_path`, `advance_rank`, `leave_path` | 7 path types, ranks, perks, progression criteria, conflicting paths. A background/origin system can hang directly on this. |
| **Ship customization, in-game** | `client/systems/shipeditor/` (S17/S18) | Exterior hull config and interior deck layout are already player-editable at a shipyard terminal, and already persist in the save. |
| **Faction reputation, economy, item generation** | `ticker.rs`, `factions.rs`, `core/item/` | Everything a "starting conditions" package would need to grant. |

**Nothing here needs inventing. It needs connecting.**

---

## 2. What's hardcoded (the actual work)

### 2.1 The player has no identity at all

This is the foundational gap. There is no player name, species, appearance,
pronoun, background, or soul — anywhere:

- `SaveFile` (`inventory.rs`) holds inventory, location, universe, soul *states*,
  hull config, interior layout. **No character.**
- The wire (`core/network/messages.rs`) carries `player_id: String` and nothing
  else. Presence (S23) syncs remote *ships*, never remote *people*.
- The avatar is literally one hardcoded lookup — `interior.rs:543-553`:
  ```rust
  // The avatar: Tib, captain of the Loup-Garou (docs/LORE.md §V) — dark …
  pixel::crew_look("tib"),
  ```

### 2.2 There is no "new game"

`AppState` (`states.rs:33`) is `MainMenu | InGame`. The main menu offers
`Launch | Settings`. And `inventory::load_save` runs in **`Startup`**
(`main.rs:218-221`), chained after `init_souls` — so the save is loaded before
the player ever sees the menu. There is no New Game / Continue distinction to
hang creation off, and no state for creation to live in.

### 2.3 Two parallel, non-communicating appearance systems

This is the sharpest structural finding:

| | `core::generator::sprite` | `client::pixel` |
|---|---|---|
| Type | `CharacterLookConfig` → `CharacterSprite` | `Look` |
| Species | `species: String` | `BodyKind` enum (5 variants) |
| Hair | `hair_style: Option<u8>` (7 styles) | `Hair` enum (7 variants) |
| Colors | `[u8;3]` triples, optional | `bevy::Color`, required |
| Deterministic | yes (`SeededRng`) | yes (`Noise`) |
| **Used by** | **the editor previewer only** | **the entire game** |

They model the same concepts with zero shared code. `generate_character_sprite`
is not called anywhere in `reachlock-client`. Whatever you build the creator on,
the other must become a renderer of it or be deleted — otherwise a player
customizes their look in one system and the game draws them from the other.

### 2.4 The Loup-Garou is baked into engine code

| Where | What |
|---|---|
| `client/crew.rs:68` | `CrewRoster::default_crew()` — six named members with lore ids, inserted unconditionally at `main.rs:211` |
| `client/pixel.rs:419-500` | `crew_look(id)` — a `match` over `"tib" \| "tove" \| "keene" \| "bardo" \| "prudence" \| "risc" \| "boris"` returning hand-authored palettes |
| `client/crew.rs:149,160` | `deck_of()` / `deck_zero_g()` call `core::generator::ship::loup_garou_interior()`. **Crew pathing consults the authored ship regardless of which ship the player is flying.** Every custom interior silently resolves decks against the Loup-Garou. |
| `core/soul/runtime.rs:388-391` | `include_str!("../../../mods/reachlock/storylines/loup_garou_souls.ron")` — **a content file compiled into `reachlock-core`.** Direct violation of iron rule #1 (core is pure) and of the S22 engine-purity guard, which `make check-purity` misses because it scans `souls/ stations/ hulls/ systems/` and not `storylines/`. |
| `client/main.rs:105-111` | Starting location hardcoded to Aethon, `system_seed: 16843009` |

The `include_str!` is the blocker to watch. An engine with the canonical crew
compiled into it cannot cleanly serve arbitrary player-created characters, and
it will fight every attempt to make crew data-driven.

### 2.5 Crew is a fixed party, not an open-world roster

`CrewRole` is a closed 5-variant enum; `default_crew()` is a fixed 6; there is
no hire, fire, recruit, injure, or death path. Duty rooms map to lore spaces on
the authored ship. For an open world, crew needs to become a population you
build, lose, and rebuild.

---

## 3. Design decisions to make before building

These change the shape of the work. My recommendation is given first.

### 3.1 Does the player get a Soul? — **Recommend: yes**

This is the highest-leverage decision in the whole plan. The soul system is
ReachLock's differentiator, and everything in it keys on soul ids:
`relationship_graph`, `RelationshipMemory` (`soul/compression.rs`), co-deliberation
trust deltas (`contract/co_deliberation.rs`), breaking points, secrets.

If the player character is a `SoulFile` like everyone else:
- NPCs form **persistent, compressing relationships with the player**, not just
  with each other. That is the open-world hook.
- Co-deliberation gains a real participant instead of a special case.
- Crew can have breaking points *about the player's* choices.
- The trope/dilemma/storyline engines get a subject to reference.

Cost: `SoulFile` is a frozen wire shape (iron rule #4), so adding a
player-character variant is a deliberate protocol revision, not an afterthought.
Do it in the first sprint, before anything depends on the old shape.

If you decide *no*, the player stays a camera with an inventory and most of the
game's emotional machinery can never point at them. I'd argue strongly against.

### 3.2 What survives of the Loup-Garou? — **Recommend: demote to content**

Keep the ship and the seven crew as an **authored starting package** — one
option among several, and a lore artifact you can encounter. Do not delete the
content; delete its privileged position in engine code. `loup_garou_interior()`
becomes one entry in a ship-template catalog under `mods/reachlock/hulls/`, and
the six crew become authored souls a "Loup-Garou veteran" background grants.

This preserves the lore in `docs/LORE.md` and the work already done, while
making it data.

### 3.3 Which appearance system wins? — **Recommend: core**

Unify on `core::generator::sprite::CharacterLookConfig`. It is deterministic,
already parametric, already has an editing UI, lives in the pure crate (so the
server and CLI can reason about appearance), and serializes. `pixel::Look`
becomes a thin Bevy-side renderer that *derives* from a `CharacterLookConfig`.
`crew_look()`'s hardcoded match dissolves into authored look configs on the
soul files.

### 3.4 How much does the creator determine? — **Recommend: a lot, reversibly**

The interesting design space here isn't sliders, it's **what the choices cost
you**. Concretely: background/origin should grant a starting career path,
faction reputation (positive *and* negative), credits, a ship, gear, crew, and
known systems — and close doors as well as open them. `CareerPath` already has
`conflicting_paths`, and the faction engine already models standing. A
"Compact deserter" origin should mean something in the economy on day one.

Keep it reversible in play: careers already support `leave_path` with
`CompletionReason`. Character creation sets the *opening position*, not a class.

### 3.5 Multiplayer identity — **Recommend: defer, but freeze the shape now**

Don't build remote-character rendering yet. Do add name/species/look to the wire
shape in the first sprint so you aren't doing a second protocol revision later.

---

## 4. The build plan

Six sprints. **S78 hard-depends on S70** (client UI framework) from the UX plan —
you cannot build a multi-step creator out of `Text::new(format!(…))` and a
hardcoded row cursor. S75–S77 have no such dependency and can start immediately.

### S75 — Player identity in core *(freeze contracts first)*
- Define `PlayerCharacter` in `reachlock-core`: id, name, pronouns, species,
  `CharacterLookConfig`, origin/background id, and a `SoulFile`.
- Decide and implement §3.1. If the player gets a soul, revise `SoulFile` (or add
  a sibling) deliberately and update the wire-shape test with a note in the
  commit message, per iron rule #4.
- Add `character: Option<PlayerCharacter>` to `SaveFile` (`Option` so existing
  saves migrate), and name/species/look to the presence wire shape.
- Extend `core/determinism.rs` — appearance and soul generation are generators,
  so goldens must cover them (iron rule #3).

**Gate:** wire-shape + determinism tests pin the new types. A save without a
character still loads.

### S76 — One appearance pipeline
- `pixel::Look` derives from `CharacterLookConfig`; delete the `crew_look()`
  match (`pixel.rs:419`).
- Move the `SpriteViewer`'s controls into a reusable widget so the editor
  previewer and the in-game creator render the same UI from the same config.
- Authored souls carry a look config; procedural NPCs derive theirs from seed.

**Gate:** a test that every `Species`/`BodyKind` round-trips, and that the same
config renders identically in editor and client.

### S77 — Decouple the Loup-Garou
- **Remove the `include_str!` from `core/soul/runtime.rs`.** Soul mutations load
  through the content index like every other content file.
- Extend `make check-purity` to scan `storylines/` and the rest of
  `mods/reachlock/`, so this can't recur.
- `CrewRoster` builds from data (an authored crew package), not `default_crew()`.
- `deck_of()` / `deck_zero_g()` take the *live* `ShipInterior`, not
  `loup_garou_interior()`.
- Starting location comes from the origin package, not `main.rs:105`.
- Ship templates become a catalog; the Loup-Garou is one entry.

**Gate:** `make check-purity` passes with the widened scan. A test that boots
with a non-Loup-Garou ship and resolves crew decks correctly.

### S78 — The creation flow *(depends on S70)*
- New `AppState::CharacterCreation`, entered from a real **New Game** on the main
  menu. Split `load_save` out of `Startup` so Continue loads and New Game
  doesn't (`main.rs:218`).
- Steps, each skippable with a "Randomize" that uses the existing seeded
  generators so *every* screen has a valid one-click answer:
  1. **Identity** — name, pronouns, species (with the in-world framing already
     written in `editor/ai.rs:91-99`: Human/Android/Robot/Voidborn/Xenotype)
  2. **Appearance** — the S76 widget, live-previewed on the walk cycle
  3. **Origin** — background cards showing exactly what each grants and costs
  4. **Ship & crew** — starting vessel and any crew the origin brings
  5. **Galaxy seed** — surface it and let players *enter* one. "The seed IS the
     game" is already in `menu.rs`'s module doc; the menu displays a seed it
     won't let you change. Shareable seeds are free multiplayer-adjacent value.
  6. **Confirm** — a summary card of the opening position
- Full keyboard + mouse + gamepad, per S70/S71. This screen is many players'
  first five minutes; it's also the most accessibility-sensitive surface in the
  game (text scale, contrast, colorblind-safe faction colors all land here).

**Gate:** create → play → save → reload → the character persists intact.

### S79 — Origins as authored content
- An `Origin` content type: starting career path, faction standing deltas,
  credits, ship template, gear, crew, known systems, and opening log entries.
- **An Origin editor** in the content suite, following the S68 pattern. This is
  what makes the game moddable at the level players care about — new origins are
  new ways to play, authored without code.
- Ship 6–10 launch origins spanning the career path types (Military, Trade,
  Exploration, Science, Political, Criminal, Freelance) plus a
  "Loup-Garou veteran" that reconstructs today's starting state exactly.

**Gate:** origin round-trips through the editor; a test that each launch origin
produces a playable opening state.

### S80 — Crew as an open-world system
- Recruit, hire, pay, injure, lose, and bury crew. Souls generated procedurally
  (`generate_soul`) or drawn from authored files.
- Open `CrewRole` beyond the fixed 5, or make roles data.
- Crew have relationships with the player's soul that persist and compress —
  the payoff for §3.1.
- Hiring surfaces at stations; crew can refuse, leave, or mutiny on breaking
  points that already exist in the data model.

**Gate:** a full loop — recruit a procedurally generated crew member, build trust
through co-deliberation, hit a breaking point, lose them.

---

## 5. Sequencing

```
S75 (identity in core) ──► S76 (one appearance pipeline) ──┐
                                                           ├──► S78 (creation flow) ──► S79 (origins) ──► S80 (crew)
S77 (decouple Loup-Garou) ─────────────────────────────────┤
                                                           │
S70 (client UI framework, from the UX plan) ───────────────┘
```

S75–S77 are independent of the UX plan and can start now. S78 is gated on S70.

**Worth noting:** character creation is the strongest argument yet for doing S70
early. It is the one screen you cannot fake with formatted text, it's the first
thing every new player touches, and building it will validate the widget kit
against a genuinely demanding surface before the rest of the game ports to it.

## 6. Two things to fix while you're in there

- **`load_save` in `Startup`** (`main.rs:218`) loads the player's save before the
  main menu renders. Any New Game path has to move it; do it as part of S78 and
  the Continue/New Game split falls out naturally.
- **`deck_of()` calling `loup_garou_interior()`** is a live bug today, not just a
  coupling problem: players who use the S18 interior editor to build a custom
  deck plan get crew deck resolution computed against a ship they aren't flying.
  Worth fixing in S77 regardless of the character work.
