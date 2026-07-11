# S06 — Mode State Machine & Transitions

**Spec:** §14 (mode states, transitions), §9 (client architecture) ·
**Wave 2 · Depends on:** S04

## Outcome

The three-mode skeleton is real: Space Flight → dock → Landed → board ship →
On-Board → take helm → Space Flight, with camera/rendering shifts and game
state (fuel, log, contracts) surviving every transition. The modes may be
visually bare; the loop must be seamless and unlosable.

## Context

- `reachlock-client/src/states.rs` has a two-state `AppState`
  (MainMenu/Playing). This sprint replaces Playing with the spec §14
  `GameMode` family.
- `ShipSystems`, `ShipLog`, `ContractRuntime`, `DeliberationState` resources
  must persist across mode switches — they are global, not per-mode.
- S04's `GeneratedSystem` gives you station positions for the docking
  proximity check.

## Freeze first

The state enums, exactly once: keep `AppState` (MainMenu / InGame) and add
`GameMode` as a sub-state (Bevy `SubStates`): `SpaceFlight`,
`Landed { location_id }`, `OnBoard { is_docked }`, `Docking`, `Undocking`,
`Paused`. (Hyperspace arrives in S09, Emergency in S19 — leave them out
until their systems exist; the index forbids dead variants.) Document the
transition diagram in the module docs.

## Deliverables

- [ ] `GameMode` sub-states with `OnEnter`/`OnExit` scene setup/teardown per
      mode. Entities tagged with a `ModeScope(GameMode)` component get
      despawned by a generic teardown system — no per-mode cleanup lists.
- [ ] Docking: proximity + `E` near a station slot → short `Docking` beat
      (camera ease) → `Landed`. Undock from a "Launch" interaction →
      `Undocking` → `SpaceFlight` with the ship placed off-station.
- [ ] Landed placeholder: top-down room render from the station's
      `GeneratedLayout` (flat rects + doors), player square that walks with
      WASD and collides with walls. (S07 makes it a place; you make it work.)
- [ ] On-Board placeholder: same renderer over the SHIP's interior layout
      (generate from hull seed), cockpit tile that offers "Take Helm".
- [ ] Boarding transitions: Landed ↔ On-Board at the airlock tile;
      On-Board → SpaceFlight at the cockpit; SpaceFlight → On-Board via key
      (walk-the-ship-in-flight, spec §14).
- [ ] Pause overlay (Esc) that actually stops the sim clock (contracts and
      fuel burn included) in every mode.
- [ ] HUD adapts: flight HUD only in SpaceFlight; location name banner in
      Landed/OnBoard.

## Acceptance gates

- Manual loop test: menu → fly → dock → walk → board → helm → fly → dock,
  twice, without a crash, a stuck state, or a lost resource (fuel value and
  ship log survive the full loop).
- Contract engine keeps evaluating in ALL modes (log entries continue while
  docked — spec §14: "crew contracts continue evaluating").
- `make check`.

## Non-goals

NPCs, market, dialogue (S07). Crew entities aboard (S08). Jump gates (S09).
Interior furnishing (S18). Any authored location content (S07).

## Gotchas

- Bevy sub-states: `#[derive(SubStates)]` with `#[source(AppState = AppState::InGame)]`
  — check the 0.18 API in the bevy docs for the exact attribute names before
  writing 500 lines against a guessed macro.
- `Landed { location_id: String }` in a state enum means Eq/Hash on String —
  fine, but transitions must construct the exact value; keep a
  `CurrentLocation` resource as the source of truth and the state variant
  data minimal.
- The rapier physics world must not tick while `Paused` or in Landed mode —
  gate `RapierConfiguration`/time scaling, don't despawn the ship.
