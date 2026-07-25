# ReachLock — UI/UX Audit & Improvement Plan

**Date:** 2026-07-24 · **Branch reviewed:** `testing` @ `8ad959b` (+ uncommitted working tree)
**Scope:** `reachlock-editor`, `reachlock-client`, `reachlock-server`, `reachlock-cli`
**Deliverable:** findings + a wave-ordered sprint plan. No code changed by this review.

---

## 0. Executive summary

ReachLock has an unusual shape for a project this size: **the architecture is
better than the surface**. Core is genuinely pure, the seed protocol is
disciplined, the mode state machine is clean, and the editor's `Editor` trait is
a well-designed extension point. What's weak is everything a human actually
touches.

Three findings dominate everything else:

1. **Ten of the twenty-six registered editors are unreachable and non-functional.**
   `Career`, `Dialogue`, `Dungeon`, `Ecosystem`, `Event`, `PlanetCulture`,
   `Recipe`, `ScriptedEncounter`, `Theme`, `Trope` are absent from both the
   Content Browser and `File → New`, and their `ui()` bodies are read-only
   `ui.label(format!(...))` dumps. You cannot author a dialogue tree, a dungeon,
   a recipe, or a scripted encounter in the content editor at all. Worse, the
   ones you *can* reach silently lose work (§1.1).

2. **The game client has no interactive UI.** Zero `Interaction` components,
   zero `Button` nodes, zero clickable anything. Every panel — settings,
   market, ship editor, contract workshop, dialogue — is a single
   `Text` entity holding a `format!` blob, driven by hardcoded keyboard cursors.
   This is why the settings panel needed `row_count()` hardcoded per tab, and
   why every new panel costs 200 lines of string formatting (§2.1).

3. **The accessibility settings are a placebo.** `colorblind_mode`,
   `text_scale`, `high_contrast_ui`, `subtitles`, `subtitle_size`, `ui_scale`,
   `show_fps`, `mouse_sensitivity`, `invert_y`, `controller_deadzone`,
   `aim_assist`, `show_tutorial_hints`, `combat_log_verbosity`, `show_latency`
   are all editable, persisted, and **read by nothing**. Only `screen_shake` and
   `hold_for_interact` are actually wired (§2.2).

Alongside those: the server's 2FA, email-verification, and password-reset state
is in-memory-only even under Postgres (so 2FA silently breaks on restart), the
registration rate limiter is globally self-DoSing, and **the working tree does
not compile its tests** — `make check` is red right now.

The plan below is four waves. Wave A is non-negotiable (data loss + security +
green tree). Wave B makes the editor a tool worth living in. Wave C makes the
game legible and accessible. Wave D is operations and distribution.

---

# PART A — FINDINGS

## 1. `reachlock-editor` — the content editor

The editor already has a written UX standard: `docs/EDITOR-HANDOFF.md`
§"Universal UX Pattern" (toolbar / searchable left list / `CollapsingHeader`
property form / typed widget mapping). Sixteen editors follow it well —
`soul.rs` is genuinely excellent, and `gate_network.rs` has a real
pan/zoom/drag canvas. **The problem is not a missing standard. It is that the
standard was never applied to the last ten editors, and the shell around all of
them has structural defects.**

### 1.1 Data loss — the ten stub editors (SEVERITY: CRITICAL)

`career`, `dialogue`, `dungeon`, `ecosystem`, `event`, `planet_culture`,
`recipe`, `scripted_encounter`, `theme`, `trope` share an identical broken
pattern. Four separate defects compound into silent, unrecoverable content loss:

**(a) `File → New` binds to and overwrites an existing file.**
`dialogue.rs:12-25` — `load_or_new()` scans the content directory and adopts
*the first `.ron` file it finds*, setting `self.path` to it.
`career.rs:26-51` hardcodes `compact_navy.ron`. So:

```
File → New → Career Path   →   edit   →   Ctrl+S   →   compact_navy.ron is overwritten
```

The shell's `open_new_editor` (`main.rs:216`) correctly passes `path: None`, but
the *editor* holds its own path, and `save_editor` (`main.rs:283`) delegates to
`save_all()` which writes to that internal path unconditionally.

**(b) The dirty flag is never set, so the close guard never fires.**
`career.rs:107-109` mutates `self.career.name` and `self.career.id` through
`text_edit_singleline` without ever setting `has_changes = true`. The tab shows
no `*`, `request_close_tab` (`main.rs:427`) sees a clean editor, and closes it
without prompting. Typed edits vanish. The other nine have no editable widget at
all, so they're merely useless rather than destructive — but `generate_from_seed`
sets the flag, meaning the *only* thing that marks them dirty is the seed reroll.

**(c) `save_all()` ignores dirtiness entirely.** Every one of these returns
`Ok(true)` after an unconditional write (`dialogue.rs:72-80`). Combined with
autosave (`main.rs:550`), which *does* gate on `has_unsaved_changes()`, you get
an inconsistent contract: manual save always writes, autosave never does.

