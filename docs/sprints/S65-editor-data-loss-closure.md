# S65 — Editor Data-Loss Closure

**Spec:** New (editor refactoring) · **Wave A (stop the bleeding)** · **Depends on:** — (standalone, blocks S67/S68/S69)

## Outcome

Every mutation in every editor sets the dirty flag. `File → New` creates a blank editor instead of copying an open file. `save_all` writes only dirty entries, not every loaded entry. The editor trait takes `&mut egui::Ui` instead of `&egui::Context` — the shell owns the `CentralPanel` layout, not each editor individually. 26 editors compile against the new trait without breaking existing behaviour. The two divergent registries are unified to one. The `hull.rs` dead file is removed from the tree.

## Context

- MASTER-PLAN.md findings E1–E13, E18, E30 (14 editor bugs, 1 dead file, 1 schema misrouting). Every one is data-loss or structural — authors save a file that silently overwrites another file, see no close guard, watch Reroll All rename cross-referenced IDs, and edit Dialogue under the Ecosystem schema.
- The root cause of E12 (`ui(&Context)`) is the oldest design decision in the editor: each editor opens its own `CentralPanel`. Because `egui::Context` is the top-level frame, editors can nest panels, misorder them, or leave gaps. Taking `&mut egui::Ui` means the shell owns the layout and editors fill content areas.
- The two registries (`register_all` in `editors/mod.rs:34` and `build_default_registry` in `app.rs:302`) are manually maintained. `register_all` is not called from anywhere; `build_default_registry` is. The dead one has diverged from the live one — `editors/mod.rs` is not the source of truth for what editors exist.
- E18 (Dialogue → ecosystem schema) is a one-line mapping fix with outsized downstream damage: authors writing dialogue trees validate against the wrong JSON schema, producing files that error on load.
- E30 (dead `editors/hull.rs`, 282 lines) is on disk but not in `mod.rs`. Study reference. Remove it — study references belong in git history, not in the working tree.

## Freeze first

### New Editor trait surface (`app.rs`)

```rust
/// Single-arg save: writes the current (single) file path. Multi-entry
/// editors (soul, station, enemy, …) use `save_all` instead.
fn save(&self, path: &Path) -> Result<(), String>;

/// Save every dirty entry to its own path. The shell delegates to this
/// for multi-entry editors (dirtiness is per-entry, not per-tab).
/// Returns `true` if any entry was written.
fn save_all(&mut self) -> Result<bool, String> { Ok(false) }

/// Called after a successful save so the dirty flag (and tab asterisk)
/// clears. Replaces the old `mark_saved`.
fn mark_saved(&mut self) {}

/// Shell-owned layout: `ui` is the content area inside a `CentralPanel`
/// the shell created. Editors draw widgets into it. If an editor needs
/// a top bar, it returns widget descriptors — the shell places them.
fn ui(&mut self, ui: &mut egui::Ui);

/// Return an optional top-bar action row. The shell positions these
/// consistently instead of each editor placing its own `TopBottomPanel`.
fn top_bar(&self) -> Vec<TopBarAction> { vec![] }

/// True when the editor's content type is a previewer or browser
/// (never persisted). The shell skips the Save/SaveAll button.
fn is_previewer(&self) -> bool { false }
```

### `TopBarAction` for shell-owned bars

```rust
pub struct TopBarAction {
    pub label: &'static str,
    pub enabled: bool,
    pub action: Box<dyn FnOnce(&mut dyn Editor)>,
}
```

### `EditorRegistry` — single source of truth

Delete `editors/mod.rs::register_all`. Keep only `app.rs::build_default_registry`. The browser and `File → New` menus iterate the same registry. A test asserts every `ContentType` variant has a registered factory.

### Schema fix (`schema.rs`)

```rust
// Old (incorrect):
ContentType::Dialogue => "ecosystem",
// New:
ContentType::Dialogue => "dialogue",
```

### Clean `new()` separation

