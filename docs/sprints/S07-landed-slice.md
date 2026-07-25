# S07 — Landed Slice (Walk a Station)

**Spec:** §14 Mode 1 (core loop steps 1–3, 6 lite) · **Wave 2 ·
Depends on:** S01, S06

## Outcome

Docking at Sorrow Station is arriving somewhere: an authored layout with
named rooms, NPCs standing in them, an interaction verb that opens
placeholder dialogue, and a market where buying and selling actually moves
credits and cargo. Generated stations get the same loop with generated
filler NPCs.

## Context

- S06 gives you the Landed mode renderer (rooms/doors/walking) and the
  docking flow. S01 gives you authored station files with `npc_spawns`
  (see `content/stations/sorrow_station.ron`).
- No economy engine exists yet (S10): prices in this sprint are static
  per-station tables — design the market UI against a `PriceSource` trait so
  S10 swaps the backend without touching the UI.

## Freeze first

1. `Interactable` component + a single interaction system (`E` when
   adjacent, prompt text from the component). Every future verb (console,
   airlock, shop, talk) goes through it.
2. `PlayerInventory` resource: credits (integer) + `BTreeMap<GoodId, u32>`
   cargo, persisted in the save alongside existing state. `GoodId` is a
   string newtype — S10 will attach real goods definitions to it.

## Deliverables

- [ ] Authored spawn: entering a station with a content override places its
      rooms with display names and its NPC spawns (colored figure + name
      label). Generated stations get 1–2 filler NPCs seeded per room kind
      (bar → patron, market → vendor).
- [ ] Talk verb: adjacent NPC + `E` opens a simple dialogue panel with 2–3
      authored lines from the content file (a `dialogue: [..]` list on the
      npc spawn — extend the station schema). Esc closes. No LLM, no souls —
      S13/S16 replace the guts; you own the panel.
- [ ] Market: stations with a Market room sell/buy from a static per-station
      price table (seeded ±15% around base). Buy/sell UI: list, price,
      quantity, confirm. Credits and cargo update; cargo respects a capacity
      number on `ShipSystems`.
- [ ] Save/load: inventory, credits, and current location survive a quit and
      relaunch (extend whatever persistence exists; if none, add a minimal
      local save file — flag this in the PR if you're creating it).
- [ ] The station's `GeneratedLayout` doors work as doorways (walk room to
      room); locked/decorative rooms are fine if marked visually.

## Acceptance gates

- Manual: dock at the authored station → read room names → talk to an NPC →
  buy 5 units at the market → undock → dock at a *generated* station → sell
  at a different price → credits changed by the spread.
- Unit tests: market math (buy/sell/capacity/insufficient credits) as pure
  functions in core or a client-side `market.rs` with no Bevy deps.
- `make check`.

## Non-goals

Real economy (S10 — you build the socket, not the power plant). Souls,
LLM dialogue (S13/S16). Foraging/crafting (post-S10 brief). Combat (S20).
Planet surfaces (same renderer later — stations only here).

## Gotchas

- Keep dialogue content IN the station/NPC content files, not hardcoded —
  S13 swaps the source, the panel stays.
- Prices and credits are integers. Display formatting is the only place a
  decimal point may appear.
- The interaction system will be fought over by later sprints (S08 consoles,
  S18 furniture): keep `Interactable` generic (label + event), not
  shop-specific.
