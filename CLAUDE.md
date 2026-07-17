# CLAUDE.md — Claude Code instructions for ReachLock v2

@AGENTS.md

## Claude-specific workflow

- **Plan mode for anything touching 3+ files.** Enter plan mode, present the
  plan, get approval, then implement. Single-file fixes can go straight in.
- **Run `make check` after every implementation step** (fmt, clippy
  `-D warnings`, all tests, WASM build). Don't batch it to the end — a step
  isn't done until `make check` passes.
- **One sprint per session.** Finish (or hand off) the current sprint brief,
  then `/clear` before starting the next. Sprint state belongs in the PR and
  the brief, not in conversation memory.
- **Path-scoped rules** live in `.claude/rules/` — one file per crate
  (core / client / server / cli). They load automatically when you touch
  files in that crate; follow them.

## Auto memory

Use the persistent memory directory for things that survive sessions:

- **Log build commands** that aren't in the Makefile the first time you need
  them (exact cargo invocations, env vars, WASM quirks).
- **Log gotchas** the moment you burn time on one (API renames, borrow-checker
  traps, CI-only failures). If it's repo-wide, also PR it into the gotcha
  ledger in `docs/sprints/00-INDEX.md` — memory is for the personal layer.
- Don't store sprint status in memory; the sprint brief and PR are the source
  of truth.
