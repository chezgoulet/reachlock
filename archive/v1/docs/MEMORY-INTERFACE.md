# The Memory Interface — Ragamuffin binding (v0)

The subset of [Ragamuffin](https://github.com/chezgoulet/ragamuffin)'s public
REST surface that REACHLOCK depends on. Ragamuffin is a **production service
with users outside this game** (hermes-agent); therefore this document is a
one-way promise: REACHLOCK binds to exactly what is listed here, and a
contract test in *Ragamuffin's* CI (R4) proves this subset keeps working.
Everything the game needs beyond it becomes a general Ragamuffin feature,
never a game-specific fork.

## Placement

- **MMO:** the server talks to a Ragamuffin deployment. Available today, as-is.
- **Single-player:** identical binding, pointed at a local Ragamuffin. Two
  additive Ragamuffin features close the offline gap (tracked on its roadmap):
  a **local embedding provider** (OpenAI-compatible endpoint, e.g. llama.cpp)
  and an **embedded vector store** option (no Qdrant container). Until they
  land, SP soul memory develops against a dev deployment; the binding is the
  same either way — that's the point of the contract.

## Vault conventions (the REACHLOCK profile)

- One vault per soul: `soul-<npc_id>` with underscores mapped to hyphens
  (`soul-tib`, `soul-doc-keene`) — Ragamuffin's `ValidVaultName` accepts
  `[a-z0-9-:]` only. Instantiating a soul ingests its `memory_seeds`
  (soul schema v1) as first-person documents.
- One shared `lore` vault: world knowledge any literate NPC could know.
  Context assembly recalls from the soul's own vault first, then `lore`.
- Memory records are markdown with front-matter: `importance` (0..1), `tags`,
  and the in-game `tick` — game time, not only wall time, so recall and prune
  can reason about the game's calendar.

## Endpoints REACHLOCK binds to

### Write a memory — `POST /v1/documents`
The governed landing point of `npc.remember` (Soul Protocol) and dialogue
`add_memory` mutations. Body carries the vault, a path/title, and markdown
content; response confirms indexing.

### Recall — `GET /vault/{name}/v1/hybrid?query=...&limit=...`
Hybrid (semantic + keyword) search over one vault. Results become `memory`
channel fragments in the perceive context, most relevant first, trimmed to the
context budget. Each result carries source path and score.

### Ingest a conversation — `POST /v1/ingest/conversation`
After a dialogue closes, the host posts the transcript:

```json
{"vault": "soul_tib", "messages": [{"role": "user", "content": "..."},
 {"role": "assistant", "content": "..."}], "context": {"tick": 10450,
 "location": "sorrow_station"}}
```

Ragamuffin's fact extraction distills it into memories with confidence and
category (`preference` / `knowledge` / `relationship`) — the soul remembers
the *facts* of the exchange, not the transcript verbatim. Response:
`{status, conversation_id, fact_count, facts}`.

### Orientation — `GET /vault/{name}/v1/briefing?agent_id=...`
Structured summary of a vault's state. Used at soul instantiation for cheap
"what do I know" assembly and by tooling/debug ("what does Tib believe?").

## The pruner is a game mechanic

Ragamuffin's pruner (confidence decay, supersession, staleness, conflict
review) is the implementation of GAME-DESIGN.md's "relationships decay if
you're gone too long." Low-importance memories fade; contradicted beliefs get
superseded; a soul that hasn't seen the player in two in-game years genuinely
half-remembers them. No game-side code required — configuration only.

## What the R4 contract test must prove (in Ragamuffin CI)

1. `POST /v1/documents` into a named vault → indexed and recallable.
2. `GET /vault/{name}/v1/hybrid` returns ranked results with source + score.
3. `POST /v1/ingest/conversation` accepts the shape above and yields facts.
4. `GET /vault/{name}/v1/briefing` returns the vault summary shape.
5. Vault isolation: a query against `soul_a` never returns `soul_b` content.
