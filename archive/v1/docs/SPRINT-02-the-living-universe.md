# Sprint 02 — The Living Universe

> **Handoff brief.** Written for agents who did not build Sprint 01. Assume
> you start cold: read this file, then the "Ground truth" list, before any
> code. Sprint 01's brief (docs/SPRINT-01-the-living-crew.md) established the
> working method — contracts first, then fan out, re-serialize at milestones —
> and it held. Keep it.

## Where Sprint 01 left the game

M1 (a rules NPC decides), M2 (Tib speaks — local LLM), M3 (Tib remembers —
cross-session vault recall) are **done, verified live, and merged to every
repo's `testing` branch** (the actively-worked branch; push there). Flight
feel is human-validated. Demo notes in `docs/demos/` are the ground truth for
what actually ran, including known gaps.

The soul pillar works. Two debts now dominate:

1. **Validation debt** — ~3k lines of fleet-written internals (pan-daemon
   session/wire, ragamuffin embeddedstore, reachlock server sims) have never
   been deeply reviewed; the GDScript trigger-DSL evaluator has never been run
   against the Python battery that defines the semantics; only flight has
   been felt by a human.
2. **The world pillar is absent in-game.** Faction/economy/tick simulators
   exist in Go with determinism tests — connected to nothing. In-game prices
   are static `base_price`, `universe.tick` is a bare counter, no faction has
   ever done anything a player can see. The design doc's thesis ("a universe
   that doesn't wait for you") has zero gameplay presence.

## Mission

Make the universe move, and make the foundation trustworthy: **embed the
simulation in single-player, surface it in play, and pay down the validation
debt** — while dialogue grows from "works" to "feels good."

### North-star acceptance test

> *Dock at Sorrow Station. Ore prices differ from yesterday because the
> simulation moved while you flew. The bar's news feed mentions a Compact
> patrol pushing into a neighboring system. Tib has an opinion about that —
> in his own words, informed by his memories — with no dead-air pauses that
> break the scene. Quit. The universe was saved mid-motion, and resumes.*

## Ground truth — read before writing code

1. `docs/SPRINT-01-the-living-crew.md` — method + operating principles §1-8
   (all still binding: contracts frozen, wire-contracts-not-code, Ragamuffin
   additive-only, authored-vs-runtime state, strict CI / lenient runtime,
   green gates at every milestone).
2. `docs/demos/M1..M3*.md` — what verifiably works and the honest gap lists.
3. `godot/framework/README.md` + `godot/framework/protocol/SOUL-PROTOCOL.md`
   + `docs/UNIVERSE-TICK.md` + `docs/MEMORY-INTERFACE.md` — the frozen
   contracts. Changing one = version bump + migration note, additive only.
4. `server/internal/` — the sims you'll be embedding (note
   `universe/http.go` — inspect what surface already exists before designing
   a new one).
5. `scripts/dev_stack.sh` — how the local stack runs (pan + ragamuffin
   embedded store + Ollama). `ragamuffin` work happens on its `testing`
   via `dev/*` branches — it is a production service (hermes-agent).

## PHASE A — contracts & decisions (serial, blocking)

### C9 — Save ↔ vault isolation (a real design decision, currently undecided)
A soul's vault outlives the save today: delete `slot0.json`, Tib still
remembers your last life. Decide it as a contract: vault names gain a
playthrough component (e.g. `soul-<playthrough_id>-<npc_id>`), the save
schema's `souls.<id>.vault` field becomes authoritative, and "new game"
means fresh vaults. Spell out what "delete a save" means for vaults.
**DoD:** MEMORY-INTERFACE.md + save schema updated (additive), MemoryStore
implements it, a fresh playthrough provably starts memory-clean.

### C10 — The simulation surface (SP embed decision)
How the Go sim runs in single-player. Recommended: a **sidecar daemon**
(`reachlock-sim serve`) exactly like Pan — loopback HTTP/NDJSON, launched by
dev_stack.sh — because it reuses the server code unmodified and keeps the
SP/MMO parity promise of UNIVERSE-TICK.md. Contract: tick advance requests
(including batch advance for time-skips), state queries (prices at location,
faction stances, event feed since tick N), and player-action inputs (trades).
Godot side: a `SimGateway` framework autoload mirroring SoulGateway's
offline-tolerant pattern — **the game must stay playable with no sim
daemon** (static fallback prices).
**DoD:** contract doc + schemas + fixtures like the Soul Protocol got;
conformance in both repos' CI... same treatment, no less.

### C11 — Express streaming (decide, don't build yet)
Dialogue latency masking may eventually want token streaming, which would be
a Soul Protocol change (`express_partial` messages → protocol v1). Decide
whether M7's latency work needs it or whether theatrical masking suffices
for now. Write the decision down in the protocol doc either way.

## PHASE B — workstreams (parallel after Phase A)

