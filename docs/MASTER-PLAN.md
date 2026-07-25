# ReachLock — Master Finding Register & Consolidated Plan

**Date:** 2026-07-24 · **Branch:** `testing` @ `8ad959b` + working tree
**Supersedes as index:** `UX-AUDIT-AND-PLAN.md`, `CHARACTER-CREATION-PLAN.md`,
`PRODUCT-LEVERAGE.md` (those remain the detailed rationale; this is the
single source of truth for *what* and *when*).

Every finding from this review session is registered below with an ID,
severity, evidence, and the sprint that closes it. 78 findings, 23 sprints,
5 waves.

---

# PART 0 — DARK SYSTEM VERIFICATION (requested)

Method: extracted every `pub fn|struct|enum|type|const|trait` from each module
and grepped `reachlock-client`, `reachlock-cli`, `reachlock-server`,
`determinism.rs`, and the content loader for each symbol individually — not
module-path greps, so re-exports and bare imports were covered.

| System | Sprint | Client | CLI | Server | Determinism | Verdict |
|---|---|---|---|---|---|---|
| `generator::dilemma` (11 public symbols) | S36 | 0 | 0 | 0 | 1 golden | **DARK — confirmed** |
| `generator::ecosystem_events` (3 symbols) | S39 | 0 | 0 | 0 | 1 golden | **DARK — confirmed** |
| `generator::scripted_encounter` (14 symbols) | S41 | 0 | 2 | 0 | 2 goldens | **DARK — confirmed¹** |
| `generator::storyline` (2 symbols) | S60 | 0 | 0 | 0 | 1 golden | **DARK — confirmed** |
| `agency::log` + `log_generation` (15 symbols) | S37 | 0² | 0 | 0 | **0** | **DARK — confirmed³** |

¹ The 2 CLI references are `content.rs:299` (list/validate dispatch) and
`content.rs:432` (schema id mapping) — **schema validation only, no execution
path**. My earlier caveat was correct; the conclusion stands.

² The single apparent client hit was a **false positive**:
`dialogue.rs:279` uses `ChoiceEffect::RelationshipDelta` from
`reachlock_core::dialogue` — a different type. The client imports
`reachlock_core::dialogue`, never `agency::log`.

³ Captain's Log is dark *and* has **zero determinism goldens** — it is outside
the S03 gate entirely, unlike the other four.

**Verdict: plan around it. All five are dark.**

## Three findings the verification turned up that are larger than the original claim

### V1 — There is no content dispatch layer *(CRITICAL)*

`content_index.rs:142` calls `walk(root, &mut files)`, which recursively parses
**every** `ContentFile` envelope in the mod tree into `ContentIndex.files`.
Exactly one system iterates that Vec: `soul::init_souls`. Dispatch by
`ContentPayload` variant across the whole client:

| Dispatched | Loaded then ignored |
|---|---|
| `Soul`, `Hull`, `HullFrame`, `RoomTemplates`, `Station` | `Career`, `Contract`, `Dialogue`, `Dungeon`, `Ecosystem`, `Event`, `PlanetCulture`, `Recipe`, `ScriptedEncounter`, `Theme`, `Trope` |

**10 of 15 payload variants are parsed from disk on every startup and dropped
on the floor.** This is the root cause under most of the "dark system" symptoms
— the systems aren't just unwired, there is no wiring mechanism.

### V2 — `content.themes` is populated and never read *(HIGH)*

`content_index.rs:133` loads `themes/` into a typed `HashMap<String, Theme>`.
Grep for any read of `.themes` outside the loader: **zero hits**. `music.rs`
does not reference `Theme` at all. S48's authored music theme pipeline —
schema, editor, content file — terminates in an unread HashMap.

### V3 — No authored contract can ever run *(CRITICAL — the deepest finding of the session)*

`contract.rs:89-99`:
```rust
impl Default for ContractRuntime {
    fn default() -> Self {
        ContractRuntime { contract: auto_helm(), … }
    }
}
```

`auto_helm()` is a hardcoded built-in. Nothing ever replaces it. Only two
systems hold `ResMut<ContractRuntime>` — the evaluator itself (`contract.rs:161`)
and network sync (`network.rs:144`).

Which means **every one of these authoring surfaces feeds nothing**:
- the contract editor in the content suite (638 lines)
- `ContentPayload::Contract` envelopes on disk
- the CLI's contract schema validation
- the S34 **contract crafting workshop** (898 lines) — no install path
- the S34 **contract library** import/share (376 lines) — no install path

