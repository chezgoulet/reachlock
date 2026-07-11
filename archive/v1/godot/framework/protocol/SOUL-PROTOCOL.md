# The Soul Protocol — v0

The wire contract between a REACHLOCK host (the Godot engine, or the MMO
server) and a **mind daemon** (Pan). One protocol, every NPC tier: a rules-run
dockworker and an LLM-run crew member differ only in which Pan provider decides
— the wire traffic is identical. This is the robots-vs-droids distinction from
GAME-DESIGN.md §6.2, made executable.

**Vocabulary is Pan's, verbatim.** `Goal`, `Trigger`, `Context`, `Capability`,
`ActionIntent`, `Decision`, `Outcome` serialize exactly as `pan-core`'s
`schema.rs` serde derives emit them (`Trigger` tagged by `kind`, `ActionIntent`
tagged by `intent`, both snake_case). REACHLOCK adds an **envelope** and a
**profile** — never new decision types. If a need appears that the vocabulary
can't express, the fix happens in Pan first, then flows here.

Status: **frozen at v0** (2026-07-02). Changes bump `protocol_version` and
require a migration note. Conformance: every fixture in `fixtures/` validates
against [schemas/soul_message.schema.json](schemas/soul_message.schema.json)
via `scripts/check_soul_protocol.py` (runs in `make check`); Pan additionally
round-trips every fixture `body` through its serde types in its own CI.

## Transport

- **TCP loopback** (`127.0.0.1`), port chosen by the host and passed to the
  daemon (`pan serve --port N`). Loopback-only binding is REQUIRED. (Godot has
  no Unix-socket client; TCP loopback is the portable baseline. A UDS listener
  MAY be offered additionally.)
- **NDJSON framing**: one JSON object per line (`\n` terminated, UTF-8, no
  pretty-printing on the wire). A line that fails to parse elicits an `error`
  message with code `bad_frame`; the connection stays open.
- One connection per host process. Souls are multiplexed over it by `soul_id`.

## Envelope

Every line is:

```json
{"v": 0, "seq": 12, "re": 4, "type": "decision", "body": { ... }}
```

- `v` — protocol version. Mismatch at `hello` → `error` code
  `version_unsupported`, then close.
- `seq` — sender-local monotonically increasing message id.
- `re` — the `seq` this message responds to. Absent on unsolicited messages.
- `type` + `body` — one of the message types below.

Both sides MUST ignore unknown *optional* envelope keys (forward compat) and
MUST reject unknown `type` values with an `error` (code `unknown_type`).

## Message types

| type | direction | body | reply |
|---|---|---|---|
| `hello` | host → daemon | `{protocol_version, profile, client}` | `welcome` |
| `welcome` | daemon → host | `{protocol_version, server, minds}` | — |
| `register_capabilities` | host → daemon | `{capabilities: [Capability]}` | `ack` |
| `instantiate_soul` | host → daemon | `{soul_id, mind, soul}` | `ack` |
| `release_soul` | host → daemon | `{soul_id}` | `ack` |
| `perceive` | host → daemon | `{soul_id, goal: Goal, context: Context}` | `decision` |
| `decision` | daemon → host | `{soul_id, goal_id, goal_revision, decision: Decision}` | — |
| `ack` | daemon → host | `{}` | — |
| `error` | either | `{code, message}` | — |
| `shutdown` | host → daemon | `{}` | connection close |

Notes:

- `instantiate_soul.mind` ∈ `rules` \| `behavior_tree` \| `llm` — the soul
  file's `mind` field (soul schema v1). `soul` is the authored birth-state
  (the npc entity, verbatim). The daemon owns runtime mind-state from then on.
- `perceive` is **asynchronous**: the host keeps playing; the `decision`
  arrives whenever the provider finishes (LLM latency is real). Hosts MUST
  correlate by `re` and MUST tolerate out-of-order decisions across souls.
- **Supersession** (Pan's abandon-path): a new `perceive` with the same
  `goal.id` and a higher `goal.revision` supersedes the old one. The daemon
  discards in-flight work at its enact boundary; the host MUST also drop any
  `decision` whose `goal_revision` is stale. Player walks away mid-sentence →
  revision bump → no orphaned dialogue.
- `Express` bodies are the NPC *speaking or emoting* — the host's channel
  (dialogue UI, ambient bark) decides rendering. `Invoke` is the only
  world-effect and MUST name a registered capability; unregistered → the
  daemon's validate stage replies `error` code `unknown_capability` (this is
  conformance case 09).
- Error codes (closed set for v0): `bad_frame`, `unknown_type`,
  `version_unsupported`, `unknown_soul`, `unknown_capability`, `invalid_args`,
  `provider_failure`, `superseded`.

## The REACHLOCK profile (`reachlock/0`)

What the generic protocol carries for this game. Other hosts (or total-
conversion mods) may define their own profiles without touching the protocol.

**Trigger usage:**
- `utterance` — the player (or another NPC) speaks. `from` is an entity id.
- `event` — a game event. `topic` is dotted lowercase (`combat.crew_saved`,
  `ship.docked`, `cargo.sold`); `payload` matches the topic's registered shape.
- `tick` — the soul's idle heartbeat (ambient behavior, relationship drift).
- `signal` — a watched scalar crossed a threshold (`hull_integrity`, `oxygen`).

**Context channels** (assembled by the host, in this order):
- `persona` — birth-state summary rendered from the soul file.
- `memory` — recall results from the memory interface (Ragamuffin), most
  relevant first.
- `history` — recent transcript/events involving this soul.
- `world` — current location, ship state, universe facts the soul would know.

**Capability namespaces** registered by the host at connect (v0 set):
- `npc.move_to {room}` — walk somewhere.
- `npc.set_task {task}` — adopt a job (repair, cook, guard).
- `npc.adjust_relationship {toward, axis, amount}` — state-write class;
  governed.
- `npc.remember {text, importance, tags}` — state-write class; the host
  forwards to the memory interface. Memory writes are world-effects and go
  through the governed path — a soul cannot silently rewrite itself.
- `npc.leave_crew {reason}` — governed, irreversible-ish; the dramatic exits.

Mods extend this set the same way they extend everything: data + framework
hooks, never engine edits.

## Versioning & evolution

`protocol_version` is a single integer. Additive optional fields do not bump
it; anything else does. The daemon replies `welcome` with the version it will
speak; a host that can't speak it disconnects cleanly. Profile evolution
(new topics, channels, capabilities) is content-level and does not touch the
protocol version.
