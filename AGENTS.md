# REACHLOCK v2 — AGENTS.md

*Read this first. Then read the sprint brief. Then read the relevant spec sections. Then write code.*

## What This Is

REACHLOCK is a procedurally-generated spacefaring MMO in Rust + Bevy + Postgres + Redis. It is a **character-creation game**: the player builds a character, picks an origin, and gets a ship and crew from it. The Loup-Garou and its crew are one authored origin among several — never an engine assumption. `make check-purity` fails if engine code names a specific ship or crew member.

The v1 Godot prototype is archived on the `archive-v1` branch. Both `main` and `testing` are pure v2; `main` became v2 in the 2026-07-25 cutover.

**Your job:** Execute sprint briefs from `docs/sprints/`. Each brief is self-contained with outcome, deliverables, acceptance gates, frozen types, non-goals, and gotchas. Read the brief, cut a branch, ship the deliverable, open a PR.

## Repository Layout

| Path | What it is |
|---|---|
| `docs/REACHLOCK-V2-SPEC.md` | Full design spec, 24 sections. Read only the sections your brief cites |
| `docs/sprints/00-INDEX.md` | Sprint index — dependency waves, playbook, gotcha ledger. **Read this before any sprint** |
| `docs/sprints/S*.md` | Sprint briefs S01–S86 |
| `reachlock-core/` | Shared library — zero rendering deps, pure functions, integer math |
| `reachlock-client/` | Bevy game client — bridge layer, ECS systems, plugins |
| `reachlock-server/` | WebSocket server — Tokio + Axum, seed service, LLM proxy |
| `reachlock-cli/` | CLI tools — `gen`, `determinism`, `content` |
| `assets/ui/` | UI stylesheets — the game's theme, hot-reloadable with F5 |
| `reachlock-editor/` | Content editor — RON round-trip, schema validation, multi-entry saving |

## Development Workflow

1. **Branch:** `sprint-v2/sXX-short-name` cut from `testing`
2. **Work:** Implement the sprint brief's deliverable checklist
3. **Verify:** `make check` — fmt, clippy -D warnings, all tests, engine purity
4. **PR:** Open against `testing`, link the sprint brief, log anything you couldn't do
5. **CI must be green:** Cross-platform determinism gates are non-negotiable

## Iron Rules (from the sprint index)

1. **Core is pure.** `reachlock-core` gets zero rendering/IO deps. Generators are pure functions. A new dependency in core must be justified in the PR; `make check-purity` fails the build if core's dependency tree pulls in a rendering, async-runtime, or HTTP crate.
2. **No floats in gameplay values.** Fixed-point (`util::rng::Fixed`, 1/1024) or plain integers for anything that affects game state. Floats are for the bridge/render layer only.
3. **New generator or generator change ⇒ extend `core/src/determinism.rs`** and recapture goldens deliberately. If the manifest changes, say so in the commit message — a silent golden change is a bug.
4. **Wire shapes are pinned.** Network tags (`network/messages.rs`), contract JSON, and content schemas have tests that lock their serialized form. Changing one is a protocol revision: update the test AND note it.
5. **Every LLM call has a visible deliberation state.** No silent inference.
6. **Offline is first-class.** Every feature must work with no server. Online adds; it never replaces.
7. **Freeze contracts first.** Each brief lists types/schemas to define and test before building the slice. If two sprints share a type, the earlier wave owns it.
8. **A system nobody can reach is not done.** A feature with no path from the player to it, and content nothing references, both count as unfinished. `make check-content` fails the build on a content tree that does not resolve.

## Gotcha Ledger (from the index — don't re-learn these)

