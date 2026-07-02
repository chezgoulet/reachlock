# The Universe Tick — simulation contract (v0)

One clock for the living universe, **identical in single-player and MMO**.
The SP engine embeds the same simulation the server runs; only the driver
differs. This document is the contract both implementations satisfy; the
determinism test in `server/` is its executable form.

## Time

- The universe advances in discrete **ticks**. One tick = **one in-game
  minute**. Faction goals, economy repricing, NPC schedules, and event
  conditions are all expressed in ticks.
- Drivers: SP advances ticks on a wall-clock scale while playing (default
  1 tick/second, pausable) and in **batches** across time-skips (jump transit
  in cryo, docked rest). MMO advances on the server wall clock, always.
- Coarser processes run on tick multiples (economy repricing every 60 ticks,
  faction goal evaluation every 1440), always derived from the same counter.

## Determinism (the property the test enforces)

> Same state + same ordered inputs + same seed → same next state. Bit-for-bit.

- **Event queue:** pending events are ordered by `(due_tick, priority,
  insertion_seq)`. Insertion sequence is the total ordering tiebreak — never
  map iteration order or wall time.
- **RNG:** no global RNG. Each consumer draws from a named stream seeded by
  `(universe_seed, stream_name, tick)`, so systems can be added or reordered
  without perturbing each other's draws.
- **Player-independent vs player-dependent** (GAME-DESIGN.md §3): the
  simulation never blocks on a player. Player actions enter as ordered inputs
  (SP: the one player; MMO: aggregated), affecting outcomes only through
  state.
- Floating point stays out of tick state; simulation quantities (prices,
  standings, supplies) are integers or fixed-point.

## Batch advancement (time skips)

`advance(state, n_ticks, inputs) → state'` must equal `n` single-tick calls.
No shortcuts that change outcomes: a 3-day cryo jump replays the same
simulation the server would have run live. This is what makes "wars start
while you're three jumps deep" honest rather than staged.

## Snapshots

Tick state serializes into the save schema's `universe` block (SP) / the
server DB (MMO). `universe.tick` in the trigger DSL context is this counter —
storyline conditions are expressed against the same clock everywhere.
