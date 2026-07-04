# Sprint 02 — The Living Slice

> **Handoff brief for a long-horizon Fable agent.** Self-contained: assume you
> start cold. Read this whole file, then the files under "Ground truth" before
> writing any code. Everything here is executable design — close the open
> contracts, build the M4 slice, and prove it with a stranger's hands.

## Mission

Close **M4 — "The slice."** The full north-star sentence, start to finish,
playable by a stranger. Mining (G6) + combat (G7) + docking (G5) + M3
("Tib remembers").

To do that honestly, this sprint also closes the one new contract the spec
needs and Sprint 01 deferred: **C9 — Save↔Vault Binding**, the wire contract
that ties the save file (C4) to the Ragamuffin vault (C3) under a single
isolation rule that holds in both local and MMO topologies.

### North-star vertical slice (the acceptance test for the whole sprint)

> *A stranger downloads the build, takes off from Sorrow Station, mines an
> asteroid, gets jumped by a pirate, fights or runs, and either docks back
> or dies. If they survive and dock back, Tib comments on the fight. If
> they quit and relaunch, Tib still remembers — and their other save's
> Tib does not.*

The first sentence is M4 as defined in the spec. The second is M3, already
verified 2026-07-02. The third — "their other save's Tib does not" — is the
new thing this sprint has to make true, and it's why C9 is in scope.

## Why this is the right sprint

Three unknowns remain after Sprint 01:

1. **Is the slice actually playable by a stranger**, or only by us? A demo
   that "works" in the build author's hands is the most common false-positive
   in this kind of work. The sprint has to falsify the claim.
2. **Does Tib's memory leak across saves?** M3 verifies the *intended*
   behaviour (memory persists across restart under the same `life_id`).
   M3 does not verify the *unintended* behaviour is absent (memory from a
   different save file does not bleed in). C9 makes the absence a contract,
   not a hope.
3. **Does the cross-repo fleet stay green under a real release tag?** The
   contracts are frozen, but the build, the conformance suite, and the
   architecture guard have not been validated end-to-end against a tagged
   slice. They need to be, or M4 is just a Sprint 01 demo with a name change.

