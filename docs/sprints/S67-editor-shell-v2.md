# S67 — Editor Shell v2

**Spec:** New (editor shell rewrite) ·
**Wave B · Depends on:** S65

Closes findings: E14, E15, E16, E17, E19, E20, E21, E22, E23, E24, E25, E26, E27, E28, E31
Includes deferred E12 from S65 (`ui(&mut Ui)` trait refactor across 26 editors)

## Outcome

The editor shell is production-ready. Every finding from the MASTER-PLAN review
— timeouts, cancel, repaint starvation, per-click runtimes, plaintext API keys,
relative paths, permanent delete, stale browser state, synchronous disk scans,
hardcoded menu padding, per-frame prefs writes, single-tab undo, missing tab
navigation, poisoned-lock panics — is closed with a concrete fix. The `Editor`
trait gains `fn ui(&mut self, ui: &mut egui::Ui)` and the shell owns all panel
creation, ending the pattern where each editor opens its own
`CentralPanel`/`SidePanel`/`TopBottomPanel`.

## Context

- **E14 — No timeout on AI requests.** `reqwest::Client::post().send().await`
  has no timeout. If the endpoint hangs the UI shows "Generating…" forever.
  Every HTTP call in `ai.rs` and `settings_window.rs` needs a timeout.
- **E15 — No cancel for running generation.** Once the thread spawns at
  `main.rs:1113`, the user has no way to abort. The cancel button needs a
  `tokio::AbortHandle` (or equivalent) wired through the shell.
- **E16 — Repaint gating starves async results.** `main.rs:1311` only calls
  `request_repaint` when `self.repaint_requested` or there was input this frame.
  Async results (`try_recv`), autosave ticks, and status expiry timers never
  trigger a repaint. The fix is `ctx.request_repaint_after()` for every timer
  path and a repaint request when the AI result channel delivers.
- **E17 — Per-click tokio runtime.** Both `main.rs:1115` and
  `settings_window.rs:117` build a new `tokio::runtime::Builder` on every click,
  calling `.unwrap()`. A single shared runtime (started once in `main`) with
  `spawn` + `AbortHandle` eliminates the cost and the panic path.
- **E19 — API key plaintext on disk, unmasked in UI.** `settings_window.rs:75`
  shows the key in a plain `TextEdit`. The key should be `password(true)` in the
  UI and stored via the OS keyring (e.g. `secret-service` on Linux, Keychain on
  macOS, Credential Manager on Windows) rather than `save/editor-settings.ron`.
  The fallback is an encrypted config file, but the keyring is the primary path.
- **E20 — Relative config paths.** `preferences_window.rs:8` and
  `settings_window.rs:12` hardcode paths relative to CWD. Silent data loss when
  the editor is launched from a different directory. Fix: use
  `dirs::config_dir()` / `dirs::data_dir()` for the editor's own files
  (preferences, settings, schemas), and the Preferences `content_root` field for
  game content. The editor's working directory is irrelevant.
- **E21 — File delete is permanent.** `browser.rs:314` calls `remove_file()`.
  Trash integration (`trash::delete` or `xdg-trash`) lets users recover mistakes.
- **E22 — Browser never highlights open file.** The browser tree shows all files
  but doesn't indicate which ones are already open in a tab. No visual feedback.
- **E23 — Every 2s rescan re-reads hulls for classification.** `browser.rs:60`
  calls `std::fs::read_to_string` on every `hulls/*.ron` file every 2 seconds to
  determine the content type. Cache the classification after the first read.
- **E24 — Directory scan is synchronous on the UI thread.** `browser.rs:99`
  runs `std::fs::read_dir` and per-file reads on the egui thread. The scan
  should be async (spawned on the shared runtime) or deferred to a background
  thread with a channel result.
- **E25 — Hardcoded menu shortcut padding.** `main.rs:791-841` uses spaces to
  right-align keyboard shortcuts in menu items. egui's `menu_button` supports
  shortcut display natively — use `Command::shortcut_text()` or the `shortcut`
  parameter on `Button`.
- **E26 — Preferences written to disk every changed frame.**
  `preferences_window.rs:192` saves on every frame a slider moves.
  Debounce: write 500ms after the last change, or on window close.