**(d) New files collide.** `dialogue.rs:74` falls back to a hardcoded
`generated_dialogue.ron`. Two new dialogue trees overwrite each other.

**(e) "Reroll All" is destructive to these types.** `SeedAction::RerollAll`
(`main.rs:1041`) calls `apply_seed` on every editor whose `accept_seed_reroll()`
is true — the default. For these ten, `generate_from_seed` just *renames the
content id* (`career.rs:87`: `self.career.id = format!("gen_career_{:#x}", seed)`).
One click on Reroll All silently rewrites the IDs of every open career, event,
recipe, and trope, breaking every cross-reference pointing at them.

### 1.2 Discoverability — 10 of 26 editors have no entry point (SEVERITY: HIGH)

- `browser.rs:29-44` — `FILE_TYPES` lists **14** content types. The ten above
  are omitted, so their directories are never scanned and their files never
  appear in the tree.
- `main.rs:728-792` — the `File → New` submenu offers **16** types across five
  groups. Same ten missing.

The only way to open a dialogue tree today is `File → Open…` pointed at
`mods/reachlock/dialogues/*.ron` — and `detect_content_type` does map
`dialogues/` correctly (`app.rs:186`), so the plumbing works. The UI just never
offers it.

### 1.3 Two divergent editor registries (SEVERITY: MEDIUM — maintenance footgun)

`editors/mod.rs:34` defines `register_all()`. **Nothing calls it.** The live
registry is `app.rs:302` `build_default_registry()`, which duplicates the same
26 registrations. They have already drifted in ordering and will drift in
content. Delete one.

### 1.4 egui panel-ordering violations (SEVERITY: MEDIUM — visible layout bugs)

egui requires `CentralPanel` to be added **last**; side/top/bottom panels claim
their rect in registration order. `main.rs` violates this twice:

- `main.rs:1217` — `SidePanel::right("preview_panel")` is registered *after*
  `CentralPanel` (`main.rs:1155`). The preview panel overlays the central
  editor instead of shrinking it.
- `main.rs:1166` — `TopBottomPanel::top("editor_tabs")` is registered on `ctx`
  from *inside* the `CentralPanel` closure. A context-level panel nested in a
  central panel's build.
- Panel order is also semantically odd: the left browser (`main.rs:1010`) is
  registered before the top `seed_panel` (`1037`) and `ai_bar` (`1075`), so
  those "top" bars start to the right of the browser rather than spanning the
  window.

**Compounding this:** every editor's `ui(&mut self, ctx: &egui::Context)` opens
its *own* `CentralPanel::default()` (all 26 do). Since the shell calls
`open.editor.ui(ctx)` from inside its own `CentralPanel`, there are two
`CentralPanel`s with the same id every frame. The `Editor::ui` signature should
take `&mut egui::Ui`, not `&egui::Context` — that single change makes the whole
class of bug impossible and lets the shell own layout.

### 1.5 Modals aren't modal (SEVERITY: MEDIUM)

`dialogs.rs:29` uses a plain `egui::Window`. Nothing blocks input behind it.
While "Save changes to X before closing?" is up, the user can keep typing in the
editor, click the menu bar, trigger another close, or hit Reroll All. Escape is
also grabbed globally (`dialogs.rs:52`) regardless of focus. Use `egui::Modal`
(or an explicit input-blocking `Area`).

### 1.6 The AI generation path can hang the tool forever (SEVERITY: MEDIUM)

- **No timeout.** `ai.rs:360` and `ai.rs:409` build a bare `reqwest::Client::new()`.
  A hung endpoint leaves the UI showing "Generating…" indefinitely.
- **No cancel.** There is no way to abort a running generation; `ai_running`
  only clears when the channel yields.
- **The result may never be picked up.** `main.rs:1311` gates repaints on
  `repaint_requested || !input.events.is_empty()`. Nothing sets
  `repaint_requested` while a generation is in flight, so if the user stops
  touching the mouse, the `try_recv` poll at `main.rs:962` never runs. The
  spinner freezes and the result lands only when the user jiggles the mouse.
  Autosave (`main.rs:550`) and status-message expiry have the same defect.
  Fix: `ctx.request_repaint_after(…)` while any timer or async op is live.
- **A whole tokio multi-thread runtime is built per click** and `.unwrap()`ed
  (`main.rs:1117-1120`, `settings_window.rs:117-120`). Build one runtime for the
  app.
- **Wrong schema wired for Dialogue.** `schema.rs:64`:
  `ContentType::Dialogue => "ecosystem"` — a placeholder that was never
  replaced. AI generation for dialogue trees validates against the *ecosystem*
  schema, so it will accept nonsense and reject valid dialogue.

### 1.7 API key handling (SEVERITY: MEDIUM)

`settings_window.rs:12,75` — the LLM API key is stored in plaintext RON at
`save/editor-settings.ron` and rendered in a plain `text_edit_singleline` (no
`.password(true)`, always shoulder-visible). `save/` *is* gitignored, so it
won't be committed, but:

