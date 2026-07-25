# S04 — System Generator

**Spec:** §5, §14 (Mode 3 generation), §17 (deep-space fidelity) ·
**Wave 1 · Depends on:** nothing

## Outcome

`generate_system(seed, biome, fidelity)` produces an entire star system as
plain data — star, planets, asteroid fields, station slots, jump-gate
position, starfield — and the client renders it: fly from the gate past
asteroids to a station orbiting a planet. One seed, one world, everywhere.

## Context

- Existing generators (`reachlock-core/src/generator/`) show the house
  style: pure fns, integer math, `SeededRng`, golden tests, determinism
  manifest entries. Planet/station/hull generators already exist —
  compose them, don't rewrite them.
- The client currently hand-places one station and one planet in
  `reachlock-client/src/systems/setup.rs` — replace that with the system
  generator's output.

## Freeze first

`GeneratedSystem` in core: star (class, color from palette), `Vec<Orbit>`
(planet params + fixed-point positions), `Vec<AsteroidField>` (center,
radius, density, resource biome), `Vec<StationSlot>` (position, kind,
station seed), `gate_position`, `starfield_seed`, `threat_level: u8`.
Positions are `FixedVec2` in world units. Serde derives — S01's authored
content and S21's frontier both consume this struct.

## Deliverables

- [ ] `generator/system.rs`: seeded layout with sane orbital spacing
      (integer radii bands), 0–3 stations by biome, 0–4 asteroid fields,
      exactly one gate. A `fidelity` knob (Full/Sparse) that reduces detail
      for deep-space systems (spec §17 "variable fidelity").
- [ ] Starfield: seeded point cloud (position, brightness, palette tint) —
      cheap data the client draws as a background layer.
- [ ] Client: `spawn_world` consumes `GeneratedSystem` — station meshes at
      station slots, planet discs at orbits, asteroid clusters (reuse hull
      generator with a small radius band for rocks), gate marker, parallax
      starfield.
- [ ] CLI: `reachlock gen system --seed N --svg map.svg` — a labeled
      top-down system map (positions, orbits, gate).
- [ ] Determinism manifest entries for `system` (Full and Sparse) over the
      canonical seed battery; goldens captured; manifest version bumped.

## Acceptance gates

```
cargo test -p reachlock-core generator::system::   # incl. golden + "orbits don't overlap"
reachlock gen system --seed 42 --svg /tmp/map.svg  # human-checks as a plausible system
make check                                          # CI determinism gate stays green
```
Manual: `make run` — fly the seeded system; same seed = same layout on wasm.

## Non-goals

Gate NETWORK between systems (S21). Economy hooks on stations (S10).
NPC traffic (S12/S19). Docking (S06).

## Gotchas

- Manifest version bump is mandatory (rule 3 in the index) — CI compares
  cross-target, so a new generator with float leakage fails on wasm vs x86.
  Integer math only; `util::trig` for angles.
- Orbit spacing: use integer radius bands with a minimum gap, then jitter
  inside the band — rejection sampling loops are fine but must be bounded.
- Keep `setup.rs`'s deliberation/contract wiring intact; you're replacing
  scenery, not gameplay.