- **E27 — Undo tracks only the active tab.** `main.rs:1304` calls
  `track_changes` only on the active editor. Inactive tabs' undo stacks never
  grow. Fix: iterate all open editors every frame.
- **E28 — No tab keyboard nav, reorder, or overflow.** Tabs are a horizontal
  row with no Ctrl+Tab/Ctrl+Shift+Tab cycling, no drag-to-reorder, and no
  overflow handling when many tabs are open.
- **E31 — `.lock().unwrap()` on `ai_status` ×6.** Six call sites lock the
  `Arc<Mutex<String>>` and unwrap. A poisoned lock (from a panic in the AI
  thread while holding the lock) panics the UI thread. Use `.lock().ok()`
  with a fallback string, or switch to an `Arc<AtomicBool>` + a channel for
  the status string.

- **E12 (deferred from S65) — `Editor::ui` takes `&mut egui::Ui` instead of
  `&egui::Context`.** All 26 editors currently create their own panels
  (`CentralPanel`, `SidePanel`, `TopBottomPanel`) inside `fn ui(&ctx)`. The
  shell should own all panel creation, passing a `&mut Ui` to the editor's
  content area. Editors that need a left sidebar (soul, station, enemy, …)
  declare that via an optional `fn side_bar(&mut self, ui: &mut egui::Ui)`.
  Editors that need a top toolbar (gate_network, soul, …) declare that via
  an optional `fn top_bar(&mut self, ui: &mut egui::Ui)`. The shell creates
  the panels once in the correct order, eliminating E9/E10/E11.

## Freeze first

### Editor trait — new method surface (`reachlock-editor/src/app.rs`)

The old `fn ui(&mut self, ctx: &egui::Context)` becomes `fn ui(&mut self, ui: &mut egui::Ui)`,
receiving the `Ui` from the shell's `CentralPanel`. Two new optional methods let
editors request a top toolbar and a left sidebar:

```rust
pub trait Editor {
    // unchanged methods — title, content_type, has_unsaved_changes, touch,
    // load, save, save_all, validate, generate_from_seed, apply_ai_json,
    // snapshot, restore_snapshot, mark_saved, accept_seed_reroll, apply_seed,
    // selected_entry_name, delete_selected, preview_ui

    /// MAIN CHANGE: receives the CentralPanel's Ui instead of creating its own.
    fn ui(&mut self, ui: &mut egui::Ui);

    /// Optional: render a top toolbar (horizontal strip above the editor).
    /// Called inside the shell's TopBottomPanel::top("editor_toolbar").
    /// Default impl does nothing.
    fn top_bar(&mut self, _ui: &mut egui::Ui) {}

    /// Optional: render a left sidebar (entry list, search, …).
    /// Called inside the shell's SidePanel::left("editor_sidebar").
    /// `default_width` is the initial panel width; editors that don't need
    /// a sidebar return None (the default).
    fn side_bar(&mut self, _ui: &mut egui::Ui) {}
}
```

### Shell panel ownership (`reachlock-editor/src/main.rs`)

The shell creates ALL panels in a single fixed order:

```
egui::TopBottomPanel::top("menu_bar")          // File/Edit/View/AI/Help menus
egui::TopBottomPanel::top("editor_toolbar")    // active editor's top_bar()
egui::TopBottomPanel::top("editor_tabs")       // tab bar
egui::TopBottomPanel::top("seed_panel")        // seed workflow bar
egui::TopBottomPanel::top("ai_bar")            // AI generation bar
egui::SidePanel::left("browser_panel")         // content browser (if show_browser)
egui::SidePanel::left("editor_sidebar")        // active editor's side_bar()
egui::SidePanel::right("preview_panel")        // preview + recent files
egui::CentralPanel::default()                  // active editor's ui()
egui::TopBottomPanel::bottom("status_line")    // status bar
```

Every `show()` call passes `ctx` directly — no panel is nested inside another
panel's closure. Editors that don't implement `top_bar()` / `side_bar()` get
zero-height / zero-width panels (or the shell skips them).

### Shared runtime singleton

```rust
// In main() — created once, dropped on exit:
let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
```

