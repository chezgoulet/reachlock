# S69 — Authoring Superpowers

**Spec:** New (cross-reference index, inline validation, diff, preview, dupe) ·
**Wave B (editor shell v2) · Depends on:** S67 (editor shell v2), S68 (missing editors complete)

## Outcome

The editor stops being 26 isolated islands. A cross-reference index maps every id to everything that references it — a CareerPath editor can see which Soul references its id, a Contract editor can see which Dialogue nodes link to its rules. Authors navigate by Ctrl+click (go-to-definition), inspect a "Find Usages" panel, and get a broken-reference report listing every dangling reference. Renaming an id propagates to all referrers in one operation. Id fields show autocomplete dropdowns sourced from the cross-reference index. Inline validation draws red squiggles under broken references with tooltip explanations. Before every save, a git-style diff lets the author review changes and warns when hand-edited RON comments would be destroyed. The editor can launch the game client with a `--content-dir` pointing at a temp copy of edited files for live preview. Duplicate and "New from Template" speed up authoring new entries.

## Context

- **26 editors with no inter-file awareness.** A CareerPath editor can't find which Soul references its `id`. A Contract editor can't see which Dialogue nodes link to its rules. Cross-references exist (souls reference faction ids, dialogue references contract ids, items reference faction ids, careers reference faction ids, stations reference hull ids, NPC spawns reference soul ids) but there is no index to query them.
- **ContentIndex already loads everything.** The client's `ContentIndex` (`reachlock-client/src/systems/content_index.rs`) walks the entire mods directory at startup, loading ContentFile envelopes and typed registries (hostile archetypes, hostile locations, charted systems, gate network, themes). The data is in memory — there is no structural barrier to building a cross-reference index. The editor needs its own copy (or a shared library variant) since the editor is a standalone binary that does not link against `reachlock-client`.
- **Existing `io::validate_content` validates against JSON schemas.** It catches shape errors (missing fields, wrong types) but not broken references. The new `validation.rs` layer builds on it: validate content structure first, then validate cross-references against the index. The two passes are independent and composable.
- **Diff-before-save requires the saved file on disk.** When an editor has a `path` and the file exists, read the current bytes, pretty-print the in-memory state, and diff them. When the file does not exist (first save), skip the diff. The diff is a string with `+`/`-` lines shown in a readonly `egui::TextEdit`.
- **Comment preservation is a known RON gotcha.** RON deserialize→serialize drops all comments. The editor already has a gotcha note about this (index, `io.rs:14`). The warning reads the file text before parsing and checks whether it contains `//` or `/*` comments. If it does and a save would overwrite them, show a confirmation dialog before proceeding.
- **Preview → Launch workflow.** The author hits "Preview" from the toolbar. The editor writes every open (and saved) content file to a temp directory, preserving the directory structure. It spawns `cargo run -p reachlock-client -- --content-dir <tempdir>` (or a prebuilt binary if available). The client loads content from `<tempdir>` instead of `mods/reachlock`. When the client exits, the temp directory is cleaned up. The editor shows a status line indicating the client is running and disallows re-preview until the client exits.
- **Template gallery** is a `File → New from Template` submenu that lists pre-made content files shipped with the editor (or in a `templates/` directory). Selecting a template copies it into the content tree and opens it. Templates are plain RON files with placeholder ids, comments explaining each field, and sensible defaults.

## Freeze first

### Cross-reference index (`reachlock-editor/src/cross_ref.rs`)

