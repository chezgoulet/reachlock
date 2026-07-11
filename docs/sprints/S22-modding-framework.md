# S22 — Modding Framework

**Spec:** §23 (all) · **Wave 6 · Depends on:** S01

## Outcome

The three-layer promise is real: game content loads through a mod loader,
**our own content ships as a bundled mod with no private APIs**, community
mods arrive as `.reachmod` packages with manifests, dependencies, and
conflict resolution — and the same CLI the team uses validates and packages
them.

## Context

- S01 built content loading from a flat `content/` dir. This sprint
  restructures that into mods: `mods/reachlock/` (the official mod,
  containing everything S01+ authored) loaded by the same loader as
  `mods/<community>/`.
- v1 precedent: the three-ring architecture and `*.reachmod` packaging
  existed in Godot (`archive/v1/`) — the ARCHITECTURE.md there explains the
  "engine has zero content" guard philosophy. Port the discipline.
- Every content schema written by S01/S10/S11/S13/S17/S20/S21 is already
  the modding API — your job is loading, not new formats.

## Freeze first

`ModManifest` in core (spec §23 example as fixture): id, name, version
(semver), author, description, dependencies, conflicts, content
declarations (`ContentAdd { type, id }`, `ContentOverride { system_id,
object_id, priority }`). Schema + wire test. Load order rules, written
down: official mod first, then dependency-sorted, then user-configured
order; later same-priority wins ONLY when the conflict policy says so.

## Deliverables

- [ ] Repo restructure: `content/` → `mods/reachlock/` with a manifest;
      the loader discovers `mods/*/mod.manifest.ron`, validates each mod's
      content against schemas at load, applies dependency-sorted order.
      All existing tests/fixtures updated — this touches many sprints'
      files; keep the diff mechanical and coordinate timing with in-flight
      branches.
- [ ] Collision handling: duplicate content ids across mods detected at
      startup → report with resolution options (skip mod / last-wins) via
      config; conflicts declared in manifests refuse to co-load.
- [ ] `.reachmod` packaging: `reachlock mod pack <dir>` (validate all
      content + manifest, zip to `.reachmod`) and `reachlock mod install
      <file>` (unzip to mods dir, re-validate). `reachlock mod list` shows
      installed mods, versions, load order, conflicts.
- [ ] Enable/disable + reorder via a `mods.config.ron` the CLI edits (a
      launcher UI is later; the config format is the contract).
- [ ] An example community mod in-repo (`examples/mods/duskway_pack/`): one
      station, one soul, one goods tweak via override — used by tests and
      shipped as documentation.
- [ ] The engine-purity guard: a CI check (script or test) asserting no
      content ids appear in engine code (`reachlock-core`/`-client`
      source) — v1's architecture guard, reborn. Grep-based is fine;
      document the allowlist mechanism.

## Acceptance gates

```
reachlock mod pack examples/mods/duskway_pack && reachlock mod install duskway_pack.reachmod
reachlock mod list                       # shows official + duskway, load order
cargo test -p reachlock-core mods::      # manifest, ordering, collision battery
make check
```
Manual: install the example mod → its station exists in-game; disable it →
gone; introduce a deliberate id collision → startup reports it with options.

## Non-goals

Launcher GUI mod manager. Community distribution platform. Scripting API
(Lua — much later; data-driven content is the v2 modding surface). Asset
(png/audio) overrides — flag the design question in the PR if trivial.

## Gotchas

- The restructure is the merge-conflict magnet of Wave 6: land it as one
  mechanical commit (`git mv` + path constants) separate from feature
  commits, and announce the path change in the PR title.
- The guard will flag prose containing faction/content words in engine
  strings (v1 hit this twice) — build the allowlist mechanism on day one.
- WASM: mod loading reads the filesystem — behind a trait so the web build
  can load the official mod from bundled bytes; don't break `make check`.
