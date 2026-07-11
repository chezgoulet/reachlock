# Sprint 01 — The Living Crew

> **Handoff brief for a long-horizon Fable agent.** Self-contained: assume you
> start cold. Read this whole file, then the three files under "Ground truth"
> before writing any code. Everything here is executable design — freeze the
> contracts, then build broadly against them.

## Mission

Turn a flyable ship into a **crew member who decides, remembers, and reacts.**
Bind three repos — **Pan** (the mind), **Ragamuffin** (the memory), and
**REACHLOCK** (the game) — through versioned wire contracts, and prove the loop
end-to-end with Tib.

### North-star vertical slice (the acceptance test for the whole sprint)

> *Fly the Loup-Garou to an asteroid, mine it, get jumped by a pirate, fight,
> dock at Sorrow Station, and Tib — who watched it happen — comments on the
> fight, his trust shifts based on whether you took the hits for him, and he
> **remembers it next session**.*

If a stranger can do that and feel like Tib is a person, the sprint succeeded.
Everything below exists to make that one sentence true and honest.

## Why this is the right sprint

This is the game's unique bet. Flight is a genre we compete in; a persistent,
model-agnostic soul that remembers you is the thing nobody else has shipped
well. It is also the highest-risk unknown (latency, small-model quality, memory
persistence), so we retire that risk now, on the least forgiving host there is:
a real-time game.

The fit is already in the types. Pan's settled v1.0 vocabulary
(`Goal`/`Trigger`/`Context`/`Capability`/`ActionIntent`) names game events as a
first-class trigger and defines `Capability` as the union of LLM tools,
behavior-tree nodes, and rules actions — exactly REACHLOCK's moddable NPC action
surface. Its three-provider leak test is the **robots-vs-droids lore made
executable**: a dockworker runs the rules provider, a crew member runs the LLM
provider, same contract, same soul file. Ragamuffin's multi-vault + conversation
ingest + briefing + pruner (supersede/conflict/stale/confidence-decay) is the
design doc's "relationships decay if you're gone too long" as an already-tested
production subsystem.

---

## Ground truth — read these first

- `reachlock/docs/ARCHITECTURE.md` — the three-ring boundary and what the guard enforces.
- `reachlock/godot/framework/README.md` — the framework contracts already live and how they're authored.
- `pan/pan-core/src/schema.rs` — the settled Pan vocabulary you will adopt as the wire vocabulary. **Do not invent a parallel one.**
- Skim `ragamuffin/SPEC.md` and `ragamuffin/internal/server/*.go` for the real endpoint surface (richer than the spec: multi-vault, `/v1/ingest/conversation`, `/v1/briefing`, hybrid search, pruner).

## Operating principles (guardrails — violating these is a defect)

1. **Contracts first, and freeze them.** Phase A is serial and blocking. A
   contract is "frozen" when it has a schema, a doc, a conformance test, and a
   version. Do not build against an unfrozen contract.
2. **The game binds to wire contracts, never to code.** REACHLOCK never imports
   a Pan crate or reaches into Ragamuffin internals. Local socket + JSON to Pan;
   REST/JSON to Ragamuffin. This is what lets three repos evolve in tandem.
3. **Adopt Pan's vocabulary; add a *profile*, not new types.** REACHLOCK
   contributes the trigger set, capability namespaces (`npc.move`, `npc.speak`,
   `npc.repair`…), and context channels — not a new decision type.
4. **Ragamuffin is additive-only. It has production users** (hermes-agent /
   Nous Research). Consume its public surface as-is. Anything the game needs
   becomes a general roadmap feature with a contract test *in Ragamuffin's CI*,
   never a game-specific fork or breaking change.
5. **Authored vs. runtime state.** Mod files are birth-state templates. Runtime
   soul state lives in saves (SP) / server (MMO). Never persist runtime state
   into mod files.