The player can craft a contract, validate it, share it, and import someone
else's. It can never execute. The ship runs `auto_helm()` forever.

This is the game's signature mechanic — "you write the rules your ship runs on"
— with no path from any authoring surface to execution. It moves to the front
of the plan.

---

# PART 1 — FINDING REGISTER

Severity: **C**ritical (data loss / security / core promise broken) ·
**H**igh · **M**edium · **L**ow

## 1.1 Editor (E)

| ID | Sev | Finding | Evidence | Sprint |
|---|---|---|---|---|
| E1 | C | `File → New` adopts an existing file and overwrites it on save | `dialogue.rs:12-25`, `career.rs:26-51` | S65 |
| E2 | C | Mutating widgets never set the dirty flag → close guard never fires → silent loss | `career.rs:107-109` | S65 |
| E3 | H | `save_all()` writes unconditionally, ignoring dirtiness; contradicts autosave | `dialogue.rs:72-80` | S65 |
| E4 | H | Unsaved files collide on a hardcoded stem (`generated_dialogue.ron`) | `dialogue.rs:74` | S65 |
| E5 | H | "Reroll All" renames content ids, breaking cross-references | `career.rs:87`, `main.rs:1041` | S65 |
| E6 | H | 10 editors absent from the Content Browser | `browser.rs:29-44` (14 of 26) | S68 |
| E7 | H | 10 editors absent from `File → New` | `main.rs:728-792` (16 of 26) | S68 |
| E8 | M | Two divergent registries; `register_all` is dead code | `editors/mod.rs:34` vs `app.rs:302` | S65 |
| E9 | M | `SidePanel::right` registered after `CentralPanel` — overlays it | `main.rs:1217` | S65 |
| E10 | M | `TopBottomPanel` registered on `ctx` inside the `CentralPanel` closure | `main.rs:1166` | S65 |
| E11 | L | Top bars start right of the browser due to registration order | `main.rs:1010,1037,1075` | S65 |
| E12 | H | All 26 editors open their own `CentralPanel`; trait takes `&Context` not `&mut Ui` | `app.rs:225` | S65 |
| E13 | M | Modals aren't modal — no input blocking behind the dialog | `dialogs.rs:29` | S65 |
| E14 | M | No timeout on AI requests — UI hangs on "Generating…" forever | `ai.rs:360,409` | S67 |
| E15 | M | No cancel for a running generation | `main.rs:1104` | S67 |
| E16 | M | Repaint gating starves async results, autosave, and status expiry | `main.rs:1311` | S67 |
| E17 | M | A tokio multi-thread runtime built per click, `.unwrap()`ed | `main.rs:1117`, `settings_window.rs:117` | S67 |
| E18 | H | `Dialogue` mapped to the **ecosystem** schema | `schema.rs:64` | S65 |
| E19 | M | API key plaintext on disk, unmasked in the UI | `settings_window.rs:12,75` | S67 |
| E20 | M | All config paths relative to CWD — silent data loss when launched elsewhere | `preferences_window.rs:8`, `settings.rs:959`, `save_backend.rs:9` | S67 |
| E21 | M | File delete is permanent — no trash, no undo | `browser.rs:314` | S67 |
| E22 | L | Browser never indicates which file is open | `browser.rs:242` | S67 |
| E23 | L | Every 2s rescan re-reads all `hulls/*.ron` to classify | `browser.rs:60,128` | S67 |
| E24 | M | Directory scan is synchronous on the UI thread | `browser.rs:99` | S67 |
| E25 | L | Menu shortcut hints use hardcoded space padding | `main.rs:793-841` | S67 |
| E26 | L | Preferences written to disk every changed frame | `preferences_window.rs:192` | S67 |
| E27 | M | Undo tracks only the active tab | `main.rs:1304` | S67 |
| E28 | M | No tab keyboard nav, reorder, or overflow | `main.rs:1166` | S67 |
| E29 | H | 10 editors lack `snapshot`/`preview_ui`/`delete_selected`/`apply_ai_json`/`touch` | trait defaults `app.rs:196-284` | S68 |
| E30 | L | Dead `editors/hull.rs` (282 lines) on disk | `editors/mod.rs:14-18` | S65 |
| E31 | M | `.lock().unwrap()` on `ai_status` ×6 — poisoned lock panics the UI | `main.rs:977-1136` | S67 |