```rust
use std::collections::{HashMap, HashSet};

/// A single reference: some content id references some target id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The file/entry that holds the reference (e.g. soul file id).
    pub source_id: String,
    /// The content type of the source (so the UI knows which editor to open).
    pub source_type: ContentType,
    /// The field path within the source where the reference appears
    /// (e.g. "faction_id", "contract_id", "dialogue_nodes[3].contract_rule").
    pub field_path: String,
    /// The id value being referenced.
    pub target_id: String,
}

/// Bidirectional cross-reference index built from the ContentIndex.
/// Maps: id → everything that references it (incoming),
///       id → everything it references (outgoing).
pub struct CrossReferenceIndex {
    /// For a given target id, all references that point to it.
    pub incoming: HashMap<String, Vec<Reference>>,
    /// For a given source id, all references it makes to other ids.
    pub outgoing: HashMap<String, Vec<Reference>>,
    /// Every known id in the content index (for autocomplete).
    pub all_ids: HashSet<String>,
    /// Maps id → (display_name, content_type) for display in UI.
    pub id_metadata: HashMap<String, IdMeta>,
}

#[derive(Debug, Clone)]
pub struct IdMeta {
    pub display_name: String,
    pub content_type: ContentType,
}

impl CrossReferenceIndex {
    /// Build the index from a snapshot of ContentIndex (files + typed registries).
    pub fn build(content: &ContentIndex) -> Self { ... }

    /// List all references that point to `id`.
    pub fn usages_of(&self, id: &str) -> &[Reference] { ... }

    /// List all references made by `id`.
    pub fn references_from(&self, id: &str) -> &[Reference] { ... }

    /// Check if `id` exists in the index at all.
    pub fn is_known(&self, id: &str) -> bool { ... }

    /// Return all ids whose display_name or id matches a fuzzy prefix.
    pub fn autocomplete(&self, prefix: &str) -> Vec<IdMeta> { ... }

    /// Rename `old_id` to `new_id` and return all files that need updating.
    /// Each entry is (source_id, source_type, field_path) — the caller
    /// opens each source and patches the field.
    pub fn rename(&self, old_id: &str, new_id: &str) -> Vec<Reference> { ... }
}
```

The build function walks every `ContentFile` in the index and every typed registry entry, extracting id fields from the payload. Reference extraction is typed per `ContentPayload` variant:
- `Soul` → `faction_id`, `career_id`
- `Contract` → referenced by dialogue nodes via `contract_id`
- `Dialogue` → each dialogue node's `contract_rule` (if present) references a contract id; `next_node` references other nodes
- `Career` → `faction_id`, `conflicting_paths[]`
- `Item` → `faction`
- `Faction` → referenced by souls, items, careers (incoming only)
- `Station` → `hull_id` (implicit from file name pattern in exterior)
- `HullFrame` → referenced by stations
- `HostileArchetype` → `id` referenced by hostile locations
- `HostileLocation` → `interior` rooms may reference NPC soul ids
- `ChartedSystem` → `gate_id`, `station_ids[]`
- `Event` → triggers may reference dialogue ids, contract ids

### ContentIndex accessor (`reachlock-editor/src/cross_ref.rs` or `app.rs`)

```rust
/// Snapshot of the loaded content index, rebuilt when the browser rescans.
/// Stored on EditorApp and passed to CrossReferenceIndex::build().
pub struct ContentIndexSnapshot {
    pub files: Vec<ContentFile>,
    pub typed: TypedContent,
    /// Mtime of the content root when the snapshot was built.
    pub built_at: SystemTime,
}
```

The snapshot is rebuilt whenever the content browser rescans. The cross-reference index is rebuilt from the snapshot. The snapshot lives on `EditorApp` alongside the index.

### Diff result (`reachlock-editor/src/diff.rs`, new file)

```rust
pub struct DiffResult {
    /// Lines of the old file (saved on disk).
    pub old: Vec<String>,
    /// Lines of the new file (in-memory state).
    pub new: Vec<String>,
    /// Unified diff string for display.
    pub unified: String,
    /// True if old and new are identical.
    pub unchanged: bool,
}

impl DiffResult {
    /// Compute diff between saved file at `path` and pretty-printed `new_text`.
    pub fn compute(path: &Path, new_text: &str) -> Result<DiffResult, String> { ... }
}
```

Uses `similar` crate (or `diff` crate) for the unified diff. Add `similar` to `reachlock-editor/Cargo.toml`.

### Inline validation trait hook

The `Editor` trait in `app.rs` gains an optional method that the validation system calls after `validate()`:

```rust
/// Validate cross-references for the current entry.
/// Returns (field_path, message) pairs. The shell renders these as
/// red squiggles under the relevant UI widget (by matching field_path
/// against the widget's `id.source()`) and shows the message in a tooltip.
fn validate_cross_refs(&self, index: &CrossReferenceIndex) -> Vec<(String, String)> {
    Vec::new()
}
```

Each editor overrides `validate_cross_refs` to check its id fields against the index.

### File → New from Template data path

```
reachlock-editor/templates/
  soul.ron
  contract.ron
  faction.ron
  career.ron
  station.ron
  ...
```

Each template is a valid `.ron` file with `ContentFile` envelope, placeholder id (`"new_soul"`, `"new_contract"`, …), default values for all fields, and `// TODO: …` comments explaining what each field does. The template directory is shipped with the repo (not generated).

## Deliverables

### 1. Cross-reference index (`reachlock-editor/src/cross_ref.rs`) — new file

