# S16B — Wave-4 / S09 Closeout (no crumbs)

**Spec:** §14, §15, §18, §22 partials · **Between waves 4 and 5 ·
Depends on:** S09f (the whole #119–#126 stack)

## Outcome

Every "honest gap" left by S09c–S09f and S13–S16 is either closed here or
explicitly re-homed to a named future sprint — nothing is silently parked.
The jump-cryo loop is fair (crew can actually reach the pods from either
deck), soul mutations fire in-game, damaged systems actually degrade and
are repaired at the system, crew speak in the world, the LLM wire carries
the voice, and the Postgres seams are real.

## Deliverables

1. **Cross-deck crew routing.** A crew member whose duty/order target is on
   the other deck walks to the ladder, climbs (leaves the active scene),
   and continues on the far side. Off-screen crew move abstractly on a
   timer. Sprites despawn at the ladder when leaving the active deck and
   spawn at the ladder when arriving. Fixes: humans Upstairs can reach
   cryo pods before a jump window; crew can respond to crises anywhere.
2. **`ORDER_ROOMS` completion.** All ship rooms (Cockpit / TechBay /
   Scanner / MedBay / Cryo) join the order list; the order panel handles
   the longer list (1–9, 0).
3. **Soul-mutation scanning.** The authored mutation arcs
   (`content/storylines/loup_garou_souls.ron`) are evaluated at runtime:
   after every applied soul event, that soul's mutations are scanned with
   the event's fields in context, fired once, and narrated to the log.
4. **Per-system damage + repair-at-the-system** (SHIPS.md §4 completion).
   Core: a `SystemDamage` map (room kind → Nominal/Damaged/Disabled) fed by
   fire events (SystemsBurning → Damaged, BurnedOut → Disabled), pure and
   tested. Client effects: damaged Scanner halves sensor range (disabled:
   scans fail), damaged Reactor cuts engine power (disabled: engines limp),
   damaged Bridge doubles weapon cooldown (disabled: guns dark). Repair is
   at the system: the affected room's console shows the state and `R`
   works a multi-action repair.
5. **Comm surfaces** (S16 completion). A `CommFeed` resource: crew speech
   renders as a fading comm line on the flight HUD and as a speech bubble
   above the speaker's figure on board. Deliberation outcomes and the
   cryo-crossing beats route through it (same voice pipeline).
6. **LLM wire revision** (one protocol bump, iron rule #4 honored).
   `llm.call` gains optional `system_prompt`, `timeout_ms`, `max_tokens`
   (serde-default; absent = old behavior). Pinned wire test updated and the
   revision named in the commit. Server: a provided system prompt replaces
   the generic wrapper (dialogue speaks with `voice_prompt` as the TRUE
   system prompt); timeout/max_tokens clamped by the server cap. Client:
   dialogue sends the voice prompt; contract deliberations send their
   `LlmConfig` budget when present.
7. **Contract-quality modifier feed** (S15 completion). `ContractRuntime`
   tracks a rolling recent-uncovered count; online outcome classification
   feeds `contract_quality_modifier` with it.
8. **Postgres completion.** `ByokStore` pg implementation against the
   existing `byok_keys` migration; server tick appends to
   `universe_events` under the `postgres` feature; `contract.sync`
   persists via the existing contracts pg store instead of log-and-drop.
   All behind the feature flag; memory paths unchanged.
9. **Housekeeping.** `save/` gitignored and untracked; stale PRs triaged
   with dispositions (#114 superseded by merged S09b/S09c work; #113
   superseded — the query-conflict fix shipped differently; #108/#73
   dispositioned per the #112 tracking issue, #112 updated).

## Explicitly re-homed (named, not parked silently)

- Jump wake-conditions programming + automation modules + pod capacity
  enforcement → the S19-adjacent "ship automation" brief (SHIPS.md §3).
- Fires harming the player avatar → S20 landed combat (needs a player
  health model).
- Dispatch routing real combat orders → S19 (its consumer).
- Offline outcome classification → by design: offline has no inference, so
  timeout→fallback IS the offline behavior (S15 gotcha).
- Speech for station NPCs without souls → Phase-2 content pass (souls are
  authored; generated NPCs keep S07 lines).

## Acceptance gates

```
cargo test -p reachlock-core crisis:: crew routing + system damage batteries
cargo test -p reachlock-client        # routing, mutation scan, comm feed
cargo test -p reachlock-server llm    # wire revision battery
make check                            # including the pinned-wire revision
```
Manual: arm a jump with a human Upstairs → they make the pods; set a fire
in the scanner room → scans degrade, R repairs at the console; ask Boris
about the mark with high trust → the mutation arc fires and the inspect
panel shows it.

## Gotchas

- The wire revision must keep old clients valid: every new field is
  `#[serde(default)]` and absent-means-previous-behavior.
- Off-screen crew movement must use the same speeds as on-screen walking
  (body kind × deck gravity) or the jump clock becomes unfair in the other
  direction.
- Repair actions are at the SYSTEM (its console), not a global key —
  that's the SHIPS.md §4 sentence this closes.