```rust
/// New editor, no path, blank state. Used by File → New.
pub fn new() -> Self;

/// Load from a path. Used by browser double-click and command line.
pub fn load(path: &Path) -> Result<Self, String>;
```

Delete the combined `load_or_new` pattern. A newly created editor has `path: None`.

## Deliverables

### 1. Trait change: `ui(&mut Ui)` + shell-owned layout (`app.rs`, all 26 editors)

- [ ] Change `fn ui(&mut self, ctx: &egui::Context)` to `fn ui(&mut self, ui: &mut egui::Ui)` on the `Editor` trait in `app.rs:196`.
- [ ] Update every editor impl to use `ui` instead of `ctx` for drawing. No editor opens its own `CentralPanel` — the shell creates one and passes `ui` from it.
- [ ] The shell (`main.rs:1140-1170`) creates a single `CentralPanel` and iterates `open_editors`, calling `editor.ui(ui)` for each visible tab.
- [ ] `TopBottomPanel` registration moves to the shell. If an editor needs a top bar, it returns `top_bar()` and the shell places the bar above the content area — not inside the editor's own closure.
- [ ] `SidePanel::right` (preview panel) registers BEFORE `CentralPanel`, not after (`main.rs:1217`).
- [ ] `TopBottomPanel` (tab bar) registers outside the `CentralPanel` closure, not inside it (`main.rs:1166`).

**Gate:** `make build` compiles. Every editor opens, draws content in the shell's panel, and saves.

### 2. `new()` / `load()` split (`app.rs`, every editor)

- [ ] Delete the `load_or_new()` pattern. Every editor gets `new()` (blank, `path: None`) and `load(path)` that populates from disk.
- [ ] `File → New` (keyboard: Ctrl+N, or menu) calls `registry.create(ContentType)` then `editor.new()`. The editor starts with no path, no content loaded — blank state.
- [ ] `File → Open` (or browser double-click) calls `editor.load(path)`.
- [ ] An editor with `path: None` cannot save to disk without a "Save As…" dialog first.

**Gate:** `File → New → Soul` creates a blank soul tab with no path. `File → Open → soul.ron` loads the file. Switching back to the blank tab shows no content from the loaded file. Test per ContentType.

### 3. Dirty flag on every mutation (all 26 editors)

- [ ] Every editor mutation widget (text field, add/remove row, color picker, drag value) calls `self.touch()` after the mutation.
- [ ] `touch()` sets a dirty flag on the active entry (multi-entry) or on the editor itself (single-entry).
- [ ] Close guard checks `has_unsaved_changes()` and prompts before discarding.
- [ ] `save_all()` checks dirtiness per entry — only dirty entries are serialized and written. Clean entries produce no disk I/O.
- [ ] `mark_saved()` clears the dirty flag.

**Gate:** Open a soul → edit a name → close tab → dialog warns of unsaved changes. Save → reopen → name persisted. Create a new hull frame → do not edit → close → no dialog.

### 4. Collision-safe stems (`dialogue.rs`, `career.rs`, all multi-entry editors)

- [ ] Replace hardcoded stems (`"generated_dialogue.ron"`) with path derived from the first entry's id or a timestamped stem.
- [ ] When saving all entries in a multi-entry editor, each entry writes to its own `path` (the path it was loaded from, or a newly assigned path for new entries).
- [ ] Two editors of the same type never write to the same file.

**Gate:** Open two dialogue editors → add one entry to each → save all → two distinct files on disk, each containing only its own entry.

### 5. `accept_seed_reroll()` gates (`career.rs`, `main.rs`)

- [ ] Editors whose content is authored relationships (gate networks, room templates, careers with fixed ids) return `false` from `accept_seed_reroll()`.
- [ ] The seed panel's "Reroll All" calls `apply_seed` only on editors that accept it.
- [ ] Ids referenced by other content files (career path ids, faction ids) are never reseeded.

**Gate:** Open a career editor and a soul editor → "Reroll All" → soul regenerates, career keeps its id.

### 6. Unify registries (`app.rs`, `editors/mod.rs`)