Stored in `EditorApp` as `runtime: tokio::runtime::Runtime`. All AI generation,
connection tests, and async browser scans are `runtime.spawn()` calls returning
`JoinHandle`. Cancel via `JoinHandle::abort()`. No per-click `Builder::new_multi_thread`.

### Keyring / platform config paths

```rust
/// Primary: keyring crate (secret-service / Keychain / Credential Manager)
/// Fallback: encrypted file under platform config dir
fn store_api_key(key: &str);
fn load_api_key() -> Option<String>;

/// Editor's own data dir (preferences, settings, schemas)
fn editor_data_dir() -> PathBuf;  // dirs::data_dir()/reachlock-editor

/// Editor's config dir (keyring fallback)
fn editor_config_dir() -> PathBuf;  // dirs::config_dir()/reachlock-editor
```

## Deliverables

### 1. Editor trait migration — `ui(&mut Ui)` across all 26 editors + shell

- [ ] Change `Editor::ui` signature in `app.rs:225` from `fn ui(&mut self, ctx: &egui::Context)`
      to `fn ui(&mut self, ui: &mut egui::Ui)`.
- [ ] Add `fn top_bar(&mut self, _ui: &mut egui::Ui) {}` and
      `fn side_bar(&mut self, _ui: &mut egui::Ui) {}` with default empty impls.
- [ ] Rewrite `main.rs` panel creation: shell owns all panels, passes `ui` from
      CentralPanel to editor, conditionally shows `top_bar` and `side_bar` panels.
- [ ] Migrate every editor in `reachlock-editor/src/editors/*.rs`:
      - Remove `egui::CentralPanel::default().show(ctx, |ui| { … })` wrapper.
      - Body goes directly into `fn ui(&mut self, ui: &mut egui::Ui)`.
      - Where editor creates a `TopBottomPanel::top(…)` → move to `fn top_bar()`.
      - Where editor creates a `SidePanel::left(…)` → move to `fn side_bar()`.
      - Gate the `side_bar` width via the trait's default_width (or let the
        shell allocate a fixed default; editors that need more control can
        return a width hint via a future trait method — for now, 200px default).
      - Where an editor calls `ctx.request_repaint()` → change to
        `ui.ctx().request_repaint()`.
      - Where an editor calls `ctx.input(|i| …)` → `ui.ctx().input(|i| …)`.
- [ ] Remove `preview_ui`'s `CentralPanel` if it creates one (it takes `&Ui` already).
- [ ] **All 26 editor files** in `reachlock-editor/src/editors/`:
      `career.rs`, `character_sprite.rs`, `charted_system.rs`, `contract.rs`,
      `dialogue.rs`, `dungeon.rs`, `economy.rs`, `ecosystem.rs`, `enemy.rs`,
      `event.rs`, `faction.rs`, `gate_network.rs`, `hull_frame.rs`,
      `hull_mesh.rs`, `item.rs`, `item_browser.rs`, `location.rs`,
      `planet_culture.rs`, `recipe.rs`, `room_templates.rs`, `scripted_encounter.rs`,
      `soul.rs`, `station.rs`, `storyline.rs`, `theme.rs`, `trope.rs`.
- [ ] Update `app.rs` test: `every_content_type_is_registered` still passes.

### 2. Command palette

- [ ] Ctrl+P / Ctrl+Shift+P opens a fuzzy-search command palette overlay.
- [ ] Palette lists: `File > New > <all ContentTypes>`, `File > Open`, `File > Save`,
      `File > Save As`, `File > Close Tab`, `File > Close All`, `File > Quit`,
      `Edit > Undo`, `Edit > Redo`, `View > Toggle Browser`, `AI > Generate`,
      `Help > Help`, `Preferences…`, `AI Settings…`, `Validate All`.
- [ ] Palette is an `egui::Window` (not a modal) with a `TextEdit` filter and a
      scrollable result list. Keyboard navigation (Arrow keys, Enter to execute,
      Esc to dismiss).
- [ ] Filter is fuzzy: "stso" matches "Station → Soul" (character sequence match).
- [ ] Actions that don't apply (e.g. Undo when no undo steps exist) are dimmed
      but still visible with disabled reason shown.