The fit is already in the sprint shape. Sprint 01's `serialize → fan out →
re-serialize` rhythm still works; the build fan-out is narrower (M4 is
four workstreams, not five) and the integration is sharper (one milestone,
one gate).

---

## Ground truth — read these first

Inherited from Sprint 01 — same files, same authority:

- `reachlock/docs/ARCHITECTURE.md` — the three-ring boundary and what the guard enforces.
- `reachlock/godot/framework/README.md` — the framework contracts already live and how they're authored.
- `reachlock/docs/SPRINT-01-the-living-crew.md` — the eight contracts (C1–C8) this sprint builds on. **Do not modify a frozen contract; add a profile, not a parallel one.**
- `reachlock/docs/demos/M3-tib-remembers.md` — the live M3 verification (2026-07-02). This is the proof M3 holds; Sprint 02's job is to make sure M3 still holds *under C9*.
- `reachlock/docs/UNIVERSE-TICK.md` — the universe-tick design. Sprint 02 does not embed it in the SP engine; that is deferred to Sprint 03.
- `pan/pan-core/src/schema.rs` — the settled Pan vocabulary.
- `ragamuffin/SPEC.md` and `ragamuffin/internal/server/*.go` — the real Ragamuffin surface.

New this sprint — required reading:

- `chezgoulet/pan#73` — the Wasm plugind PR that lands the SDK and ABI. The body of C9 references its capability taxonomy.
- `chezgoulet/ragamuffin#732` — "v1.1 Spec: Vault Access Control + Zero-Knowledge Storage." C9 is the contract that implements this spec.
- `chezgoulet/reachlock#24` — the wave-1 exit test "complete the Duskway run." This is the M4 gate's automated half.

## Operating principles (guardrails — violating these is a defect)

Inherited from Sprint 01 (verbatim, still binding):

1. **Contracts first, and freeze them.** A contract is "frozen" when it has a schema, a doc, a conformance test, and a version. Do not build against an unfrozen contract.
2. **The game binds to wire contracts, never to code.** Local socket + JSON to Pan; REST/JSON to Ragamuffin. Three repos evolve in tandem because nothing imports across the seam.
3. **Adopt Pan's vocabulary; add a *profile*, not new types.**
4. **Ragamuffin is additive-only.** It has production users. Anything the game needs becomes a general roadmap feature with a contract test *in Ragamuffin's CI*, never a fork.
5. **Authored vs. runtime state.** Mod files are birth-state templates. Runtime soul state lives in saves (SP) / server (MMO). Never persist runtime state into mod files.
6. **Strict in CI, lenient at runtime.** Schema violations fail the build; loaders log and continue.
7. **Keep it green, keep it playable.** `make check` in each repo, headless Godot import + boot, architecture guard must pass at every milestone. Expand the guard's scope as new engine dirs appear.
8. **Every task ships with a Definition of Done and a verify command.** If you can't verify it, it isn't done.

New this sprint:

9. **`life_id` is the unit of soul isolation. The same contract holds in local and MMO.** A save/vault path that only works in one mode is a defect. If a code path assumes "we're local" or "we're MMO" and branches on it, that branch is a contract violation to fix or a `# Sprint 03` tag to remove.
10. **A merged PR is not a closed issue.** Pre-sprint hygiene: the repos have issue-tracking drift. Sprint 02 starts with that drift closed; it does not paper over it.

## How to run this with a fleet (optimize the long horizon)

The shape is the same as Sprint 01, scaled to M4's smaller surface:

- **Phase A (contracts)** — C9, do yourself, serially. One new contract.
- **Phase B (build)** — five workstreams (P / R / G / S / Content) that are independent *once C9 is frozen*. Same branch convention: `sprint02/pan`, `sprint02/raga`, `sprint02/godot`, `sprint02/server`, `sprint02/content`.
- **Phase C (integration)** — M4 is a single barrier. The barrier has two halves: an automated half (wave-1 exit test #24, C9 contract test, headless boot, architecture guard) and a human half (the stranger-playtest synthesis). Both must pass for the sprint to close.

Sprint length: **7 weeks**. Phonon is **out of scope** (deferred to Sprint 03+). The four cross-repo blockers for this sprint are: pan plugind landing (#60, #62), pan wave-1 exit test (#17), reachlock wave-1 exit test (#24), and the Ragamuffin observability + Qdrant family (#747, #781–#785). Resolve or accept each before sprint start.

---

# Pre-sprint: close the drift (do this first, do it once)

Before any Sprint 02 work begins, close the issues that are already done.
This is a `chore:` batch — one PR per repo, no design content, no scope
expansion. The drift list, with evidence:

| Repo | Issue | Title | Evidence | Close with |
|------|------:|-------|----------|------------|
| pan | #2 | (Wave 0) type-state pipeline | PR #67 merged | `completed in #67` |
| pan | #3 | (Wave 0) loop | PR #67 merged | `completed in #67` |
| pan | #4 | (Wave 0) events | PR #67 merged | `completed in #67` |
| pan | #5 | (Wave 0) providers | PR #67 merged | `completed in #67` |
| pan | #7 | (Wave 0) compile-fail guards | PR #67 merged | `completed in #67` |
| pan | #66 | CI pipeline: staged wave-gated jobs | PR #69 merged | `completed in #69` |
| pan | #68 | Harden event stream shutdown | PR #71 merged | `completed in #71` |
| phonon | #215 | Missing onDeactivate() in MacSystem7/EInk/Synthwave packs | PR #218 merged | `fixed in #218` |
| phonon | #217 | SynthwavePack uses System.currentTimeMillis() | PR #218 merged | `fixed in #218` |
| ragamuffin | #783 | memory provider tool injection gated on 'memory' in enabled_toolsets | duplicate of #781–#785 cluster | `duplicate of #781` |

**Verify:** after the batch lands, the four repos show 150 open issues, not 160. The drift-closure PR is the only sprint-2 PR that has no design content; everything else does.

---

# PHASE A — CONTRACTS (serial, blocking, priceless)

### C9 — Save↔Vault Binding (the new contract)

- **Purpose:** Define how the save file (C4) and the Ragamuffin vault (C3) are bound, with `life_id` isolation enforced by the contract, not by the deployment topology.
- **Defines:**
  - `life_id` as a first-class, opaque, versioned string in the save schema (C4 extension) and in every vault record.
  - The isolation rule: a vault lookup scoped to one `life_id` must never return a record tagged with a different `life_id`. This holds in local mode and in MMO mode, by the same code path, with no mode-specific branching at the contract layer.
  - Two key-derivation strategies, one per topology:
    - **Local mode** — vault file is encrypted at rest with a key derived from the player's chosen passphrase (Argon2id, parameters pinned in the contract). The `life_id` is the save-file UUID. The bundled Ragamuffin binary speaks to a local vault file.
    - **MMO mode** — vault is server-managed (issue #53, wave-5, out of scope for Sprint 02). The `life_id` is the account ID. The MMO server runs a Ragamuffin instance on behalf of all clients. Server-managed vaults are **not** zero-knowledge — the MMO server can read its own database; this is a known property of MMO mode, not a bug. The contract states this so future contributors don't try to "fix" it.
  - Memory-origin tags: every vault record carries an `origin` enum of `planted | organic | scripted`. Tib-planted memory and organic player-earned memory are *not* isolated from each other under the same `life_id` — they share the vault. They are *isolated from* any other `life_id`'s vault, planted or otherwise.
  - Migration: an existing Sprint 01 M3 save has a single `life_id` (the save UUID) and a vault that was tagged implicitly. The C9 contract defines the migration: a one-time re-tag using the existing save UUID as the `life_id`, no data loss, and an upgrade marker in the save header.
- **DoD:**
  - Spec doc in `reachlock/godot/framework/protocol/SAVE-VAULT-BINDING.md`.
  - Schema update to `godot/framework/schemas/save.schema.json` (add `life_id`, `vault_origin_tag_version`).
  - Conformance test that runs in **both** reachlock CI and Ragamuffin CI: build a save with `life_id=A`, write a vault record tagged with `life_id=A`, write a second record tagged with `life_id=B`, then assert that the A-scoped lookup returns only A's record. Repeat in MMO topology (using a stub Ragamuffin instance) and assert the same.
  - An M3 round-trip test under the new schema: build a save, run the gratitude-mutation demo, quit, relaunch, verify Tib still references the volley, verify a *second* save's Tib does not.
  - `framework_version` bumped in `reachlock/godot/framework/README.md`. Mirror issue opened in pan and Ragamuffin referencing the spec doc URL.

**Why this is irreversible:** the moment two saves share a process and we don't isolate, we have a data-leak bug. The contract is the firewall.

---

# PHASE B — BUILD (parallel once C9 is frozen)

The five workstreams inherit from Sprint 01. Each item below is a
`P-M4-*` / `R-M4-*` / `G-M4-*` / `S-M4-*` / `CT-M4-*` work-package
with its own DoD and verify command. Cross-reference with the issue
numbers in the **Sprint 02 issue list** at the end of this document.

## Workstream P — Pan (the mind) · branch `sprint02/pan`

- **P-M4-1 — Land the Wasm plugind.** Merge `pan#73`, get the SDK templates
  green, document the ABI in the existing `pan-core` README. *Verify:*
  `make check` in `pan/` is green; the conformance test in PR #73 passes;
  the example plugin loads in `pan serve` and round-trips a capability
  invocation. *Issue:* `pan#60`, `pan#62`.
- **P-M4-2 — Health/observability.** Implement `pan-core`'s `/health`
  endpoint, with uptime and per-plugin liveness. *Verify:* `curl /health`
  on a running `pan serve` returns 200 with the expected fields; CI
  exercises the endpoint under a plugin crash. *Issue:* `pan#58`.
- **P-M4-3 — Config.** Land `pan#70` (TOML config with imports and env
  override). *Verify:* a pan config with an `imports` block and a
  `PAN_LLM_BACKEND=anthropic` override resolves to the right backend
  without code changes. *Issue:* `pan#56`.
- **P-M4-4 — Wave-1 exit test.** "Pan CLI agent works end-to-end." A CLI
  channel + a rules provider + a capability invocation, all under the
  wave-1 acceptance bar. *Verify:* `pan wave1-exit` returns 0. *Issue:*
  `pan#17`.
- **P-M4-5 — ADR for `provider.llm`.** Land the architectural decision
  record that fixes `provider.llm` as one plugin with pluggable
  `LLMBackend` adapters (BYOK + local). *Verify:* ADR merged; the LLM
  provider is loaded as a plugin in `pan serve`, not a hard-coded
  import. *Issue:* `pan#50`.

## Workstream R — Ragamuffin (the memory) · branch `sprint02/raga` · ADDITIVE ONLY

- **R-M4-1 — Qdrant write contention fix.** Resolve `ragamuffin#747`:
  storage timeouts and "no peer responded" under high-ingest load.
  *Verify:* a benchmark run at 4× the previous high-ingest rate completes
  without timeouts; the regression test for this scenario is in Ragamuffin
  CI. *Issue:* `ragamuffin#747`.
- **R-M4-2 — Plugin observability cluster.** Close `ragamuffin#781`
  through `ragamuffin#785`: a status tool, refresh-signal emission,
  `is_available()` reading config not just env, no silent failures on
  tool injection, gated tool-injection with guardrails. *Verify:* the
  five issues are individually closed with PRs that each add a
  regression test; the M3 demo (`./scripts/dev_stack.sh && make godot`)
  surfaces a `ragamuffin_status` line in the log. *Issues:*
  `ragamuffin#781`, `#782`, `#783` (will already be closed as a duplicate
  in pre-sprint), `#784`, `#785`.
- **R-M4-3 — C9 contract test in Ragamuffin CI.** Extend the Sprint 01 R4
  test with the C9 contract: per-`life_id` isolation, two key-derivation
  strategies (local pass; MMO tested with a stub server), origin-tag
  invariants. *Verify:* the conformance test is in `ragamuffin/.github/`
  CI and runs on every PR; it fails on any future change that would
  cross-contaminate `life_id`s. *Issue:* `ragamuffin#732` (the spec this
  implements).
- **R-M4-4 — 127.0.0.1 by default in `dev_stack.sh`.** Stop binding the
  dev stack to `0.0.0.0`. *Verify:* `./scripts/dev_stack.sh` defaults to
  `127.0.0.1`; an opt-in flag (`REACHLOCK_DEV_BIND=0.0.0.0`) restores
  the wide bind; a regression test asserts the default. *Note:* this
  addresses one of the four cheapest items in the letter Claude Fable
  sent; the others are folded into other work-packages above.

## Workstream G — Godot engine (the host) · branch `sprint02/godot`

- **G-M4-1 — G5 docking + mode transition.** Verify the real space→landed
  flow at Sorrow Station from Sprint 01 still works, polish the camera
  and the docked-UI affordances, and add a regression test that drives
  the sequence headlessly. *Verify:* headless test `dock_round_trip`
  passes: spawn at Sorrow, take off, re-approach, dock, land, exit. The
  path is one continuous camera + mode change, not a state swap.
  *Issue:* `reachlock#25` (Station interior — already exists) and the
  underlying docking plumbing.
- **G-M4-2 — G6 mining loop.** Target an asteroid, fire the mining
  hardpoint, watch cargo rise, sell at Sorrow for credits, see the
  economy good's price update. *Verify:* headless test `mine_and_sell`
  passes: target → extract → cargo count rises by 1 → sell → credits
  rise by `price_at_station`. *Note:* mining is a partial feature of
  `reachlock#23` (mission system: the Duskway run); G-M4-2 is the slice
  that takes the existing space-slice mine code and tightens it into a
  loop the stranger can complete.
- **G-M4-3 — G7 space combat slice.** Hardpoint weapons fire, one pirate
  with basic AI, subsystem/health damage, the "get jumped" beat. The
  pirate can be destroyed and can damage you. *Verify:* headless test
  `pirate_encounter` passes: spawn pirate, take a hit, return fire,
  pirate hull → 0, survivor state restored. *Issue:* `reachlock#14`,
  `reachlock#15`, `reachlock#16`, `reachlock#17`.
- **G-M4-4 — M3 verification under C9.** Re-run the M3 demo
  (`docs/demos/M3-tib-remembers.md`) under the new save↔vault contract.
  Tib's memory across restart must work *and* a second save's Tib must
  not see it. *Verify:* a scripted save/reload cycle, plus a
  `second_save_isolation` test that asserts vault B does not return
  vault A's records.
- **G-M4-5 — Mod loader on the slice.** Install a sample mod, verify it
  mounts, verify a broken mod is a warning, not a crash. *Verify:*
  `mod_loader_smoke` test: load the bundled `reachlock` mod, then load
  a deliberately-broken mod, assert the second is reported and the
  game continues. *Issue:* `reachlock#8`.
- **G-M4-6 — Wave-0 exit test polish.** Re-run the existing
  `reachlock#11` exit test, fix any drift, expand its scope to include
  the mod loader and the C9 save schema. *Verify:* `make check` runs
  the wave-0 exit test as a single target and it passes.

## Workstream S — Go server (the sim) · branch `sprint02/server`

- **S-M4-1 — Faction simulator tick.** Replace the Sprint 01 stub.
  *Verify:* `make check` in `server/`; a unit test that runs a tick
  with a known seed and asserts standings advance deterministically.
  *Issue:* Sprint 01 S1 (carried forward; not yet an open issue
  number, file in `server/internal/factions/`).
- **S-M4-2 — Economy engine.** Supply/demand pricing. *Verify:* a unit
  test that floods a good and asserts its price falls. *Issue:* Sprint
  01 S2 (carried forward).
- **S-M4-3 — Universe tick determinism.** Implement C8 (universe tick)
  and a determinism test: same seed + inputs → same tick outputs.
  *Verify:* `server/internal/universe/` has a `Determinism` test that
  hashes the output of a 1000-tick run with a fixed seed and compares
  to a pinned golden hash. *Note:* the design is C8-equal between SP
  and MMO; the *embed* of the tick in the SP engine is Sprint 03 work.
- **S-M4-4 — Architecture guard expanded.** The guard from Sprint 01
  must now cover `server/`, not just `godot/`. *Verify:*
  `scripts/check_architecture.py` runs in CI and passes against HEAD
  with the new scope; a known-bad PR (server imports godot) fails
  the guard.

## Workstream Content · branch `sprint02/content`

- **CT-M4-1 — Verify Sprint 01 content is in place.** Tib soul v1, Sorrow
  Station, Tib dialogue tree, one pirate NPC + ship, asteroid + economy
  good. *Verify:* `make validate` green; all required files conform to
  their schemas; the guard is clean.
- **CT-M4-2 — M4 release mod.** Bundle the slice content as a single
  release-tagged mod the stranger-playtest build installs by default.
  *Verify:* a fresh build with no `mods/` directory still has a working
  slice; a build with the release mod layered on top is identical.
- **CT-M4-3 — Stranger brief.** A one-page printable brief, plain
  language, no jargon. The exact text lives in the M4 stranger-playtest
  protocol below. *Verify:* the brief file is in
  `reachlock/docs/demos/M4-stranger-brief.md` and is referenced from
  the build's first-run screen.

---

# PHASE C — INTEGRATION (M4 is the only milestone)

### M4 — "The slice."

The single integration barrier. The sprint closes when both halves pass:

- **Automated half:**
  - `make check` green in reachlock, server, pan, and Ragamuffin.
  - Headless Godot import + boot clean against the M4 release tag.
  - Architecture guard green with expanded scope (now covers `server/`).
  - The wave-1 exit test (`reachlock#24`) is green.
  - The C9 contract test is green in both reachlock CI and Ragamuffin CI.
  - The M3 round-trip test under C9 is green.
  - The mod loader smoke test is green.
- **Human half:** the **M4 stranger-playtest protocol** has run, the
  synthesis has been written, and the synthesis reports **"M4 ready"** per
  the pass criteria below.

---

## M4 Stranger-Playtest Protocol

**The M4 DoD says "the north-star slice is playable end-to-end by someone
who didn't build it." This protocol is the falsifiable form of that sentence.**

### Recruitment

- **5 strangers**, recruited from outside the four-repo contributor set.
- Definition of "stranger": has not opened a PR, issue, or commit in `chezgoulet/{reachlock,pan,ragamuffin,phonon}` in the last 90 days, and is not a regular playtester of prior waves.
- Mix: at least 1 has played a space game before, at least 1 has not. At least 1 is non-technical. No two from the same household.
- Recruited by direct ask, not by social-media call.

### Setup

Each stranger gets, on a USB stick or download link:

- A **fresh build** of reachlock at the M4 release tag, with the C9 contract compiled in. Local mode only. Ragamuffin bundled.
- A **one-page brief** (`docs/demos/M4-stranger-brief.md`, written by Content in CT-M4-3):
  > *"This is a slice of a space game. You're docked at Sorrow Station. You can take missions, mine asteroids, and there are pirates in this system. Survive, dock back, or die trying. The session ends at 30 minutes or when you dock back at Sorrow Station — whichever comes first. There are no wrong answers. We are not watching your skill; we are watching the game."*
- **No hints about controls** unless they ask. If they ask, the answer is "the menus are your friend."
- The stranger runs the build on their own machine, ideally, but a sandboxed machine is fine if they prefer.
- **Recording**: screen capture if consented, otherwise observer notes. No audio required. No compensation is offered to testers.

### Capture sheet (one per stranger)

A one-page form, filled by the observer (or the stranger themselves, if they prefer):

| Field | Type |
|---|---|
| Did the game launch without help? | Y/N, time to first frame |
| Did the player find the controls? | Y/N, free text: how |
| Did the player take off from Sorrow Station? | Y/N, time, what triggered it |
| Did the player successfully dock back at Sorrow Station? | Y/N, time, what went wrong if not |
| Did the player mine an asteroid? | Y/N, time, what they tried |
| Did the player get jumped by the pirate? | Y/N, time, what they did |
| How did the session end? | dock back / died / quit / 30-min timeout |
| Three things that worked | free text |
| Three things that broke or felt wrong | free text |
| "Would you keep playing?" | 1–5 scale, one sentence why |

### Pass criteria (quorum, not unanimity)

- **Launch:** 5/5 launch without help. Any launch failure is a P0.
- **Mined:** ≥4/5 successfully mine at least one asteroid. (This is the M4 spec's central G6 test.)
- **Jumped:** ≥4/5 get jumped by the pirate at least once. (G7 spawn must trigger reliably.)
- **Dock back:** ≥3/5 successfully dock back at Sorrow Station. (G5 must work end-to-end; we allow for some strangers dying or ragequitting before they figure out docking.)
- **Survive-or-30-min:** ≥3/5 reach 30 minutes or successfully dock back.
- **"Keep playing":** median ≥3 on the 1–5 scale.
- **Blocker rule:** any blocker reported by ≥2 strangers is a P1 fix before M4 closes. Blockers reported by 1 stranger are noted but not gating.

### What the protocol does NOT test

- MMO mode. Local only — MMO is wave-5 / `reachlock#53` work.
- Save/load across multiple restarts. That's an automated test against the C9 contract, not a human test.
- Mod loader. That's an automated conformance test (load a sample mod, verify it mounts).
- Long-session soul memory. The 30-minute window doesn't exercise M3; that's covered by the automated M3 verification.

### Outputs

- One capture sheet per stranger (5 sheets, one page each).
- A **one-page synthesis** with: the 5 pass-criteria numbers, the top-3 cross-stranger blockers, the top-3 cross-stranger positive surprises, and a "M4 ready / M4 needs polish pass / M4 needs re-scope" verdict.
- The synthesis becomes the M4 close-out artifact and the input to the post-M4 retro that scopes Sprint 03.

### When in the sprint

- Recruit strangers in week 4 so they have the build by week 5.
- The protocol runs at **week 6 of 7**, after M4 plumbing is "feature complete" and after C9 has been merged. The week-7 polish pass is informed by the synthesis.
- If the synthesis verdict is "M4 needs re-scope," the sprint slips and the post-M4 retro happens at week 8.

---

## Definition of done for the sprint

- The **M4 stranger-playtest synthesis** reports "M4 ready" against the pass criteria above.
- The **C9 — Save↔Vault Binding** contract is frozen, versioned, documented, conformance-tested, and the conformance test runs in both reachlock CI and Ragamuffin CI.
- All eight Sprint 01 contracts (C1–C8) are still frozen and still pass conformance.
- `make check` green across reachlock, server, pan, and Ragamuffin.
- Headless Godot import + boot clean against the M4 release tag.
- Architecture guard green with expanded scope (covers `godot/`, `godot/framework/`, and `server/`).
- Soul runtime state persists across a restart in local mode, with `life_id` isolation verified (M3 round-trip + `second_save_isolation` test both green).
- The pre-sprint drift-closure list is closed (10 issues across pan, phonon, ragamuffin — see the table at the top of this document).
- The cross-stranger blockers flagged P1 by the synthesis are fixed and re-verified.
- The M4 release mod exists, the M4 stranger brief exists, and both are referenced from the build's first-run path.

## Dependency map (what blocks what)

```
C9 ─┬─ R-M4-3 (contract test in raga CI)
    ├─ G-M4-4 (M3 re-verification under C9)
    └─ second_save_isolation test
P-M4-1 (pan#60, #62) ─┬─ P-M4-4 (pan#17 wave-1 exit)
                     └─ unblocks the entire pan plugin roadmap (Sprint 03+)
P-M4-2 (pan#58) ──────── P-M4-4
P-M4-3 (pan#56) ──────── dev_stack / release mod
R-M4-1 (rag#747) ─────── R-M4-3
R-M4-2 (rag#781–#785) ── R-M4-3
G-M4-1 (G5 dock) ──────── M4
G-M4-2 (G6 mine) ──────── M4
G-M4-3 (G7 combat) ────── M4
G-M4-4 (M3 under C9) ──── M4
G-M4-5 (mod loader) ───── M4
G-M4-6 (wave-0 polish) ── M4
S-M4-1 (faction tick) ─── M4
S-M4-2 (economy) ──────── M4
S-M4-3 (universe-tick determinism) ── M4 (design only; embed is Sprint 03)
S-M4-4 (guard expanded) ── M4
CT-M4-2 (release mod) ──── M4
CT-M4-3 (stranger brief) ── stranger-playtest
M4 (automated half) ───── stranger-playtest (human half) ── sprint close
```

## Explicitly OUT of scope (resist the pull)

- **Universe-tick embed in the SP engine.** C8 is *designed* to be the same in SP and MMO; the SP embed itself is a Sprint 03 candidate. This sprint builds S-M4-3 (the determinism test) but does not wire the tick into `godot/scripts/`.
- **MMO server.** Out of scope; the MMO server scaffold is `reachlock#53` (wave-5). The C9 contract *names* MMO mode and pins its key-derivation strategy as a known property, but the MMO-mode implementation is not in this sprint.
- **Phonon sprint-c auth hardening (`phonon#240`–`#246`).** Deferred to Sprint 03+. The pre-sprint drift-closure still applies to `phonon#215` and `phonon#217`; the rest of the phonon backlog is untouched this sprint.
- **All wave-2+ reachlock content.** Faction engine, full storyline arcs, multiple endings, ship customization catalogue, Steam prep, accessibility, the MMO server scaffold, the seven crew soul files, all five faction definitions, the Veil signal storyline, the Duskway runs as a content tree (not as the wave-1 exit test), economic balancing. None of these are M4.
- **M5/M6 design work.** The spec ends at M4. Design past M4 belongs in a post-M4 retro, not in Sprint 02's input.
- **The 3K-line review pass** Claude Fable flagged. Important, but a recurring habit, not a sprint deliverable.
- **Anything in pan past the Wasm plugind landing.** `channel.http`, `gov.policy`, `cap.mcp`, all the sprint-2+ plugins in pan: not this sprint. They unblock later.

---

## Sprint 02 issue list (verified, 2026-07-02)

The audit reconciled the four repos against the M4 scope. Numbers below
are GitHub issue numbers; the in-sprint set is the *only* set this
sprint is expected to advance. Anything not listed is either closed in
pre-sprint, deferred to Sprint 03+, or wave-2+ in reachlock and out of
M4's path.

### reachlock (14 in sprint, of 69 open)
- #1, #6, #8, #10, #11, #12, #13, #14, #15, #16, #17, #19, #23, **#24 (the M4 gate)**
- 55 issues deferred to Sprint 03+ (wave-2 through wave-6)

### pan (7 in sprint, of 60 open)
- **#60, #62 (the pan gate; in PR #73)**
- #17 (pan wave-1 exit test), #50 (ADR: provider.llm), #56 (config), #58 (health/observability), #75 (admission trait)
- 7 issues closed in pre-sprint drift batch (#2, #3, #4, #5, #7, #66, #68)
- 46 issues deferred to Sprint 03+ (sprint-2 and later)

### Ragamuffin (6 in sprint, of 13 open)
- **#732 (the C9 spec home)**
- **#747 (the rag infra gate)**
- **#781, #782, #784, #785 (the rag observability gate; #783 closed as duplicate in pre-sprint)**
- 6 issues deferred (#664, #690, #691, #706, #707, #760)

### phonon (0 in sprint, of 18 open)
- 2 issues closed in pre-sprint drift batch (#215, #217)
- 16 issues deferred to Sprint 03+ (sprint-b, sprint-c, sprint-d, planning)

**Totals:** 10 pre-sprint closes, 27 in-sprint issues, 123 deferred to Sprint 03+. 10 + 27 + 123 = 160 (matches `gh issue list --state open` at 2026-07-02).

---

## After the sprint

The post-M4 retro produces three artefacts:

1. **The M4 close-out note** in `docs/demos/M4-the-slice.md` (mirrors the format of `M3-tib-remembers.md`).
2. **The Sprint 03 candidate list**, which is the 123 deferred issues plus the universe-tick SP embed, plus any new scope the retro surfaces.
3. **A retro on the *process*** — serialize-fan-out-reserialize held; the new piece this sprint was the human-half gate, and the question for Sprint 03 is whether the same shape (automated half + human half per milestone) is worth keeping or whether Sprint 03 has a different integration shape.
