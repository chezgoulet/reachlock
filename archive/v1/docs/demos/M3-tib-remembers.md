# M3 — "Tib remembers" ✅ (2026-07-02)

The pitch moment, verified live across sessions — with **zero external
services**: Ragamuffin on its new embedded SQLite vector store, embeddings
from local Ollama (`nomic-embed-text`), the mind from local `gemma4:e4b`.
No Qdrant, no Docker, no cloud, no keys.

## The exchange

**Session A** — the debrief happens; the gratitude choice fires its
`add_memory` mutation and the game ships it to Tib's vault (`soul-tib`):

> *The captain put the ship between me and the skiff's last volley. Took
> hull damage for it. They meant it.*

**Session B** — a relaunch, days later in game time, an oblique question:

Player: *"Been thinking about that run out by the drift. You holding up?"*

Recall (hybrid search over his vault) surfaces the memory; it rides the
`memory` context channel into the perceive; and:

Tib: **"The hull held. You know that. It was a nasty volley."**

Nobody told him about the volley in that session. The vault did.

## Also verified

- The **real game** (headless, the actual playtest save) goes store-online,
  ingests Tib's authored `memory_seeds` at instantiation, and
  *"what happened to your sister on Earth"* recalls the Québec City seed.
- Vault isolation, auto-provisioning, and per-collection SQLite files —
  including a bug found and fixed live in Ragamuffin's runtime vault
  provisioning, which ignored `RAGAMUFFIN_VECTOR_STORE=embedded`.
- Offline honesty: with no store running, memories accumulate as
  `pending_memories` in the save and drain on reconnect; souls fall back to
  authored seeds. The game never requires the stack.

## Known gaps (tracked, non-blocking)

- **Conversation fact-distillation** (`/v1/ingest/conversation`) times out
  against a CPU-bound reasoning model — extraction wants a faster/non-thinking
  model or a Ragamuffin-side think-off/timeout option (additive roadmap).
  The documents path — which the game's mutations and `npc.remember` actually
  use — is unaffected.
- `/v1/documents` doesn't parse front-matter yet (Ragamuffin gap doc); the
  client filters front-matter chunks out of recall until it does.
- Emotional drift/pruner tuning (in-game-tick-aware fading) is configured but
  not yet exercised.

## Run it

```
./scripts/dev_stack.sh    # ragamuffin (embedded) + pan (llm mind)
make godot                # fly, fight, dock, talk — then quit, relaunch, ask
```
