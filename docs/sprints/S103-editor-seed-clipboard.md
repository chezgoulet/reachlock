# S103 — Editor Seed Clipboard Button

**Wave: UX-QoL · Depends on:** S67 (Editor shell)

## Outcome

The seed panel's seed value gets a "Copy" button that copies the seed to the system clipboard. One-click shareability for seeds.

## Context

The editor's seed panel (`seed_workflow.rs`) shows a seed value with "Reroll All" and "Lock Current" buttons. The seed is the core of determinism — share a seed and another author gets the exact same generated content. But there's no way to copy it except manually selecting the number and using Ctrl+C.

### Key files

| File | Role |
|------|------|
| `reachlock-editor/src/seed_workflow.rs` | Seed panel UI — add Copy button |
| `reachlock-editor/src/main.rs` | `EditorApp` — seed workflow integration |

## Freeze first

### Copy button behavior

1. Button is always visible next to the seed value display
2. Clicking copies to clipboard
3. Status feedback: button text changes to "Copied!" for 2 seconds, then reverts to "Copy"
4. Reset timer: if clicked again while showing "Copied!", reset the 2s timer

## Deliverables

### 1. Add clipboard dependency

- [ ] Add `arboard` crate to `reachlock-editor/Cargo.toml` (cross-platform clipboard, no system deps on Linux beyond x11/wayland libs already present)
- [ ] Alternative: use `egui::Output::copied_text` — egui has built-in clipboard support via `ui.output_mut(|o| o.copied_text = "...")`. Prefer this (no new dependency).

### 2. Add Copy button to seed panel

- [ ] In `seed_workflow.rs`: next to the seed display, add `if ui.button("📋 Copy").clicked() { ... }`
- [ ] On click: `ui.output_mut(|o| o.copied_text = seed.to_string());`
- [ ] Track a `copy_cooldown: Option<Instant>` in `SeedWorkflow`
- [ ] While cooldown is active (within 2s), button shows "✓ Copied!" and is non-clickable (or clicking resets timer)

### 3. Alternative: Add to seed panel display

If the seed is shown as a `DragValue` (editable number), add the Copy button to the right of it in the `ui.horizontal` layout.

```rust
ui.horizontal(|ui| {
    ui.label("Seed:");
    ui.add(egui::DragValue::new(&mut self.seed).range(0..=SEED_MASK));
    if self.copy_cooldown.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
        ui.add_enabled(false, egui::Button::new("✓ Copied!"));
    } else {
        if ui.button("📋 Copy").clicked() {
            ui.output_mut(|o| o.copied_text = self.seed.to_string());
            self.copy_cooldown = Some(Instant::now());
        }
    }
});
```

### 4. Reset copy text if seed changes

- [ ] When the seed value changes (via DragValue or Reroll All), reset `copy_cooldown` to `None` so the button reverts to "📋 Copy"

## Acceptance gates

```bash
cargo clippy -p reachlock-editor -- -D warnings
cargo test -p reachlock-editor

# Manual:
# 1. Open editor → seed panel shows seed value
# 2. Click "📋 Copy" → button changes to "✓ Copied!"
# 3. Paste (Ctrl+V) into a text editor → seed value is pasted
# 4. Wait 2s → button reverts to "📋 Copy"
# 5. Change seed value → Copy button resets

make check
```

## Non-goals

- "Copy as JSON-safe seed" (seed ≤ 2^53 already enforced by SEED_MASK)
- "Copy with generation params" (copies seed only)
- "Share seed" to any online service
- Keyboard shortcut for copy (Ctrl+C already copies selected text in egui)

## Gotchas

- **egui clipboard is frame-scoped.** `ui.output_mut(|o| o.copied_text = ...)` sets clipboard text that persists after the frame. No need for a native clipboard crate.
- **`Instant` vs `std::time::Instant`.** Use `std::time::Instant` for the cooldown timer (same as `EditorApp` uses for autosave and status expiry).
- **Button text with emoji.** egui on Linux may not render emoji in buttons if the default font lacks glyph coverage. Test on the target platform. If emoji doesn't render, use "[Copy]" and "[Copied]" as fallback text.
- **`ui.output_mut` panics if called outside of a `Response` context.** Always call it after rendering a widget (inside a `clicked()` handler is fine).
