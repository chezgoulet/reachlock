# S18 — Ship Editor: Interior

**Spec:** §19 (interior editor) · **Wave 5 · Depends on:** S08

## Outcome

The player reconfigures how they live: grid-placed rooms from templates,
auto-generated corridors between door connectors, furniture that changes
gameplay numbers, adjacency bonuses that reward thoughtful layouts — and
the On-Board mode walks the result. Interior layout becomes progression.

## Context

- Spec: ship interiors are NOT authored — generated from hull frame + player
  placement. Room TEMPLATES are authored content
  (`content/hulls/room_templates.ron`).
- S08 renders and walks `GeneratedLayout` interiors; crew duty rooms map by
  `RoomKind`. Your output must be the same `GeneratedLayout` type so S08
  needs zero changes to walk an edited ship.
- `ShipInteriorLayout` is specified in §19.

## Freeze first

Core types: `ShipInteriorLayout { hull_id, rooms: Vec<PlacedRoom>,
corridors, furniture: Vec<PlacedFurniture>, seed }`;
`RoomTemplate { kind, size, required_systems, furniture_slots }` (authored,
schema'd); `fn realize(&ShipInteriorLayout, templates) ->
GeneratedLayout` — the single function turning player placement into the
walkable layout. Corridor auto-gen: connect door connectors with L-shaped
integer paths; unreachable room = validation error, not a runtime surprise.

## Deliverables

- [ ] `content/hulls/room_templates.ron`: the spec's template list (cockpit,
      bridge, med bay, engineering, quarters, galley, cargo, airlock,
      hydroponics, workshop, armory, brig) with sizes and furniture slots.
- [ ] `realize()` with validation: bounds inside hull grid area, no
      overlaps, required rooms present (cockpit + airlock minimum), all
      rooms reachable via corridors — reuse the connectivity-test style from
      `generator/station.rs`.
- [ ] Adjacency bonuses: authored pairs on templates (galley+quarters →
      relationship recovery; engineering+cargo → repair transfer) computed
      into a `LayoutBonuses` struct consumed by ShipSystems (numbers can be
      inert until their systems exist — the pipe and the display matter).
- [ ] Furniture: grid placement inside rooms from template slots; each piece
      carries stat contributions (med station → heal rate). Rendered in
      On-Board mode as colored fixtures with labels.
- [ ] Editor mode (docked, from the same Shipyard surface as S17 —
      coordinate): palette of templates → drag/arrow-key placement on the
      hull grid → live validation feedback → corridor preview → apply
      (persists; On-Board respawns from the new layout; crew duty rooms
      remap by kind).
- [ ] Cost preview per change (credits; same debug latitude as S17).

## Acceptance gates

```
cargo test -p reachlock-core interior::   # realize validation battery:
                                          # overlap, unreachable, missing-required
make check
```
Manual: move the galley next to quarters → bonus appears in a layout summary
panel; walk the edited ship; Boris's duty room remaps when engineering
moves.

## Non-goals

Crisis-event interactions with layout (fires spreading — Phase 2). Refit
time. Exterior anything (S17). Room damage states (S19+).

## Gotchas

- `realize()` output feeding S08 unchanged is the whole contract — if S08
  needs edits, your output type drifted; fix on your side.
- Corridor auto-gen must be deterministic from placement order-independent
  input: sort connectors before pathing or two identical layouts diff.
- Grid math is integers end to end; the editor cursor is the only
  screen-space float.
