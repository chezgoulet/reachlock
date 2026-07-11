# M2 — "Tib speaks" ✅ (2026-07-02)

The soul's mind is now a real LLM, behind the exact same contract as M1's
rules provider. Nothing above the Provider trait changed — that is the
robots-vs-droids thesis paying rent.

## What ran

```
PAN_LLM_BASE=http://127.0.0.1:11434 PAN_LLM_MODEL=gemma4:e4b pan serve
# protocol test: instantiate tib (mind: llm), perceive the debrief utterance
```

Player: *"I wasn't going to let them touch you."*

Tib (gemma4:e4b, local, 9.0s): **"It cost them more than they thought. Keep
the shields up."**

Context assembled per the profile: persona (from his soul file), memory (his
authored seeds — Québec, the ship, the captain's silence), world (docked
after the ambush, hull damage taken shielding him). The same run verified
in-game: headless landed mode, Tib's `location.player_arrived` perceive
returned an LLM decision through SoulGateway.

## What it took (the honest engineering)

- **Reasoning models don't converse.** gemma4 burned every token "thinking"
  and returned empty content. Ollama's OpenAI-compat endpoint ignores
  `think:false`; its native `/api/chat` honors it. The provider auto-detects
  the dialect at startup.
- **Small models narrate.** Stage directions, asterisks, truncated tails —
  `clean_line` keeps only the spoken sentence(s).
- **Failure is a game state.** Inference failure → `Conclude(abandoned)` →
  the dialogue runner falls back to authored text immediately instead of
  making the player wait out the timeout. The game is playable with no
  model, a slow model, or a broken model.

## To play it

```
PAN_LLM_BASE=http://127.0.0.1:11434 PAN_LLM_MODEL=gemma4:e4b \
  ~/git/chezgoulet/pan/target/release/pan serve --port 40707 &
make godot   # fly, mine, survive Vex, dock, talk to Tib at the bar
```

The debrief's `generated` nodes are now his own words; the authored spine
(choices, trust mutations) is unchanged. ~5–11s per line on this machine's
CPU — local-inference latency is the M3-adjacent tuning problem.

## Next (M3 — "Tib remembers")

His `npc.remember` invokes and `pending_memories` currently accumulate in the
save. M3 connects Ragamuffin (per docs/MEMORY-INTERFACE.md): vault per soul,
conversation ingest, recall in the `memory` channel — quit, relaunch, and he
brings it up himself.
