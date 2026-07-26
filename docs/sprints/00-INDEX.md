# ReachLock v2 — Sprint Index

The complete [v2 spec](../REACHLOCK-V2-SPEC.md) broken into fleet-distributable
sprints. Each sprint is one self-contained brief: outcome, deliverables,
acceptance gates, frozen contracts, non-goals, gotchas. No time estimates,
no line counts — milestones and outcomes only.

**Already done (not in any sprint):** workspace + full plugin stack;
cross-target determinism harness in CI (x86_64/aarch64/i686 bit-identical);
seed protocol + 53-bit seeds; contract engine + signed evaluation chains;
generators (hull/station/planet/music/ui/noise/palette); WS ledger server
with first-write-wins discovery, verify service, tier-gated LLM proxy stub;
CLI (`gen`, `determinism`); flyable client with offline deliberation UX.
See git log `fd93f71..048a14a` and the README.

## Waves

Sprints inside a wave are parallel-safe (disjoint files, frozen interfaces).
A sprint may start when its listed dependencies are merged.

| Wave | Sprint | Title | Depends on |
|---|---|---|---|
| 1 | S01 | Content pipeline & override system | — |
| 1 | S02 | Client networking (online mode) | — |
| 1 | S03 | Server persistence & auth (Postgres proven) | — |
| 1 | S04 | System generator (a whole star system from one seed) | — |
| 1 | S05 | Item generator (gear, icons, tiers) | — |
| 2 | S06 | Mode state machine & transitions | S04 |
| 2 | S07 | Landed slice (walk a station) | S01, S06 |
| 2 | S08 | On-Board slice (walk your ship) | S06 |
| 2 | S09 | Flight, jump gates & cryo transit | S04, S06 |
| 3 | S10 | Economy engine | S01 |
| 3 | S11 | Faction engine & reputation | S01 |
| 3 | S12 | Universe tick integration (online + offline parity) | S10, S11 |
| 4 | S13 | Soul system | S01 |
| 4 | S14 | Real LLM providers behind the proxy | S03 |
| 4 | S15 | LLM agency & failure model | S13, S14 |
| 4 | S16 | Dialogue & deliberation UX | S13, S14 |
| 5 | S17 | Ship editor — exterior | S05 |
| 5 | S18 | Ship editor — interior | S08 |
| 5 | S19 | Space combat | S05, S09 |
| 5 | S20 | Landed combat | S07 |
| 6 | S21 | Gate network & the procedural frontier | S04, S09 |
| 6 | S22 | Modding framework | S01 |
| 6 | S23 | MMO presence & coordination | S02, S03 |
| 7 | S25 | Content editor suite (standalone dev/modder GUI) | S01, S04, S05 |
| 7 | S26 | Server operations — observability, admin API, graceful degradation | S03, S23 |
| 7 | S27 | LLM cost & quota management | S14, S26 |
| 7 | S28 | Payments & subscriptions (Stripe) | S23, S26 |
| 7 | S29 | Voice chat (WebRTC, spatial audio, P2P signaling) | S23 |
| 7 | S30 | Agent tooling — CI gate, codex CLI, auto-generated context | — (standalone) |
| 7 | S31 | Game settings & preferences — keybinds, audio/video, accessibility | — (infrastructure) |

Phase-4 polish (economy balancing, audio pass, UI pass, beta) was deliberately
NOT pre-cut into sprints — these briefs get written against real systems once
the systems exist. S48 (Procedural Audio Engine) is the first sprint cut from
this phase. Colonization (spec §17) waits for a live MMO — cut it as its own
sprint after S23 ships.

### Wave 8 — LLM Gameplay

Sprints that make LLM behavior into gameplay *content* rather than gameplay
*support*. These are the mechanics that make ReachLock about LLMs.

| Sprint | Title | Depends on |
|---|---|---|
| S33 | Crew dynamics — concurrent crew deliberation & argument | S13, S15, S16 |
| S34 | Contract crafting — player expression through rule-based characters | S13, S15, S16 |
| S35 | Persistent relationship memory — characters that remember weeks | S13, S15, S16 |
| S36 | Procedural dilemma generator — situations designed for the LLM edge | S04, S15, S16 |
| S37 | Captain's log — LLM-written personal narrative from session traces | S15, S16, S33 |
| S38 | Deliberation theater — sequential group deliberation, player as audience | S15, S16, S33 |

### Wave 9 — Living Galaxy

The galaxy is alive. Ecosystems. Cultures. An economy of goods and upgrades.
Career paths. Piracy. Missions that read the universe state. Planets that
feel like worlds.

