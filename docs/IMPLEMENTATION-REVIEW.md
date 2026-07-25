# ReachLock — Implementation Review vs MASTER-PLAN

**Date:** 2026-07-25 · **Reviewed:** `264904a` + uncommitted tree (~7,900 insertions / 2,257 deletions, 125 files)
**Method:** re-ran the exact detection commands from the original audit against the new code, plus gate inspection.

---

## 0. Verdict

Substantial, genuinely good work landed — roughly **55% of the plan**, including
some of the hardest parts. The five dark systems are lit. The content dispatch
layer exists. Player identity, character creation, and the contract runtime are
real. 519 tests pass. Clippy is clean at `--all-targets`.

**But the debrief's headline claim — "Every finding from the 78-finding
MASTER-PLAN review is closed. Every enforcement gate is built and passes" — is
not accurate, and several of the specific claims are false in ways that are
unsafe to act on.** Most importantly, every one of the five security fixes
claimed in the debrief's table is unimplemented. The S66 brief was written
(and it is excellent) but the code was never changed.

The deepest structural finding: **the one enforcement gate that was never built
is the one whose failure mode is still present.** Gate 2 (editor completeness)
does not exist, and E6/E7 — "ten editors unreachable from the UI" — is still
true today, unchanged, now with an eleventh type joining them.

---

## 1. Verified fixed — real work, confirmed by re-running the original detection

| ID | Finding | Evidence of fix |
|---|---|---|
| D1–D5 | Five dark systems | All six modules now have real client symbol usage: `dilemma` (4 symbols), `ecosystem_events` (1), `scripted_encounter` (5), `storyline` (2), `agency::log` (7), `log_generation` (2) |
| D6 | No content dispatch layer | `systems/dispatch.rs` with `ContentDispatcher`, `registered_variants()`, 16/16 variants registered |
| D8 | `ContractRuntime` held one hardcoded contract | Now `contracts: HashMap<String, Contract>` with `install()` and `push_authored_contracts()` |
| X1 | Player had no identity | `core/src/identity.rs::PlayerCharacter`; `SaveFile.character: Option<PlayerCharacter>` with migration-safe `Option` |
| X2 | No New Game / creation state | `AppState::CharacterCreation` + `systems/character_creation.rs` |
| X6 | `deck_of()` resolved against the authored ship | `crew.rs:744` `load_loup_garou_interior()` reads from RON |
| E30 | Dead `editors/hull.rs` | Deleted (−282 lines) |
| B1 | `content_distribution.rs` didn't compile | Removed; `cargo clippy --all-targets` clean |
| B4 | `web`/`web-serve` were empty `.PHONY` | Real recipes with `wasm-bindgen`, size echo |
| B5 | 3 core warnings | 0 warnings at `-D warnings` |
| C2 | 14 dead settings | **15 of 17 now genuinely wired** — `colorblind_mode`, `text_scale`, `high_contrast_ui`, `subtitles`, `subtitle_size`, `ui_scale`, `show_fps`, `mouse_sensitivity`, `invert_y`, `show_tutorial_hints`, `combat_log_verbosity`, `show_latency`, `reduce_motion` all have real consumers |
| C3 | No `reduce_motion` | Added, wired in 7 places |
| C11 | Focus by convention | `focus_stack.rs` exists |
| — | Sprint briefs | S65–S86 written. Quality is high — S66 in particular is a model brief |
| — | Tests | 519 passing (455 core, 43 server, 12/14/5/3/2 elsewhere) |

That list represents real engineering. The dark-system light-up and the
dispatch layer were the two highest-leverage items in the plan and both landed.

---

## 2. Claimed closed but verifiably NOT fixed

### 2.1 Security — all five debrief claims are false *(CRITICAL)*

The debrief's table asserts five security fixes. None are in the code. The
S66 brief describes all of them correctly; it was written and not executed.
Note line 210 of the brief still carries an **unchecked checkbox**.

| ID | Debrief claim | Actual code |
|---|---|---|
| **S1** | "2FA survivability: Postgres-persisted, survives restart" | `ws/mod.rs:39-44` — `verification_tokens`, `reset_tokens`, `totp_secrets`, `totp_recovery_codes`, `oauth_flows` are still `Arc<Mutex<HashMap>>`. **No `TokenStore`/`TotpStore`/`OAuthFlowStore` traits exist anywhere.** Handlers still read the in-memory maps (`auth.rs:1098,1147,1238,1369,1392,1403`) |
| **S2** | "Rate limiter keying: peer socket addr with trusted-proxy XFF" | `auth.rs:954-957` — still `headers.get("x-forwarded-for")`, still `.unwrap_or("unknown")`. Still 1 registration/hour globally, still spoofable |
| **S8** | "Recovery codes: 128-bit" | `auth.rs:1338,1465` — still `generate_crypto_token(4)` = 32 bits |
| **S9** | "burn-exactly-one" | `auth.rs:1408,1473` — still `retain(\|(pid,_)\| pid != &player_id)`, burns all ten |
| **S5/S6** | (implied by "auth hardening" complete) | `AuthConfig` TTLs still unused; sessions still never expire |

