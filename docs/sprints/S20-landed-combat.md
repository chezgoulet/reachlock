# S20 — Landed Combat

**Spec:** §22 (landed combat), §14 Mode 1 step 8 (dungeon-lite) ·
**Wave 5 · Depends on:** S07

## Outcome

Zelda-school top-down combat in Landed mode: lock-on, light/heavy attack,
dodge roll, block; melee and ranged weapons from the item system; one crew
companion fighting alongside you under their contract; enemies on behavior
trees; and one authored hostile location to fight through.

## Context

- Same core/client split as S19: combat math is pure core; Bevy renders.
  If S19 merged first, REUSE its `combat/` damage model and behavior-tree
  scaffolding — humanoid enemies are new states (Idle/Patrol/Chase/Attack/
  Flee), not a new framework. Coordinate in the PR if racing S19.
- S07 gives the Landed renderer, `Interactable`, NPCs, and inventory; S05
  gives weapon items (melee/ranged stats).
- Crew companions use the CONTRACT system (spec §22: tactical decisions may
  deliberate; default fallback "follow player, attack nearest").

## Freeze first

Core additions: humanoid `CombatantState` machine + `fn humanoid_step`
(pure, like S19's); melee resolution (arc + timing windows: attack
startup/active/recovery as tick counts), block/parry windows, dodge
i-frames — all integer tick math so feel is tunable by data. Enemy
archetypes as data: `content/schemas/hostile.schema.json` (stats, weapon,
behavior weights).

## Deliverables

- [ ] Player combat controller: lock-on cycle, light/heavy (timing windows),
      dodge roll (i-frames + stamina), block; ranged aim-and-fire with
      ammo from inventory. Weapon stats from equipped S05 items (a simple
      equip slot on `PlayerInventory` — coordinate with S17's ItemRef).
- [ ] Enemies: 2–3 authored archetypes (raider melee, raider gunner,
      security bot) spawned in hostile rooms; behavior trees chase/flank/
      flee-at-low-HP; telegraphed attacks (windup flash) so dodging is a
      read, not a guess.
- [ ] Companion: one crew member accompanies (chosen at the airlock),
      driven by an authored combat contract: rules for engage-range and
      retreat-at-HP; LLM edge allowed for "unexpected" (rare) with visible
      deliberation; fallback = spec default. Companion death is DOWNED
      (revive on victory), not permadeath (Phase 2 decides).
- [ ] Environmental bits: explosive barrel, breakable crate (loot: credits/
      consumables), locked door + keycard — enough verbs for one dungeon
      rhythm.
- [ ] One authored hostile location: `content/locations/derelict_hold.ron` —
      a raider-held derelict interior reachable from a system POI, 5–8 rooms,
      a mini-boss (beefed archetype with a second phase), a reward cache.
      Validated by the S01 pipeline.
- [ ] Health/damage UI: player HP, lock-on marker, companion status chip,
      damage numbers optional-but-cheap.

## Acceptance gates

```
cargo test -p reachlock-core combat::humanoid   # timing windows, i-frames,
                                                # parry math, no-trap states
reachlock content validate content/locations/derelict_hold.ron
make check
```
Manual: clear the derelict with Tib as companion — dodge through a
telegraph, parry once, watch Tib retreat at low HP per her contract, beat
the mini-boss, loot the cache.

## Non-goals

Stealth/takedowns (later brief). Full Predecessor dungeons with puzzles and
tools (Phase 2 content pass — you deliver the combat verbs they'll use).
On-board boarding combat (own brief). Interrogation (S16-adjacent, later).

## Gotchas

- Combat ticks are fixed-rate integer logic; render interpolates. Frame-rate
  dependent i-frames are the classic bug — test at simulated 30 and 144 fps.
- The companion's deliberation must never freeze the fight: contract
  evaluation is instant; only the LLM edge defers, and its fallback fires
  on timeout mid-combat like anywhere else.
- Keep archetype tuning in content files — balancing later must not need a
  recompile.
