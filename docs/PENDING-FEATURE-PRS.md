# Tracking: stale remote feature PRs — DO NOT MERGE WHOLESALE

This PR tracks three remote branches that are **not** part of the v2 sprint wave
(already merged into `testing` via `sprint-v2/s06-mode-state-machine`, fast-forward
`69704f1..18b58ed`). They are documented here so they are not silently lost, but
**none should be merged into `testing` as-is.**

## Why none can be merged as-is

All three branches were cut from the **pre-v2 history** (the old `main` that still
contained the v1 Godot/Go prototype). Diffed against current `testing`, each one
re-introduces that prototype wholesale:

| Branch | Commits ahead/behind `testing` | v1 prototype files dragged in |
|---|---|---|
| `origin/feat/pre-commit-hook-74` (#74) | 1 / 110 | `godot/` (21+ files), `server/` (Go), `scripts/` |
| `origin/feature/mission-gate-self-jump-103` (#103) | 1 / 46 | `godot/` (~1746 files) |
| `origin/feature/save-slot-ring-104` (#104) | 2 / 46 | `godot/` (~1748 files) |

`AGENTS.md` is explicit: the v1 Godot prototype lives on `archive-v1`, and the
`testing` branch is pure v2 (Rust + Bevy + Postgres + Redis). Merging any of these
would dump the v1 prototype back into v2.

## Per-branch disposition

### #74 — pre-commit hook for architecture guard
- **Intent:** add `.githooks/pre-commit` (architecture guard) + CI wiring.
- **Real footprint:** tiny and desirable (one hook + CI tweak).
- **Problem:** the branch also carries the full v1 `godot/` + Go `server/`.
- **Action:** `git cherry-pick b470ee8` (the single hook commit) onto a fresh
  v2 branch, drop everything else, re-review, then merge.

### #103 — mission-gate self-jump
- **Intent:** `fix: gate self-jump route behind doss_deal_struck flag`
  (`7fc6b53`).
- **Problem:** sits on top of the v1 prototype; the diff is ~1,929 files, almost
  all `godot/`.
- **Action:** locate the v2-equivalent self-jump logic (likely in
  `reachlock-client/src/systems/jump.rs`) and re-apply the gating fix directly on
  v2. Do not merge the branch.

### #104 — save-slot ring
- **Intent:** rotating 5-slot checkpoint system (`753391b`) + the same self-jump
  fix (`7fc6b53`).
- **Problem:** same v1 prototype baggage as #103.
- **Action:** design the save-slot ring against v2 state (likely a new
  `reachlock-core`/`reachlock-client` module). Port the idea, not the branch.

## Status: branches deleted 2026-07-25

`main` is now the v2 tree (the v1 → v2 cutover), and every v1-era remote branch
was deleted so nothing drags the Godot/Go prototype back in. **This document is
now the only record of what those branches contained** — the commits below are
unreachable and will be garbage-collected.

| Branch (deleted) | Commit | Disposition |
|---|---|---|
| `feat/pre-commit-hook-74` | `b470ee8` | **Done.** The hook idea shipped in v2 as `.githooks/pre-commit`, which runs the whole `make check` gate. |
| `feature/mission-gate-self-jump-103` | `7fc6b53` | **Open.** See below. |
| `feature/save-slot-ring-104` | `753391b` | **Open.** See below. |
| `tracking/stale-feature-prs` | — | Superseded; this file lives on `main`. |
| `wave-0/engineering-backbone`, `wave-0/three-ring-architecture` | — | v1-era CI and architecture guard; v2's equivalent is `make check` + `make check-purity`. |

The full v1 prototype remains on **`archive-v1`**, which is deliberately kept.
Note that the three feature branches above were cut from old `main`, not from
`archive-v1`, so their *diffs* are gone even though the prototype they sat on
is preserved.

## Still to do in v2

Two behaviours were described on those branches and have **not** been
implemented in the Rust client. Neither is a port — both need designing against
v2 state.

### Gate the self-jump route behind a story flag (was #103)
`reachlock-client/src/systems/jump.rs::self_jump` currently has no story
precondition — confirmed by grep: no `doss_deal_struck` or equivalent flag
exists anywhere in v2. The v1 fix gated the route so the player could not skip
ahead of the narrative. v2 needs an equivalent, most likely reading a storyline
or faction flag rather than a bare boolean.

### Rotating save-slot ring (was #104)
v2 saves to a single `save/player.ron` (`reachlock-client/src/save_backend.rs`),
so a corrupt or bad write loses everything. The v1 design rotated five
checkpoint slots. Worth doing against `SaveFile`, and it pairs naturally with
the character-creation work (a per-character save set).