### 3. Tab navigation, reorder, overflow

- [ ] Ctrl+Tab / Ctrl+Shift+Tab cycles tabs left/right.
- [ ] Tabs are drag-reorderable via `egui::DragValue` or manual `dnd` — at
      minimum, Alt+Left/Alt+Right moves the active tab.
- [ ] Overflow: when tabs exceed available width, a left/right scroll arrow
      appears on each side of the tab bar. Scroll wheel on the tab bar pans it.
- [ ] Middle-click closes a tab.

### 4. Shared tokio runtime + timeouts + cancel

- [ ] `EditorApp` holds `runtime: tokio::runtime::Runtime` initialized once
      in `main()` and passed through `eframe::CreationContext` or stored in the
      app struct.
- [ ] AI generation spawns via `self.runtime.spawn(…)` returning `JoinHandle`.
- [ ] Cancel button next to the AI spinner calls `JoinHandle::abort()`.
- [ ] `reqwest::Client` in `ai.rs` and `settings_window.rs` gets `timeout(Duration::from_secs(120))`
      on every request. `test_connection` gets `timeout(Duration::from_secs(10))`.
- [ ] `settings_window.rs` spawns `test_connection` on the shared runtime instead
      of building one.
- [ ] Status message updates on abort: "AI generation cancelled."

### 5. `request_repaint_after` for all async/timers

- [ ] All timer-driven state paths call `ctx.request_repaint_after(Duration)`:
      - Autosave tick: `request_repaint_after(1s)` after every frame while
        autosave is enabled.
      - Status expiry: `request_repaint_after(5s)` when a non-sticky status
        message is set.
      - AI result poll: `request_repaint_after(100ms)` after spawning, cleared
        when result arrives.
      - Browser scan: `request_repaint_after(2s)` when last_scan was None
        (post-invalidate).
- [ ] Remove the `repaint_requested: bool` flag — `request_repaint_after` is the
      single mechanism.
- [ ] Remove the `ctx.input(|i| !i.events.is_empty())` condition at
      `main.rs:1312` — egui already repaints on interactive input without it.

### 6. Keyring + platform config dir

- [ ] Dependencies: add `keyring`, `dirs` to `reachlock-editor/Cargo.toml`.
- [ ] `editor_data_dir()` → `dirs::data_dir()/reachlock-editor` (for schemas,
      preferences, autosave backup).
- [ ] `editor_config_dir()` → `dirs::config_dir()/reachlock-editor` (for
      settings.ron, keyring fallback).
- [ ] API key stored via `keyring::Entry` with service `"reachlock-editor"` and
      user `"ai-api-key"`. Fallback: encrypted file under `editor_config_dir()`
      using the same pattern as `save_config` but with a machine-local key
      (or just `serde` + restrictive permissions since this is a dev tool).
- [ ] Preferences path: `editor_data_dir()/preferences.ron`.
- [ ] AI settings path: `editor_config_dir()/settings.ron`.
- [ ] Schema cache path: `editor_data_dir()/schemas/`.
- [ ] Migration: on first run, copy `save/editor-preferences.ron` and
      `save/editor-settings.ron` to the new paths if they exist and the target
      doesn't. Log the migration. After S67, the old paths are never read.

### 7. API key masking

- [ ] In `settings_window.rs:75`, change `TextEdit::singleline` to
      `TextEdit::singleline(…).password(true)` so the key shows as bullets.
- [ ] Add a "Show" toggle (eye icon / checkbox) next to the key field.
- [ ] Key is loaded from keyring (via `load_api_key()`) at window open, not
      from the RON file. If keyring is unavailable, fall back to the config
      file (still masked in UI).
- [ ] On save, write to keyring (via `store_api_key()`). Also write the
      fallback file for environments without a keyring daemon, but NEVER store
      the key in the preferences/settings RON alongside non-secret values.

### 8. Async browser scan

- [ ] `browser.rs:scan_if_stale()` runs on a background thread (spawned via
      the shared runtime). Results delivered via `std::sync::mpsc::Receiver`
      stored on the browser struct.
- [ ] The browser reads the receiver once per frame at the start of `ui()`.
- [ ] While a scan is in flight, the browser shows the previous results with
      a subtle "refreshing…" indicator in the header.
