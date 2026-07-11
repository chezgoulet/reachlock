# S13 — Soul System

**Spec:** §15 (all) · **Wave 4 · Depends on:** S01

## Outcome

NPCs and crew are people, not spawn points: soul files define who they are
(identity, personality, emotional state, memories, relationships, goals,
breaking points), emotional triggers run through the contract engine, and
events write memories and move relationships. Boris exists — terse,
protective, and defensive about the mark on his forearm.

## Context

- Souls are DATA (spec: "not live LLM connections"). The contract engine
  answers *how*; you build *who*. S16 builds *what they say*.
- S01 gives the content pipeline; souls are its fourth content type.
- v1 inspiration (read for character canon, not code):
  `archive/v1/godot/mods/reachlock/` soul files — the crew is Tib, Tove,
  Bardo, Doc Keene, Prudence, Risc, Boris. Prudence is a droid jump pilot
  (canon decision, 2026-07-08). NO aliens exist in this universe — humans,
  droids, robots only.

## Freeze first

Core `soul/` module mirroring spec §15's structs — with the codebase rule
applied: every `f32` in the spec (trust, intensity, emotional_weight)
becomes fixed-point `i64` (1024 = 1.0). `SoulFile`, `Identity`,
`Personality`, `EmotionalState { dominant_mood, intensity, triggers }`,
`Memory`, `Relationship`, `Goal`, `BreakingPoint`, `Secret`. Emotional
triggers reuse `contract::Condition` — do not invent a second condition
language. Schema: `content/schemas/soul.schema.json`. Wire-shape test.

## Deliverables

- [ ] `content/souls/boris.ron` authored per the spec §15 example (adapted
      to the frozen types), plus one more crew member (Tib) and one station
      NPC to prove the format generalizes.
- [ ] Soul runtime in core: load souls via the content pipeline;
      `apply_event(soul, SoulEvent) -> soul` pure transitions — emotional
      triggers evaluated through the contract engine's `condition_holds`,
      mood shifts recorded, memories appended with emotional weight,
      relationship trust/familiarity moved.
- [ ] Soul→contract bridge: a soul's active mood and relationship values are
      injected into the `EvalContext` of its contracts as fields
      (`mood.defensive`, `trust.player`, …) so authored contracts can gate
      on them — port the spec's "asked about the mark" flow as a test:
      trigger fires → mood shifts Defensive → contract rule deflects.
- [ ] Breaking points: evaluated after each event; crossing one emits a
      `SoulBreak` event (LeaveShip etc.) for the game layer to act on.
      Deliver the event, not the consequence.
- [ ] Soul mutations (spec §15): `content/storylines/` mutation entries
      (AddTrait/RemoveTrait/SetRelationship/UnlockSecret/AddGoal) applied
      through the same `apply_event` path; fired-once semantics like S11
      chapters.
- [ ] Client: S08's `CrewRoster` members link to souls by id; the crew
      member inspect panel shows public bio, visible mood, and relationship
      standing with the player. Secrets stay hidden until unlocked.
- [ ] Persistence: soul runtime state (moods, memories, relationships,
      unlocked secrets) lives in the save; authored files stay immutable.

## Acceptance gates

```
cargo test -p reachlock-core soul::   # trigger→mood, memory weight, bridge fields,
                                      # breaking points, mutation once-only
reachlock content validate content/souls/boris.ron
make check
```
Manual: inspect Boris aboard → Stable; trip a damage event → mood shifts;
the log shows the shift.

## Non-goals

Dialogue text and LLM context assembly (S16). Crew spatial-social behavior
(post-S13 brief). The other five crew souls (Phase 2 content pass). Soul
memory summarization/RAG (later; v1's ragamuffin lessons apply but bring no
dependency now).

## Gotchas

- Reuse `contract::Condition` for triggers — a second predicate DSL is the
  v1 mistake we're not repeating.
- Memory lists grow; cap with weight-based eviction (keep the formative,
  drop the forgettable) and test the cap.
- Souls are content: no soul struct may reference Bevy types or the save
  format directly — runtime state is a separate serde struct keyed by
  soul id.