- The path is **relative to CWD**. Launch the editor from anywhere but the
  workspace root and it silently loses settings, preferences, recent files, and
  the content root — then writes a fresh `save/` wherever you happened to be.
  Same defect in `preferences_window.rs:8`, `settings.rs:959`,
  `save_backend.rs:9`. Use a proper config dir (`directories`/`dirs` crate) with
  a documented override.
- Prefer OS keyring, or at minimum an env-var override
  (`REACHLOCK_EDITOR_API_KEY`) so keys never need to touch disk.

### 1.8 Smaller editor issues

| # | Finding | Location |
|---|---|---|
| a | Content browser deletes files with `fs::remove_file` — permanent, no trash, no undo | `browser.rs:314` |
| b | Browser never shows which file is open; `selectable_label(false, …)` is hardcoded | `browser.rs:242` |
| c | Every browser rescan (2 s TTL) `read_to_string`s every `hulls/*.ron` to classify it | `browser.rs:60,128` |
| d | Directory scan is synchronous on the UI thread — will stutter as content grows | `browser.rs:99` |
| e | Menu shortcut hints are hardcoded spaces (`"Open…            Ctrl+O"`) — misaligns at any zoom or font | `main.rs:793-841` |
| f | Preferences save to disk on *every* `changed()` frame — dragging the font slider writes the file per frame | `preferences_window.rs:192` |
| g | Undo only tracks the active tab (`main.rs:1304`); background changes (Reroll All, AI apply) land as one delayed step | `main.rs:111` |
| h | No tab keyboard navigation (Ctrl+Tab, Ctrl+1..9), no tab reordering, no drag | `main.rs:1166` |
| i | `preview_ui`, `snapshot`/`restore_snapshot`, `selected_entry_name`/`delete_selected`, `apply_ai_json`, `touch` are unimplemented on all 10 stub editors — no undo, no preview, no Delete key, no AI | trait defaults `app.rs:196-284` |
| j | `editors/hull.rs` (282 lines) is dead — commented out of `mod.rs:14-18` but still on disk | `editors/mod.rs` |
| k | Mutex `.unwrap()` on `ai_status` in 6 places — a poisoned lock panics the UI thread | `main.rs:977-1136` |

### 1.9 What's already good (preserve it)

`soul.rs` is the reference: species colors + hints, `CollapsingHeader` sections
with *explanatory* titles ("Personality — voice and vocabulary"), a real
dialogue-graph sub-editor with `ComboBox` node targets, ranged `DragValue`s,
per-entry dirty tracking via `touch()`, correct multi-entry `save_all`, snapshot
undo, and a preview card. `gate_network.rs` has pointer-anchored scroll zoom,
middle/right-drag pan, node dragging, and status-colored arrows. **These two are
the bar. The plan below is largely "make the other 24 match."**

---

## 2. `reachlock-client` — the game

### 2.1 There is no UI layer (SEVERITY: HIGH — architectural)

Confirmed by survey: **0** `Interaction` components, **0** bevy `Button` nodes,
**0** `ImageNode`, **4** `BackgroundColor` across the entire crate. 39
`Text::new` entities and 37 `Node` blocks, all absolutely positioned.

Every "panel" is one text entity whose contents are rebuilt by a `format!`:

- `hud.rs:69-220` spawns 11 absolutely-positioned text entities at hardcoded
  px/% offsets. `update_hud_status` (`hud.rs:227`) needs an 8-way `ParamSet`
  because Bevy's system-param arity cap was hit — and it *still* had to be split
  into `update_hud_panels`.
- `settings_ui.rs` is 744 lines of string formatting plus a hardcoded
  `row_count()` per tab (`settings_ui.rs:66-75`). Add a setting, forget to bump
  the count, and the cursor silently can't reach it.
- The market, ship exterior editor, and ship interior editor all *share the same
  text entity* (`hud.rs:391-417`) and are distinguished by an enum. Two panels
  can never be open at once.

**Consequences:** no mouse anywhere in the game (including the main menu), no
gamepad, no focus model, no hover/tooltips, no scrolling in long lists, no
localization seam, no reflow on window resize, no screen-reader surface,
overlapping panels at small window sizes, and a per-panel cost of ~200 lines.

### 2.2 Accessibility settings are non-functional (SEVERITY: HIGH)

Audit of every field in `settings.rs` against its consumers outside
`settings_ui.rs`:

| Setting | Wired? | Notes |
|---|---|---|
| `accessibility.screen_shake` | ✅ | `ship.rs:563` |
| `accessibility.hold_for_interact` | ✅ | `interaction.rs:218` |
| `accessibility.colorblind_mode` | ❌ | **no consumer** |
| `accessibility.text_scale` | ❌ | **no consumer** |
| `accessibility.high_contrast_ui` | ❌ | **no consumer** |
| `accessibility.subtitles` | ❌ | **no consumer** |
| `accessibility.subtitle_size` | ❌ | **no consumer** |
| `video.ui_scale` | ❌ | **no consumer** |
| `video.show_fps` | ❌ | **no consumer** |
| `controls.mouse_sensitivity` | ❌ | **no consumer** |
| `controls.invert_y` | ❌ | **no consumer** |
| `controls.controller_deadzone` | ❌ | **no consumer** (no gamepad support at all) |
| `gameplay.aim_assist` | ❌ | **no consumer** |
| `gameplay.show_tutorial_hints` | ❌ | **no consumer** (no tutorial exists) |
| `gameplay.combat_log_verbosity` | ❌ | **no consumer** |
| `network.show_latency` | ❌ | **no consumer** |
| audio volumes, fullscreen, vsync, resolution, render_scale, auto_dock, autosave interval, server_url, voice device | ✅ | wired |

