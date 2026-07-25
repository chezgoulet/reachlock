# S01 — Content Pipeline & Override System

**Spec:** §10 (all), §5 (Override System), §12 (P1 leftovers) · **Wave 1 · Depends on:** nothing

## Outcome

Authored content is a first-class citizen: a hand-written `.ron` file
validates against a schema, previews from the CLI, and **renders in the
client instead of the generated asset** when an override exists. The bridge
cannot tell the difference — that is the acceptance test.

## Context

- Generators emit plain structs (`GeneratedMesh`, `GeneratedLayout`, …) in
  `reachlock-core/src/generator/mod.rs`. `AssetSource` enum already exists
  there as a stub.
- The client bridge (`reachlock-client/src/bridge.rs`) converts those structs
  to Bevy assets and must NOT grow content-awareness.
- CLI lives in `reachlock-cli/` (`gen`, `determinism` subcommands exist).

## Freeze first

1. `reachlock-core/src/content/` module: `ContentFile` envelope (id,
   display_name, asset_type, seed, universe, priority, payload), the
   `Priority` enum (procedural 0 / curated 50 / event 75 / authoritative 100),
   and `resolve(overrides, seed_params) -> Resolved<T>` — the single function
   that decides authored-vs-generated. Unit-test the priority table
   (authoritative always wins; curated falls back when content missing;
   event respects `expires_at`).
2. JSON Schemas in `content/schemas/`: `hull.schema.json`,
   `station.schema.json`, `contract.schema.json` (start with these three).
   Schemas validate the RON→JSON projection of each content type.

## Deliverables

- [ ] `content/` directory at repo root with at least: one authored hull
      (`content/hulls/loup_garou.ron`), one authored station
      (`content/stations/sorrow_station.ron`), one authored contract
      (`content/contracts/cryo_pilot.ron` — port the spec §6 YAML example).
- [ ] Core `content` module: RON deserialization into the SAME structs the
      generators emit (serde derives on `GeneratedMesh` etc. as needed), the
      priority resolver, seed derivation for authored content
      (`hash("content_override", system_id, object_id)` per spec §10).
- [ ] CLI: `reachlock content validate <path>` (schema check + integrity:
      no degenerate triangles, door connectors reference real rooms, seed in
      53-bit range) and `reachlock content preview <path>` (reuse the SVG/PPM
      exporters from `gen.rs` — no Bevy window needed).
- [ ] Client: on world spawn, check a content index for an override of the
      player hull id; if present, render it. Demo: the authored Loup-Garou
      hull replaces the generated corvette.
- [ ] Loader reads `content/` from disk at startup (local mode). Server
      distribution of overrides is S23's problem, not yours.

## Acceptance gates

```
reachlock content validate content/stations/sorrow_station.ron   # exit 0
reachlock content validate <a file with a dangling door ref>     # exit 1, names the door
cargo test -p reachlock-core content::                           # priority table green
make check
```
Manual: `make run` shows the authored hull, not a generated one.

## Non-goals

Server-side `content_overrides` table wiring (S03 owns the table, S23 the
distribution). Authoring GUI. Mod manifests (S22). Souls (S13).

## Gotchas

- Add the `ron` crate to core only — it is pure and wasm-safe.
- Serde derives on generator structs must not change their field names:
  authored files ARE the compatibility surface. Pin with a round-trip test.
- Authored meshes use the same `Fixed` fixed-point vertices. A float in a
  RON file is a validation error, not a conversion.