- winit 0.30.13 on Wayland: panics at `state.rs:694` with `NonZeroU32::new(self.size.width).unwrap()` when the compositor doesn't send a configure event before first render. Workaround: `WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11` in Makefile `run` target. Remove when bevy's winit dep bumps past 0.30.13.
- bevy 0.18: mesh types import from `bevy::mesh::`, not `bevy::render::mesh::`. `Timer::finished` is now `is_finished`. `RapierPhysicsPlugin::<()>` (unit generic, not `NoUserData`).
- Rust raw strings: `r#"…"#` dies on SVG/hex color literals containing `"#` — use `r##"…"##`.
- `contract::engine::Outcome` borrows the contract; own the verdict (clone what you need) before mutating the runtime that holds it.
- The workspace builds with `debug = false` (Bevy debuginfo target dirs run to many GB and this box's disk sits ~97% full). Don't flip it in a PR.
- `~/.cargo/bin` is not on PATH in fresh shells: `export PATH="$HOME/.cargo/bin:$PATH"`.
- Seeds are ≤ 2^53 (JSON float survival). `Seed::new` masks; keep it that way.
- Bevy query filters trip clippy `type_complexity`; `#[allow]` on the system fn is the accepted pattern.
- RON round-trip drops comments: `reachlock-editor/src/io.rs::write_ron` pretty-prints authored content files, but `ron` cannot preserve hand-written comments through deserialize→serialize. Never round-trip a commented file through the editor, and don't add comments expecting them to survive a save.
- Toolchain is pinned via `rust-toolchain.toml` (channel `1.96.0`, `rustfmt`+`clippy`). Bumping the channel is a separate commit that must NOT be bundled with unrelated changes, because the new rustfmt may reformat the whole tree — land the fmt pass as its own commit first, then the feature work.
- Branch discipline: each sprint/slice gets its own branch off `testing` (see the index's playbook). Don't pile unrelated slices onto one branch; keep PRs focused so `make check` stays green per-change.
- Multi-entry editors (soul/station/enemy/…) save each dirty entry to its own `entry.path` via `Editor::save_all`. Never make a single-entry `save(path)` collapse all loaded entries onto the tab path — that silently loses authored content in the other entries.
- RON is not JSON about aggregates. A fixed-size array `[u8; 3]` serializes as a **tuple** — `Some((176, 148, 92))`, not `Some([176, 148, 92])` — and a newtype struct like `VariationMask(u16)` needs its parens: `allowed_variations: (65535)`. Both fail at parse time with a message that names the struct, not the line's real problem.
- Authored content must be wrapped in a `ContentFile` envelope or the dispatch layer never sees it. `mods/reachlock/themes/calm_exploration.ron` sat as a bare `Theme(...)` and the one authored theme silently never reached the audio engine. `content check` reports these as UNPARSEABLE — a file in a content directory that parses as no known payload is skipped by every loader, which is worse than a file that errors.
- Career `conflicting_paths` are symmetric and validated at load.
- `Res<T>` in a system panics at *runtime* if nothing registered `T` — the
  compiler cannot see it, and with `debug-names` off the panic names neither
  the system nor the parameter. Thirteen resources shipped declared, read and
  never registered. `make check-resources` now catches them; use
  `Option<Res<T>>` when absence is a real state.
- A `spawn_*` system registered in `Update` must be idempotent. The onboarding
  overlay was not, and spawned a fresh full-screen panel every frame — hundreds
  of opaque layers over the game, and the frame rate with them.
- Full-screen overlays belong on the translucent `surface.scrim` class, not an
  opaque one, or they hide the scene they are annotating.
- fundsp's `Sequencer::push_relative(start, end, ...)` takes an **end time**,
  not a duration, and asserts the fades fit in `end - start`. Passing a
  duration makes every event after the first end before it begins, and the
  assert fires on the audio thread.
- Bevy removed `Res<ButtonInput<GamepadButton>>` and `Res<Axis<GamepadAxis>>`
  in 0.15 — gamepads are entities now. The types still exist, so the old code
  compiles and then fails parameter validation at runtime. Query `&Gamepad`.
- UI colors come from the stylesheet, never from code. Name a style class —
  `theme::text("row.value", …)` for new UI, `theme::fg` / `theme::surface` when
  migrating an existing widget — and let `assets/ui/phosphor.ron` decide the
  color. `make check-theme` fails on a literal `TextColor`/`BackgroundColor`/
  `BorderColor`. F5 reloads the stylesheet in-game.
- RON enum variants are snake_case (`species: human`, `asset_type: crew_package`),
  and a newtype payload variant needs its second paren: `payload: origin((…))`.
  Getting either wrong makes the file unparseable, which every loader silently
  skips. Adding `a -> b` without `b -> a` is a load-time failure, not a warning.

## Build & Test

```bash
cargo build                    # debug
cargo test                     # all tests
cargo clippy --workspace --all-targets -- -D warnings    # CI gate
cargo run -p reachlock-client  # fly the red polygon
make check                     # fmt + clippy + test + purity + content
reachlock content check mods/reachlock   # whole-tree reference integrity
git config core.hooksPath .githooks   # opt-in: run `make check` on every commit
```

## First Sprint to Reference

- **S04** (System Generator) — extends the existing generator modules with `generate_system()`
- **S05** (Item Generator) — adds item type hierarchy and procedural icons
- Check the index for exact dependencies before starting

## Reference Documents

- `docs/REACHLOCK-V2-SPEC.md` — full architecture
- `docs/sprints/00-INDEX.md` — fleet playbook, iron rules, gotcha ledger
- `docs/sprints/S*.md` — individual sprint briefs
- `README.md` — repo overview
