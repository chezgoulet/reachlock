# S17 — Ship Editor: Exterior

**Spec:** §19 (exterior editor) · **Wave 5 · Depends on:** S05

## Outcome

Docked at a shipyard, the player opens the exterior editor: pick a hull
frame, place weapons on its hardpoint slots, choose an engine, set paint
layers and decals — with a live orbit-camera preview — and fly out in the
ship they configured. The ship is the character sheet; this is where it
gets written.

## Context

- `HullConfiguration` is specified in §19; hull classes and generation exist
  (`generator/hull.rs`), items exist (S05 — weapons/engines are
  `GeneratedItem`s), palettes exist (`util/color.rs`).
- S09 froze `HullHandling` — engine choice and plating mass must feed it
  (that's what makes the editor gameplay, not dress-up).
- Hull FRAMES are authored content: this sprint adds
  `content/hulls/*.ron` frame definitions (structural constants +
  customizable zones + hardpoint slot positions) via S01's pipeline.

## Freeze first

Core types: `HullConfiguration { hull_id, seed, hardpoints: Vec<Hardpoint>,
engine: ItemRef, plating: Vec<ArmorSegment>, paint: PaintScheme, decals }`
where `Hardpoint { slot_id, item: ItemRef, size_class }` and `PaintScheme`
is palette-reference-based (primary/secondary/accent as palette slots, not
raw colors — the generator resolves on render, per spec). `ItemRef` =
(seed + ItemSeed params) so configs stay data. Schema + wire test; this
struct goes in the save AND (later) the seed ledger diffs.

## Deliverables

- [ ] `content/hulls/` frames for three classes (shuttle, corvette,
      freighter): slot layouts, engine mounts, zone definitions — validated
      by `reachlock content validate`.
- [ ] `HullConfiguration → GeneratedMesh` composition in core: base hull +
      hardpoint attachments (item icons/geometry placed at slots) + paint
      resolution. Deterministic; add a `hull_config` determinism-manifest
      entry over a fixture config; bump manifest version.
- [ ] Handling derivation: `fn handling(&HullConfiguration) -> HullHandling`
      — engine model sets thrust/burn, plating mass slows turn. Unit-test
      the direction of every tradeoff.
- [ ] Editor mode (Bevy UI, reachable from a Shipyard interactable while
      docked at stations with a Shipyard room): slot list + item picker
      (from a debug stock of S05 items until inventory exists), engine
      picker, paint tab (palette slots), decal tab (faction insignia by
      reputation gate if S11 merged, else all). Orbit-camera live preview
      re-renders on every change.
- [ ] Apply/cancel with cost preview (flat per-change credit costs; S07
      credits if merged, else free with a PR note). Applying persists the
      config to the save and respawns the flight-mode ship from it.

## Acceptance gates

```
cargo test -p reachlock-core hull_config::   # composition golden, handling tradeoffs
make check
```
Manual: reconfigure paint + engine → preview updates live → apply → undock
→ the ship looks and handles like the config; relaunch → it persisted.

## Non-goals

Interior editor (S18). Visual damage model on plating (S19 consumes
`ArmorSegment`). Refit TIME simulation (cost only). Inventory/ownership of
items (debug stock is fine — say so in the PR).

## Gotchas

- The preview must render through the SAME core composition fn as flight
  mode — two renderers will drift; one did in v1.
- Paint slots resolve through the faction/seed palette at render: storing
  raw RGB in the config is the bug the spec explicitly warns about.
- Editor UI state machine: keep it a `GameMode`-adjacent state under
  Landed (S06 owner) — coordinate the enum change in the PR.