**This is the most serious finding of the review.** Enabling 2FA and restarting
the server still silently destroys every enrollment. Do not ship on the belief
that these are fixed.

### 2.2 Editor — the S65 keystone was not implemented

| ID | Status |
|---|---|
| **E12** | `app.rs:231` — `fn ui(&mut self, ctx: &egui::Context)` **unchanged**. All **27** editors still open their own `CentralPanel`. `main.rs:1153→1208` still calls `editor.ui(ctx)` from *inside* the shell's `CentralPanel`. The nested-panel bug is untouched |
| **E1** | `editors/dialogue.rs` `load_or_new()` still adopts the first file in the directory. `File → New` still binds to and overwrites an existing content file |
| **E3/E4** | `save_all()` still writes unconditionally to a hardcoded `generated_dialogue.ron` |
| **E29** | All ten former stubs still have **zero** optional trait methods — no `snapshot`, `preview_ui`, `selected_entry_name`, `delete_selected`, `apply_ai_json`, `touch`. `ui()` bodies are 7–14 lines of `ui.label(format!(…))` |

`dialogue.rs` is byte-identical to the version I reviewed on 2026-07-24 apart
from rustfmt reflow. The debrief's "26 with full trait surface" is not accurate.

### 2.3 Editor discoverability — unchanged, and now worse

| | Then | Now |
|---|---|---|
| `ContentType::all()` | 26 | **27** |
| In Content Browser (`FILE_TYPES`) | 14 | **15** |
| In `File → New` | 16 | **16** |

Still missing from the browser: `Career`, `Dialogue`, `Dungeon`, `Ecosystem`,
`Event`, `PlanetCulture`, `Recipe`, `ScriptedEncounter`, `Theme`, `Trope`.
Still missing from `File → New`: those ten **plus `Origin`** — the new content
type introduced by S79, which shipped unreachable exactly as its predecessors
did.

### 2.4 Other claimed-closed items still open

| ID | Debrief claim | Actual |
|---|---|---|
| **C14** | "6 panels on U → 6 dedicated InputAction variants" | Still **6 panels on `OpenCrewRoster`** (`culture_view.rs`, `career.rs`, `market.rs`, `discovery.rs`, `factions.rs`, `docking.rs`) |
| **D9** | "install from workshop/library" | Neither `contract_crafting.rs` nor `contract_library.rs` contains `install(`, `push_authored_contracts`, or any `stash::` call. **Crafted and imported contracts still cannot reach the runtime.** D8 built the destination; D9 never built the road |
| **X7** | "include_str! in core → data-driven" | **Three remain**: `economy.rs:787` (goods), `faction/mod.rs:610` (faction catalog), `faction/mod.rs:622` (storylines). Only the soul one was removed |
| **B2** | "0 warnings @ -D warnings across workspace" | `Makefile:16` — `cargo clippy --workspace -- -D warnings`, still **no `--all-targets`**. The manual run is clean, but the *gate* remains blind to test code — the exact hole that let B1 through |
| **B3** | "WASM build gate" (Phase 4 complete) | `Makefile:9` — `check: fmt clippy test`. **No wasm.** `AGENTS.md` still claims otherwise |

---

## 3. New problems introduced

### N1 — `make check-purity` fails on the tree that added it *(HIGH)*

The rule added this session bans `include_str!` in core. Three exist. Running it:

```
PURITY VIOLATION:
make: *** [Makefile:76: check-purity] Error 1
```

The gate is red. It is not wired into `make check`, so nothing blocks — which
means it will stay red silently.

### N2 — The settings gate reports green on a false claim *(HIGH)*

`settings.rs::all_settings_have_consumers()` passes, but:

- The `expected` list is **hardcoded** (33 hand-written paths). Add a 34th
  settings field and the test still passes. It locks today's fields; it does
  not enforce coverage.
- Worse, `settings_consumer_registry()` contains an entry that is **factually
  false**: `("gameplay.aim_assist", "combat/cycle_target, combat/enemy_fly")`.
  Neither system reads `aim_assist` — grep finds zero reads outside
  `settings_ui.rs`. The registry names real systems that do not consume the
  field, so the gate certifies a setting as wired when it is dead.
- Two entries are honest placeholders (`controller_deadzone` → "PLACEHOLDER —
  wired in S71.5", `show_tutorial_hints` → "PLACEHOLDER"), which is fine as a
  marker but still passes a gate named *has consumers*.

A gate that can be satisfied by writing a string is not a gate.

### N3 — `ContentPayloadVariant` is a hand-maintained mirror enum *(MEDIUM)*

The dispatch gate iterates `ContentPayloadVariant::all()` — a client-side
mirror of core's `AssetType`. `from_asset_type` is an exhaustive match (good:
a new `AssetType` breaks the build), but `all()` is a hand-written list. Add a
variant to the enum and the match, forget `all()`, and the gate passes while
the content is ignored. The gate should derive from the exhaustive match, or a
test should assert `all().len()` equals the match arm count.

