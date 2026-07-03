# System Integration — how the three repos become one game

> Read this before reading code. It traces one complete interaction through
> every process in the stack, names the contracts that bind them, and tells
> you where to look when any layer misbehaves. The executable version of
> this document is the integration test harness at
> `tests/soul-protocol-harness/` — if it passes, the boundaries described
> here hold.

## The cast

Three repos, one game, plus a local model server:

| process | repo | language | role |
|---|---|---|---|
| Godot host | [REACHLOCK](https://github.com/chezgoulet/reachlock) | GDScript | the game: modes, flight, scenes, dialogue UI, mod loader |
| `pan serve` | [Pan](https://github.com/chezgoulet/pan) | Rust | the mind daemon: hosts souls, decides via rules or LLM |
| `ragamuffin` | [Ragamuffin](https://github.com/chezgoulet/ragamuffin) | Go | the knowledge server: NPC memory vaults across sessions |
| Ollama | (external) | — | local inference: chat model for Pan, embeddings for Ragamuffin |

The Go MMO server (`server/`) also lives in REACHLOCK; its simulation
packages (`server/internal/universe|economy|factions`) are built and
determinism-tested but **not yet connected to the game** — wiring them in
(SimGateway, M6) is Sprint 02 work and follows the same loopback-daemon
pattern described here.

## Architecture

```
                        REACHLOCK (the game repo)
┌─────────────────────────────────────────────────────────────────────┐
│  Godot host                                                          │
│                                                                      │
│  Ring 2  godot/mods/reachlock/     content: Tib, Sorrow Station,     │
│                                    factions, dialogue — data files   │
│  Ring 1  godot/framework/          contracts: schemas, Soul          │
│                                    Protocol, shared fixtures         │
│  Ring 0  godot/scripts/            engine: SoulGateway,              │
│                                    DialogueRunner, MemoryStore,      │
│                                    ModLoader — zero content          │
└───────┬──────────────────────────────────────────────┬──────────────┘
        │ Soul Protocol                                │ Memory Interface
        │ NDJSON over TCP loopback                     │ REST over HTTP loopback
        │ 127.0.0.1:40707                              │ 127.0.0.1:8000
        ▼                                              ▼
┌──────────────────────┐                    ┌─────────────────────────┐
│  pan serve (Rust)    │                    │  ragamuffin (Go)        │
│  session · registry  │                    │  vaults: soul-tib, lore │
│  minds:              │                    │  hybrid recall,         │
│   · rules (no model) │                    │  briefing, ingestion,   │
│   · llm ─────────┐   │                    │  pruner (decay)         │
└──────────────────┼───┘                    └───────────┬─────────────┘
                   │ chat completions                   │ embeddings
                   │ http://127.0.0.1:11434             │ /v1 (OpenAI-compat)
                   ▼                                    ▼
              ┌────────────────────────────────────────────┐
              │  Ollama — gemma4:e4b + nomic-embed-text    │
              └────────────────────────────────────────────┘
```

Everything binds **loopback only**. No process requires the others: the
game runs without Pan (souls fall back to authored dialogue), Pan runs
without Ollama (`llm` mind disabled, rules still decide), and memory writes
queue as `pending_memories` in the save when Ragamuffin is down, draining
on reconnect (verified in M3).

## One complete interaction — perceive → decide → remember → recall

You dock at Sorrow Station and walk to the bar.

**1. Perceive.** The docking sequence fires a game event. The
`SoulGateway` autoload ([godot/scripts/framework/soul_gateway.gd](../godot/scripts/framework/soul_gateway.gd))
— which at boot performed `hello`/`welcome`, registered the host's
capabilities, and instantiated Tib's soul from his npc file — assembles a
`perceive` for Tib's soul and writes one NDJSON line to `127.0.0.1:40707`:

```json
{"v":0,"seq":41,"type":"perceive","body":{"soul_id":"tib",
  "goal":{"id":"amb_00318","revision":1,"objective":"React to what just happened.",
          "trigger":{"kind":"event","topic":"ship.docked","payload":{"station":"sorrow_station"}}},
  "context":{"fragments":[
    {"channel":"persona","body":"…rendered from Tib's soul file…"},
    {"channel":"memory","body":"…recall results, most relevant first…"},
    {"channel":"history","body":"…recent transcript…"},
    {"channel":"world","body":"…location, ship state…"}]}}}
```

The `memory` fragment is where Ragamuffin already entered the picture: the
host recalled from Tib's vault *before* perceiving (step 4 below).

**2. Decide.** Pan routes the goal to the soul's mind. A rules-minded soul
matches the trigger against its authored rule list — no model, deterministic
(that was M1). An llm-minded soul like Tib becomes a chat completion against
Ollama; the reply becomes an `express` intent (M2). Either way, every
`invoke` intent is enacted through Pan's pipeline — resolve → validate →
govern → execute — and an invoke naming an unregistered capability is
rejected with `error: unknown_capability` before it touches the world. The
decision comes back correlated by `re`:

```json
{"v":0,"seq":17,"re":41,"type":"decision","body":{"soul_id":"tib",
  "goal_id":"amb_00318","goal_revision":1,"decision":{"intents":[
    {"intent":"express","body":"Back in one piece. The drift was ugly this week."},
    {"intent":"conclude","outcome":"achieved"}]}}}
```

`express` is the NPC speaking: the `DialogueRunner`
([godot/scripts/framework/dialogue_runner.gd](../godot/scripts/framework/dialogue_runner.gd))
renders it as Tib's line. `perceive` is asynchronous — the game keeps
running while the model thinks, and a stale `goal_revision` (you walked
away mid-sentence) is dropped by the host.

**3. Remember.** The encounter becomes memory through the governed path:
either the soul emits `invoke: npc.remember` (a world-effect, so it goes
through Pan's pipeline and back to the host), or a dialogue choice fires an
`add_memory` mutation. Both land in `MemoryStore`
([godot/scripts/framework/memory_store.gd](../godot/scripts/framework/memory_store.gd)),
which posts to Ragamuffin:

```
POST http://127.0.0.1:8000/v1/documents
  → vault "soul-tib": "The captain put the ship between me and the
     skiff's last volley. They meant it."  (importance, tags, game tick)
```

One vault per soul (`soul-<npc_id>`, underscores→hyphens), plus a shared
`lore` vault. A soul cannot silently rewrite itself — every memory write is
a governed world-effect.

**4. Recall.** Days later, a new session. `instantiate_soul` re-creates
Tib from his authored birth-state; the host warms context from the vault
(`GET /vault/soul-tib/v1/briefing`). When you speak to him, the host runs
hybrid recall before perceiving:

```
GET http://127.0.0.1:8000/vault/soul-tib/v1/hybrid?query=…&limit=…
```

The ranked results ride the `memory` context channel into the perceive —
and Tib mentions the volley nobody told him about in this session. That
exchange, verified live, is [docs/demos/M3-tib-remembers.md](demos/M3-tib-remembers.md).
Ragamuffin's pruner (confidence decay, supersession, staleness) is the
game mechanic "relationships fade if you're gone too long" — configuration,
not game code.

## The contracts, precisely

| boundary | contract | port / transport | spec |
|---|---|---|---|
| Godot ⇄ Pan | Soul Protocol v0 (frozen 2026-07-02) | TCP loopback, host-chosen port; default **40707** (`REACHLOCK_PAN_PORT`, `pan serve --port N`; `--port 0` = OS-assigned). NDJSON: one compact JSON envelope per `\n`-terminated line | [godot/framework/protocol/SOUL-PROTOCOL.md](../godot/framework/protocol/SOUL-PROTOCOL.md) |
| Godot ⇄ Ragamuffin | Memory Interface v0 (subset of Ragamuffin's public REST) | HTTP loopback, default **8000** (`REACHLOCK_MEMORY_URL`) | [docs/MEMORY-INTERFACE.md](MEMORY-INTERFACE.md) |
| Pan ⇄ Ollama | OpenAI-compatible chat (or Ollama native, auto-detected) | HTTP, default **11434** (`PAN_LLM_BASE`, `PAN_LLM_MODEL`; unset = llm mind disabled) | Pan `pan-daemon/src/llm.rs` |
| Ragamuffin ⇄ Ollama | OpenAI-compatible embeddings | same server, `/v1` (`RAGAMUFFIN_EMBEDDING_BASE_URL`) | Ragamuffin docs |
| game ⇄ sim (future) | Universe tick / SimGateway (Sprint 02 M6) | loopback daemon, same pattern | [docs/UNIVERSE-TICK.md](UNIVERSE-TICK.md) |

**Envelope:** `{"v":0,"seq":N,"re":M?,"type":…,"body":…}` — `seq` is
sender-local and strictly increasing; `re` correlates a response to the
request it answers. Ten message types; closed error-code set (`bad_frame`,
`unknown_type`, `version_unsupported`, `unknown_soul`,
`unknown_capability`, `invalid_args`, `provider_failure`, `superseded`).
Changing any of this bumps `protocol_version` and requires a migration
note — additive only.

**The shared fixtures** are the language-neutral truth of the wire, 15
files covering all 10 message types, kept **byte-identical** in both repos
(the harness enforces this):

- REACHLOCK: `godot/framework/protocol/fixtures/`
- Pan: `pan-daemon/tests/fixtures/`

Never edit a fixture to make a failing implementation pass — fix the
implementation, or change the contract deliberately (version bump).

## Running and proving the stack

```sh
./scripts/dev_stack.sh        # start pan + ragamuffin (expects Ollama up)
make godot                    # play
./scripts/dev_stack.sh stop
```

**The integration test harness** boots its own isolated `pan serve` on an
OS-assigned port (never the dev-stack's 40707) with a deterministic stub
LLM, and walks the entire lifecycle plus every error path:

```sh
make harness                  # = python3 tests/soul-protocol-harness/run_harness.py
```

21 steps, exit 0 on full pass. It runs in CI (both repos) as the
`soul-protocol-harness` job — **a PR that breaks it is rejected, not
reviewed**. Comparison rules and the one documented divergence (fixture
06's `npc.remember` invoke, which Pan's v0 LLM provider does not yet emit)
are spelled out in [tests/soul-protocol-harness/README.md](../tests/soul-protocol-harness/README.md).

Unit-level gates below it: `make check` (REACHLOCK: architecture guard,
mod validation, per-fixture schema conformance, DSL battery), `cargo test`
(Pan: wire round-trips every fixture through its serde types),
`go test ./internal/... -short` (Ragamuffin).

## Debugging a failure, layer by layer

**Start by localizing:** run `make harness`. If it passes, the Godot ⇄ Pan
boundary is healthy and your bug is above it (Godot) or beside it
(Ragamuffin). If it fails, the step name tells you which message broke.

- **Godot host.** `make godot-import` catches script/parse errors headless.
  Mod data: `make validate`; engine/content separation: `make architecture`.
  The gateway logs every NDJSON line it sends and receives — read the game
  log before suspecting the daemon.
- **The wire.** Per-fixture schema conformance: `make protocol` (REACHLOCK)
  and `pan check-conformance` / `cargo test` (Pan). To poke the daemon by
  hand, connect with any line-oriented TCP client and paste fixture
  `message` objects as single compact lines — the daemon answers one line
  per line. A `bad_frame` reply means your JSON didn't parse; `unknown_type`
  means the envelope was fine but the `type` isn't in the closed set.
- **Pan.** Dev-stack log: `~/.local/share/reachlock/logs/pan.log`. Lines to
  look for: `pan llm: PAN_LLM_BASE unset — llm mind disabled` (souls will
  answer `conclude: abandoned` to utterances — the "moment passes"
  fallback), `warm-up failed` (Ollama down or model missing), and
  `connection error` (host crashed mid-frame; the daemon survives and
  awaits the next connect). One connection per host process — a second
  connect cleanly drops the first.
- **Ragamuffin.** Log: `~/.local/share/reachlock/logs/ragamuffin.log`.
  Liveness: `curl http://127.0.0.1:8000/v1/briefing`. Vault contents are
  per-collection SQLite files under `~/.local/share/reachlock/db/`. If
  recall returns nothing: check the vault name (`soul-tib`, hyphens), then
  whether embeddings work at all (Ollama below). Offline is not an error —
  memories queue in the save as `pending_memories` and drain on reconnect.
- **Ollama.** `ollama list` must show the chat model (`gemma4:e4b`) and the
  embedding model (`nomic-embed-text`); `curl http://127.0.0.1:11434/v1/models`
  proves the OpenAI-compatible surface. A CPU-bound reasoning model that
  "thinks" through its token budget produces empty completions — Pan
  disables think mode on the native API, but model choice matters (see the
  M3 known-gaps list).

## Why this document exists

Sprint 01 proved each pillar in isolation-plus-one: a rules NPC decides
(M1), Tib speaks through a live model (M2), Tib remembers across sessions
(M3). Sprint 02 adds more daemons (the sim), more souls (Tove), and more
surfaces (stations, interiors, planets) — all of them joining **this**
topology, through versioned loopback contracts with shared fixtures and an
integration harness that refuses to let the boundaries drift. When you add
a system, copy the pattern, not just the code.