A colorblind player can set Deuteranopia, see it persist across restarts, and
receive exactly zero change. That is worse than not offering the option.

There is also **no `reduce_motion` setting** despite heavy screen shake, barrel
rolls, camera blends, parallax dust, and hyperspace transit effects.

### 2.3 Onboarding and legibility (SEVERITY: MEDIUM — this is the "fun" gap)

- **Main menu** (`menu.rs`) is a keyboard-only two-item list: Launch, Settings.
  No Quit, no Continue/New Game distinction, no mouse, no seed entry despite
  "the seed IS the game" in the module doc — the seed is *displayed* but not
  editable.
- **No tutorial, no tooltips, no contextual hints.** `show_tutorial_hints`
  exists as a setting with nothing behind it. The only teaching surface is
  `HelpText` (`hud.rs:160`), a 12 px grey keybind list in the corner.
- **The deliberation moment is the game's signature and it renders as one line
  of text** (`hud.rs:291-301`): `"⟳ {crew} is considering the situation…"`. Iron
  rule #5 says every LLM call has a *visible deliberation state* — it's visible,
  but it carries none of the drama the spec §18 / S38 "Deliberation Theater"
  describes. This is the single highest-leverage "fun" investment in the client.
- **Mode transitions are text swaps.** `"DOCKING…"`, `"HYPERSPACE…"` in the
  location banner.
- **No damage/threat feedback hierarchy.** Hull, shields, fires, and breach all
  compete as equal-weight text in one status line (`hud.rs:256`).
- **Colour is the only channel for state** in several places (offline badge,
  fuel/hull, faction standing), which interacts badly with §2.2.

### 2.4 Client correctness notes

- `settings_ui.rs:66-75` — `row_count()` hardcoded per tab; drifts silently.
- `pause.rs:38` and `menu.rs:88` both early-return when `ui.open`, so the
  settings panel owns the keyboard by convention rather than by a focus stack.
  A third overlay will conflict.
- `main.rs:105-111` — `CurrentLocation` is constructed with a magic literal
  `system_seed: 16843009` (`0x01010101`) inline in the app builder.
- Panels are toggled by independent booleans (`ReputationPanelVisible`,
  `CulturePanelVisible`, `DiscoveryPanelVisible`, `CareerPanelVisible`,
  `MissionBoardVisible`, `SignatureCollectorVisible`, …) with no mutual
  exclusion and no z-order. Several can occupy the same screen space at once.

---

## 3. `reachlock-server`

### 3.1 Auth state is in-memory even under Postgres (SEVERITY: CRITICAL)

`ws/mod.rs` `AppState` holds these as plain `Arc<Mutex<…>>`, unconditionally —
there is no Postgres-backed variant:

```
verification_tokens   Mutex<HashMap<String,(String,i64)>>
reset_tokens          Mutex<HashMap<String,(String,i64)>>
totp_secrets          Mutex<HashMap<String,String>>       ← 2FA secrets
totp_recovery_codes   Mutex<Vec<(String,String)>>         ← 2FA recovery
oauth_flows           Mutex<HashMap<String,String>>
```

Consequences:
- **Server restart destroys all 2FA enrollments.** Users who enabled 2FA get
  logged in without it thereafter, with no notification. Security regression
  that presents as "it just works."
- Pending email verifications and password resets die on restart. Since
  `login` refuses unverified accounts (`auth.rs:1045`), a restart between
  registration and verification **permanently bricks the account** — `resend`
  requires a session, and registration returns one, so it's recoverable only if
  the client kept that token.
- Nothing works across more than one server instance. No horizontal scaling.
- `auth.rs:1053` comments acknowledge this (`"For now, TOTP secrets are
  in-memory map on state"`). It was never followed up.

Sessions are equally in-memory (`MemorySessionStore`, `auth.rs:375`) — but at
least `SessionStore` is a trait with a documented Redis/Postgres path.

### 3.2 Registration rate limiter is a global self-DoS (SEVERITY: HIGH)

`auth.rs:933-939`:

```rust
let client_ip = headers.get("x-forwarded-for")
    .and_then(|v| v.to_str().ok())
    .unwrap_or("unknown");
if REGISTER_LIMITER.is_limited(&format!("register:{}", client_ip)) { … }
```

with `REGISTER_LIMITER = AuthRateLimiter::new(1, 3600)` (`auth.rs:61`).