| Sprint | Title | Depends on |
|---|---|---|
| S39 | Ecosystem & life — procedural organisms, discovery catalog, event-driven change | S04, S05, S36 |
| S40 | Trope engine: templates — authored narrative beats with procedural fill | S04, S36 |
| S41 | Trope engine: scripted encounters — fully authored, multi-scene encounters | S40 |
| S42 | Unified career progression — military, trade, exploration, science, criminal | S11 |
| S43 | Piracy — ship capture, contraband, notoriety, bounty system, pirate havens | S11, S19, S20, S42 |
| S44 | Advanced economy: goods — luxury, cybernetics, production chains, investment | S10, S05 |
| S45 | Ship room upgrades — widgets, repurposing, power budget, tiered progression | S18, S05, S44 |
| S46 | Mission engine — context-aware mission generation from economy/politics/state | S10, S11, S40, S42 |
| S47 | Planet scale & culture — cities, language, customs, architecture, coherent cultures | S04, S11 |

### Wave 10 — Audio & Polish

The first Phase-4 sprint. These briefs are written against the real systems
as they exist after S47, not against design intent.

| Sprint | Title | Depends on |
|---|---|---|
| S48 | Procedural audio engine (fundsp) — real-time seeded music, theme riffing, authored overrides | S01, S05, S06, S09 |

### Wave 11 — Server Infrastructure

The server works for real. Postgres stores work, Redis caches sessions and
rate limits, and players register with passwords instead of dev tokens.

| Sprint | Title | Depends on |
|---|---|---|
| S49 | Postgres store wiring — REACHLOCK_DB selects Pg stores in AppState | S03 |
| S50 | Redis integration — session, rate-limit, presence | S49 |
| S51 | Real authentication — register/login/logout, bcrypt passwords, WS enforcement | S49, S50 |

### Wave 12 — Determinism Closure

Every generator has a golden test. Every content type has a real schema.

| Sprint | Title | Depends on |
|---|---|---|
| S52 | Generator golden entries — 13 generators get determinism manifest entries | — |
| S53 | Content schema closure — dedicated JSON schemas for all types | — |

### Wave 13 — Bug Fixes & Database Completion

Known bugs are fixed. The database schema matches the code.

| Sprint | Title | Depends on |
|---|---|---|
| S54 | Bug fixes (LLM metric, voice broadcast, faction directory cleanup) + migration 0003/0004 | S52, S53 |

### Wave 14 — Editor & CLI

Every content type has an editor. Authors can validate, preview, and publish.

| Sprint | Title | Depends on |
|---|---|---|
| S55 | Last 4 editors — dungeon, event, dialogue, recipe | S54 |
| S56 | CLI content commands — preview, publish | S55 |

### Wave 15 — Server Routes

Missing HTTP routes and WebSocket messages are wired.

| Sprint | Title | Depends on |
|---|---|---|
| S57 | Server route completion — POST /seed/discover, GET /content/system/{id}, player.jumped, player.disconnected | S56 |

### Wave 16 — Content Authoring

The content directory gets structure and real files. Faction profiles, the
7 Loup-Garou crew souls, storylines, and the first Predecessor dungeon.

| Sprint | Title | Depends on |
|---|---|---|
| S58 | Content scaffold files — directory structure + templates | S57 |
| S59 | 7 Loup-Garou crew souls — the canonical crew as authored .ron files | S58 |
| S60 | Storyline framework — faction profiles, 3 story arcs, first Predecessor dungeon | S59 |

### Wave 17 — Client Polish

Missing client systems are built: resource gathering, signature collector,
deliberation renderer. NPC voice synthesis is wired.

| Sprint | Title | Depends on |
|---|---|---|
| S61 | Missing client systems — resource_gathering, signature_collector, deliberation_renderer, fix library placeholder | S60 |
| S62 | Voice synthesis fix — replace no-op placeholder with real TTS thread | S61 |

## Fleet playbook (read before starting any sprint)

**Branching.** One branch per sprint: `sprint-v2/sXX-short-name`, cut from
`testing`. Merge back to `testing` via PR. Never touch `archive/v1/` — it is
read-only inspiration.

**Gates.** Your sprint is done when `make check` passes locally (fmt, clippy
`-D warnings`, all tests, engine purity) and CI is green — including the
cross-platform determinism gate. These are non-negotiable.

**Iron rules (spec §13, enforced by CI and review):**
1. **Core is pure.** `reachlock-core` gets zero rendering/IO deps. Generators
   are pure functions. If you need a new dependency in core, it must compile
   be justified in the PR, and `make check-purity` must stay green.
2. **No floats in gameplay values.** Fixed-point (`util::rng::Fixed`, 1/1024)
   or plain integers for anything that affects game state. Floats are for the
   bridge/render layer only.
3. **New generator or generator change ⇒ extend `core/src/determinism.rs`**
   and recapture goldens deliberately. If the manifest changes, say so in the
   commit message — a silent golden change is a bug.
4. **Wire shapes are pinned.** Network tags (`network/messages.rs`), contract
   JSON, and content schemas have tests that lock their serialized form.
   Changing one is a protocol revision: update the test AND note it.
5. **Every LLM call has a visible deliberation state.** No silent inference.
6. **Offline is first-class.** Every feature must work with no server. Online
   adds; it never replaces.
7. **Freeze contracts first.** Each brief lists types/schemas to define and
   test before building the slice — the v1 "Phase A" pattern. If two sprints
   share a type, the earlier wave owns it.