## 1.2 Client (C)

| ID | Sev | Finding | Evidence | Sprint |
|---|---|---|---|---|
| C1 | H | No UI layer: 0 `Interaction`, 0 `Button`, 0 `ImageNode` in 23k LOC | whole crate | S70 |
| C2 | H | 14 settings are editable, persisted, and read by nothing | `settings.rs` vs consumers | S71 |
| C3 | M | No `reduce_motion` setting despite heavy motion effects | `settings.rs:153` | S71 |
| C4 | M | Main menu: keyboard-only, no Quit, seed displayed but not editable | `menu.rs` | S70/S78 |
| C5 | H | No tutorial, tooltips, or contextual hints; `show_tutorial_hints` is dead | `hud.rs:160` | S72 |
| C6 | H | Deliberation — the signature moment — is one line of grey text | `hud.rs:291-301` | S72 |
| C7 | M | Mode transitions are text swaps (`"DOCKING…"`) | `hud.rs:282-284` | S72 |
| C8 | M | No damage/threat feedback hierarchy; all state competes as equal text | `hud.rs:256` | S72 |
| C9 | H | Colour is the only channel for several game states | `hud.rs`, `factions.rs` | S71 |
| C10 | M | `settings_ui::row_count()` hardcoded per tab; drifts silently | `settings_ui.rs:66-75` | S70 |
| C11 | M | Focus is a convention (`if ui.open { return }`), not a focus stack | `pause.rs:52`, `menu.rs:100` | S70 |
| C12 | L | Starting location is a magic literal in the app builder | `main.rs:105-111` | S77 |
| C13 | H | Six independent `*Visible` booleans, no z-order, no mutual exclusion | `main.rs:145-158` | S70 |
| C14 | H | **Six panels bound to the same key** — one press opens six overlapping panels | `culture_view.rs:30`, `career.rs:33`, `market.rs:99`, `discovery.rs:29`, `factions.rs:28`, `docking.rs:146` | **S64** |
| C15 | M | Market, ship exterior, and ship interior editors share one text entity — mutually exclusive by construction | `hud.rs:391-417` | S70 |

## 1.3 Server (S)

| ID | Sev | Finding | Evidence | Sprint |
|---|---|---|---|---|
| S1 | C | Auth state in-memory even under Postgres — **restart destroys all 2FA enrollments** | `ws/mod.rs` AppState | S66 |
| S2 | C | Register limiter keys on `X-Forwarded-For` → `"unknown"`: 1 signup/hour globally, and spoofable | `auth.rs:933-939`, `:61` | S66 |
| S3 | H | Login limiter keyed on attacker-controlled login text | `auth.rs:999` | S66 |
| S4 | M | Rate-limiter bucket map grows unbounded — memory exhaustion vector | `auth.rs:32` | S66 |
| S5 | H | `AuthConfig` TTLs settable via admin API and ignored by the code | `auth.rs:973,1128,1150` | S66 |
| S6 | H | Sessions never expire | `MemorySessionStore`, `auth.rs:375` | S66 |
| S7 | H | 2FA activates before verification — unscanned QR locks the user out | `auth.rs:1249` vs `:1054` | S66 |
| S8 | H | Recovery codes are 32-bit; challenge endpoint unrated-limited | `auth.rs:1255,1365` | S66 |
| S9 | H | Using one recovery code burns all ten | `auth.rs:1369` | S66 |
| S10 | M | Password reset doesn't check `banned_at` / `deleted_at` | `auth.rs:1159` | S66 |
| S11 | M | `login` leaks `player_id` pre-2FA; inconsistent response shape | `auth.rs:1056,1079` | S66 |
| S12 | M | Verification/reset emails hardcode `https://reachlock.example` | `auth.rs:976,1131,1153` | S66 |
| S13 | M | Registration returns a session while `login` refuses unverified accounts | `auth.rs:963` vs `:1045` | S66 |
| S14 | M | No CORS, body limit, or trace layer — CORS blocks the WASM client | router | S66 |
| S15 | M | Client-supplied `system_prompt` makes the server a general LLM proxy | `llm_proxy.rs:43-46` | S66 |

## 1.4 Build & gates (B)