- With no reverse proxy setting `X-Forwarded-For`, **every** request keys on
  `register:unknown` → the entire server accepts **one registration per hour,
  globally**.
- With a proxy, the header is client-controlled and trivially spoofed, so the
  limit is bypassed by rotating a made-up value.

Both failure modes at once. The limiter must key on the peer socket address
(`ConnectInfo<SocketAddr>`) or a trusted-proxy-validated XFF, not a raw header.

Related: `LOGIN_LIMITER.is_limited(&body.login)` (`auth.rs:999`) keys on
attacker-controlled login text — case/whitespace variants each get a fresh
bucket. And `AuthRateLimiter.buckets` (`auth.rs:32`) grows without bound; unique
keys are an easy memory-exhaustion vector.

### 3.3 Other auth defects

| # | Finding | Location |
|---|---|---|
| a | **`AuthConfig` TTLs are ignored.** `password_reset_token_ttl_mins` (60), `verification_token_ttl_hours` (24), `session_ttl_hours` (24) are configurable and settable via `/admin/auth-config`, but the code hardcodes `now_secs() + 24*3600` and sessions never expire at all | `auth.rs:973,1128,1150`; `MemorySessionStore` has no expiry |
| b | **2FA enrollment activates before verification.** `tfa_enable` inserts into `totp_secrets` immediately (`auth.rs:1249`), and `login` gates on that map's presence (`auth.rs:1054`). A user who never scans the QR is locked out of their own account | `auth.rs:1235` |
| c | **Recovery codes are 32-bit.** `generate_crypto_token(4)` → 4 bytes → `XXXX-XXXX`. Brute-forceable, and the challenge endpoint has no rate limit | `auth.rs:1255,1365` |
| d | **Recovery-code use burns *all* codes.** `retain(|(pid,_)| pid != &player_id)` deletes every remaining code for the player, then pushes one replacement. Comment says "burn the code and generate a replacement" — it burns ten | `auth.rs:1369` |
| e | Password reset doesn't check `banned_at` / `deleted_at` | `auth.rs:1159` |
| f | `login`'s 2FA branch returns `player_id` *before* the second factor is satisfied; the success branch returns `None`. Inconsistent, and leaks an identifier pre-auth | `auth.rs:1056,1079` |
| g | Verification/reset emails hardcode `https://reachlock.example` — a placeholder domain. In production, users receive dead links | `auth.rs:976,1131,1153` |
| h | Registration returns a live session token *before* email verification, while `login` refuses unverified accounts. Two different verification policies on the same account state | `auth.rs:963` vs `auth.rs:1045` |

Credit where due: Argon2id with tuned params, constant-time comparisons via
`subtle`, a dummy-verify on unknown login to blunt timing enumeration, AES-GCM
for TOTP secrets at rest, split selector/verifier tokens, and a constant-time
admin key check (`admin.rs:53`) are all correct and well done.

### 3.4 HTTP surface gaps (SEVERITY: MEDIUM)

No `CorsLayer`, no `DefaultBodyLimit`, no `TraceLayer` anywhere in the router.
The absent CORS layer specifically blocks the S24 WASM web client from calling
any HTTP endpoint cross-origin — a shipping blocker for web distribution.

### 3.5 The LLM proxy accepts a client-supplied system prompt (SEVERITY: MEDIUM)

`llm_proxy.rs:43-46` — `LlmOverrides::system_prompt: Option<String>` "replaces
the generic wrapper as the TRUE system prompt". Any authenticated player can
therefore use the server as a general-purpose LLM proxy on the operator's API
key, for any purpose. Quota (`quota.rs`) bounds the *cost* but not the *use*.
For a paid-tier product this needs a prompt allowlist/template-id scheme, or at
minimum server-side prefixing that the client cannot displace, plus logging of
non-template prompts.

---

## 4. Cross-cutting: build, gates, and docs

- **The tree does not compile its tests.**
  `reachlock-server/tests/content_distribution.rs` references
  `ServerMessage::ContentSync` and `ClientMessage::RequestContent`, neither of
  which exists on the wire enums (6× `E0599`). `make check` runs `cargo test
  --workspace` — **it is red right now.**
- **`make clippy` misses test code.** `cargo clippy --workspace -- -D warnings`
  has no `--all-targets`, which is exactly why the above slipped through.
- **The WASM gate does not exist.** `AGENTS.md` and `CLAUDE.md` both state
  "`make check` — fmt, clippy, all tests, WASM build". The Makefile's `check`
  target is `fmt clippy test`. The `.PHONY: web web-serve wasm-bindgen-cli`
  line declares three targets **with no recipes at all**. S24's web
  distribution has no build path.
- 3 live warnings in `reachlock-core` (unused import `crate::util::Fixed`,
  items after test module, `== false`).

---

# PART B — THE PLAN

Four waves. Wave A is a prerequisite for everything; B, C, D can run in
parallel after it. Each sprint is sized to one session per the repo playbook,
branched `sprint-v2/sXX-name` off `testing`.

## Wave A — Stop the bleeding (must ship first)