- [ ] Define `Reference`, `IdMeta`, `CrossReferenceIndex` structs as frozen.
- [ ] Define `ContentIndexSnapshot` struct (wrapper around `Vec<ContentFile>` + typed registries).
- [ ] `CrossReferenceIndex::build()` walks every `ContentFile` in the snapshot + every typed registry entry and extracts references by pattern-matching on `ContentPayload`:
  - `Soul` → `faction_id`, `career_id`
  - `Dialogue` → `contract_id` (on nodes that have contract rules)
  - `Career` → `faction_id`, `conflicting_paths[]`
  - `Item` (via CoreItem) → `faction`
  - `Station` → hull id (extracted from exterior editor data)
  - `ChartedSystem` → `gate_id`, `station_ids[]`
  - `HostileArchetype` → `id` referenced by hostile locations
  - `HostileLocation` → NPC soul id references in room spawns
  - `Contract` → referenced by dialogue (incoming only — no outgoing refs from contracts themselves as of S68)
- [ ] `incoming` maps every known target id to the references pointing at it.
- [ ] `outgoing` maps every known source id to the references it makes.
- [ ] `all_ids` contains every known id across all content types.
- [ ] `id_metadata` maps each id to its display_name + content_type.
- [ ] `autocomplete(prefix)` returns fuzzy-matching candidates (character-sequence match, same as command palette filter).
- [ ] `rename()` returns the list of references that must be updated (caller patches in each editor).
- [ ] Snapshot and index are rebuilt on every content browser rescan.
- [ ] Store the `ContentIndexSnapshot` and `CrossReferenceIndex` on `EditorApp`.
- [ ] Test: build index from known fixtures → verify incoming/outgoing counts; verify `is_known` for existing ids and `!is_known` for bogus ids.

### 2. Go-to-definition: Ctrl+click on id

- [ ] When the user Ctrl+clicks (or right-click → "Go to Definition") on a rendered id label in any editor, the shell looks up the id in `cross_ref.id_metadata`.
- [ ] If found, open the file for that id in a new tab (or focus existing tab if already open). Use `open_editor_for_file` with the content type and path resolved from the index.
- [ ] If not found, show status: "No definition found for '{id}'".
- [ ] Every editor that renders id fields (faction_id, soul_id, contract_id, career_id, …) wraps them in a clickable label with a distinct style (underlined, link color) to signal interactivity. The click handler fires a shared `GoToDefinition(String)` action on the shell.
- [ ] Right-click context menu on any id label: "Go to Definition", "Find Usages".

### 3. Find usages panel

- [ ] When the user selects a field whose value is an id (or right-clicks → "Find Usages"), a new right-side panel (or an `egui::Window`) opens showing all incoming references from `cross_ref.incoming[id]`.
- [ ] Each usage row shows: source file name, source content type, field_path. Clicking a row navigates to the source file (same as go-to-definition).
- [ ] If the id is not found in the index: "No usages found for '{id}'".
- [ ] The panel is toggleable via `View → Find Usages` or Ctrl+Shift+F. It closes when the active tab changes to a non-id context.
- [ ] The panel title shows usage count: "Usages of '{id}' (3)".

### 4. Broken-reference report

- [ ] `File → Validate All` already exists and runs each editor's `validate()`. Extend it: after all editors validate, run a global cross-reference check:
  - For every reference in `cross_ref.outgoing`, check if the target exists in `cross_ref.all_ids`.
  - If not, it's a broken reference.
- [ ] Show results in the existing `validation_report` window. Each broken reference shows: `{source_file}.ron → {target_id} (via {field_path})`.
- [ ] Group by source file, sort by target id.
- [ ] Add a "Broken References" indicator in the status bar: count of broken refs in the current active editor, or "0 broken refs" when clean.
- [ ] `View → Broken Reference Report` re-runs the global check and opens the window.
- [ ] Test: build index with a known-broken reference (target does not exist) → report lists it. Fix the target → report is empty.

### 5. Rename-with-referrers

- [ ] Right-click on an id → "Rename" (or select the id field and press F2). A dialog asks for the new id string.
- [ ] On confirmation, `CrossReferenceIndex::rename(old_id, new_id)` returns all references that need updating.
- [ ] For each reference, open the source file in its editor (if not already open), apply the change at `field_path`, and mark the editor dirty.
- [ ] After all patches, rebuild the cross-reference index (trigger a browser rescan).
- [ ] Status message: "Renamed '{old_id}' → '{new_id}' (updated 3 files)".
- [ ] If one of the source files fails to load/update, show an error: "Rename incomplete: failed to update {file}: {error}". The already-patched files remain patched (no transaction rollback — warn in the UI and let the author fix manually).
- [ ] Test: rename a faction id, verify that all souls referencing the old id now reference the new id and are marked dirty.

