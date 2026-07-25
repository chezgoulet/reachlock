# S19 — Space Combat

**Spec:** §22 (space combat), §14 Mode 3 step 3 · **Wave 5 ·
Depends on:** S05, S09

## Outcome

Dogfights: enemies with behavior trees (Patrol/Engage/Evade/Retreat/
RequestReinforcements), weapons firing from hardpoints, power management
across weapons/shields/engines, subsystem targeting, and escape options —
deterministic, fair, and LLM-free. Enemy difficulty scales with system
threat level.

## Context

- Spec is explicit: enemy AI is behavior trees in `reachlock-core` — NO LLM
  (enemies aren't crew). Crew combat callouts route through the existing
  contract/deliberation system, unchanged.
- S04's `GeneratedSystem.threat_level` seeds encounters; S05 items define
  weapon stats (damage/range/fire_rate keys); S09's `HullHandling` drives
  both sides' flight; S17's hardpoints (if merged) position the guns —
  else fire from hull center with a PR note.
- v1 inspiration: `archive/v1/godot/` PatrolController steering behaviors
  (seek/pursue/flee) — the feel to match, not the code.

## Freeze first

Core `combat/` module: `BehaviorState` enum + `fn enemy_step(state, senses)
-> (BehaviorState, Intent)` as a pure transition (senses = fixed-point
distances/health/ally-count; Intent = thrust/turn/fire/flee vector). Damage
model: `fn apply_hit(target, weapon_stats, subsystem) -> DamageResult` —
hull HP, shield absorption by type, subsystem states (engines/weapons/
sensors/drive: Nominal/Damaged/Disabled). All deterministic; property-test
the state machine (no state can trap: every state has an exit condition).

## Deliverables

- [ ] Enemy spawning: seeded encounters from threat_level (patrol wings near
      gates/asteroids; interceptor + bomber classes with distinct
      `HullHandling` and behavior weights).
- [ ] Client combat systems: enemy entities fly their Intents via rapier;
      projectiles (kinetic) and beams (energy) from weapon stats with
      cooldowns; hit detection → `apply_hit`; explosion/debris feedback
      (lyon + palette, no assets).
- [ ] Power management: a three-way power split (weapons/shields/engines)
      on quick keys (arrow-select + adjust), modifying fire cooldown,
      shield recharge, and available thrust — displayed on the HUD.
- [ ] Subsystem targeting: cycle target subsystems (`T`); disabled enemy
      engines strand it, disabled weapons silence it — and symmetrically on
      the player (S09 handling degrades with damaged engines).
- [ ] Escape: boost away (fuel), chaff (consumable slot — debug stock), and
      the S09 emergency self-jump under fire (its malfunction chance
      rises — wire the modifier).
- [ ] Crew during combat: contract evaluations continue; author one combat
      contract (damage-control priority per spec §22 table) whose fallback
      is "repair nearest" — deliberation under fire is allowed and logged.
- [ ] Player death: hull 0 → ship lost → respawn at last docked station with
      a log entry and a credit hit. Harsh-but-recoverable; permadeath
      options are Phase 2.

## Acceptance gates

```
cargo test -p reachlock-core combat::   # behavior transitions, no-trap property,
                                        # damage/shield/subsystem math
make check
```
Manual: pick a fight near a gate → disable an interceptor's engines → a
bomber forces shield management → flee by self-jump; die once and respawn
sane.

## Non-goals

Boarding/on-board combat (own brief post-S13). Capital ships/bosses
(content pass later). Faction warfare fleet battles (Phase 3). Loot drops
(needs inventory — flag if trivial to stub).

## Gotchas

- Combat math in core (integers), presentation in client (floats) — the
  test battery must run without Bevy.
- Enemy Intents cap turn/thrust by THEIR HullHandling — AI that ignores
  physics feels like cheating and breaks the §22 fairness promise.
- rapier collision groups: separate player/enemy/projectile groups from the
  start or friendly fire will be a launch-week bug.
