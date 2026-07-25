# S16 — Dialogue & Deliberation UX

**Spec:** §15 (soul→dialogue), §6 (deliberation UX), §14 Mode 1 step 6 ·
**Wave 4 · Depends on:** S13, S14

## Outcome

Talking to an NPC feels like talking to someone: authored dialogue plays
instantly, and when the conversation leaves the script, the soul's context
assembles into an LLM call — the NPC visibly considers (deliberation state
in the dialogue panel), answers in their voice, and the exchange writes
memories and moves the relationship. One dialogue surface serves crew and
station NPCs, online and offline.

## Context

- S07 built the dialogue panel with authored lines; S13 built souls and the
  event pipeline; S14 gives real inference online. Offline with no local
  model: unscripted branches fall back to authored deflections — never a
  hang, never lorem ipsum.
- v1's hard-won latency rule (M10): authored beat renders INSTANTLY, the
  generated line arrives as a follow-up beat; moving on supersedes the
  in-flight call. Port the principle, not the code.

## Freeze first

`DialogueContext` in core: the exact, size-bounded context assembled for an
NPC inference call — public identity, personality summary, current mood +
intensity, top-K memories by emotional weight, relationship-with-speaker,
active goals, the last N exchange turns, and the player's utterance/choice.
One function: `assemble(soul_state, history, input) -> DialogueContext`,
unit-tested for bounds (context never exceeds a fixed budget) and for
secret-safety (unrevealed `Secret` content NEVER enters the context).

## Deliverables

- [ ] Dialogue graph upgrade: authored nodes support choices, conditions
      (reuse `contract::Condition` over soul/reputation fields), and
      mutations (soul events, reputation events) — schema extension +
      validation. Author one real conversation for Boris using it (the
      mark-on-forearm deflection arc from spec §15).
- [ ] The unscripted edge: a "say something else" free-input choice (and
      authored `llm_edge: true` nodes) route through `assemble` →
      deliberation state IN the panel ("Boris is considering…" with the
      mood visible) → response beat in-character → soul events applied
      (memory written, trust moved).
- [ ] Voice shaping: the system prompt template renders personality
      (speaking_style, quirks, values) and CURRENT mood into instructions;
      responses are post-processed (strip meta, length cap). Terse Boris
      must read terse — test the template output, not the model.
- [ ] Supersession: leaving the dialogue or picking another choice cancels
      the in-flight call cleanly (client-side abandon + log "…loses the
      thread" beat — v1's pattern).
- [ ] Offline behavior: no provider → the edge choice yields an authored
      per-soul deflection line (from the soul file) + a log note. Classic
      universe: identical.
- [ ] Crew comms during flight: contract deliberation results render as crew
      speech bubbles/comm lines using the same voice pipeline (the S02/S15
      reasoning text, spoken in-character).

## Acceptance gates

```
cargo test -p reachlock-core dialogue::   # assemble bounds, secret-safety,
                                          # condition gating, template rendering
make check
```
Manual: talk to Boris → authored deflection; insist via free input (online,
stub or real model) → deliberation in-panel → in-voice reply → trust dipped
in the inspect panel; walk away mid-think → clean supersession.

## Non-goals

Voice/STT input (v1's Ear — own brief much later). Woven-dialogue world
effects with `may` grants (v1 M13 concept — later brief once this is
stable). Interrogation (S20). RAG/memory-vault retrieval (top-K by weight
is enough now).

## Gotchas

- Secret-safety is the test that matters: the LLM must not be handed what
  the player hasn't earned — enforce at `assemble`, not in the prompt.
- The panel is the one dialogue surface (S07's) — extend it; a second panel
  is a merge conflict and a UX bug.
- Free input goes to the server as-is: cap length client-side and
  sanitize logging server-side (S14 rule).
