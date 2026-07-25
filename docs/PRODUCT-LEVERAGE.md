# ReachLock — Where the Next Level Actually Is

**Date:** 2026-07-24 · **Companion to:** `docs/UX-AUDIT-AND-PLAN.md`, `docs/CHARACTER-CREATION-PLAN.md`
**This is an argument, not a checklist.** It says where the leverage is and why.

---

## 0. The finding that reframes the question

**You do not need more systems. Five major ones are already built, tested,
golden-pinned — and completely invisible to the player.**

I checked every core module and generator against its client-side usage:

| System | Sprint | Core refs | Client refs | Status |
|---|---|---|---|---|
| **Procedural Dilemma Generator** | S36 | 92 | **0** | dark |
| **Ecosystem Events** (extinction, invasion, mutation) | S39 | 30 | **0** | dark |
| **Scripted Encounters** (multi-scene, branching, consequences) | S41 | 16 | **0** | dark |
| **Captain's Log** (`NarratorVoice`, `detect_key_moments`, `LogEntry`) | S37 | 20+ | **0** | dark |
| **Storyline generator** | S60 *(the most recent commit)* | 8 | **0** | dark |

`generate_dilemma`, `generate_storyline`, `generate_log_entry`,
`detect_key_moments`, `EcosystemEvent`, `ScriptedEncounter` — every one of them
has zero references in `reachlock-client`. The engine contains a simulation
nobody can see.

This is a *pattern*, not five accidents. The sprint process is producing
excellent, well-tested core systems and stopping at the crate boundary. S60
landed four commits ago and is already dark. Left alone, S61–S63 will be too.

**So the highest-leverage work is not "build the next system." It is "build the
surface for the ones you have," and then change the sprint definition of done so
this stops happening.**

The single cheapest fix: add "player can reach it" to the sprint acceptance
gates. A core system with no client surface is not done; it's inventory.

---

## 1. The actual pitch is buried behind a crew console

ReachLock has one genuinely novel idea, and it isn't procedural generation or
the soul system. It's this:

> **You write the rules your ship runs on. When reality exceeds your rules, a
> person with a personality, a mood, and a history with you decides instead —
> and you live with what they chose.**

Nothing else on the market does that. The contract engine + `agency` outcome
model + co-deliberation is a real design invention, and the code is good:
`agency::weights` composes modifiers from universe tier, crew trust, contract
quality (`recent_uncovered` — write sloppy rules, get worse outcomes), and
equipment. `agency::deliberation_roll` makes it deterministic per contract/tick.
`resolve_outcome` spreads across six outcome classes. This is a *system*, with
teeth.

**How the player meets it today:**
- One line of grey 16px text: `"⟳ {crew} is considering the situation…"` (`hud.rs:291`)
- Contract authoring lives in a keyboard-driven text panel behind a crew console
  interaction, sharing a text entity with the market and the ship editor
  (`hud.rs:391-417` — they literally cannot be open at once)
- The contract *library* (S34) lets players import and share contracts by
  pasting RON into a text buffer (`contract_library.rs:51`)

That last one deserves emphasis. **Player-authored, shareable automation is a
UGC economy** — the kind of thing that keeps a game alive for a decade. It
exists. It's a paste-RON-into-a-textbox flow reachable only from a console
inside your ship.

### What "next level" looks like here

1. **Make deliberation a scene, not a status line.** This is S72 in the UX plan
   and I'd promote it to the top of the list. The player should see: what the
   crew member is weighing, which of *your* rules ran out, their mood and history
   with you, the decision, and the cost. Let the player interject — and let
   interjecting have a relationship cost, since `co_deliberation.rs` already
   models trust deltas.
2. **Promote contract authoring to a first-class mode.** It's the ship editor's
   equal, not a submenu. It's also the natural home for the editor's condition-tree
   widget (`editor/editors/widgets.rs`) — the same UI, shipped in-game.
3. **Build a real contract exchange.** Server-side (`services/library.rs` exists),
   browsable, rated, forkable, with attribution. "Someone else's ship logic,
   running on your ship, deliberated by your crew" is a strong hook.

---

## 2. The best shared-world feature is already in the architecture, unused

`services/seed.rs` implements **atomic first-write-wins discovery** — a Postgres
`UNIQUE(universe, system_id)` constraint as the arbiter, with `discoverer_id`
persisted, and `you_discovered` returned to the client. The seed resolver hashes
the discoverer into the derivation (`seed/resolver.rs:37`), so *who found it*
literally shapes what's there.

The tests are explicit: `"first discoverer wins"` / `"second discoverer loses"`.

