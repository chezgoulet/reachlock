# S21 — Gate Network & the Procedural Frontier

**Spec:** §17 (all except colonization) · **Wave 6 · Depends on:** S04, S09

## Outcome

Two kinds of space, one galaxy: charted space is an authored gate graph —
designed systems, faction-controlled gates, statuses that change — and
beyond it lies deep space, generated from galactic coordinates, discovered
via FTL, named by the first player there, recorded first-write-wins in the
ledger. The frontier is the boundary, and crossing it changes the game's
texture.

## Context

- S09 built gate TRANSIT with a placeholder destination
  (`hash(seed, "gate")`); you replace that with the real network.
- S04's `fidelity` knob (Full/Sparse) was built for you; deep space uses
  Sparse far from gates.
- Seed ledger and discovery flow exist end-to-end (server + S02 client).
  Deep-space system ids are `uncharted_{coord-hash}`; naming rides the
  ledger's diffs (`seed.modify` with `{ "name": … }`).
- Gate statuses (Active/Blockaded/Restricted/Contested/Destroyed) interact
  with S11 factions if merged (control, access by reputation); else statuses
  are static authored data — note in PR.

## Freeze first

Core `galaxy/` module: `GateNetwork { gates: Vec<Gate> }` where
`Gate { from: SystemId, to: SystemId, status, controlled_by }` — authored
content (`content/gate_network/*.ron`, schema'd, spec §17 example is the
fixture). `GalaxyCoord { x, y, z: i64 }` and
`fn deep_space_seed(coord, universe) -> Seed` (deterministic, golden-
tested — this derivation is protocol, like `derive_seed`). Charted systems
get authored `content/systems/*.ron` entries (id, position, biome, display
name).

## Deliverables

- [ ] The starting region authored: 6–8 charted systems from canon (aethon,
      verne, cadence, sorrow, earth, the_veil + frontier fringe) with the
      spec's gate graph — statuses included (earth Blockaded, the_veil
      Restricted).
- [ ] Gate transit destination: S09's jump reads the network — gate choice
      UI when a system has multiple gates; Blockaded/Restricted gates
      refuse (or demand reputation per S11) with in-fiction messaging.
- [ ] Galaxy map screen (nav console/`G`): charted systems + gate edges +
      statuses; your position; deep-space discoveries appear as you make
      them, with their names.
- [ ] FTL to coordinates: pick a heading + distance on the map (or enter
      coords) → S09's self-jump risk model applies → arrive at
      `deep_space_seed(coord)` generated Sparse → discovery flow fires
      (online: `seed.discover`; offline: local ledger in the save).
- [ ] Naming: first discoverer (online: `you_discovered`; offline: always)
      may name the system; name persists via ledger diffs and shows on the
      map for everyone.
- [ ] Fidelity gradient: distance-from-nearest-gate drives Sparse
      generation and threat (emptier, stranger, riskier — set the §17
      texture with data, not new systems).

## Acceptance gates

```
cargo test -p reachlock-core galaxy::   # coord→seed golden, graph validation
                                        # (no dangling gate endpoints)
reachlock content validate content/gate_network/core_region.ron
make check
```
Manual: jump aethon→verne through the gate UI; bounce off the earth
blockade; FTL past the fringe → discover, name it "Goulet's Rest", see it
on the map; (online) a second client sees the name.

## Non-goals

Colonization (own brief after S23). Gate CONSTRUCTION by factions (Phase 3).
Story content in charted systems beyond ids/names (Phase 2 content pass).
Galaxy-scale pathfinding/autopilot.

## Gotchas

- `deep_space_seed` joins `derive_seed` as frozen protocol — golden test,
  loud comment, never change.
- Offline and online discovery must share one code path with the ledger
  behind a trait (the save is just another SeedStore — you may find S03's
  trait already fits; use it).
- The map is data-driven from `GateNetwork` + ledger state — don't cache
  authored positions in client code.