- [ ] Delete `editors/mod.rs::register_all()` and the old registrations at lines 34-110.
- [ ] `build_default_registry()` in `app.rs:302` is the sole source of truth.
- [ ] Add a test: every `ContentType` variant (excluding previewers) has a factory registered in the default registry.
- [ ] Delete `editors/hull.rs` from disk. Study reference: that's what git log is for.

**Gate:** The test `every_content_type_is_registered` passes.

### 7. Modal fix (`dialogs.rs`)

- [ ] Replace `show(|ui| …)` confirmation patterns with `egui::Modal`.
- [ ] Modal blocks input to all panels behind it.
- [ ] Keyboard: Enter confirms, Escape cancels.

**Gate:** Open close-unsaved dialog → press Escape → dialog closes without actioning. Attempt to click a panel behind the dialog → click does not register.

### 8. Schema mapping fix (`schema.rs`)

- [ ] `ContentType::Dialogue` maps to schema id `"dialogue"`, not `"ecosystem"`.

**Gate:** The schema round-trip test catches Dialogue → ecosystem mismatch before this sprint.

## Acceptance gates

```
# Build
cargo build -p reachlock-editor

# Every ContentType loads and draws
cargo run -p reachlock-editor &
# Manual: File → New → each type creates a blank tab with no path

# Data-loss regression test
cargo test -p reachlock-editor editor::data_loss  # after tests are written for this sprint

# Schema wire test
cargo test -p reachlock-editor schema::dialogue_maps_correctly

# Registry completeness test
cargo test -p reachlock-editor app::tests::every_content_type_is_registered

# Dead file removed
test ! -f reachlock-editor/src/editors/hull.rs

make check
```

Manual: Open a soul → edit personality → close → dialog fires → Cancel → edit a second field → Save → reopen → both edits persisted. File → New → Career → add a rank → tab shows asterisk → Save As… → file written. Open the saved file → first edit still present.

## Non-goals

- The ten editors missing from the browser and `File → New` (E6, E7, E29) — that's S68
- Editor shell v2 improvements (E14–E17, E19–E28, E31) — that's S67
- Cross-reference index, rename-with-referrers, diff-before-save — S69
- Contract crafting workshop (S34) integration — S81

## Gotchas

- The `ui(&mut Ui)` change touches all 26 editor files and the trait definition. The compiler catches every call site (callers pass `&egui::Context`, the trait now expects `&mut egui::Ui`), so a missed editor won't silently compile — but the interdiff is noisy. Land this in a single commit with `[S65] editor: ui(&mut Ui)` so blame can filter through it.
- Some editors use `ctx.request_repaint()` to drive animation. `egui::Ui` has `ui.ctx().request_repaint()`, so replace `ctx.request_repaint()` with `ui.ctx().request_repaint()` everywhere.
- Multi-entry editors (soul, station, enemy, hull_frame) have per-entry paths. The `save_all` signature takes `&mut self` so new entries can record the path they were just written to. Single-entry editors keep the old `save(path)` — nothing changes for them.
- `egui::Modal` requires a `&egui::Context`, which the shell still has. The shell is the right place to open modals (close confirmation, delete confirmation). Editors signal a pending action by returning a variant — the shell opens the modal.
- `register_all` in `editors/mod.rs` was dead code but it was a parallel registry that had to be manually kept in sync. After this sprint there is exactly one registry. The dead file removal (`hull.rs`) is intentional — if someone needs it as a reference, `git show testing:reachlock-editor/src/editors/hull.rs` is one command.

## Gotcha-adjacent (newly discovered, add to 00-INDEX.md ledger)

- `egui::Ui` lifetime: when calling `ui.ctx().request_repaint()` inside a `CentralPanel` closure, the `ctx` borrows from `ui`, which borrows from the panel. Do not stash `ctx` across an `await` point or a nested `show` call — the borrow checker catches this, but the error message says "cannot return from closure" which is confusing. Pre-empt by extracting `ctx` once at the top of the closure and using it for repaint/async dispatches that need `&Context`.
