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

## Recommended process
1. Keep these remote branches as reference only.
2. For each, open a fresh v2 branch from `testing`, cherry-pick/re-implement the
   v2-relevant commit, and open a targeted PR.
3. Confirm `godot/` and Go `server/` are absent from the resulting diff before
   requesting review.