- [ ] Scan is cancellable: if a new scan is requested (via `invalidate()`)
      while one is running, the old one's results are discarded on arrival.

### 9. Cache hull classification

- [ ] `browser.rs:classify_hull_file` results are cached: `HashMap<PathBuf, ContentType>`
      populated on first scan, invalidated only when `invalidate()` is called
      or the file's mtime changes.
- [ ] Avoids re-reading every hull file on every 2s scan tick.

### 10. Trash-not-delete

- [ ] Add `trash` crate to `reachlock-editor/Cargo.toml`.
- [ ] Replace `std::fs::remove_file(&path)` in `browser.rs:314` with
      `trash::delete(&path)`. Log a warning if trash is unavailable.
- [ ] On platforms without trash (e.g. headless Linux), fall back to
      `remove_file` with a confirmation dialog that says "Permanent delete."
- [ ] Status message: `"{name} moved to trash"`.

### 11. Debounced preferences

- [ ] `preferences_window.rs:192` no longer writes on every changed frame.
- [ ] Instead, set a dirty flag and start a debounce timer (500ms).
- [ ] If another change arrives within 500ms, reset the timer.
- [ ] On timer expiry, flush to disk. Also flush on app close.
- [ ] Visual feedback: the status bar shows "Preferences saved" briefly on
      write.

### 12. All-tab undo tracking

- [ ] `main.rs:1304` iterates ALL open editors, not just the active tab:
      `for open in &mut self.open_editors { open.track_changes(); }`.
- [ ] Global undo/redo (Ctrl+Z/Y) acts on the active tab as before, but every
      tab accumulates undo steps regardless of focus.

### 13. Poison-tolerant locks

- [ ] Every `self.ai_status.lock().unwrap()` in `main.rs` is replaced with
      `self.ai_status.lock().ok().map(|s| s.clone()).unwrap_or_default()` or
      equivalent. A poisoned lock returns the default string.
- [ ] Consider switching `ai_status` from `Arc<Mutex<String>>` to
      `Arc<AtomicPtr<String>>` or `Arc<RwLock<String>>` for read-heavy access.
      At minimum, the six unwrap sites are made safe.
- [ ] Same treatment for any other `Mutex`/`RwLock` lock in the editor crate
      that `.unwrap()`s.

### 14. Browser highlight open files

- [ ] Pass the set of open file paths (from `self.open_editors`) into the
      browser on each frame.
- [ ] Files that are already open show a small icon or bold label in the tree.
- [ ] Hover tooltip: "Already open in tab '{name}'".

### 15. Menu shortcut cleanup

- [ ] Replace hardcoded space-padded shortcut text with `egui::Button::shortcut()`
      or a `Label` with `TextStyle::Monospace` right-aligned via layout.
- [ ] Menus use a consistent pattern: action label on the left, shortcut on the
      right, no manual alignment.

## Acceptance gates

```
cargo test -p reachlock-editor          # all tests pass (registry, directory
                                        # round-trip, suggest_stem, etc.)
cargo run -p reachlock-editor           # editor starts, command palette on Ctrl+P
                                        # tab nav with Ctrl+Tab, AI generation
                                        # responds to Cancel, preferences
                                        # debounced, browser async
# Command palette: Ctrl+P → type "soul" → Enter opens a Soul editor
# Tab nav: Ctrl+Tab cycles tabs; Alt+Left/Right reorders
# AI cancel: enter prompt → click Generate → click Cancel within 2s → status says "cancelled"
# Preferences debounce: open Preferences → drag font scale slider → quit →
#   preferences file written at most once, not per-frame
# Keyring: set API key → close AI settings → reopen → key field shows masked bullets
# Trash: right-click a file in the browser → Delete → file moves to trash (check `$TRASH`)
# Open-file highlight: open a file → browser shows it bolded/with icon
# All-tab undo: open two editors → edit tab A → switch to tab B → edit tab B →
#   Ctrl+Z on tab B undoes B's change → switch to tab A → Ctrl+Z undoes A's change
make check
```

Manual: open 10+ files to trigger tab overflow → scroll arrows appear.
Middle-click a tab → it closes. Drag a tab by its label → it reorders.
Open editor with `WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11` (per gotcha ledger).

