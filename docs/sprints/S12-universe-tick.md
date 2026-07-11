# S12 — Universe Tick Integration (Online + Offline Parity)

**Spec:** §8 (tick service), §20/§21 (tick consumers) · **Wave 3 ·
Depends on:** S10, S11

## Outcome

The universe moves whether you're there or not, in both modes: offline, a
local ticker advances economy + factions + storylines while you play (and
catches up on load); online, the server's tick service runs the same core
step and broadcasts events to every session. Same seed + same event log =
same universe, everywhere. That's the pillar, proven by a test.

## Context

- The server tick task exists as a heartbeat
  (`reachlock-server/src/services/tick.rs`, skip-on-overrun already
  correct). S10/S11 give you `EconomyState::tick` and the faction step as
  pure core functions.
- `universe_events` table exists in the migration; `ServerMessage::UniverseEvent`
  exists on the wire.

## Freeze first

Core `sim` module: `UniverseState { tick_no, economy, factions, chapters,
event_log }` with ONE entry point `fn advance(&mut self, seed) ->
Vec<SimEvent>` composing the S10/S11 steps in a fixed order (economy →
factions → storylines). Serde on all of it — this struct IS the save's
world block and the server's authoritative state.

## Deliverables

- [ ] `UniverseState::advance` + ordering test (the composition order is a
      compatibility promise — pin it).
- [ ] Offline ticker: a client resource advances `UniverseState` on a game
      clock (respecting S06 pause), persists it in the save, and on load
      fast-forwards elapsed ticks with a hard cap (spec: fail states are
      fine; an unbounded catch-up loop is not).
- [ ] Server: the tick task owns a `UniverseState` per universe tier,
      advances it on the interval, persists snapshots + appends to
      `universe_events` (postgres feature) or memory, and broadcasts
      `SimEvent`s as `universe.event` messages.
- [ ] Client (online): received universe events update the LOCAL
      `UniverseState` view (prices, faction standings, news) instead of
      ticking locally — one authority per mode, never both.
- [ ] News surfaces: station ticker + a "Galactic News" screen on the nav
      console rendering the event feed with tick timestamps.
- [ ] Parity test (the headline): two `UniverseState`s, same seed, same
      injected player-trade log, one advanced "offline-style" and one
      "server-style" — identical serialized state at tick N. Runs in CI.

## Acceptance gates

```
cargo test -p reachlock-core sim::            # parity + ordering + catch-up cap
cargo test -p reachlock-server tick           # server tick persists + broadcasts
make check
```
Manual: play offline 10 minutes → prices drifted, news accumulated; quit,
relaunch → catch-up ran; connect online → local ticker stops, server events
flow.

## Non-goals

Tick-driven NPC ship VISUALS in-system (S19 territory). Contract-driven
autotrading. Colonization queues. Chapter CONTENT beyond the S11 fixture.

## Gotchas

- One authority: the offline ticker must fully stop in online mode (and
  vice versa) — a double-ticking universe inflates twice as fast and the
  bug looks like "balance".
- Fast-forward cap: pick a bound (e.g. one in-game week of ticks), log the
  truncation to the ship's log as fiction ("the markets moved while you
  slept").
- Snapshots: serialize `UniverseState` whole; don't invent a delta format
  yet.
