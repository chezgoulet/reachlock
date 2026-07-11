# S08 — On-Board Slice (Walk Your Ship)

**Spec:** §14 Mode 2 (core loop 1–3, 6) · **Wave 2 · Depends on:** S06

## Outcome

The ship is a place: crew members stand at their stations and wander
off-shift, consoles are physical objects you walk to (helm, engineering,
nav), orders can be issued to crew, and the ship's log console shows the
full contract decision history. The FTL fantasy, walkable.

## Context

- S06 gives the On-Board renderer and mode transitions.
- Crew are placeholder entities here — soul files arrive in S13. Define crew
  as data now: `CrewMember { id, name, role, station_room }` list on a
  `CrewRoster` resource.
- The contract log already exists (`ShipLog` resource) — the log console
  surfaces it; deliberation events (`DeliberationState`) should also appear.

## Freeze first

`CrewRoster` resource + `CrewMember` struct (serde, in the save). Ship
interior room kinds already exist (`generator::RoomKind`) — map roles to
duty rooms (engineer→Reactor, pilot→Bridge/cockpit). S13 will attach souls
by `CrewMember.id`; keep ids stable strings ("boris", "tib", …).

## Deliverables

- [ ] Crew rendering: each roster member drawn in their duty room; a simple
      shift cycle (duty room ↔ quarters/galley on a timer) so the ship
      breathes. Pathing can be teleport-with-fade or straight-line walk —
      no A* required yet.
- [ ] Consoles as `Interactable`s (S07's component if merged; else your own,
      coordinate in PR): Helm (take the helm → SpaceFlight), Engineering
      (shows fuel + a "vent/refill" debug action), Nav (shows current system
      map — reuse the S04 SVG layout drawn with lyon), Log (scrollable
      contract decision history with timestamps).
- [ ] Orders: click/select a crew member → order menu ("go to <room>",
      "hold position"). Orders override the shift cycle until cleared.
      Orders are recorded in the ship log ("You ordered Boris to the cargo
      hold").
- [ ] Contract visibility: while docked or flying, contract evaluations keep
      appending to the log (already true — prove it in the log console UI,
      including the deliberation entries with their reasoning text).
- [ ] The interior layout comes from the ship's hull seed via
      `generate_station`-style layout (or the existing hull interior path
      from S06) — one source of truth for "my ship's rooms".

## Acceptance gates

- Manual: board while docked → find Boris in the reactor room → order him to
  quarters → he goes and stays → take the helm → fly → walk back mid-flight
  → the log console shows the fuel-warning entries from the flight.
- Unit tests: shift-cycle and order-override logic as pure functions.
- `make check`.

## Non-goals

Souls/personalities (S13). Crisis events — fires, breaches (Phase 2 brief,
cut after S13). Interior EDITING (S18). Crew relationships/spatial social
graph (S13+). Boarding combat (post-S19).

## Gotchas

- Two sprints touch interaction and interior rendering (S07, S18): keep your
  systems in `systems/onboard/` and coordinate shared components through the
  PR, not silent duplication.
- Crew entities must despawn/respawn across mode switches via the S06
  `ModeScope` pattern; the ROSTER resource persists, the sprites don't.
- Log console: render newest-last with a cap; the `ShipLog` currently holds
  6 entries — raise the cap into a scrollback (e.g. 200) as part of this
  sprint and say so in the PR.