### 6. Autocomplete in id fields

- [ ] Every id field in every editor shows an autocomplete dropdown when the user types in it. The dropdown is a filtered list from `cross_ref.autocomplete(input)`.
- [ ] Dropdown renders as a popup below the text field (egui `ComboBox` or custom popup). Keyboard navigation: Arrow keys to select, Enter to accept, Esc to dismiss.
- [ ] If the typed value exactly matches a known id, no dropdown is shown (already resolved).
- [ ] If the typed value does not match any known id and the field loses focus, inline validation marks it as a broken reference (deliverable 7).
- [ ] Editors that currently use a plain `TextEdit` for id fields switch to a custom `IdField` widget that wraps `TextEdit` + autocomplete popup. The widget is defined in a shared location (`reachlock-editor/src/widgets.rs` or within `cross_ref.rs`).

### 7. Live inline validation (`reachlock-editor/src/validation.rs`) — new file

- [ ] After every frame, the shell runs a lightweight validation pass on the active editor:
  1. Call `editor.validate()` (existing JSON schema validation).
  2. Call `editor.validate_cross_refs(&cross_ref)` (new trait method, default no-op).
- [ ] Collect all `(field_path, message)` pairs from the cross-ref pass.
- [ ] For each pair, find the egui widget whose `id.source()` matches `field_path` and paint a red squiggle under it (or a red border on the text field).
- [ ] Hovering over a squiggled widget shows a tooltip with the validation message.
- [ ] Validation runs every frame but is cached: if the editor's snapshot hasn't changed since the last frame, reuse the previous validation results.
- [ ] The preview panel's existing validation issue count (in `preview.rs`) now reflects BOTH structural and cross-reference issues.
- [ ] Test: open a Soul editor, set `faction_id` to a non-existent id → red squiggle appears on the field. Set it to a known faction id → squiggle clears after next frame.

### 8. Diff-before-save (`reachlock-editor/src/diff.rs`) — new file

- [ ] When saving an editor that has a `path`, before writing to disk:
  1. Read the current file from disk (if it exists).
  2. Pretty-print the in-memory state to a string via `ron::ser::to_string_pretty`.
  3. Compute unified diff using the `similar` crate.
  4. Show the diff in a modal window with a scrollable side-by-side or unified view (unified is simpler).
