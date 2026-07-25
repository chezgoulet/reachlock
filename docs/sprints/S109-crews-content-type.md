# S109 — Crews ContentType (Orphaned Directory)

**Wave: UX-Hardening · Depends on:** S01 (Content pipeline), S80 (Crew open-world system)

## Outcome

The `mods/reachlock/crews/` directory is registered as a proper `ContentType::CrewPackage` in the editor, browser, File→New menu, and content completeness gate. Crew packages can be created, edited, and saved through the editor instead of being hand-authored as raw RON files.

## Context

`CrewRoster::load_from_content()` (`crew.rs:265`) reads from `mods/reachlock/crews/`:

```rust
let crews_dir = std::path::Path::new("mods/reachlock/crews");
```

But `crews/` has no corresponding `ContentType` variant, no browser directory entry, no File→New entry, and no editor. It is invisible to the content pipeline and the completeness gates.

The file format is `CrewPackage` from `reachlock_core::crew`:
```rust
pub struct CrewPackage {
    pub id: String,
    pub name: String,
    pub members: Vec<CrewEntry>,
}

pub struct CrewEntry {
    pub soul_id: String,
    pub role: String,
    pub duty_room: Option<String>,
    pub starting: bool,
    pub salary: u64,
}
```

### Key files

| File | Role |
|------|------|
| `reachlock-editor/src/app.rs` | Add `ContentType::CrewPackage` variant |
| `reachlock-editor/src/browser.rs` | Add to `FILE_TYPES` array |
| `reachlock-editor/src/editors/` | New file `crew_package.rs` — editor implementation |
| `reachlock-client/src/systems/crew.rs` | `load_from_content()` — no changes needed (reads from disk) |
| `reachlock-core/src/content/event.rs` | `AssetType` — may need `CrewPackage` variant |

## Freeze first

### ContentType variant

```rust
// In app.rs ContentType enum:
CrewPackage,
```

Mapping:
- `directory()` → `"crews"`
- `name()` → `"Crew Package"`
- NEW_MENU_GROUPS: add to "Characters" group
- FILE_TYPES: add to browser array

### Editor traits

The crew package editor:
- Loads `CrewPackage` from RON files in `crews/`
- Shows a list of crew entries with soul_id, role, duty_room, starting flag, salary
- Each entry row is editable (text fields for soul_id/role, dropdown for duty_room, checkbox for starting, DragValue for salary)
- "Add Entry" button; "Remove Entry" on × button
- Validation: check that `id` is non-empty, each entry has a `soul_id`, `role` is non-empty, `salary >= 0`
- Seed generation: randomizes crew members from available souls
- Save: writes `CrewPackage` to RON

### AssetType

The content loader already handles `CrewPackage` in `content/envelope.rs` — verify the `ContentPayload` variant exists. If not, add:
```rust
// In ContentPayload enum:
CrewPackage(CrewPackage),
```

## Deliverables

### 1. Add ContentType variant

- [ ] Add `CrewPackage` to `ContentType` enum in `editor/src/app.rs`
- [ ] Add `directory()` return `"crews"`
- [ ] Add `name()` return `"Crew Package"`
- [ ] Add `from_directory("crews")` return `Some(CrewPackage)`
- [ ] Add to `ContentType::all()` array
- [ ] Add to `NEW_MENU_GROUPS` under "Characters" group

### 2. Add to browser

- [ ] Add `ContentType::CrewPackage` to `FILE_TYPES` in `editor/src/browser.rs`
- [ ] Verify the browser scans `crews/` directory and shows `.ron` files

### 3. Create crew package editor

- [ ] New file: `editor/src/editors/crew_package.rs`
- [ ] Implement `Editor` trait:
  - `load`: reads `CrewPackage` from RON
  - `save`: writes `CrewPackage` to RON
  - `validate`: checks id, soul_ids, roles, salary
  - `ui`: renders entry list with edit fields
  - `generate_from_seed`: picks random souls for crew members
  - `accept_seed_reroll`: `false` (id-renaming editor)
  - `snapshot` / `restore_snapshot`: RON round-trip
- [ ] Register in `build_default_registry()`: `r.register(ContentType::CrewPackage, crate::editors::crew_package::create_editor);`
- [ ] Add `mod crew_package;` to `editors/mod.rs`

### 4. Verify content pipeline

- [ ] Ensure `content/envelope.rs` can parse `CrewPackage` from RON
- [ ] Ensure `AssetType::CrewPackage` variant exists or add it
- [ ] Ensure the `content check` CLI validates crew package files

### 5. Update gate tests

- [ ] `browser_covers_every_file_backed_type` → ensure CrewPackage passes
- [ ] `new_menu_covers_every_type` → ensure CrewPackage passes
- [ ] `every_type_constructs_an_editor` → ensure CrewPackage passes
- [ ] `a_new_editor_writes_nothing_to_disk` → ensure CrewPackage passes

### 6. Test

- [ ] `make check` passes with the new ContentType
- [ ] `cargo test -p reachlock-editor` — all gate tests pass

## Acceptance gates

```bash
cargo test -p reachlock-editor
cargo clippy -p reachlock-editor -- -D warnings

# Manual:
# 1. Open editor → File → New → Characters → Crew Package
# 2. Add crew entries with soul_ids + roles + salary
# 3. Save → verify file created in mods/reachlock/crews/
# 4. Open from browser → verify entries load
# 5. Launch game → crew roster picks up the package (if it's the starting crew)

make check
```

## Non-goals

- Visual crew portrait browser in the editor
- Drag-and-drop crew member ordering
- Soul picker dropdown (type the `soul_id` as text — later sprint)
- Crew package preview in the right panel

## Gotchas

- **`crews/` directory may not exist yet.** The editor's directory scan returns empty if the directory is missing. Add `std::fs::create_dir_all()` in the editor constructor or the save path, similar to how other editors handle it.
- **`CrewPackage` references `soul_id` strings.** These are validated at game load time, not at editor save time. The editor's `validate()` should check for empty `soul_id` but doesn't need to verify the soul exists (it might be in a different mod or not yet created).
- **Three gate tests will fail until the variant is registered everywhere.** Run `make check` with `RUST_LOG=warn` to see exactly which test fails, then fix the registration, rerun. Repeat until green.
- **The `AssetType` enum may need a new variant.** Check `reachlock-core/src/content/envelope.rs` or wherever `AssetType` is defined. If it doesn't have `CrewPackage`, the content loader won't pick up `crews/` files. Add the variant (and update the match arms in all match blocks over `AssetType`).