| ID | Sev | Finding | Evidence | Sprint |
|---|---|---|---|---|
| B1 | C | `content_distribution.rs` doesn't compile — `make check` is red now | 6× `E0599` | **S64** |
| B2 | H | `make clippy` lacks `--all-targets`, which is why B1 slipped through | `Makefile:14` | S64 |
| B3 | H | No WASM gate despite `AGENTS.md`/`CLAUDE.md` claiming one | `Makefile:9` | S64 |
| B4 | M | `web`/`web-serve`/`wasm-bindgen-cli` are `.PHONY` with no recipes | `Makefile:47` | S64/S74 |
| B5 | L | 3 live warnings in `reachlock-core` | unused `Fixed`, items-after-tests, `== false` | S64 |
| B6 | H | `check-purity` doesn't scan `storylines/` — misses X7 | `Makefile:57-60` | S77 |

## 1.5 Character & identity (X)

| ID | Sev | Finding | Evidence | Sprint |
|---|---|---|---|---|
| X1 | C | The player has no identity: no name, species, appearance, or soul anywhere | `SaveFile`, `network/messages.rs` | S75 |
| X2 | H | No New Game; `load_save` runs in `Startup` before the menu renders | `main.rs:218-221`, `states.rs:33` | S78 |
| X3 | H | Two parallel appearance systems sharing zero code | `core::generator::sprite` vs `client::pixel` | S76 |
| X4 | H | `crew_look()` is a hardcoded match over 7 lore ids | `pixel.rs:419-500` | S76 |
| X5 | H | `default_crew()` hardcodes six named crew, inserted unconditionally | `crew.rs:68`, `main.rs:211` | S77 |
| X6 | C | **Live bug:** `deck_of()`/`deck_zero_g()` resolve against `loup_garou_interior()` regardless of the player's ship | `crew.rs:149,160` | S77 |
| X7 | C | Content file `include_str!`'d into `reachlock-core` — iron rule #1 violation | `soul/runtime.rs:388-391` | S77 |
| X8 | L | Starting location hardcoded | `main.rs:105-111` | S77 |
| X9 | M | Crew is a fixed party — no recruit, hire, injure, lose, or death | `crew.rs`, `CrewRole` closed enum | S80 |

## 1.6 Dark systems & dispatch (D) — verified Part 0

| ID | Sev | Finding | Evidence | Sprint |
|---|---|---|---|---|
| D1 | H | Dilemma generator dark (S36) | 0 refs outside core | S82 |
| D2 | H | Ecosystem events dark (S39) | 0 refs outside core | S84 |
| D3 | H | Scripted encounters dark (S41) — CLI validation only | 0 execution path | S82 |
| D4 | H | Storyline generator dark (S60) | 0 refs outside core | S82 |
| D5 | H | Captain's Log dark (S37) **and outside the determinism gate** | 0 refs, 0 goldens | S83 |
| D6 | C | **No content dispatch layer** — 10 of 15 `ContentPayload` variants loaded then ignored | `content_index.rs:142` | **S81** |
| D7 | H | `content.themes` populated and never read — S48 pipeline terminates | `content_index.rs:133` | S81 |
| D8 | C | `ContractRuntime` holds one hardcoded `auto_helm()`; no authored contract ever loads | `contract.rs:89-99` | **S81** |
| D9 | C | Crafting workshop and library cannot install a contract into the runtime | no `ResMut<ContractRuntime>` in either | **S81** |

## 1.7 Product leverage (P)

| ID | Sev | Finding | Evidence | Sprint |
|---|---|---|---|---|
| P1 | H | The contract engine is the product's one novel idea and is buried behind a console | `hud.rs:391-417` | S81/S72 |
| P2 | H | First-write-wins discovery is fully built and produces one transient message | `services/seed.rs`, `network.rs:186` | S85 |
| P3 | H | The world simulates but never remembers the player; no long arc | `agency::log` dark | S83 |
| P4 | M | Contract exchange has a server service but no real client surface | `services/library.rs` | S86 |
| P5 | M | Origins/backgrounds don't exist; careers have no distinctive rewards | `career/mod.rs` | S79 |

---

# PART 2 — CONSOLIDATED SPRINT PLAN

## Wave A — Stop the bleeding *(blocking; nothing else ships first)*