**What reaches the player:** `network.rs:186-191` reads `you_discovered` and
shows a transient message. That's all. No naming rights, no charted-by
attribution, no discovery record, no permanence.

This is the strongest MMO hook the design has, and the hard part — atomic
contention resolution without application locks, matching the "server is a
ledger, not a simulator" architecture — **is finished**.

### What "next level" looks like here

- **Naming rights.** First to chart a system names it. That name propagates to
  every other player's galaxy map, forever. Cheap to build on a ledger that
  already stores `discoverer_id`.
- **Attribution everywhere.** "Charted by ⟨player⟩, cycle 4417." Discovery
  becomes a visible layer on the frontier (S21) rather than a boolean.
- **Discovery as a career.** `PathType::Exploration` exists in the career system
  with nothing distinctive behind it. Naming rights are the reward.

This is asynchronous multiplayer that fits an offline-first game: you never need
another player online, but the galaxy is visibly shaped by everyone who came
before. Far better fit than the presence/chat model (S23), which currently syncs
ship positions and gives two players no reason to matter to each other.

---

## 3. The world simulates, but it doesn't remember *you*

Universe ticks advance the economy and factions. Souls hold memories and
relationships. Reputation moves. But there is no artifact that says *what your
playthrough was.*

The Captain's Log (S37) is exactly that system, and it's dark. It has
`detect_key_moments`, `score_significance` (weighted by `RelationshipDelta`s),
`LogMomentType`, and `NarratorVoice` — it was designed to narrate your run in a
chosen voice, keyed on what actually mattered emotionally.

Turn it on and you get, nearly for free:
- **A shareable artifact of a playthrough.** Seeds are already shareable; a
  narrated log of what happened on *that* seed is the social object.
- **Legible consequence.** The player sees why the world changed and who changed
  with them.
- **A reason to keep playing.** Right now careers have ranks and that's the whole
  progression story. There's no long arc. The storyline generator (S60) — also
  dark — is the other half of this.

**The gap under all of this: nothing in the game gives the player a *why*.**
Factions have doctrines and goals; the economy has real supply chains;
ecosystems evolve. The player experiences all of it as numbers moving. The
narrative layer that explains the simulation is built and unwired.

---

## 4. Three concrete things I'd fix that punch above their size

**a) Six panels are bound to the same key.** `culture_view.rs:30`, `career.rs:33`,
`market.rs:99`, `discovery.rs:29`, `factions.rs:28`, and `docking.rs:146` all
toggle on `InputAction::OpenCrewRoster`. With no panel z-order or mutual
exclusion (the six independent `*Visible` booleans from the UX audit), one
keypress opens six overlapping text blobs. This is a five-minute fix that makes
the game feel like a different product.

**b) The interaction prompt is the only teaching surface and it's 12px grey.**
`hud.rs:160`. Everything the player can do is discoverable only by pressing keys
and seeing what happens.

**c) Nothing distinguishes a good decision from a bad one at the moment it's
made.** `agency::contract_quality_modifier` already punishes sloppy rules via
`recent_uncovered`. The player is never told. Surface it — "your rules left 4
gaps this cycle" — and rule-writing becomes a skill with a feedback loop instead
of a chore.

---

## 5. What I'd actually sequence

Against the existing plans, here's where I'd put this work:

**Immediately, alongside Wave A (cheap, high signal):**
- Fix the six-panels-one-key collision
- Add "reachable by the player" to the sprint acceptance gates so no further
  system ships dark

**Promoted to the front of the UX plan:**
- **S72 (Deliberation Theater)** — I originally placed this last in Wave C. It
  should be first after the UI framework. It's the pitch.

**A new wave, after S70 (client UI framework):**
- **Light up the five dark systems.** Dilemmas, ecosystem events, scripted
  encounters, storylines, captain's log. Five sprints of core work already paid
  for; each needs a surface, not a rebuild.
- **Contract authoring as a first-class mode** + a real contract exchange
- **Discovery permanence** — naming rights, attribution, discovery as career

**Deferred deliberately:**
- More procedural generators. You have more generation than you have ways to
  see it.
- Real-time multiplayer depth. Asynchronous shared-world (discovery) is a better
  fit for an offline-first game and the ledger architecture already supports it.

---

## 6. The one-sentence version

ReachLock's problem is not that it lacks systems — it's that it has a rich,
tested, deterministic simulation and almost no surface area, and the single
most novel idea in it (author your rules; a person with a history decides when
they run out) is currently one line of grey text behind a console.

Build the surface. The game is already in there.