6. **Strict in CI, lenient at runtime.** Schema violations fail the build; the
   in-engine loaders log and continue so a broken mod is a report, not a crash.
7. **Keep it green, keep it playable.** `make check` in each repo, headless
   Godot import + boot, and the architecture guard must pass at every milestone.
   Extend the guard's scope as new engine dirs appear (`framework/`, then
   `server/` per the known boundary debt).
8. **Every task ships with a Definition of Done and a verify command.** If you
   can't verify it, it isn't done.

## How to run this with a fleet (optimize the long horizon)

The plan is shaped for **serialize → fan out → re-serialize**:

- **Phase A (contracts)** — do yourself, serially. These are the priceless,
  irreversible decisions. Get them reviewed/frozen before fanning out.
- **Phase B (build)** — five workstreams (Pan / Ragamuffin / Godot / Server /
  Content) that are independent *once the contracts are frozen*. Fan out
  subagents, one per workstream, each pointed at this file plus its workstream
  section. Give each a branch (`sprint01/pan`, `sprint01/raga`, …).
- **Phase C (integration)** — re-serialize at each milestone (M1→M4); a
  milestone is a synchronization barrier where workstreams meet against a
  running demo.

Only fan out work that is genuinely contract-separated. When a subagent reports
back, relay what changed against the contract, not the file diff.

---

# PHASE A — CONTRACTS (serial, blocking, priceless)

Each lands as: a schema/spec file in `reachlock/godot/framework/` (or
`framework/protocol/`), a section in the framework README, a conformance test,
and a `framework_version` note. Mirror each as an issue in every repo it binds.

### C1 — Soul Protocol (the wire contract)
- **Purpose:** the message envelope Pan and the game exchange. Perceive→decide
  loop. Adopts `Goal`/`Trigger`/`Context`/`Capability`/`ActionIntent` verbatim
  as the JSON payload vocabulary.
