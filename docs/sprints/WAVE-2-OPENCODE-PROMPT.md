# Wave 2 execution — opencode handoff prompt

Copy everything in the fenced block below into opencode as the opening task.
It is self-contained; opencode should read the referenced brief files itself
rather than have them pasted in.

---

```
You are executing Wave 2 of the ReachLock v2 rebuild — a Rust + Bevy 0.18
game workspace. Work autonomously, keep the tree green, and open one PR per
the integration plan below. Ask me only if a decision is genuinely
ambiguous and not resolvable from the briefs.

## Repository & environment
- Repo: /home/c/git/chezgoulet/reachlock  (git, GitHub remote `origin`,
  default branch `main`, integration branch `testing`).
- Crates: reachlock-core (pure, wasm-safe, NO rendering/IO deps),
  reachlock-client (Bevy 0.18), reachlock-server (Axum), reachlock-cli.
- Every shell: `export PATH="$HOME/.cargo/bin:$PATH"` (cargo is not on a
  fresh PATH) and `export CARGO_TARGET_DIR=/home/c/git/chezgoulet/reachlock/target`
  (one shared target dir — the box's disk is tight; do NOT create per-worktree
  target dirs). If you run parallel builds, cargo's target lock serializes
  them; that's expected, never work around it.
- The 24 sprint briefs live in docs/sprints/ (00-INDEX.md is the wave table +
  fleet playbook + gotcha ledger). READ 00-INDEX.md FIRST.

## Base branch (important)
Wave 2 depends on Wave 1 (S01/S04/S06-inputs), which is in PR #110
(`wave-1 → testing`), likely already merged by the time you start.
- If #110 is merged: branch off `testing`.
- If #110 is still open: branch off `wave-1`.
Confirm with `gh pr view 110 --json state,merged` before cutting branches.

## The seven iron rules (CI-enforced — violating one fails the merge)
1. reachlock-core stays pure: zero rendering/IO deps; any new core dep must
   compile to wasm32 and be justified in the PR.
2. No floats in gameplay values — fixed-point (`util::rng::Fixed`, 1/1024)
   or integers. Floats only at the render/bridge layer (`.to_f32()` at the
   call site, never stored in game state).
3. New generator or generator change ⇒ extend core/src/determinism.rs and
   recapture goldens deliberately; note any manifest change in the commit.
   (Wave 2 is mostly gameplay, not generators — S09 explicitly says transit
   rolls are NOT manifest entries. Don't add manifest rows for gameplay RNG.)
4. Wire shapes are pinned (network/messages.rs, contract JSON, content
   schemas) — changing a serialized shape is a protocol revision: update the
   locking test AND note it.
5. Every LLM call has a visible deliberation state (reuse `DeliberationState`;
   don't build a second overlay).
6. Offline is first-class: every feature works with no server. Online adds,
   never replaces.
7. Freeze contracts first: each brief has a "Freeze first" section — define
   and test the types/enums/schemas BEFORE building the slice.

## Wave 2 sprints (briefs in docs/sprints/)
- S06 — Mode State Machine & Transitions   (depends on S04 ✓ done in W1)
- S07 — Landed Slice: Walk a Station        (depends on S01 ✓, S06)
- S08 — On-Board Slice: Walk Your Ship      (depends on S06)
- S09 — Flight, Jump Gates & Cryo Transit   (depends on S04 ✓, S06)

## Execution order — S06 IS THE LINCHPIN
Unlike Wave 1, this wave is NOT fully parallel. S07, S08, and S09 all depend
on S06's `GameMode` sub-state machine and its per-mode scene setup/teardown.
So:
1. Do S06 FIRST, alone. Land it green (its acceptance gates + `make check`)
   before starting anything else. Freeze the `GameMode` enum exactly once —
   the other three sprints build against it, so churn here is expensive.
2. THEN do S07, S08, S09 — these are mutually parallel-safe (disjoint files,
   they share only the frozen `GameMode` interface). If you delegate to
   sub-agents/parallel tasks, give each its own git worktree
   (`git worktree add .claude/worktrees/<name> -b sprint-v2/sXX-<name> <base>`)
   on the shared CARGO_TARGET_DIR, and have each do SCOPED verification only
   (its crate's tests + fmt + clippy -D warnings); you own the full
   `make check` at integration.

## Per-sprint discipline
- Branch `sprint-v2/sXX-<name>`; commit as you go; do NOT push feature
  branches or open per-sprint PRs — they integrate into one wave branch.
- Gate EVERY commit on `cargo clippy -p <crate> --all-targets -- -D warnings`
  (note --all-targets: plain per-crate clippy misses test-only lints — a
  Wave 1 cast slipped through exactly this way) and `cargo fmt --all`.
- Bevy 0.18 SubStates: check the real 0.18 API (`#[derive(SubStates)]`,
  `#[source(...)]`) in the installed bevy docs before writing against a
  guessed macro — see S06's gotchas.
- Client builds pull Bevy (~6 GB in the shared target); that's fine on the
  freed disk, but prefer `cargo check`/scoped tests over full `cargo run`
  during development. Do NOT assume a display server — build + tests only;
  leave the manual "fly the loop" smoke test to me.

## Wave 1 carry-over you will hit in S09 (already documented)
S02 shipped the online seed.discover→adopt path, but scene regeneration on a
differing canonical seed is a deliberate stub — see the
`// S02 TODO(integrator):` at the top of reachlock-client/src/systems/network.rs.
It gets finished in S09, when gate-jump introduces a real per-system
`SystemId`/multi-system registry (setup.rs currently hardcodes one
SYSTEM_SEED). S09's and S06's Gotchas sections already spell this out —
follow them.

## Integration & PR
- Cut a `wave-2` branch off the same base; merge S06, then S07/S08/S09 into
  it, reconciling the mechanical conflicts (shared files like
  client/src/main.rs, systems/mod.rs, states.rs — inserts are additive/
  alphabetical; the plan in 00-INDEX describes the etiquette).
- Full gate on wave-2 before PR: `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, and the wasm gate
  `cargo check -p reachlock-client --target wasm32-unknown-unknown`
  (or `make check`, which runs fmt+clippy+test+wasm).
- Open ONE PR `wave-2 → testing`. Let CI run the full matrix (x86_64,
  aarch64, wasm32 determinism + full-plugin WASM build). Merge on green.
- Heads-up: this repo's classic-Projects integration makes `gh pr edit`
  error on title/body; use `gh api -X PATCH repos/chezgoulet/reachlock/pulls/<n>`
  for those. `gh pr ready` works.

## Definition of done for Wave 2
The three-mode loop is real and unlosable: Space Flight → dock → Landed →
board → On-Board → helm → Space Flight, resources surviving every transition
(S06); you can walk a station and a ship (S07/S08); flight has handling
depth and gate-jump plays the cryo sequence with the cryo-pilot contract
holding the helm (S09). All acceptance gates in each brief pass, `make check`
is green, and CI is green on the wave-2 → testing PR.

Start by reading docs/sprints/00-INDEX.md and docs/sprints/S06-*.md, confirm
the base branch, then freeze S06's GameMode enum.
```
