# S96 — Panel Migration Batch (Settings, Library, Ship Editor)

**Wave: UX-Refactor · Depends on:** S94 (PanelWidget), S95 (Contract workshop migration — same patterns)

## Outcome

Three more panels migrate from raw text concatenation to the shared `PanelWidget` rendering + `SelectablePanel` navigation: Settings UI, Contract Library, and Ship Editors (exterior + interior). Each retains its exact behavior but delegates layout, cursor, and key navigation to the shared system.

## Context

These three panels implement the same manual text-building pattern as the contract workshop, just at smaller scale:

| File | Lines | Pattern |
|------|-------|---------|
| `settings_ui.rs` | 749 | Tab bar, row cursor, A/D adjust, Enter activate, Esc close, text capture for keybind rebind |
| `contract_library.rs` | ~250 | Tab bar, sort toggle, row selection, Enter opens detail |
| `shipeditor/exterior.rs` | ~400 | Row selection for hardpoint/item/plating, Tab/A/D/Enter for edit |
| `shipeditor/interior.rs` | ~450 | Row selection for room placement, Tab/A/D/Enter for edit |

Each migration removes the manual string building and keyboard parsing, replacing it with `SelectablePanel` rows + the shared `navigate_selectable_panel` system.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/settings_ui.rs` | Settings — migrate or replace |
| `reachlock-client/src/systems/contract_library.rs` | Library — migrate |
| `reachlock-client/src/systems/shipeditor/exterior.rs` | Exterior editor — migrate |
| `reachlock-client/src/systems/shipeditor/interior.rs` | Interior editor — migrate |
| `reachlock-client/src/widget_kit/panel.rs` | SelectablePanel (from S94) |

## Freeze first

### Settings tab → SelectablePanel mapping

The settings UI has 6 tabs. Each tab's rows map to `SelectableRow` variants:

```rust
fn settings_rows_for_tab(tab: SettingsTab, draft: &Settings) -> Vec<SelectableRow> {
    match tab {
        SettingsTab::Audio => vec![
            SelectableRow::Slider { label: "Master volume".into(), value: draft.audio.master_volume, min: 0.0, max: 1.0 },
            SelectableRow::Slider { label: "Music volume".into(), value: draft.audio.music_volume, min: 0.0, max: 1.0 },
            SelectableRow::Slider { label: "SFX volume".into(), value: draft.audio.sfx_volume, min: 0.0, max: 1.0 },
            SelectableRow::Slider { label: "Voice volume".into(), value: draft.audio.voice_volume, min: 0.0, max: 1.0 },
            SelectableRow::Toggle { label: "Mute when unfocused".into(), value: draft.audio.mute_when_unfocused },
        ],
        // ... same pattern for Video, Controls, Gameplay, Accessibility, Network
    }
}
```

### Settings-specific row types

Settings has two special cases that `SelectableRow` doesn't cover natively:

```rust
pub enum SettingsRow {
    Standard(SelectableRow),
    /// "Reset all keybinds to defaults" — a confirmable action, not a value
    ResetAction { label: String, confirmed: bool },
    /// "Server URL: http://..." with text edit mode
    TextEdit { label: String, value: String, editing: bool },
}
```

### Ship editor row mapping

```rust
fn exterior_rows(state: &ShipEditorState, cfg: &ShipConfig, content: &ContentIndex) -> Vec<SelectableRow> {
    vec![
        SelectableRow::Choice {
            label: "Hull".into(),
            choices: /* available hull frames */,
            selected: /* index of current */,
        },
        // Hardpoints: one Choice row per slot
        // Plating: one Slider row per zone (mass adjust)
        // Paint: Choice rows for primary/secondary/accent
        // Decals: Choice rows per slot
    ]
}
```

## Deliverables

### 1. Settings UI migration

- [ ] Extract `settings_rows_for_tab(SettingsTab, &Settings) -> Vec<SettingsRow>` pure function
- [ ] Replace `render()` function (which builds raw string) with `SelectablePanel` component assignment
- [ ] Replace manual `handle_row()` key dispatch with `navigate_selectable_panel` (from S94) + per-row activation handlers
- [ ] Keep keybind capture mode and text edit capture mode — these are modal sub-states not handled by SelectablePanel
- [ ] Reduce `settings_ui.rs` from 749 → <350 lines

### 2. Contract library migration

- [ ] Extract `library_rows(&LibraryState, &ContentIndex) -> Vec<SelectableRow>` pure function
- [ ] Replace `render_library_panel` with `SelectablePanel` component
- [ ] Replace `library_system` input handling with shared navigation + tab/action handlers
- [ ] Keep sort/filter state in `ContractLibraryState`

### 3. Ship exterior editor migration

- [ ] Extract `exterior_rows(&ShipEditorState, &ShipConfig, &ContentIndex) -> Vec<SelectableRow>` pure function
- [ ] Replace `editor_panel_text()` string building with `SelectablePanel`
- [ ] Replace `editor_system` input handling with shared navigation
- [ ] Keep `ShipEditorState` resource (holds draft, selected row indices)

### 4. Ship interior editor migration

- [ ] Extract `interior_rows(&InteriorEditorState, &InteriorConfig) -> Vec<SelectableRow>` pure function
- [ ] Replace `interior_panel_text()` string building with `SelectablePanel`
- [ ] Replace `interior_editor_system` input handling with shared navigation
- [ ] Keep `InteriorEditorState` resource

### 5. Verify no regressions

- [ ] Every toggle adjusts the correct setting
- [ ] Every slider increments at the expected rate
- [ ] Keybind capture still works (press Enter on a keybind row → capture mode → press key → bind)
- [ ] Text edit still works (press Enter on server URL → type text → Enter commits)
- [ ] Ship editor hardpoint cycling unchanged
- [ ] Library sort/filter unchanged

## Acceptance gates

```bash
cargo test -p reachlock-client settings
cargo test -p reachlock-client contract_library
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# Settings: open from pause → cycle all 6 tabs → adjust sliders → rebind a key → apply → verify
# Library: open from crew console → browse contracts → sort → import one
# Ship editor: open from shipyard → cycle hulls → add hardpoint → adjust plating
# Interior: open from interior-refit → add room → cycle room type

make check
```

## Non-goals

- Redesigning any panel layout
- Adding new features during migration
- Visual styling beyond cursor rendering
- Moving ship editor preview to 2D (that's S101)

## Gotchas

- **Settings has TWO modal input modes** (keybind capture, text edit). `navigate_selectable_panel` must NOT consume keys while in these modes. Check the mode flag before dispatching to shared navigation.
- **Settings must preview audio volume changes.** The `adjust` helper sets a `preview_volume` flag that spawns a tone. After migration, this flag must still propagate — the row activation handler for volume sliders should still set the preview flag.
- **Ship editor panels render via `hud.rs` `update_hud_panels`.** The editor text is currently built by `editor_panel_text()` called from `hud.rs:611`. After migration, the editor data is set on a `SelectablePanel` component instead, and `render_selectable_panel` from S94 handles display. The `hud.rs` panel text path for ship editors becomes empty — the `SelectablePanel` renderer handles it.
- **Settings visibility check.** `menu_input` and `pause` check `settings_ui.open` before processing input. After migration, verify the visibility check still works with the new component-based approach.
