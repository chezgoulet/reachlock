# S05 — Item Generator

**Spec:** §16 (hierarchy, generation model, naming, icons) ·
**Wave 1 · Depends on:** nothing

## Outcome

Every item type in the spec's hierarchy can be generated from
`(seed, item_type, tier, faction, biome)`: a name, flavor text, a procedural
icon, and stats inside the tier's band. Two same-tier items with different
seeds trade off differently; two identical rolls differ visually.

## Context

- House generator style: `reachlock-core/src/generator/` (pure, integer,
  golden-tested, manifest-registered). Palettes in `util/color.rs`,
  textures as `GeneratedTexture`.
- Nothing consumes items yet — S17 (editor), S19/S20 (combat), S10
  (economy) all build on your types. You are freezing a load-bearing
  contract: be conservative.

## Freeze first

`reachlock-core/src/item/` module: `ItemType` tree (Equipment/Consumable/
Component/Implant/Cosmetic with the spec §16 subtypes), `ItemStats` as a
`BTreeMap<StatKey, i64>` of fixed-point stats (damage, range, fire_rate,
shield_hp, recharge, thrust, turn, sensor_range, mining_rate, repair_rate,
weight…), `Rarity`, `GeneratedItem { id, seed, display_name, description,
icon: GeneratedTexture, stats, rarity }`, and `ItemSeed { seed, item_type,
tier, faction, biome }`. Serde everywhere. Stat KEYS are string-stable —
pin them with a wire-shape test like `contract/protocol.rs` does.

## Deliverables

- [ ] `generate_item(ItemSeed) -> GeneratedItem`.
- [ ] Name generation: `{adjective} {material} {base}` templates per type
      with seeded word tables ("Scorched Ferrite Autocannon"). Description
      templates likewise.
- [ ] Stat bands per (type, tier): tier sets the band, seed picks the
      tradeoff point inside it. Property test: 500 seeded tier-4 items all
      stay in band; at least two distinct stat profiles exist.
- [ ] Icon formulas per top-level family (energy = glowing core, kinetic =
      angular barrel, shield = concentric hexes, consumable = vial, implant
      = circuit tracery) — small `GeneratedTexture`s (e.g. 24×24) built from
      noise/trig/palette primitives. Faction palette tints; wear from seed.
- [ ] CLI: `reachlock gen item --seed N --type kinetic_cannon --tier 4
      [--ppm icon.ppm]` printing name, stats, rarity.
- [ ] Determinism manifest entries (`item` per family) + goldens; manifest
      version bump.

## Acceptance gates

```
cargo test -p reachlock-core item::
reachlock gen item --seed 7 --type energy_laser --tier 3 --ppm /tmp/icon.ppm
make check
```

## Non-goals

Crafting recipes and skill checks (§16 crafting — belongs with S10's
economy content or a later sprint). Implant gameplay effects (S15 consumes
the hooks). Inventory UI. Equipping logic.

## Gotchas

- Icons are data (`GeneratedTexture`), never Bevy types — core stays pure.
- `weight` is fixed-point too. The spec writes `f32`; the codebase rule
  (index, rule 2) wins.
- Keep the word tables in Rust const arrays, not content files — authored
  item content arrives via S01's pipeline later; the generator must be
  self-contained.
