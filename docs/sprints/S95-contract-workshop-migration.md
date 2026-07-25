# S95 — Contract Workshop Migration to PanelWidget

**Wave: UX-Refactor · Depends on:** S94 (PanelWidget), S34 (Contract workshop)

## Outcome

`contract_crafting.rs` is reduced from 943 lines to ~300 lines. The keyboard navigation, tab cycling, row selection, cursor rendering, and adjustment logic are delegated to the shared `SelectablePanel` widget from S94. Only the *content* — what each tab shows and how data changes on keypress — remains in the workshop file.

## Context

`contract_crafting.rs` is the single largest client file. It implements a 4-tab keyboard-driven editor:

| Tab | Content | Complexity |
|-----|---------|------------|
| Rules | List of rules, each with Condition, Action, Priority columns. Sub-selection per column. | High |
| LLM | Temperature slider, model name, system prompt text input. | Medium |
| Persona | Crew assignment list, role cycling, persona toggle. | Medium |
| Simulation | Scenario selector, eval results display. | Low |

The current implementation manually tracks: selected tab, selected rule, selected column (RuleCol), text edit buffers, key-by-key character input. All of this is shared widget logic that belongs in `SelectablePanel`.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/contract_crafting.rs` | The workshop — to be refactored |
| `reachlock-client/src/widget_kit/panel.rs` | `SelectablePanel` widget (from S94) |
| `reachlock-client/src/main.rs` | System registration |

## Freeze first

### Data extraction — tab content functions

The workshop's content is separated from the widget logic:

```rust
/// Pure function: build the rows for a tab given the current draft + selection state.
fn build_rules_rows(draft: &ContractDraft, sel_rule: usize, sel_col: RuleCol) -> Vec<SelectableRow> {
    // Returns SelectableRow::Action, Choice, Toggle, etc.
}

fn build_llm_rows(draft: &ContractDraft) -> Vec<SelectableRow> { ... }
fn build_persona_rows(draft: &ContractDraft) -> Vec<SelectableRow> { ... }
fn build_simulation_rows(draft: &ContractDraft, sim: &SimState) -> Vec<SelectableRow> { ... }
```

### Workshop-specific row types

Some workshop rows need custom rendering (e.g., rule conditions with nested `Condition` tree). These use `SelectableRow::Custom { id: String, label: String, data: Box<dyn Any> }` or a dedicated `WorkshopRow` enum that extends `SelectableRow`.

```rust
/// Workshop-specific row types that don't fit the generic SelectableRow variants.
pub enum WorkshopRow {
    Standard(SelectableRow),
    ConditionTree { label: String, condition: String, depth: usize },
    EvalResult { scenario: String, matched: usize, reasoning: String },
}
```

## Deliverables

### 1. Extract tab content builders

- [ ] `build_rules_rows()` — converts the current draft's rules into a `Vec<SelectableRow>`
- [ ] `build_llm_rows()` — LLM config rows (temperature slider, model text edit, prompt text)
- [ ] `build_persona_rows()` — crew assignment rows
- [ ] `build_simulation_rows()` — scenario list + eval result rows
- [ ] Each function is pure: takes data, returns rows. No Bevy queries.

### 2. Replace input handling with SelectablePanel navigation

- [ ] Delete the manual `Tab`/`W`/`S`/`A`/`D`/`Enter` key handling in `workshop_system`
- [ ] Replace with `navigate_selectable_panel` calls from S94
- [ ] The workshop system now only handles: open/close lifecycle, data mutations on row activation, building the row list on draft change

### 3. Build the render path

- [ ] `render_workshop_panel` now creates a `SelectablePanel` component
- [ ] The `render_selectable_panel` system from S94 handles cursor highlighting and tab header rendering
- [ ] Workshop-only rows (`ConditionTree`, `EvalResult`) are rendered inline with the standard rows

### 4. Reduce line count

- [ ] Target: `contract_crafting.rs` < 400 lines (from current 943)
- [ ] Delete: manual character input, manual cursor tracking, manual row formatting, tab header building

### 5. Keep existing behavior

- [ ] Tab cycling (Tab key) → unchanged
- [ ] Row selection (W/S) → unchanged
- [ ] Column cycling (A/D) → unchanged  
- [ ] Rule condition editing → unchanged
- [ ] Action verb selection → unchanged (ACTION_VERBS list preserved)
- [ ] Simulation scenario selection + eval → unchanged

### 6. Test

- [ ] `cargo test -p reachlock-client contract_crafting` — existing tests still pass
- [ ] Manual: open workshop, cycle tabs, edit a rule, run simulation, close

## Acceptance gates

```bash
cargo test -p reachlock-client contract_crafting
cargo clippy -p reachlock-client -- -D warnings

# Manual: all 4 workshop tabs render and are navigable
# Rule: add/remove rules, edit conditions, change actions
# LLM: adjust temperature, set model
# Persona: assign crew, toggle persona
# Simulation: pick scenario, see eval results

make check
```

## Non-goals

- Changing the contract draft data model
- Adding new workshop features (this is a refactor, not an extension)
- Migrating to egui (the workshop stays Bevy UI text-based)
- Visual upgrades beyond cursor rendering

## Gotchas

- **`ContractDraft` has nested mutable state.** Rules have conditions. Conditions have sub-conditions. The key handling must remain for the sub-editor (condition tree editing). Don't replace the condition editor — it's too deeply nested for generic `SelectableRow` handling.
- **The workshop pushes `FocusStack` layers (S90).** After migration, ensure the push/pop still fires when the panel opens/closes.
- **`WorkshopTab::Rules` has a sub-selection: `RuleCol` (Condition/Action/Priority).** The `SelectablePanel` widget handles a single `selected_row`. The workshop's sub-column selection is an additional dimension — keep the `RuleCol` tracker in the workshop and use it to decide which adjustment key affects which column.
- **Simulation tab uses `evaluate()` from core.** The eval results are not interactive rows — they're display-only output. Use `SelectableRow::Label` for them.
- **The `ContractWorkshopState` resource must stay.** It holds the draft, selected tab, selected rule, selected column, text edit buffers. The widget handles *rendering and navigation* but not *data ownership*.
