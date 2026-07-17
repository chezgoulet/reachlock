# ReachLock v2 — Sprint Index

The complete [v2 spec](../REACHLOCK-V2-SPEC.md) broken into fleet-distributable
sprints. Each sprint is one self-contained brief: outcome, deliverables,
acceptance gates, frozen contracts, non-goals, gotchas. No time estimates,
no line counts — milestones and outcomes only.

**Already done (not in any sprint):** workspace + full plugin stack on WASM;
cross-target determinism harness in CI (x86_64/aarch64/wasm32 bit-identical);
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
| 6 | S24 | Web distribution & release pipeline | — (any time) |
| 7 | S25 | Content editor suite (standalone dev/modder GUI) | S01, S04, S05 |
| 7 | S26 | Server operations — observability, admin API, graceful degradation | S03, S23 |
| 7 | S27 | LLM cost & quota management | S14, S26 |
| 7 | S28 | Payments & subscriptions (Stripe) | S23, S26 |
| 7 | S29 | Voice chat (WebRTC, spatial audio, P2P signaling) | S23 |
| 7 | S30 | Agent tooling — CI gate, codex CLI, auto-generated context | — (standalone) |

Phase-4 polish (economy balancing, audio pass, UI pass, beta) is deliberately
NOT pre-cut into sprints: those briefs get written against real systems once
the systems exist. Colonization (spec §17) waits for a live MMO — cut it as
its own sprint after S23 ships.

## Fleet playbook (read before starting any sprint)

**Branching.** One branch per sprint: `sprint-v2/sXX-short-name`, cut from
`testing`. Merge back to `testing` via PR. Never touch `archive/v1/` — it is
read-only inspiration.

**Gates.** Your sprint is done when `make check` passes locally (fmt, clippy
`-D warnings`, all tests, WASM build) and CI is green — including the
cross-platform determinism gate. These are non-negotiable.

**Iron rules (spec §13, enforced by CI and review):**
1. **Core is pure.** `reachlock-core` gets zero rendering/IO deps. Generators
   are pure functions. If you need a new dependency in core, it must compile
   to wasm32 and be justified in the PR.
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

**Gotcha ledger (hard-won, don't relearn):**
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
- S25 (editor suite) is native-only — exempt from `make check` WASM build.
  `bevy_egui` + `wgpu` render targets don't compile to wasm32.

**Handoff etiquette.** Read your brief top to bottom, then read the spec
sections it cites, then the files it lists — in that order. Deliver exactly
the checklist; log anything you couldn't do in the PR description. Surprises
belong in the PR, not in silent scope changes.