### S64 — Green tree & real gates
*Small. Unblocks every other sprint's `make check`.*

- Fix or delete `reachlock-server/tests/content_distribution.rs`. If content
  distribution is a real feature, add the `ContentSync`/`RequestContent`
  variants and the wire-shape test that pins them (iron rule #4). If it was cut,
  delete the test and note it.
- `make clippy` → `cargo clippy --workspace --all-targets -- -D warnings`.
- Add a real `wasm` target (`cargo build --workspace --target
  wasm32-unknown-unknown` excluding the editor/server) and wire it into `check`,
  matching what AGENTS.md already claims.
- Write the missing `web` / `web-serve` / `wasm-bindgen-cli` recipes, or drop
  the `.PHONY` line and mark S24 as unstarted in the index.
- Clear the 3 core warnings.

**Gate:** `make check` green from a clean clone.

### S65 — Editor data-loss closure
*The single highest-value fix in the repo.*

- **`Editor::ui` takes `&mut egui::Ui`, not `&egui::Context`.** Mechanical
  change across 26 editors; the shell owns all panel layout thereafter. Kills
  the nested-`CentralPanel` class of bug permanently.
- Fix shell panel ordering in `main.rs`: browser → seed → ai-bar → tabs →
  status → preview → **`CentralPanel` last**.
- **`File → New` must produce an empty document.** Split `load_or_new()` into
  `new()` (blank defaults, `path: None`) and `load()`. No editor may adopt an
  arbitrary existing file on construction.
- **Every mutating widget sets the dirty flag.** Introduce a
  `changed |= ui.add(…).changed()` discipline (already used correctly in
  `widgets.rs`) and a `#[must_use]`-style helper so it can't be forgotten.
- **`save_all()` must respect dirtiness**; unify the manual-save and autosave
  contracts.
- **Unsaved-file save target** derives from the entry id with collision
  handling, never a hardcoded stem.
- **`accept_seed_reroll()` returns `false`** for every editor whose
  `generate_from_seed` only renames an id. Better: make Reroll All show a
  preview of what it will touch and require confirmation.
- `dialogs.rs` → `egui::Modal` (true input blocking, focus-scoped Escape).
- Delete the dead `editors::register_all` and `editors/hull.rs`.
- Fix `schema.rs:64` — map `Dialogue` to a real dialogue schema (author it if
  S53 left it pending) or return `None` so the AI bar honestly reports
  "no schema".

**Gate:** a test that, for every registered `ContentType`, constructs a new
editor, asserts `path == None` and `has_unsaved_changes() == false`, mutates it,
and asserts the dirty flag flips. Plus a test that `save_all` on a clean editor
writes nothing.

### S66 — Server auth hardening
- Move `verification_tokens`, `reset_tokens`, `totp_secrets`,
  `totp_recovery_codes`, `oauth_flows` behind traits with in-memory **and**
  Postgres implementations, mirroring `PlayerStore`/`SessionStore`. Encrypt TOTP
  secrets at rest via the existing `encrypt_secret` path (already correct).
- Rate limiting: key on `ConnectInfo<SocketAddr>`; accept `X-Forwarded-For` only
  from a configured trusted-proxy list. Bound the bucket map (LRU or periodic
  sweep). Add a limiter to `tfa_challenge`.
- Honour `AuthConfig` TTLs everywhere; add session expiry to `SessionStore`.
- Two-phase 2FA enrollment: store as *pending* until `tfa_verify` succeeds;
  `login` gates on *confirmed* only.
- Recovery codes: ≥128 bits, burn exactly one, keep the rest.
- `reset_password` checks `banned_at` / `deleted_at`; `login` returns a
  consistent response shape.
- Email base URL from config (`REACHLOCK_PUBLIC_URL`), not a placeholder domain.
- Decide the verification policy: either registration does *not* return a
  session, or `login` allows unverified accounts with reduced scope. Pick one.
- Add `CorsLayer` (configurable origins), `DefaultBodyLimit`, `TraceLayer`.
- LLM proxy: template-id or allowlist for `system_prompt`; log and meter
  free-form prompts separately.

**Gate:** integration test — enable 2FA, restart the server (rebuild `AppState`
from the same Postgres), log in, and the second factor is still required.

---

## Wave B — Make the editor a tool worth living in

### S67 — Editor shell v2
*Now that `Editor::ui` takes a `Ui`, the shell can be good.*

- **Command palette** (Ctrl+P): jump to any content file by name across all
  types; jump to any command. This is the single biggest usability win for a
  26-type editor.
- **Tab navigation**: Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+1..9, middle-click close,
  drag to reorder, overflow menu. Close buttons on the tab itself, not as a
  separate `x` button beside it (`main.rs:1198`).
- **Async correctness**: one shared tokio runtime; `reqwest` timeouts
  (connect + total, configurable); a Cancel button; `ctx.request_repaint_after`
  while any async op or timer is live, so autosave, status expiry, and AI
  results fire without mouse motion.
- **Secrets**: OS keyring for the API key with env-var override; mask the field;
  move config to a platform config dir with a `REACHLOCK_EDITOR_CONFIG`
  override. Same for the client's `save/`.
- **Browser**: async/debounced scan off the UI thread; cache hull classification
  by mtime; highlight the open file; show dirty state per file; move Delete to
  the OS trash (`trash` crate) with an undo toast.
- **Preferences**: debounce disk writes (save on close/blur, not per frame).
- **Menus**: use egui's `shortcut_text` instead of hardcoded spacing.
- **Undo**: track every open tab, not just the active one; label steps
  ("Undo: rename soul") rather than "Undo (7 left)".
- Replace `.lock().unwrap()` with poison-tolerant access so a background thread
  panic can't take down the UI.

### S68 — The missing ten
*Bring `career`, `dialogue`, `dungeon`, `ecosystem`, `event`, `planet_culture`,
`recipe`, `scripted_encounter`, `theme`, `trope` up to the `soul.rs` bar.*

Each gets the full `EDITOR-HANDOFF` Universal UX Pattern: searchable left
entry list, `CollapsingHeader` property form with explanatory section titles,
typed widgets (`ComboBox`/`DragValue` with ranges/`TextEdit`/checkbox), inline
validation, `touch()` per-entry dirty tracking, `save_all` fan-out,
`snapshot`/`restore_snapshot`, `selected_entry_name`/`delete_selected`,
`preview_ui`, `apply_ai_json`.

Two of these deserve bespoke graphical editors rather than forms — model them on
`gate_network.rs`:

- **Dialogue tree** — node graph canvas: nodes by `NodeType` (colour-coded),
  choice edges with condition badges, drag to connect, orphan/dead-end
  detection, and a "walk the tree" simulator panel that plays the dialogue with
  test variables bound.
- **Dungeon layout** — grid canvas: draw/resize rooms, drag connectors, tag
  rooms (entrance/boss/puzzle/treasure) with icons, live connectivity check
  ("boss room unreachable from entrance").

Also register all ten in `browser.rs::FILE_TYPES` and the `File → New` menu,
grouped sensibly (add a "Narrative" group: Dialogue, Event, Trope, Scripted
Encounter, Storyline; a "World" group: Ecosystem, Planet Culture, Dungeon; a
"Systems" group: Career, Recipe, Theme).

**Gate:** a table-driven test asserting every `ContentType::all()` entry
(a) appears in `FILE_TYPES` or is an explicit previewer, (b) appears in the
`File → New` menu tree, (c) implements `snapshot` and `preview_ui`. Make the
test the enforcement mechanism so the eleventh editor can't repeat this.

### S69 — Authoring superpowers
*This is what makes the editor genuinely fun and what protects the game's
longevity.*

- **Cross-reference index.** Content ids are strings everywhere (`faction_id`,
  `item_id`, `next_node`, `system_id`, …). Build an index at load and offer:
  ID autocomplete on every reference field; go-to-definition (click a
  `faction_id` → opens that faction); find-usages; and a **broken-reference
  report** in the Validate All window. Renaming an id offers to update all
  referrers. Today a typo'd `next_node` is discovered at runtime, if ever.
- **Live validation**, not just on-demand: run `validate()` each frame (or
  debounced) and render errors inline next to the offending field in red, per
  the handoff spec. Surface a per-tab error count on the tab itself.
- **Diff before save.** Show the RON diff for the dirty entries. Given the known
  "RON round-trip drops comments" gotcha, also warn loudly when saving a file
  whose on-disk text contains comments.
- **Playtest hook.** `Preview → Launch in game` — boot the client with
  `--content-root <this dir> --start <this entity>` so an author can see a
  station, soul, or dungeon in situ without a full session. Closes the loop
  between authoring and play, which is the whole longevity argument.
- **Templates & duplication.** Right-click → Duplicate (with id auto-increment),
  and a per-type template library so a new soul starts from an archetype rather
  than blank.

---

## Wave C — Make the game legible, accessible, and fun

### S70 — Client UI framework
*The enabling sprint. Everything in C depends on it.*

- Adopt a real widget layer. Two viable routes — pick one deliberately:
  - **`bevy_egui`** — fastest path, and immediately shares idioms and possibly
    code with the content editor. Best fit given the team already has deep egui
    fluency. Weaker for a diegetic sci-fi look.
  - **Native `bevy_ui`** with a small in-house widget kit (button, list,
    scroll, slider, tabs, focus ring, tooltip, modal). More work; better art
    direction control; keeps WASM size down.
- Build the missing primitives either way: a **focus/input stack** (replacing
  the `if ui.open { return; }` convention in `pause.rs`/`menu.rs`), **panel
  z-order and mutual exclusion** (replacing the six independent `*Visible`
  booleans), **mouse + gamepad + keyboard** on every control, hover tooltips,
  and scrollable lists.
- Port the highest-traffic surfaces first: main menu (add Quit, Continue, and an
  editable seed field — "the seed IS the game"), settings, market, mission
  board, ship editor.
- Delete `settings_ui.rs::row_count()`; rows come from the widget tree.

### S71 — Make accessibility real
*Every setting in §2.2 gets a consumer, or gets removed.*

- `text_scale` / `ui_scale` / `subtitle_size` → drive a UI scale resource that
  every text and layout node reads (trivial once S70 lands).
- `high_contrast_ui` → a second palette with a documented minimum contrast
  ratio; test it.
- `colorblind_mode` → a **semantic palette**: no gameplay state may be conveyed
  by hue alone. Pair every colour with a glyph/shape/pattern (the editor's
  Validation Report already does this correctly with ✔/✘ — copy that). Provide
  Protanopia/Deuteranopia/Tritanopia-safe variants.
- `subtitles` → captions for voice lines and comm chatter (S29/S62 voice already
  exists and is currently audio-only).
- `mouse_sensitivity`, `invert_y`, `controller_deadzone` → wire, and add actual
  gamepad support (the deadzone setting implies it and it doesn't exist).
- `aim_assist`, `combat_log_verbosity`, `show_latency`, `show_fps` → wire or
  delete. Do not ship a setting with no effect.
- **Add `reduce_motion`**, gating screen shake, barrel roll, camera blends,
  parallax, and hyperspace effects.
- Add remappable-everything verification: a test that every `InputAction` is
  reachable in the Controls tab and that no system reads a hardcoded `KeyCode`.
  (Note the existing memory'd gotcha: a new `KeyCode` must be added to the
  `settings.rs` name/from_name/display string table or it round-trips as `KeyF`.
  Make that a compile-time-exhaustive match or a test, not a convention.)

**Gate:** an "accessibility conformance" test enumerating every settings field
and asserting a non-`settings_ui` consumer exists. It should be impossible to
add a dead setting.

### S72 — Deliberation Theater & onboarding
*This is the "fun" sprint. The deliberation moment is ReachLock's signature and
it currently renders as one line of grey text.*

- **Deliberation Theater** (spec §18 / S38): when a crew member deliberates,
  stage it. Portrait or console framing, the *actual* context they're weighing,
  the rules that ran out, a visible thinking beat, the verdict, and the
  consequence — with the player able to interject. Make the co-deliberation
  crew conference (`comms.rs`, S33) a real scene rather than a text feed.
  Honour iron rule #5 by making the visible state *informative*, not merely
  present.
- **Onboarding**: a first-run sequence that teaches the three modes, docking,
  the contract engine, and what a seed means. Wire `show_tutorial_hints`.
  Contextual first-time hints keyed to actions, dismissible and re-enableable.
- **Feedback hierarchy** in the HUD: threat and damage states escalate visually
  (colour + shape + motion + audio), rather than competing as equal text in one
  line. Distinct treatments for hull breach, fire, shield collapse, and fuel
  critical.
- **Mode transitions** get real beats — docking, undocking, hyperspace entry and
  wake, cryo. The `TransitionBeat` resource and `Docking`/`Undocking` states
  already exist to hang this on.
- **Diegetic help**: replace the corner keybind dump with contextual prompts on
  the object being looked at, plus a proper in-game reference (the ship's
  computer) that reads live keybinds.

---

## Wave D — Operations & distribution

### S73 — Server operations surface
- Admin endpoints exist (`ws/admin.rs`) but have no interface. Build a minimal
  operator console (a static page served by the server, or extend
  `reachlock-cli`): player management, audit log, quota/cost dashboards, tick
  control, health. `services/metrics.rs` + `observability.rs` are already in
  place to feed it.
- Structured request logging (`TraceLayer`) with request ids, and Prometheus
  metrics for auth outcomes and rate-limit hits.

### S74 — Web distribution
- Deliver the missing `web` / `web-serve` build (S24, currently `.PHONY` with no
  recipe). Verify the CORS layer from S66 against the WASM client, and confirm
  `localStorage` save-backend parity with the native filesystem backend.
- Enforce `SIZE_BUDGET`.

---

## Suggested ordering

```
S64 (green tree) ──┬─► S65 (editor data loss) ──► S67 (shell v2) ──► S68 (ten editors) ──► S69 (superpowers)
                   │
                   ├─► S66 (auth hardening) ────────────────────────► S73 (ops) ──► S74 (web)
                   │
                   └─► S70 (client UI framework) ──┬─► S71 (accessibility)
                                                   └─► S72 (theater & onboarding)
```

S64 → S65 → S70 are the critical path. S66 can run fully in parallel (different
crate, no shared types). S68 is the largest single body of work and is
parallelizable across ten independent files once S65 lands the trait change.

## Two enforcement mechanisms worth building once

Both of the biggest findings in this audit are *coverage* failures — a thing was
added to an enum and never added to the UI. Rather than fixing them once, make
them un-repeatable:

1. **Editor completeness test** (S68 gate) — every `ContentType` must appear in
   the browser, the New menu, and implement the full trait surface.
2. **Settings completeness test** (S71 gate) — every settings field must have a
   consumer outside the settings UI.

These two tests are cheap and would have caught both of this review's headline
findings at the commit that introduced them.