- [ ] The diff window has: Accept (save anyway), Cancel (don't save), and "Don't show again for this file" (stores a preference for the file path).
- [ ] Clean files (modified only by autosave with no semantic change) show "No changes — file is up to date" and skip the dialog.
- [ ] First-time saves (file doesn't exist on disk) skip the diff entirely.
- [ ] Add `similar` crate to `reachlock-editor/Cargo.toml`.
- [ ] Test: open a file, edit a field, save → diff shows the changed line. Cancel → file unchanged.

### 9. Comment preservation warning

- [ ] Before saving, read the raw file text (not parsed RON) and check for comment markers: lines starting with `//` or `/*` (multiline comments delimited by `/*` … `*/`).
- [ ] If comments are found AND the file will be overwritten (diff shows changes), show a warning dialog: "This file contains hand-edited comments that will be lost on save."
- [ ] The dialog has: "Save anyway" (proceed), "Cancel" (don't save), and "Don't warn me about comments in this file again" (stores a preference per file path).
- [ ] The warning is integrated into the diff-before-save flow: the comment check happens before or alongside the diff check. If both trigger, show a combined dialog with the diff AND the comment warning.
- [ ] The warning does NOT block saves in files without comments — it only triggers when comments are detected.
- [ ] Test: create a .ron file with a `// comment`, open it, edit a value, save → warning appears. Remove the comment, save → no warning.

### 10. Preview → Launch in game

- [ ] Toolbar button "Preview in Game" (or `File → Preview in Game`). Available when at least one editor has unsaved changes or at least one file is open.
- [ ] On click:
  1. Save all dirty editors (calls `save_editor` for each dirty tab).
  2. Create a temp directory via `tempfile::tempdir()`.
  3. Copy all content files from the content root that are loaded in the index into the temp dir, preserving directory structure.
  4. Overwrite any files that are currently open in editors with their in-memory state (pretty-printed RON).
  5. Spawn `cargo run -p reachlock-client -- --content-dir {tempdir.path()}` as a child process.
  6. Print the command to the editor log/status.
- [ ] While the child process is running:
  - Show status: "Client running (PID {pid})…" with a "Kill Client" button.
  - Disable the Preview button (greyed out, tooltip: "Client is already running").
  - Temp directory is NOT deleted until the client exits.
- [ ] When the child process exits:
  - Clean up the temp directory.
  - Show status: "Client exited (code {code})."
  - Re-enable the Preview button.
- [ ] If `cargo` is not available or the binary is not built, show a status message: "Client not built — run `cargo build -p reachlock-client` first." and a link to the build command.
- [ ] After first successful preview, remember the path to the prebuilt binary (from `cargo build --message-format=json`) or let the author configure a binary path in Preferences.
- [ ] Add `tempfile` crate to `reachlock-editor/Cargo.toml`.
- [ ] Offline-first: Preview works with no server — the client runs locally with local content.

### 11. Duplicate

- [ ] Right-click on a tab → "Duplicate" (or `Edit → Duplicate` when the active editor supports it).
- [ ] Opens a dialog asking for the new id and display name. Pre-fills: `{old_id}_copy`, `{old_display_name} (Copy)`.
- [ ] On confirm:
  1. Clone the editor's data.
  2. Replace `id` and `display_name` with the new values.
  3. Open the clone in a new tab (unsaved, no path).
  4. The clone has no path — saving requires Save As.
- [ ] The duplicate action is available for every editor type (single and multi-entry). For multi-entry editors (soul, station, enemy, …), only the currently selected entry is duplicated, not the entire editor.
- [ ] Status message: "Duplicated '{old_name}' as '{new_name}'."

### 12. Templates — File → New from Template

- [ ] `File → New from Template` submenu lists every template in `reachlock-editor/templates/`.
- [ ] Template files are named by content type: `soul.ron`, `contract.ron`, `faction.ron`, `career.ron`, `hull_frame.ron`, `station.ron`, `dialogue.ron`, `dungeon.ron`, `ecosystem.ron`, `planet_culture.ron`, `theme.ron`, `trope.ron`, `scripted_encounter.ron`, `event.ron`, `recipe.ron`.
- [ ] Selecting a template:
  1. Read the template file.
  2. Parse it as a `ContentFile`.
  3. Open a new editor for that content type with the template data loaded.
  4. The editor is unsaved, no path, and the id/display_name are the template defaults (the author changes them before saving).
- [ ] Templates are bundled in the repo and installed alongside the editor binary (or loaded from `editor_data_dir()/templates/` with a fallback to the source tree).
- [ ] Authors can add custom templates by placing `.ron` files in `editor_data_dir()/templates/`. The submenu reloads on each open.
- [ ] Test: File → New from Template → Soul → a new soul editor opens with three pre-filled example dialogue lines and `faction_id: "compact"`.

## Acceptance gates

```
cargo test -p reachlock-editor cross_ref::   # index build, incoming/outgoing counts, autocomplete
cargo test -p reachlock-editor diff::        # diff compute, unchanged detection
cargo test -p reachlock-editor validation::  # broken reference detection, fix clears
cargo test -p reachlock-editor               # all tests pass
cargo run -p reachlock-editor                # editor starts

# Cross-reference index: open any content file → right-click an id → "Find Usages"
#   → panel shows all referrers. Click a referrer → navigation opens that file.
# Go-to-definition: Ctrl+click on a faction id in a Soul editor → opens faction file.
# Autocomplete: start typing a faction_id → dropdown shows matching factions.
# Inline validation: set faction_id to "nonexistent" → red squiggle + tooltip.
# Broken-reference report: View → Broken Reference Report → lists dangling refs.
# Rename: right-click a faction id → Rename → enter new name → all souls
#   referencing it are updated and marked dirty.
# Diff-before-save: open a soul file → edit a field → Ctrl+S → diff window shows
#   the changed line. Cancel → file unchanged. Accept → file saved.
# Comment warning: open a .ron file with // comments → edit → save → warning shown.
# Preview → Launch: click Preview → client launches with edited content.
# Duplicate: right-click tab → Duplicate → enter new id → clone tab opens.
# Template: File → New from Template → Soul → new soul with template data.
make check
```

Manual: open a soul that references faction "compact" → Ctrl+click "compact" → faction editor opens. Change faction id to "compact_navy" via Rename → the soul editor shows "compact_navy" in its faction_id field and is marked dirty. Close without saving → reopen original file → faction_id is back to "compact" (rename was not persisted — correct).

## Non-goals

- Hot-reload of content in a running client (the client must restart to pick up changes — Preview → Launch does a full restart)
- Server-side cross-reference validation (the editor is a standalone dev tool; the server never validates references)
- Web-based preview (native-only — client requires OpenGL/Vulkan/WGPU)
- Transactional rename rollback (partial failures leave some files patched — the editor warns and lets the author fix manually)
- Cross-reference index persisted to disk (rebuilt on every scan — the index is a derived cache, not a source of truth)
- Validation of game-balance rules (only structural and broken-reference validation)
- WASM build for the editor (native-only — same exemption as S67/S68)
- Auto-fix for broken references (the report is diagnostic only; the author fixes manually)

## Gotchas

- **Cross-reference index must be rebuilt on every browser rescan.** The browser already rescans every 2s (S67 makes it async). After each scan, rebuild `ContentIndexSnapshot` from the file tree, then rebuild `CrossReferenceIndex` from the snapshot. The rebuild should be fast (<50ms for a content dir with ~500 files) — if it's not, profile and cache per-directory results.
- **Reference extraction is per-content-type and fragile.** Each `ContentPayload` variant has different fields that hold ids. The extraction in `CrossReferenceIndex::build` must handle every variant. When a new content type is added (future sprint), the match must be extended — there is no reflection. The test that builds an index from all known fixture files catches omissions.
- **Autocomplete in egui `TextEdit`.** egui does not have a built-in autocomplete widget. The `IdField` widget must position a popup manually using `egui::Area` or `egui::ComboBox` with a custom filter. Use `ComboBox` with `set_selected_option` and a manual text input — or build a custom popup that tracks cursor position. Model on the command palette pattern (S67): a `TextEdit` + a scrollable list below it, shown/hidden based on focus and input length.
- **Go-to-definition must handle the case where the target file doesn't exist on disk** (it exists only in the cross-reference index as an `id` from a typed registry, but the file is procedurally generated or not yet authored). In that case, Ctrl+click opens a new (unsaved) editor for that content type with the id pre-filled, so the author can create the missing file.
- **Diff-before-save for multi-entry editors.** Soul, station, and enemy editors use `save_all()` which writes each dirty entry to its own file. The diff must be computed per-file: before `save_all()` runs, snapshot every dirty entry's current bytes on disk, pretty-print the in-memory state, diff each pair, and show a combined diff window listing all files with changes. If the author cancels, no file is written.
- **Comment detection is best-effort.** Detecting comments in raw RON is a heuristic: check for `//` at the start of a line (outside a string) and `/*` anywhere. False positives are possible (a string containing `//` as data). The heuristic should be conservative: if in doubt, show the warning. The "Don't show again" preference per file path silences it for trusted files.
- **Preview → Launch with `--content-dir`.** The client must support a `--content-dir` CLI argument (check if it exists; if not, S69 adds it). The argument overrides the default `mods/reachlock` path used in `ContentIndex::load_content_index`. If the client does not accept `--content-dir`, preview will launch the client without custom content (fall back to normal content dir and warn the author).
- **Temp directory cleanup.** If the editor crashes while the client is running, the temp directory leaks. Use `tempfile::Builder::new().prefix("reachlock_preview_").tempdir()` which deletes the directory on drop. If the drop doesn't run (process kill -9), the leak is acceptable — the system temp dir is cleaned by the OS periodically.
- **Template directory default path.** Templates live in `reachlock-editor/templates/` in the source tree. When the editor is installed (not run from source), they should be at `editor_data_dir()/templates/`. The load code checks the bundled path first (relative to the executable via `std::env::current_exe()`), then falls back to `editor_data_dir()/templates/`. If neither exists, the `File → New from Template` submenu shows "(no templates found)".
- **RON round-trip on duplicate.** Duplicating an entry clones the in-memory data and opens a new editor. The clone is not a file copy — it is a `ron::to_string` of the data followed by `ron::from_str` in the new editor. This is the same round-trip that happens during save. Ensure the data struct derives `Clone` (all core types do).
- **`similar` crate version.** Add `similar = "2"` to `reachlock-editor/Cargo.toml`. The unified diff output is a `String`; render it in an `egui::TextEdit` with `monospace()` font. Lines starting with `+` are colored green, `-` are red, `@@` are blue. The coloring is done by checking the first character of each line in the diff text when rendering.
