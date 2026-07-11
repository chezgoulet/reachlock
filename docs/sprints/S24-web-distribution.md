# S24 — Web Distribution & Release Pipeline

**Spec:** §2 (WASM target, Tauri option), Phase 4 (distribution) ·
**Wave 6 · Depends on:** nothing (any time after Wave 1)

## Outcome

The game is a URL: a release-profile WASM build with wasm-bindgen, a
self-contained web shell, size kept honest by CI, and versioned release
artifacts (web bundle + Linux/Windows desktop binaries) produced by a tag
push. The spike promise — browser-playable — cashed in.

## Context

- `cargo build --target wasm32-unknown-unknown` is green in CI (dev
  profile). Nothing web-shaped exists beyond that: no bindgen step, no
  html, no release tuning.
- v1 precedent: `archive/v1/ci.yml` had a version-stamped export job that
  smoked the EXPORTED artifact — that discipline carries over.
- The `wasm-bindgen-cli` version must match the `wasm-bindgen` crate
  version Bevy pins — extract it from `Cargo.lock` in CI rather than
  hardcoding.

## Freeze first

The release profile in the workspace `Cargo.toml`: `opt-level = "z"` vs
`3` measured (pick by benchmark, record numbers in the PR), `lto = "thin"`,
`strip = true`, `codegen-units = 1`. And the web shell contract:
`web/index.html` + loader owns canvas sizing, a loading progress bar
(Bevy WASM boots slow; a blank page reads as broken), and an unhandled-
panic banner (console-only panics are invisible to players).

## Deliverables

- [ ] `make web`: release build → `wasm-bindgen --target web` →
      `web/dist/` with index.html, JS glue, `.wasm`; served locally via
      `make web-serve` (any static server; python3 http.server is fine —
      COOP/COEP headers documented for when threading arrives).
- [ ] `wasm-opt -Oz` pass if binaryen available (optional locally,
      required in CI); brotli/gzip size reported.
- [ ] Boot correctness on wasm: audio requires a user gesture in browsers —
      gate audio start behind the menu's ENTER press; local saves go to
      a storage backend trait (filesystem native / localStorage or IDB on
      web) — coordinate with whoever owns saves if in flight.
- [ ] CI release workflow (`release.yml`, tag push `v2.*`): web bundle +
      native Linux + Windows (cross or matrix) binaries, version-stamped
      from the tag, attached to a GitHub Release. The web bundle is smoked:
      headless chromium loads the page and asserts the canvas appears and
      no panic banner (playwright or chrome --headless script).
- [ ] Size budget gate in CI: compressed .wasm size printed on every PR and
      failed if it exceeds a budget (set the initial budget from reality
      +20%, record it in the workflow with a comment on how to raise it
      consciously).
- [ ] README "Play it" section: the URL story (GitHub Pages or static host
      for the `testing` channel — wire Pages deployment if the repo allows;
      else document the manual publish).

## Acceptance gates

```
make web && make web-serve   # → browser: menu, ENTER, fly, dock; audio after gesture
git tag v2.0.0-alpha.N && git push --tags   # → release artifacts appear, smoke green
make check
```

## Non-goals

Tauri desktop wrapper (later; native binaries suffice). Mobile. Steam.
Multithreaded WASM/atomics (needs COOP/COEP + nightly features — document,
don't attempt). Asset streaming/code-splitting.

## Gotchas

- rapier2d + `opt-level = "z"` has historically produced pathological
  compile times on some versions — if it bites, `opt-level = 3` for the
  physics crates only via `[profile.release.package]`.
- `wasm-bindgen` crate/CLI version skew fails with a cryptic schema error —
  the Cargo.lock extraction step is not optional.
- localStorage caps ~5MB: saves must stay small or go IndexedDB — measure
  the current save size before choosing; note the choice in the PR.
- Keep `make check`'s dev-profile wasm gate untouched — the release path is
  additive.