- **Defines:** transport (local socket, newline-delimited JSON — pick and
  document, e.g. UDS on Linux, TCP loopback fallback); message framing;
  handshake with `protocol_version`; the REACHLOCK **profile** (which
  `Trigger` kinds fire, which capability namespaces exist, which `Context`
  channels: `persona`, `memory`, `history`, `world`); error/abandon semantics
  (reuse Pan's supersession).
- **DoD:** spec doc + JSON Schemas for each message; a language-neutral
  conformance suite (golden request/response fixtures) that both Pan and the
  Godot bridge run. **Irreversible because** every NPC interaction and both
  providers are authored against it.

### C2 — Soul file v1 (schema)
- Extends the existing `npc.schema.json` (v0 = birth-state identity). Adds:
  `memory_seeds` (authored starting memories → ingested into the vault on
  instantiation), `emotional_baseline`, `relationship_graph` (initial edges),
  and **`mind`: `rules` | `behavior_tree` | `llm`** (the robot/droid tier —
  a data field, moddable). Migration note v0→v1.
- **DoD:** schema + validator support; `tib.json` upgraded; guard/validator green.

### C3 — Memory interface (Ragamuffin binding)
- The subset of Ragamuffin's REST surface REACHLOCK depends on: per-soul vault
  namespacing, `recall`/hybrid search, `/v1/ingest/conversation`,
  `/v1/briefing` for prompt assembly. Document request/response shapes the game
  relies on so Ragamuffin knows what it must not break.
- **DoD:** interface doc + a contract-test fixture set that becomes R4 in
  Ragamuffin's CI.

### C4 — Save format (schema)
- Snapshot of runtime state: soul runtime (vault ref + Pan state handle), player,
  ship damage/cargo, faction standings, `universe_tick`. SP serialization =
  SQLite; define the logical schema so SP and MMO persistence are one design.
- **DoD:** schema + a round-trip test (serialize → load → deep-equal).

### C5 — Event trigger condition language (the storyline authoring API)
- The §8 DSL: `if faction.compact.trust < -50 and player.location == "verne"
  then trigger "bounty_hunters"`. Define grammar, the variable namespace
  (`faction.*`, `player.*`, `soul.*`, `universe.*`), evaluation semantics, and
  safety (no side effects in conditions).
- **DoD:** grammar doc + a reference evaluator with a test battery. **Irreversible
  because** every storyline card and dialogue branch is written in it.

### C6 — Dialogue graph (schema)
- Authored dialogue trees: nodes, branching conditions (uses C5), capability
  invocations, and soul-mutation commands. Must interleave authored lines with
  LLM-generated lines from Pan.
- **DoD:** schema + validator + one authored example (Tib) that parses.

### C7 — Location (schema)
- Stations/planets: docking services, biome, NPC spawn table, economy links.
  Unblocks docking and the landed transition.
- **DoD:** schema + `sorrow_station.json` + validator green.

### C8 — Universe tick (contract)
- Tick granularity, event-queue semantics, deterministic ordering. **Identical
  for SP-local and MMO-server** — this is the property that lets the same
  simulation run both places.
- **DoD:** contract doc + a determinism test (same seed + inputs → same tick
  outputs) usable by both the Go server and any SP embed.

---

# PHASE B — BUILD (parallel once Phase A is frozen)

## Workstream P — Pan (the mind) · branch `sprint01/pan`
- **P1 — Sidecar/daemon mode.** Pan runs as a local process speaking C1 over
  the socket. *DoD:* `pan serve` accepts a connection, completes a
  perceive→decide→respond round trip; **verify:** conformance fixtures pass.
- **P2 — Rules provider end-to-end (no LLM).** A decision flows through the full
  `validate`/`govern`/`enact` pipeline via the daemon. *Verify:* CI test drives
  a `Trigger::Event` and asserts an `ActionIntent::Invoke`.
- **P3 — LLM provider, model-agnostic.** BYOK (Anthropic default — consult the
  `claude-api` skill for current model ids) **and** local (llama.cpp). *Verify:*
  same fixture yields a valid `Express`/`Invoke` under both.
- **P4 — Capability registry from mods.** Capabilities the game registers (via
  manifest) become the `Capability` set Pan chooses among. *Verify:* an
  unregistered capability is rejected at `validate`.
- **P5 — Conformance green.** Pan passes the C1 suite in its own CI.

## Workstream R — Ragamuffin (the memory) · branch `sprint01/raga` · ADDITIVE ONLY
- **R1 — Local embedding provider.** OpenAI-compatible/llama.cpp endpoint so SP
  runs offline. *Verify:* `/recall` returns results with no cloud key set.
- **R2 — Embedded vector store option** (e.g. sqlite-vec) so small/offline
  deployments need no Qdrant container. Config-selected; Qdrant stays default.
  *Verify:* smoke test passes against the embedded backend.
- **R3 — Soul-vault conventions doc.** Per-NPC vault namespacing + memory
  record shape, built on existing multi-vault/ingest/briefing. No core changes.
- **R4 — Contract test in Ragamuffin CI** proving the C3 subset holds, so a
  future change that would break REACHLOCK fails in Ragamuffin's own pipeline.
- **Guardrail:** every change is a general feature. If it only helps the game,
  it's in the wrong repo.

## Workstream G — Godot engine (the host) · branch `sprint01/godot`
- **G1 — Pan bridge.** Async, non-blocking Godot client for the C1 socket, with
  reconnect. Lives in `godot/scripts/framework/`. *Verify:* headless boot opens
  the socket, round-trips one decision, logs it.
- **G2 — NPC representation.** An in-engine soul instance driven by DataRegistry
  soul files + Pan decisions. Generic — no content ids in engine code.
- **G3 — Dialogue UI.** Dialogue window rendering authored trees (C6) +
  LLM lines from Pan; choices feed conditions (C5).
- **G4 — Save/load** implementing C4. *Verify:* fly, mutate state, save, quit,
  reload, state restored.
- **G5 — Docking + mode transition.** Real space→landed flow at a station
  (C7), not just a mode swap. *Verify:* approach → dock → landed at Sorrow.
- **G6 — Mining loop.** Target asteroid → extract → cargo. *Verify:* cargo count
  rises; ties to an economy good.
- **G7 — Space combat slice.** Hardpoint weapons fire, one pirate with basic AI,
  subsystem/health damage, the "get jumped" beat. *Verify:* pirate can be
  destroyed and can damage you.

## Workstream S — Go server (the sim) · branch `sprint01/server`
- **S1 — Faction simulator tick** replacing the stub; loads faction data through
  a loader (resolves the ARCHITECTURE.md boundary debt — expand the guard to
  `server/`). *Verify:* a tick advances standings deterministically.
- **S2 — Economy engine.** Supply/demand pricing replacing the stub. *Verify:*
  flooding a good drops its price.
- **S3 — Universe tick** implementing C8, same design usable by an SP embed.

## Workstream Content · branch `sprint01/content`
- **CT1 — Tib soul v1:** memory seeds (Québec, the crew), emotional baseline,
  `mind: llm`, arc hooks.
- **CT2 — Sorrow Station** location (C7).
- **CT3 — Tib dialogue tree** (C6) + soul mutations (trust shifts on
  combat-save / on running from the fight).
- **CT4 — One pirate NPC** (`mind: rules`) + an enemy ship hull.
- **CT5 — Asteroid resource + economy goods** for the mining loop.
- *Verify:* `make validate` green; all files conform; guard clean.

---

# PHASE C — INTEGRATION MILESTONES (re-serialize here)

Each milestone is a barrier: stop, wire the workstreams together, run the demo,
fix the seams, then proceed. Commit a short demo note per milestone.

- **M1 — "A rules NPC decides."** (P1,P2,G1,G2,P4) An NPC perceives a game event
  and acts via the rules provider, no LLM. Proves the bridge + capability path.
- **M2 — "Tib speaks."** (P3,G3,CT1,CT3,C5,C6) Talk to Tib; authored tree +
  LLM lines; a choice evaluates a condition and fires a mutation.
- **M3 — "Tib remembers."** (R1,R2,R3,R4,C3,C4,G4) Conversation ingested to his
  vault; briefing-assembled context; **quit, relaunch, he references it.** This
  is the pitch.
- **M4 — "The slice."** The full north-star sentence, start to finish, playable
  by a stranger. (mining G6 + combat G7 + docking G5 + M3.)

## Definition of done for the sprint

- The north-star slice is playable end-to-end by someone who didn't build it.
- All eight contracts frozen, versioned, documented, conformance-tested.
- `make check` green in reachlock and server; Pan and Ragamuffin CI green
  including the new conformance/contract tests; headless Godot import + boot
  clean; architecture guard green with expanded scope.
- Ragamuffin changed additively only; hermes-agent's usage is unaffected.
- Soul runtime state persists across a restart via the save format.

## Dependency map (what blocks what)

```
C1 ─┬─ P1 ─ P2 ─ M1
    ├─ P3 ──────── M2
    └─ G1 ─ G2 ─ M1
C2 ── CT1 ─ M2
C3 ─── R3 ─ R4 ─ M3
C4 ── G4 ─────── M3
C5 ─┬ C6 ─ G3 ─ M2
C7 ── CT2 ─ G5 ─ M4
C8 ── S1/S2/S3 (parallel; lands in M4 world state)
R1/R2 ───────── M3
G6/G7 + CT4/CT5 ─ M4
```

## Explicitly OUT of scope (resist the pull)

MMO server networking, subscriptions/inference credits, the ship editor, the
Zelda dungeon, the full hull catalogue, multiplayer. Keep the architectural
seams (universe state behind an interface, souls behind the gateway) but spend
zero build time here. A groomed backlog for these creates gravity; don't feed it.
```