8. **A system nobody can reach is not done.** Every sprint's acceptance gates
   must include a *player-reachable* (or author-reachable, for editor work)
   path, named explicitly: which key, menu, or panel opens it. Five sprints
   — S36 dilemmas, S37 captain's log, S39 ecosystem events, S41 scripted
   encounters, S60 storylines — shipped complete, tested, golden-pinned core
   systems with **zero** client references. Ten editors shipped registered but
   absent from the content browser and `File > New`. A core module with tests
   and no surface is inventory, not a feature.

**Acceptance-gate template.** Every brief's gate section answers all four:

| Question | Example |
|---|---|
| Does `make check` pass? | fmt, clippy `--all-targets`, tests, engine purity |
| **How does a human reach it?** | "Dock → crew console → F2 installs the contract" |
| What test fails if someone deletes the wiring? | `consumer_coverage_test` |
| What did the manifest/wire shape change? | "determinism manifest v31 → v32: added …" |

The third column is the one that matters. Prose in a brief does not survive;
an enum-driven test does. Prefer a gate that iterates a `…::all()` over one
that compares against a hand-written list — a hardcoded list can only
re-confirm what someone already remembered to type.

**Gotcha ledger (hard-won, don't relearn):**
- RON is not JSON about aggregates. A fixed-size array `[u8; 3]` serializes as
  a **tuple** — `Some((176, 148, 92))`, not `Some([176, 148, 92])` — and a
  newtype struct like `VariationMask(u16)` needs its parens:
  `allowed_variations: (65535)`. Both fail at parse time with a message that
  names the struct rather than the line's real problem.
- Authored content must be wrapped in a `ContentFile` envelope or the dispatch
  layer never sees it. `themes/calm_exploration.ron` sat as a bare
  `Theme(...)` and the one authored theme never reached the audio engine.
  `reachlock content check` reports these as UNPARSEABLE: a file in a content
  directory that parses as no known payload is skipped by every loader, which
  is worse than one that errors.
- JSON schemas drift behind the Rust types and silently reject valid content.
  `soul.schema.json` had no `look` property for the whole of S76+;
  `career_path.schema.json` demanded a string `faction_id` when the type is
  `Option<String>`. Changing a content type means changing its schema.
- Career `conflicting_paths` are symmetric and validated at load — `a -> b`
  without `b -> a` is a load failure, not a warning.
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
- UI colors come from `assets/ui/*.ron`, never from code. Name a style class
  (`theme::text` for new UI, `theme::fg` / `theme::surface` when migrating an
  existing widget); `make check-theme` fails on a literal `TextColor` /
  `BackgroundColor` / `BorderColor` in the client. F5 reloads the stylesheet
  in-game, and a broken edit keeps the last good theme rather than blanking
  the UI.
- Bevy's built-in default font is a **FiraMono subset** with no box-drawing or
  geometric glyphs — `●◉○ ▸ ⚠ ↑↓ ←→` all render as tofu. The theme loads
  DejaVu Sans Mono from `assets/fonts/`; any text entity that misses the
  themed font falls back to the subset and starts showing boxes.
- RON enum variants are snake_case and newtype payload variants need a second
  paren — `payload: origin((…))`, `asset_type: crew_package`, `species: human`.
  Both mistakes make the file unparseable, and every loader skips silently
  what it cannot parse.
- A `spawn_*` system registered on `Startup` can never run again. Anything the
  player can leave and come back to belongs on `OnEnter(state)` with a matching
  `OnExit` teardown — the main menu was spawned at `Startup` and despawned by
  hand, so backing out of character creation left a dead grey window.
- bevy 0.18: mesh types import from `bevy::mesh::`, not `bevy::render::mesh::`.
  `Timer::finished` is now `is_finished`. `RapierPhysicsPlugin::<()>` (unit
  generic, not `NoUserData`).
- Rust raw strings: `r#"…"#` dies on SVG/hex color literals containing `"#` —
  use `r##"…"##`.
- `contract::engine::Outcome` borrows the contract; own the verdict (clone
  what you need) before mutating the runtime that holds it.
- The workspace builds with `debug = false` (Bevy debuginfo target dirs run
  to many GB and this box's disk sits ~97% full). Don't flip it in a PR.
- `~/.cargo/bin` is not on PATH in fresh shells: `export PATH="$HOME/.cargo/bin:$PATH"`.
- Seeds are ≤ 2^53 (JSON float survival). `Seed::new` masks; keep it that way.
- Bevy query filters trip clippy `type_complexity`; `#[allow]` on the system
  fn is the accepted pattern.
- S31 (settings): `KeyCode` doesn't derive serde natively. Store keybinds as
  strings via Bevy's `KeyCode`→`&str` conversion; deserialize via lookup.
  Never hardcode another key literal — use `settings.key(InputAction::Foo)`.

**Handoff etiquette.** Read your brief top to bottom, then read the spec
sections it cites, then the files it lists — in that order. Deliver exactly
the checklist; log anything you couldn't do in the PR description. Surprises
belong in the PR, not in silent scope changes.
