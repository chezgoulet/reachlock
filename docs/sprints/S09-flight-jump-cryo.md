# S09 — Flight, Jump Gates & Cryo Transit

**Spec:** §14 Mode 3 (fly/navigate/scan/jump/self-jump), §6 (cryo-pilot) ·
**Wave 2 · Depends on:** S04, S06

## Outcome

Flight has depth and travel has stakes: hulls handle differently, sensors
reveal the system progressively, and jumping a gate plays the cryo sequence
with the cryo-pilot contract genuinely holding the helm — including the
Hyperspace mode where an anomaly can force Boris to deliberate mid-jump.
Emergency self-jump exists and can go wrong.

## Context

- Flight currently: one force/torque profile in
  `reachlock-client/src/systems/ship.rs`. Hull classes exist in core
  (`generator/hull.rs::HullClass`).
- The cryo-pilot contract is the spec's canonical example and already a test
  fixture in `core/src/contract/engine.rs`. The client's auto-helm flow in
  `systems/contract.rs` shows the deliberation wiring to copy.
- S04 gives gate position and system contents; S06 gives the mode machine
  (you add the `Hyperspace` variant — it was deliberately left out until
  this sprint).

## Freeze first

`HullHandling` in core: `{ mass, thrust, turn_rate, drift_damping,
boost_mult, fuel_burn }` — fixed-point, derived per `HullClass` (+ seed
jitter), consumed by the client as f32 at the bridge. S17 (editor) and S19
(combat) both read this struct; it is load-bearing.

## Deliverables

- [ ] Handling model: flight params come from `HullHandling`; add brake and
      boost (boost burns fuel faster). A corvette must feel different from a
      freighter in hand.
- [ ] Sensors: contacts beyond sensor range render as unknown blips;
      scanning (hold `S` in proximity/facing) resolves identity. Sensor
      range on `ShipSystems` (S05 items will feed it later — plain number
      for now). System map (nav console + `M` overlay) reflects
      known-vs-unknown.
- [ ] Gate jump: fly into the gate ring → confirm → `Hyperspace` mode:
      crew-to-cryo beat, tunnel visual (lyon + palette), the cryo-pilot
      contract evaluating on a compressed tick against transit state
      (distance_to_destination decreasing, seeded event injection).
      Uneventful jumps end with wake_crew at the destination system
      (generate it from the destination seed — for now `hash(seed, "gate")`
      as the destination id; S21 replaces this with the real network).
- [ ] Transit anomalies: a seeded roll injects an uncovered field mid-jump →
      deliberation state (visible during hyperspace) → offline fallback or
      (online) server response decides; outcome lands in the ship log with
      the full story.
- [ ] Emergency self-jump (`J` outside gates): higher fuel cost + seeded
      malfunction chance (fixed-point probability) → outcomes from
      "arrived off-course" to "hull stress damage"; never a silent fail —
      log narrates.
- [ ] Fuel dock: refuel while docked for credits (talks to S07's inventory
      if merged; else a debug refuel — note in PR).

## Acceptance gates

- Manual: jump a gate uneventfully; jump until an anomaly fires and read the
  log story; self-jump with low fuel and eat a malfunction.
- Unit tests in core: transit tick evaluation (contract fires wake_crew at
  distance threshold), malfunction roll distribution is seed-deterministic.
- `make check` (manifest untouched — transit events use the seeded RNG but
  aren't a generator; do NOT add manifest entries for gameplay rolls).

## Non-goals

The authored gate NETWORK and deep-space discovery (S21). Combat during
transit (S19). Multi-system persistence beyond "destination generates and
is flyable". Crew injury states (Phase 2).

## Gotchas

- Determinism discipline: transit event rolls must derive from
  `(system_seed, jump_count)` so replays reproduce — never from wall time.
- Hyperspace must pause rapier or scope it out (S06's pattern); the ship
  entity persists, the space scene doesn't.
- The deliberation overlay already handles the offline timeout; reuse
  `DeliberationState`, don't build a second one for hyperspace.
- **Wire up S02's canonical-seed adoption (integrator carry-over from Wave 1).**
  Jumping to a new system is where `setup.rs` finally grows a real per-system
  `SystemId` and a multi-system registry (today `spawn_world` hardcodes a
  single `SYSTEM_SEED`). When it does, complete the `// S02 TODO(integrator):`
  left at the top of `reachlock-client/src/systems/network.rs`: on arrival,
  `seed.discover` the destination, and if the server's `seed.canonical`
  differs from the locally generated seed, drive scene (re)generation from
  `SeedState::adopted` instead of merely logging it. Until this sprint the
  online path correctly discovers/adopts and logs but has no second system to
  regenerate into, so the hook is a stub by design — not a bug.