**S64 — Green tree, real gates, and the key collision**
Closes: B1, B2, B3, B4(part), B5, **C14**
- Fix or delete `content_distribution.rs`; if content sync is real, add the wire
  variants + shape test (iron rule #4).
- `cargo clippy --workspace --all-targets -- -D warnings`.
- Add the WASM build to `check`, matching what AGENTS.md already claims.
- Clear the 3 core warnings.
- **Give each panel its own `InputAction`.** Five-minute fix, disproportionate
  effect on how the game feels.
- **Add "player-reachable" to the sprint acceptance-gate template** so no
  further system ships dark.

*Gate:* `make check` green from a clean clone.

**S65 — Editor data-loss closure**
Closes: E1, E2, E3, E4, E5, E8, E9, E10, E11, E12, E13, E18, E30
- `Editor::ui` takes `&mut egui::Ui`; shell owns layout; fix panel order.
- Split `load_or_new()` → `new()` (blank, `path: None`) + `load()`.
- Dirty flag on every mutation; `save_all` respects it; collision-safe stems.
- `accept_seed_reroll() == false` where seed only renames ids.
- `egui::Modal`; delete `register_all` and `editors/hull.rs`; fix the Dialogue
  schema mapping.

*Gate:* for every `ContentType`, a new editor has `path == None`, is clean,
flips dirty on mutation, and `save_all` on a clean editor writes nothing.

**S66 — Server auth hardening**
Closes: S1–S15
- Persist verification/reset/TOTP/recovery/oauth state behind traits with
  Postgres impls; encrypt at rest via the existing `encrypt_secret`.
- Rate limit on peer socket addr with trusted-proxy XFF; bound the bucket map;
  limit `tfa_challenge`.
- Honour every `AuthConfig` TTL; add session expiry.
- Two-phase 2FA enrollment; ≥128-bit recovery codes; burn exactly one.
- Ban/delete checks on reset; consistent login response; real public URL.
- CORS + body limit + trace layer; template-gate the LLM `system_prompt`.

*Gate:* enable 2FA → restart the server → the second factor is still required.

## Wave B — The systems you already paid for

**S81 — Content dispatch + the contract pipeline** ⚑ *highest leverage in the plan*
Closes: **D6, D7, D8, D9**, P1(part)
- Build a **content dispatch layer**: a registry mapping each `ContentPayload`
  variant to the system that consumes it, so loading and consuming are one
  contract instead of a Vec nobody reads.
- Wire the 10 ignored variants; wire `content.themes` into `music.rs`.
- **`ContractRuntime` holds a *set* of contracts**, loaded from
  `ContentPayload::Contract` + player-crafted + library-imported.
- **Install paths from the crafting workshop and the library into the live
  runtime.** This is the sentence "you write the rules your ship runs on"
  becoming true for the first time.

*Gate:* author a contract in the editor → it loads and evaluates in game.
Craft one in the workshop → install → it runs. Import from the library → it runs.
A test asserting every `ContentPayload` variant has a registered consumer.

**S82 — Narrative systems light-up**
Closes: D1, D3, D4
- Dilemmas surface at decision points; scripted encounters execute
  (`evaluate_scripted_encounter`/`advance_scene`/`apply_consequences` all exist);
  storylines drive faction arcs; tropes instantiate.
- Depends on S81's dispatch layer.

**S83 — Captain's Log & consequence memory**
Closes: D5, P3
- Wire `detect_key_moments` / `score_significance` / `NarratorVoice` /
  `generate_log_entry`. A narrated, shareable artifact of a playthrough.
- **Add determinism goldens** — this system is currently outside the gate.

**S84 — Living world surfacing**
Closes: D2
- Ecosystem events (extinction, invasion, mutation) become visible and
  consequential; planet culture reaches the player.

**S67 — Editor shell v2**
Closes: E14, E15, E16, E17, E19, E20, E21, E22, E23, E24, E25, E26, E27, E28, E31
- Command palette; tab nav; one shared runtime with timeouts + cancel;
  `request_repaint_after` for all async/timers; keyring + platform config dir;
  async browser scan; trash-not-delete; debounced prefs; all-tab undo;
  poison-tolerant locks.

**S68 — The missing ten editors**
Closes: E6, E7, E29
- Full `soul.rs`-grade editors for all ten; graph canvas for Dialogue, grid
  canvas for Dungeon (model on `gate_network.rs`); register in browser + menu.

*Gate:* table-driven test — every `ContentType` appears in `FILE_TYPES`, in
`File → New`, and implements the full trait surface.

**S69 — Authoring superpowers**
- Cross-reference index (autocomplete, go-to-definition, find-usages,
  broken-reference report, rename-with-referrers); live inline validation;
  diff-before-save with comment warning; `Preview → Launch in game`;
  duplicate + templates.

## Wave C — Make it legible, accessible, and fun

**S70 — Client UI framework** ⚑ *gates S72, S78*
Closes: C1, C4(part), C10, C11, C13, C15
- Choose `bevy_egui` (fast, shares idioms with the editor) or a native
  `bevy_ui` widget kit (more work, better art direction). Decide deliberately.
- Focus/input stack, panel z-order + mutual exclusion, mouse + gamepad +
  keyboard, tooltips, scrollable lists. Port menu/settings/market/board first.

**S72 — Deliberation Theater & onboarding** ⚑ *promoted — this is the pitch*
Closes: C5, C6, C7, C8, P1
- Stage the deliberation: what's being weighed, which of *your* rules ran out,
  the crew member's mood and history with you, the verdict, the cost. Let the
  player interject at a relationship cost (`co_deliberation.rs` models it).
- Surface `recent_uncovered` — "your rules left 4 gaps this cycle" — so
  rule-writing becomes a skill with feedback.
- First-run onboarding; contextual hints; feedback hierarchy; real mode beats;
  diegetic help replacing the 12px keybind dump.

**S71 — Make accessibility real**
Closes: C2, C3, C9
- Every dead setting gets a consumer or gets deleted. Semantic palette (no
  state by hue alone — pair every colour with a glyph, as the editor's ✔/✘
  already does). Captions. Gamepad. `reduce_motion`. Exhaustive keybind table.

*Gate:* conformance test — every settings field has a non-`settings_ui` consumer.

## Wave D — Character & open world

**S75 — Player identity in core** (X1) · **S76 — One appearance pipeline** (X3, X4)
**S77 — Decouple the Loup-Garou** (X5, X6, X7, X8, B6, C12)
**S78 — The creation flow** (X2, C4) *— depends on S70*
**S79 — Origins as authored content** (P5) · **S80 — Crew as an open-world system** (X9)

Detail in `docs/CHARACTER-CREATION-PLAN.md`.

## Wave E — Shared world & distribution

**S85 — Discovery permanence** (P2)
- Naming rights for first discovery; "charted by ⟨player⟩" attribution across
  every player's galaxy map; discovery as the Exploration career's reward.
  The hard part (atomic first-write-wins, `discoverer_id` persisted) is done.

**S86 — Contract exchange** (P4) · **S73 — Server ops surface** · **S74 — Web distribution** (B4)

---

# PART 3 — SEQUENCING

```
S64 ─┬─► S65 ─► S81 ⚑ ─┬─► S82 ─► S83 ─► S84
     │              └─► S67 ─► S68 ─► S69
     ├─► S66 ────────────────────────► S73 ─► S74
     └─► S70 ⚑ ─┬─► S72 ⚑
                ├─► S71
                └─► S78 ─► S79 ─► S80
S75 ─► S76 ─► S77 ──────────► S78
S85 / S86 after S81
```

**Critical path:** S64 → S65 → **S81** → S70 → S72.

**Start-now, no dependencies:** S66 (server, different crate), S75/S76/S77
(character groundwork, no UI dependency).

**The three ⚑ sprints,** in order of leverage:
1. **S81** — makes the game's signature mechanic real for the first time.
2. **S70** — unblocks everything player-facing.
3. **S72** — turns the mechanic into an experience worth having.

---

# PART 4 — THE ENFORCEMENT GATES

Nearly every finding in this register is a **coverage failure**: something was
added to an enum, a trait, or a settings struct and never added to the surface
that exposes it. Build these four tests once and the class stops recurring.

| Gate | Asserts | Sprint | Would have caught |
|---|---|---|---|
| **Content dispatch** | Every `ContentPayload` variant has a registered consumer | S81 | D6, D7, D8, D9 |
| **Editor completeness** | Every `ContentType` is in the browser, in `File → New`, and implements the full trait | S68 | E6, E7, E29 |
| **Settings completeness** | Every settings field has a consumer outside `settings_ui` | S71 | C2 |
| **Player-reachable** | Sprint acceptance template requires a client surface | S64 | D1–D5 |

Add the fourth to the sprint brief template in `docs/sprints/00-INDEX.md`.
It is the cheapest single change in this entire document, and it is the one
that prevents the next S60 from shipping dark four commits before someone
notices.
