# S94 — PanelWidget Shared Abstraction

**Wave: UX-Refactor · Depends on:** S70 (Client UI framework)

## Outcome

A single `PanelWidget` struct + render system replaces the duplicated "text concatenation + keyboard input" pattern used by 8+ game panels. Every panel that currently builds display strings manually gets a shared render path. New panels in the future use `PanelWidget` instead of copy-pasting the pattern.

## Context

Every information panel in the game follows the same anti-pattern:

1. Define `*Visible: bool` resource
2. Toggle on `KeyJustPressed`
3. `spawn_*_panel` — creates a `Text` + `Node` entity at absolute position
4. `render_*_panel` — builds a `String` by pushing lines, reads visibility, assigns to `**text`
5. If interactive: a separate input system reads `ButtonInput<KeyCode>` directly

This pattern is copied across:
- `factions.rs` (reputation panel) — 140 lines
- `discovery.rs` (discovery panel) — 304 lines
- `career.rs` (career panel) — 141 lines
- `log_ui.rs` (captain's log) — similar pattern
- `culture_view.rs` (culture panel) — similar pattern
- `mission_board.rs` — similar pattern
- `contract_crafting.rs` (workshop) — 943 lines (worst offender)
- `contract_library.rs` — 200+ lines
- `settings_ui.rs` — 749 lines

The goal is NOT to perfectly abstract every panel. The goal is to share the boring parts (spawn/despawn, visibility toggle, text rendering, z-index, keyboard cursor) so panel authors focus on *what data to show* rather than *how to paint pixels*.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/widget_kit/` | New module — panel widget types |
| `reachlock-client/src/widget_kit/mod.rs` | Module root |
| `reachlock-client/src/main.rs` | Register new systems |

## Freeze first

### PanelWidget data types

```rust
// widget_kit/panel.rs

use bevy::prelude::*;

/// One row in a panel. Rows can be headers, key-value pairs, toggles, or sliders.
#[derive(Clone)]
pub enum PanelRow {
    /// A non-interactive section header: "── CAREERS ──"
    Header(String),
    /// A plain label: "No data loaded."
    Label(String),
    /// A key-value pair: "Credits: 1500"
    KeyValue { key: String, value: String },
    /// An empty separator row.
    Separator,
}

/// A non-interactive text panel with scrollable content.
/// For panels that just display information (factions, discovery, career, log).
#[derive(Component)]
pub struct InfoPanel {
    pub title: String,
    pub rows: Vec<PanelRow>,
}

/// An interactive panel with a keyboard cursor.
/// For panels with selectable rows and adjustable values (settings, editors).
#[derive(Component)]
pub struct SelectablePanel {
    pub title: String,
    pub tabs: Vec<String>,
    pub active_tab: usize,
    pub rows: Vec<SelectableRow>,
    pub selected_row: usize,
}

/// One interactive row in a SelectablePanel.
#[derive(Clone)]
pub enum SelectableRow {
    Toggle { label: String, value: bool },
    Slider { label: String, value: f32, min: f32, max: f32 },
    Choice { label: String, choices: Vec<String>, selected: usize },
    KeyBind { action_label: String, key: String },
    Action { label: String },
}
```

### Render systems (shared, not per-panel)

```rust
/// Renders ALL InfoPanel entities. Runs in Update.
pub fn render_info_panels(
    mut query: Query<(&InfoPanel, &mut Text, &mut Visibility)>,
) {
    for (panel, mut text, mut vis) in &mut query {
        *vis = Visibility::Visible;
        let mut lines = vec![format!("── {} ──", panel.title)];
        for row in &panel.rows {
            match row {
                PanelRow::Header(h) => lines.push(h.clone()),
                PanelRow::Label(l) => lines.push(format!("  {l}")),
                PanelRow::KeyValue { key, value } => lines.push(format!("  {key}: {value}")),
                PanelRow::Separator => lines.push(String::new()),
            }
        }
        **text = lines.join("\n");
    }
}
```

### Visibility toggle

```rust
/// System that toggles visibility: reads a resource, applies to panel entity.
pub fn toggle_info_panel(
    keys: Res<ButtonInput<KeyCode>>,
    trigger_key: KeyCode,  // or InputAction
    mut query: Query<&mut Visibility, With<InfoPanel>>,
) {
    // Pattern: check key, flip Visibility::Hidden ↔ Visible
}
```

## Deliverables

### 1. Create `widget_kit/panel.rs`

- [ ] Define `PanelRow`, `InfoPanel`, `SelectablePanel`, `SelectableRow` types (as above)
- [ ] Implement `render_info_panels` system
- [ ] Implement `render_selectable_panel` system (handles cursor highlight, row formatting)
- [ ] Implement `navigate_selectable_panel` system (reads ArrowUp/Down, Tab for tab switch, A/D for adjust, Enter for activate)

### 2. Implement shared visibility toggle helper

- [ ] Add `panel_visibility_toggle_system` that takes a resource + key + query, toggles Hidden↔Visible
- [ ] Pattern: multiple panels can share this system via different queries

### 3. Update `widget_kit/mod.rs`

- [ ] Add `pub mod panel;` 
- [ ] Re-export key types

### 4. Gate test — no new panel should use raw text patterns

- [ ] Add a test in `widget_kit/panel.rs`:
```rust
#[test]
fn panel_widget_renders_all_row_types() {
    let panel = InfoPanel { ... };
    // Verify the rendered string contains expected substrings
}
```

### 5. Register in main.rs

- [ ] Add `render_info_panels`, `render_selectable_panel`, `navigate_selectable_panel` to the Update schedule under `in_state(AppState::InGame)`

## Acceptance gates

```bash
cargo test -p reachlock-client widget_kit::panel
cargo clippy -p reachlock-client -- -D warnings

# Manual: Verify existing panels (factions, discovery, career) still render correctly
# (Migration to PanelWidget happens in S95/S96 — this sprint only builds the widget)

make check
```

## Non-goals

- Migrating any existing panel to use PanelWidget (that's S95/S96)
- Visual styling beyond text formatting
- Egui bridge integration (these are Bevy UI, not egui)
- Dynamic layout / flexbox (just text lines with newlines)

## Gotchas

- **`SelectablePanel` is for settings-like panels.** The contract workshop (943 lines) has deeply nested editing (rule conditions, action params, scenario selectors). `SelectablePanel` handles the top-level tab↔row↔adjust pattern but NOT the sub-editing. The workshop needs its own migration plan (S95).
- **`InfoPanel` can be rendered by a single `render_info_panels` system** that iterates ALL `InfoPanel` components. No per-panel render system needed — just set the `rows` field from the data source before rendering.
- **Don't remove the `PanelRow::Separator` variant.** Newline-only separators are used extensively in current panels and must be preserved for visual grouping.
- **The existing `widget_kit/` has skeletons for button, dropdown, list, etc.** `panel.rs` is a new addition — don't change the existing skeleton files unless they're needed.