### Workstream V — pay the validation debt (start immediately; needs no contracts)
- **V1 — DSL conformance bridge.** Run the 27-case battery from
  `scripts/trigger_dsl.py` against `godot/scripts/framework/trigger_dsl.gd`
  in CI (headless Godot script that loads the cases — export them to JSON
  from the Python side so there is ONE case list). Divergence = CI failure.
  This is the cheapest highest-certainty fix in the sprint.
- **V2 — deep review of fleet internals.** `/code-review`-grade passes over:
  pan-daemon `session.rs`/`wire.rs` (protocol edge cases, supersession),
  ragamuffin `embeddedstore` (SQL injection surface, concurrency, the
  unimplemented SetPayload/UpdateVectors paths), reachlock `server/internal/*`
  (determinism claims vs implementation). File findings as issues; fix the
  correctness ones in-sprint.
- **V3 — vault hygiene.** Delete the `soul-smoketest` vault and the planted
  `memories/tick_54_debrief.md` from `soul-tib` (test fixtures contaminating
  real play state). Then make contamination structurally impossible: tests
  use a dedicated store path, never `~/.local/share/reachlock`.
- **V4 — dev-stack security defaults.** Ragamuffin currently binds 0.0.0.0
  with auth disabled. Bind loopback in dev_stack.sh (config exists — verify;
  if not, that's an additive ragamuffin flag) and set auth keys.

### Workstream U — the universe, in-game (after C10)
- **U1 — sim sidecar** (`reachlock-sim`): wrap the existing loader + sims
  behind the C10 surface. Deterministic; state snapshots to/from the save's
  `universe` block.
- **U2 — SimGateway + live market.** Landed market buys/sells at sim prices;
  player trades flow back as inputs; prices visibly differ across visits.
- **U3 — time passes.** Docked time and flight time advance the sim (batch
  advance on undock per UNIVERSE-TICK.md). Save/load snapshots mid-motion.
- **U4 — the feed.** A station news panel rendering the sim's event feed
  (faction stance changes, skirmishes). Feed items also broadcast as soul
  perceive events (`news.compact_advance`) so crew can react — this is where
  the world pillar and soul pillar touch for the first time.

### Workstream D — dialogue feel (after C11 decision)
- **D1 — per-soul worker threads in pan-daemon.** One slow mind must not
  queue the crew. Keep supersession semantics exact (tests).
- **D2 — latency theater.** In-scene "Tib considers…" beats, dialogue UI
  polish, and the `history` context channel (currently unwired) carrying the
  running transcript so multi-turn conversations cohere.
- **D3 — organic memory formation.** Fix conversation fact-distillation:
  additive ragamuffin support for a think-disabled/faster extraction model
  and a configurable LLM timeout; wire dev_stack accordingly. The M3 magic
  must come from play, not authored mutations.
- **D4 — sustained-play quality pass.** A human plays 20 minutes of
  conversation; capture line-quality/latency findings as tuning issues
  (prompt budget, max_tokens, model choice).

### Workstream S2 — the second crewmate (after C9; exercises D1)
- **S2a — Tove**: soul v1 file (mind: llm), memory seeds, relationships to
  tib/player, one dialogue graph with generated beats.
- **S2b — On Board first pass**: walkable ship interior scene (rooms from
  the hull's `interior_rooms`), both crew present, talk aboard. Two live
  souls at once is the real test of D1 and of context assembly discipline.

## PHASE C — milestones (integration barriers, demo note each)

- **M5 — "Trust the foundation."** V1–V4 done; findings triaged; a fresh
  playthrough starts memory-clean. *(Workstream V complete.)*
- **M6 — "The universe moves."** North-star sentence, first half: prices
  changed while you flew; the feed reports a faction event; save/resume
  mid-motion. *(U1–U4.)*
- **M7 — "It feels like talking to someone."** No dead-air breaks; coherent
  multi-turn; a memory formed from unscripted conversation recalled next
  session. *(D1–D4.)*
- **M8 — "Two souls aboard."** Talk to Tib and Tove aboard the Loup-Garou;
  they perceive the same feed event; a human plays the whole north-star
  sentence end-to-end and it holds. *(S2 + everything.)*

## Out of scope (unchanged pull, keep resisting)

MMO netcode, subscriptions/credits, ship editor, Zelda dungeons, hull
catalogue. Also: promoting ragamuffin `testing` → `main` is a **human
release decision** (production deployment) — prepare nothing beyond green CI.

## Standing verification gates

Every milestone: `make check` (reachlock), `cargo test` (pan workspace),
`go build ./... && go test ./internal/... -short` (ragamuffin), headless
Godot import + boot clean, architecture guard green, and — new this sprint —
V1's DSL conformance bridge. A milestone without its demo note in
`docs/demos/` didn't happen.
