# S102 — Editor AI Panel: History, Quick-Params, Diff

**Wave: UX-QoL · Depends on:** S67 (Editor shell), Phase 2.5 AI generation (existing)

## Outcome

The editor's AI generation bar becomes an expandable panel with:
1. **Prompt history** — last 10 prompts saved per content type, with re-submit
2. **Quick-access parameter sliders** — temperature, max tokens, model picker (without opening the separate AI Settings modal)
3. **Diff preview** — before applying AI output, show which fields changed with old→new values

## Context

Current AI bar (`editor/src/main.rs:1027-1105`) is a single-line text area + "Generate" + "Clear" buttons. Parameters (temperature, max tokens, model) require opening AI → AI Settings in a separate modal window. No history. No diff.

```rust
// Current: all-or-nothing apply
match open.editor.apply_ai_json(&result.json_value) {
    Ok(_) => { applied = true; }
    // ...
}
```

The editor has no way to see what the AI changed before applying. If the model hallucinates a field, the user only discovers it after the values are already in the editor fields.

### Key files

| File | Role |
|------|------|
| `reachlock-editor/src/main.rs` | AI bar rendering (lines 1027-1105) |
| `reachlock-editor/src/ai.rs` | `AiConfig`, `generate_content`, `GenerationResult` |
| `reachlock-editor/src/settings_window.rs` | AI settings modal — extract quick params from here |
| `reachlock-editor/src/app.rs` | `Editor` trait — `apply_ai_json`, `snapshot` |

## Freeze first

### Prompt history struct

```rust
/// Per-content-type prompt history. Persisted to save/ai_history.ron.
pub struct PromptHistory {
    /// (content_type_name, prompt_text, timestamp)
    entries: Vec<(String, String, std::time::SystemTime)>,
    max_entries: usize,  // 10
}
```

### Diff entry struct

```rust
/// One field-level diff: what changed and from what.
pub struct FieldDiff {
    pub field_path: String,      // "identity.name" or "personality.traits[0]"
    pub old_value: String,       // RON representation of old value
    pub new_value: String,       // RON representation of new value
}
```

### Quick-params integration

The AI bar, when expanded, shows:
```
┌─ AI Generation ──────────────────────────────────┐
│ Model: [llama3.2:3b    ▼]  Temp: [0.7  ==──]    │
│ Tokens: [4096]  ⚙ Settings…                      │
│                                                  │
│ Prompt:                                          │
│ ┌──────────────────────────────────────────────┐ │
│ │ a grizzled voidborn smuggler                 │ │
│ └──────────────────────────────────────────────┘ │
│                                                  │
│ [▼ History]                          [Generate]  │
│ ▸ "a tier-7 kinetic railgun"                    │
│ ▸ "nebula system on far frontier"               │
│                                                  │
│ ── Diff Preview ────────────────────────────────│
│ ✓ identity.name: "Kaelen" → "Zara"              │
│ ✓ personality.traits: ["stoic"] → ["stoic",...] │
│ ⚠ species: Voidborn → Human (may be incorrect)  │
│                                                  │
│ [Accept All]  [Reject]  [Accept Selected]       │
└──────────────────────────────────────────────────┘
```

## Deliverables

### 1. Add prompt history

- [ ] Add `PromptHistory` struct to `reachlock-editor/src/ai.rs`
- [ ] Add `load_prompt_history()` / `save_prompt_history()` functions (persist to `save/ai_history.ron`)
- [ ] Add `history: PromptHistory` field to `EditorApp`
- [ ] In the AI bar: add a "History" dropdown that shows last 10 prompts for the active content type
- [ ] Clicking a history entry: fills the prompt text area

### 2. Add quick-access parameters

- [ ] Add `quick_temp: f32`, `quick_max_tokens: u32`, `quick_model: String` fields to `EditorApp`
- [ ] In the AI bar, above the prompt text area: render model selector (ComboBox), temperature slider (DragValue), max tokens input
- [ ] These fields are initialized from `AiSettingsWindow` config on startup; changes here update the config used for generation
- [ ] "Settings…" link opens the full `AiSettingsWindow` for advanced config

### 3. Add diff preview

- [ ] Before applying `result.json_value` to the editor:
  - Call `editor.snapshot()` to get the current state as RON
  - Deserialize the current RON and the new JSON value into `serde_json::Value`
  - Recursively diff the two values, building `Vec<FieldDiff>`
- [ ] Render diffs in the AI bar below the generate button: each diff shows field_path + old → new
- [ ] Diffs are color-coded: green (safe), yellow (unexpected type change), red (field missing in new)
- [ ] Buttons: "Accept All" (applies), "Reject" (discards), "Accept Selected" (future)

### 4. Diff preview UI

- [ ] Only visible when `ai_result_rx` has received an outcome AND the outcome is Ok
- [ ] The existing "AI content applied" status message becomes the diff panel
- [ ] Diff panel disappears when Accept or Reject is clicked
- [ ] On Accept: calls `editor.apply_ai_json()` as before
- [ ] On Reject: discards the result, keeps the current editor state

### 5. Co-locate the AI bar changes

- [ ] The AI bar is a `TopBottomPanel::top("ai_bar")` — the expanded version adds more rows
- [ ] Collapse to single-line mode when not in use (current behavior), expand when the user clicks the model/temp row or starts typing a prompt

### 6. Test

- [ ] Prompt history persists across editor restarts
- [ ] Quick-params override AiConfig settings for that generation
- [ ] Diff correctly identifies field changes
- [ ] Accept applies changes; Reject discards

## Acceptance gates

```bash
cargo clippy -p reachlock-editor -- -D warnings
cargo test -p reachlock-editor

# Manual:
# 1. Open editor → type prompt → Generate → see diff preview
# 2. Verify diff shows old→new per field
# 3. Click Accept → editor fields updated
# 4. Open AI bar again → history dropdown shows the prompt
# 5. Click history entry → fills prompt field
# 6. Adjust temperature slider → generation uses new temp

make check
```

## Non-goals

- Streaming diff as the model generates
- Field-level accept/reject (all-or-nothing accept is sufficient)
- AI generation cost estimation in the editor
- Multiple provider support in quick-params (use the configured provider from AiSettings)
- Prompt templates / saved prompt library

## Gotchas

- **`editor.snapshot()` returns RON.** The diff must compare the old snapshot (RON → `serde_json::Value`) with the new AI output (already `serde_json::Value`). RON ↔ JSON round-trip may lose type information (e.g., RON tuple `(176, 148, 92)` vs JSON array `[176, 148, 92]`). Normalize both to a common representation before diffing.
- **Not all editors support `snapshot()`.** The `Editor` trait's `snapshot()` returns `None` for previewers. The diff feature only works for editors that implement snapshot. Gate the diff UI on `editor.snapshot().is_some()`.
- **`AiConfig` is owned by `AiSettingsWindow`.** Quick-params should reference the same config or a copy. Generate uses `ai_settings.config()` for endpoint/key; quick-params only override model/temp/tokens for the generation call.
- **Diff rendering is string-based.** Building `FieldDiff` as strings means long text fields (backstory, public_bio) will show truncated old/new in the diff. Show first 80 chars with "..." for long values.
- **History per content type, not global.** "grizzled voidborn" is only meaningful for Soul editor. Store `(content_type_name, prompt)` in history and filter by active editor type.