## Non-goals

- Editor content changes (S68 adds the 10 missing editors; S69 adds validation
  and cross-references — this sprint only touches the shell and the trait)
- Wire shapes / protocol changes (no network messages change)
- Server-side changes (editor is a standalone dev tool)
- Hot-reload or live-preview in the game client (future sprint)
- WASM build for the editor (native-only — `bevy_egui` + `wgpu` render targets
  don't compile to wasm32, per the existing exemption)
- Improving the AI generation output quality (just timeouts + cancel + keyring)
- Dark mode toggle improvements (already works via Preferences)

## Gotchas

- **`egui::Ui` borrow checker:** `ui.ctx().request_repaint()` borrows from `ui`,
  which borrows from the panel. Do not stash `ctx` across an `await` point or
  a nested `show()` call. Extract `ctx` once at the top of the closure:
  `let ctx = ui.ctx().clone();` then use `ctx.request_repaint_after(…)` inside
  closures. The compiler error says "cannot return from closure" which is
  confusing — the real issue is the nested borrow.
- **`request_repaint_after` in egui:** This method takes a `Duration` and is
  available on `&egui::Context`. It schedules exactly one repaint after the
  duration, not periodic repaints. Call it every frame for recurring timers
  (autosave, status expiry). The timer is one-shot; you must re-request after
  handling the event if you want another tick.
- **Keyring on headless Linux/WSL:** `secret-service` requires a running D-Bus
  secret service daemon. On systems without one (WSL, containers, CI), the
  `keyring` crate's `Entry::new()` succeeds but `set_password` / `get_password`
  may fail at runtime with `PlatformFailure(…)`. The fallback path (encrypted
  file under `editor_config_dir()`) must handle this gracefully — log a warning
  and store the key in a file with `600` permissions. The RON settings file
  should NEVER contain the key, even as a fallback.
- **`dirs::config_dir()` vs `dirs::data_dir()`:** On Linux, `config_dir()` is
  `~/.config/reachlock-editor` and `data_dir()` is `~/.local/share/reachlock-editor`.
  Preferences and settings (small, user-editable) go in `config_dir`; schemas
  and backups (large, auto-generated) go in `data_dir`. On macOS, both resolve
  under `~/Library/` (Application Support vs Preferences). The distinction
  matters for backup scoping.
- **Removing `repaint_requested`:** The old `request_repaint` flag was necessary
  because unconditional `ctx.request_repaint()` busy-loops at 100% CPU. With
  `request_repaint_after`, the pattern is: every timer/async path calls
  `request_repaint_after(duration)` once. egui will paint one frame at that
  point and stop. Do NOT call `request_repaint()` unconditionally in `update()`
  — that re-introduces the busy loop.
- **Shared runtime lifetime:** The tokio runtime is created in `main()` and
  stored on `EditorApp`. It must outlive any spawned `JoinHandle`. On app exit,
  the runtime drops and aborts all remaining handles. AI generation handles
  should be proactively aborted on exit to avoid "task x was leaked" warnings.
- **Tab reorder via drag:** egui does not have built-in drag-to-reorder for
  tabs. Implement as Alt+Left/Alt+Right move (simpler and keyboard-accessible).
  If drag-to-reorder is desired, use `egui::DragValue` on each tab or a
  manual `dnd` implementation — but keep it behind a `#[cfg(feature = "dnd")]`
  gate if it pulls in drop-target machinery.
- **`trash` crate on Linux:** Requires `gio` or `kioclient` at runtime. If
  neither is available (minimal container, WSL without desktop), `trash::delete`
  returns an error. Handle it: show a confirmation dialog saying "Permanent
  delete? (trash unavailable)" and call `std::fs::remove_file` only on
  confirmation.
- **Browser scan channel:** The async scan sends `Vec<(ContentType, Vec<PathBuf>)>`
  over a channel. On the UI thread, the browser reads `try_recv()` once per
  frame. If a new scan replaces the old one, the receiver buffer may accumulate
  stale results. Use a single-element channel (`Receiver<ScanResult>`) and drop
  the sender on re-scan so only the latest result arrives.