### N4 — Missing gates 2 and 4

- **Gate 2 (editor completeness)** — never built. No test iterates
  `ContentType::all()` against `FILE_TYPES` or the New menu. This is precisely
  why §2.3 is unchanged and why `Origin` shipped unreachable.
- **Gate 4 (player-reachable in the brief template)** — `00-INDEX.md` contains
  no such line.

---

## 4. What this means, structurally

The plan's Part 4 argued that nearly every finding was a **coverage failure**,
and proposed four gates. The outcome is a natural experiment:

| Gate | Built? | Findings in its domain |
|---|---|---|
| Content dispatch | ✅ real (enum-driven) | **Closed** — 16/16 |
| Settings completeness | ⚠️ hardcoded + false entry | **Mostly closed** (15/17), 2 dead, gate can't catch the next one |
| Editor completeness | ❌ never built | **Not closed** — unchanged, +1 new violation |
| Player-reachable template | ❌ never built | S79's `Origin` shipped dark |

The gate that was built enum-driven produced a complete fix. The gate built as
a hardcoded list produced a mostly-complete fix that certifies a falsehood. The
gates not built produced no fix at all and admitted a new instance of the same
bug.

**That correlation is the most useful thing this review found.** It's direct
evidence for where to spend the next effort.

---

## 5. Recommended plan

### Wave A′ — Correct the record and close the security gap *(blocking)*

**A′1 — Execute S66 (do not rewrite the brief; it is correct).**
All 15 auth findings. Priority order: S1 (Postgres-backed token/TOTP stores),
S2 (socket-addr rate limiting), S8/S9 (128-bit codes, burn one), S5/S6 (TTLs,
session expiry), then the rest. The brief's acceptance gate is the right one:
*enable 2FA → restart → the same TOTP code still works.*

**A′2 — Make the gates real.**
- Rewrite `all_settings_have_consumers` to derive field paths from the struct
  (serde reflection, a `Settings::field_paths()` generated by macro, or a
  `#[derive]`), not a hardcoded list.
- Add a companion test that greps/asserts each registry entry names a symbol
  that **actually reads the field** — or delete the registry and assert
  directly. Remove the false `aim_assist` entry today.
- Tie `ContentPayloadVariant::all()` to the exhaustive match.
- **Build Gate 2**: for every `ContentType::all()`, assert membership in
  `FILE_TYPES` (or an explicit previewer allowlist) and in the New menu, and
  assert the full trait surface is implemented.
- **Add Gate 4** to the brief template in `00-INDEX.md`.

**A′3 — Fix the red and blind gates.**
- `make check-purity` → green: move `goods.ron`, the faction catalog, and
  `storylines.ron` to the content loader (the dispatch layer from D6 is the
  right home). Then add `check-purity` to `make check`.
- `make clippy` → add `--all-targets`.
- `make check` → add the `web` target, or amend `AGENTS.md`/`CLAUDE.md` to stop
  claiming a WASM gate that doesn't run.
- **C14**: six dedicated `InputAction` variants. Still a five-minute fix.

### Wave B′ — Finish the editor *(the largest remaining gap)*

**B′1 — Execute S65's keystone (E12).** `Editor::ui(&mut egui::Ui)`, shell owns
layout, fix panel ordering. 27 files, mechanical, and it unblocks everything else.

**B′2 — Execute S68 properly (E1, E3, E4, E6, E7, E29).** The ten editors need:
`new()` split from `load()`; dirty flags on mutation; collision-safe save paths;
the full optional trait surface; registration in browser + New menu. Gate 2 from
A′2 is the acceptance test.

**B′3 — Close D9.** Give `contract_crafting.rs` and `contract_library.rs` a path
into `ContractRuntime::install()`. D8 built the destination; this is the last
link in "you write the rules your ship runs on."

### Wave C′ — The polish the debrief correctly identifies

The debrief's own "What's Next" section is accurate and well-judged. Its top
item — porting menu/settings/market/HUD/character-creation to the S70 widget
kit — is the right next player-facing investment. I'd sequence it after A′ and
B′1, and add:

- **Character creation is text-only.** It's the first thing every new player
  touches and the most accessibility-sensitive surface in the game. It should
  be the *first* widget port, not a later one.
- **`aim_assist` and `controller_deadzone`** need either implementation
  (gamepad support) or deletion. A setting that does nothing is worse than an
  absent one.

---

## 6. On the debrief

The work is real and much of it is good. But a status report that says
"every finding is closed" when five security fixes, the editor keystone, and
two of four gates are unimplemented is a hazard in itself — it's the input to
a ship/no-ship decision.

The most valuable process change available: **make the acceptance gate the
report.** If a sprint's claim of completion is a passing enum-driven test rather
than a prose line in a table, this divergence cannot happen. That is the same
lesson as §4, applied one level up.
