# S71 — Make Accessibility Real

**Spec:** §6 (UI/UX), §13 (accessibility) · **Wave C (legible, accessible, fun) · Depends on:** S70 (Client UI framework)

## Outcome

Every settings field has a consumer. Every colour-coded state has a glyph companion. Motion effects respect `reduce_motion`. Captions render for voice chat and NPC speech. Gamepad navigation works on every panel. The keybind table is exhaustive and rebindable. The Settings completeness gate (MASTER-PLAN Part 4) prevents dead settings from recurring.

Closes findings: **C2** (14 settings editable, persisted, read by nothing), **C3** (no `reduce_motion`), **C9** (colour is the only channel for several game states).

## Context

S31's deliverable 4 ("Settings consumers") existed on paper but shipped incomplete — 14 settings were wired into the UI, written to disk, and read by exactly zero systems (C2). The brief was correct; the coverage failure slipped past code review because there was no test asserting every field has a consumer.

This sprint builds the **Settings completeness enforcement gate** — a conformance test requiring every field in `Settings` (and all sub-structs) to have a registered consumer outside `settings_ui`. The editor already has the pattern: post-S81, every `ContentPayload` variant has a registered consumer. This sprint applies the same pattern to settings.

The UX-AUDIT-AND-PLAN (§2.2) calls out every dead setting by name. The semantic palette requirement (§2.3) — colour as the only channel for game state — directly disables C9. `reduce_motion` (C3) is called out as missing despite heavy screen shake, barrel rolls, camera blends, parallax dust, and hyperspace transit effects.

### Key files

| File | Role |
|---|---|
| `reachlock-client/src/settings.rs` | The `Settings` struct + all sub-structs |
| `reachlock-client/src/systems/settings_ui.rs` | Remove dead rows; ensure every displayed setting is wired |
| `reachlock-client/src/systems/audio.rs` | Wire audio volume settings, mute‑when‑unfocused |
| `reachlock-client/src/systems/hud.rs` | Wire colorblind mode, text_scale, screen_shake, semantic glyphs |
| `reachlock-client/src/systems/ship.rs` | Wire control settings (mouse_sensitivity, invert_y) |
| `reachlock-client/src/systems/menu.rs` | Wire accessibility settings, high-contrast UI |
| `reachlock-client/src/main.rs` | Wire network settings (server_url, auto_connect, show_latency) |
| `reachlock-client/src/systems/gamepad.rs` | New — gamepad navigation logic |
| `reachlock-client/src/systems/captions.rs` | New — captions overlay for voice + NPC speech |

## Freeze first

### Settings completeness test definition

A conformance test in `reachlock-client/src/settings.rs`:

```rust
/// Every Settings field must have a registered consumer outside settings_ui.
/// Add a mapping here when you wire a field. If you add a field without
/// adding its consumer, this test fails.
fn test_all_settings_have_consumers() {
    let consumers = settings_consumer_registry();
    // Recursively enumerate every field in Settings.
    // For each field, assert consumers.contains_key(field_path).
    // Field paths: "audio.master_volume", "accessibility.reduce_motion", etc.
}
```

The consumer registry is a `HashMap<&'static str, &'static str>` mapping field paths to consumer descriptions. Adding a new field without a registry entry is a compile-time or test-time failure. This is the gate that prevents C2 from recurring (MASTER-PLAN Part 4).

### `reduce_motion` field and effect description

Add to `AccessibilitySettings`:

```rust
pub reduce_motion: bool,  // false = full motion, true = reduce
```

When `reduce_motion` is `true`:
- `screen_shake` is forced to `0.0` (no camera shake)
- Animation speed multipliers (barrel roll, camera blends, hyperspace transition) are capped at `0.25×` or skipped
- Parallax scrolling speed is halved
- Particle effects (dust, engine trails) are reduced to 30% density
- Mode transitions (docking, hyperspace) use fade-in/fade-out instead of animated sweeps

### Semantic palette rules

No game state is conveyed by hue alone. Every coloured indicator pairs with a glyph, shape, or text label:

| State | Colour channel | Glyph / companion |
|---|---|---|
| Faction standing (ally / neutral / hostile) | Green / grey / red | ★ = ally, ◆ = neutral, ⚠ = hostile |
| Health / hull integrity | Green → yellow → red | █ full, ▆ damaged, ▄ critical, ∅ destroyed |
| Threat level (combat) | White → yellow → red | ◇ passive, ◈ alert, ◆ engaged, ◆◆ critical |
| Reputation tier | Bronze / silver / gold / platinum | Ⅰ / Ⅱ / Ⅲ / Ⅳ |
| Offline / online badge | Grey / blue | ⛔ offline, ✓ online |
| Fuel level | Green → red | ⛽ █▆▄▂ |
| Target lock | Red box | ◆ locked (filled diamond on reticle) |
| Cargo / inventory fullness | Green → red | □ empty, ■ █ partial, ■ ■ ■ full |
| Scanner contact friendly/hostile | Blue / red | ◆ friendly, ◆ hostile |
| Mission difficulty | Green / yellow / red / skull | ★ easy, ★★ medium, ★★★ hard, ☠ extreme |

The same pattern the editor already uses (✔ / ✘ in the Validation Report) applied across every HUD element.

## Deliverables

### 1. Settings audit — wire or delete every dead field

- [ ] Enumerate every field in `Settings` + sub-structs. For each field, trace whether a system outside `settings_ui` reads it.
- [ ] Fields with a consumer: wire the consumer (see deliverables 2–8).
- [ ] Fields without a consumer AND no plausible future consumer: **remove from the struct**. Do not ship dead settings.
- [ ] Known dead fields from UX audit that must be wired (not deleted): `colorblind_mode`, `text_scale`, `high_contrast_ui`, `screen_shake` (already wired? verify), `subtitles`, `mouse_sensitivity`, `invert_y`, `controller_deadzone`, `aim_assist`, `combat_log_verbosity`, `show_latency`, `show_fps`. Wire each.
- [ ] If any field is genuinely placeholder (e.g., `controller_deadzone` before gamepad is done), mark it `#[doc = "PLACEHOLDER — wired in S71.5 gamepad deliverable"]` so the conformance test knows it has a planned consumer.
- [ ] `settings_ui.rs`: remove any settings row that corresponds to a deleted field. Ensure `row_count()` returns the correct live count (fixes C10).

### 2. `reduce_motion` implementation

- [ ] Add `reduce_motion: bool` to `AccessibilitySettings`.
- [ ] Camera shake: multiply shake amplitude by `0.0` when `reduce_motion` is true (in `systems/hud.rs` or wherever screen shake is applied).
- [ ] Animation speed: introduce an `AnimationSpeedMultiplier` resource initialized from settings. Systems that play timed animations (barrel roll, camera blends, hyperspace transit) read this multiplier. When `reduce_motion`, cap at `0.25×` or skip the animation entirely.
- [ ] Parallax: in the parallax system (background star layers), divide scroll speed by 2 when `reduce_motion`.
- [ ] Particles: particle emitters (engine trails, debris, dust) reduce spawn rate to 30% when `reduce_motion`.
- [ ] Mode transitions: replace animated sweeps with cross-fade when `reduce_motion`. The transition still takes *some* time (100 ms fade) so the player isn't disoriented by instant scene cuts.
- [ ] Wire the settings checkbox in the Accessibility tab. Changing it takes effect immediately.
- [ ] Test: toggle `reduce_motion` on → screen shake stops, animations are near-instant, parallax slows, particles thin. Toggle off → full motion restored.

### 3. Semantic palette — glyph companion for every coloured state

- [ ] Define a `SemanticGlyph` type (simple `char` or `&'static str`, or an enum for the renderer to pick the right sprite/font glyph).
- [ ] Build a registry mapping each game state (faction standing, health level, threat level, reputation tier, online/offline, fuel, target lock, cargo fullness, scanner contact, mission difficulty) to a `(Colour, SemanticGlyph)` pair.
- [ ] In `hud.rs`: every place colour is the *only* signal, render the glyph alongside the colour. For example, faction tags show `★` (ally) / `◆` (neutral) / `⚠` (hostile) regardless of colourblind mode.
- [ ] In `factions.rs`: reputation bars show the tier glyph + colour.
- [ ] In `combat.rs`: threat indicators show the threat glyph + colour.
- [ ] Colourblind mode (`Protanopia`/`Deuteranopia/`Tritanopia`): swap the colour palette *while keeping the same glyphs*. The glyphs already carry the meaning; the colour is redundant reinforcement.
- [ ] The editor already does this for validation (✔/✘). Reuse the same rendering primitive.
- [ ] Test: in every colourblind mode, every game state indicator is distinguishable by glyph alone (manual verification). Automated: assert every state has a glyph in the registry and the glyph is non-empty.

### 4. Captions for voice chat and NPC speech

- [ ] Create `CaptionsOverlay` — a Bevy UI node (from S70's feather framework) anchored at the bottom-center of the screen, showing the most recent 1–3 lines of captions.
- [ ] Wire `settings.accessibility.subtitles` (bool) and `subtitle_size` (f32) to control visibility and font scale.
- [ ] Voice chat (S29): when a voice packet is received with a transcript (from the speech-to-text pipeline), push the speaker name + text to the captions queue.
- [ ] NPC speech (S62 voice synthesis): when a TTS line plays, push the same line as a caption.
- [ ] Caption format: `"[Speaker] Text."` — fade in over 200 ms, display for `duration * 1.5` (minimum 2 s), fade out over 500 ms.
- [ ] Captions respect `text_scale` and `high_contrast_ui`.
- [ ] Test: toggle subtitles on → speech lines appear as captions. Toggle off → captions disappear mid-line.

### 5. Gamepad navigation

- [ ] Create `reachlock-client/src/systems/gamepad.rs`.
- [ ] Implement a **focus ring** for all UI panels (menu, settings, pause, HUD overlays). The focus ring is a visible highlight (2 px border + background tint) on the currently selected interactive element.
- [ ] Gamepad D-pad / left stick: navigate the focus ring (up/down/left/right). Map to `InputAction::UiUp` / `UiDown` / `UiLeft` / `UiRight` (add these to `InputAction`).
- [ ] A button: activate / confirm (maps to `InputAction::Interact` or a new `UiConfirm`).
- [ ] B button: back / cancel (`UiCancel`).
- [ ] X / Y: mapped to context-specific actions (e.g., X = open inventory, Y = open map — configurable in keybinds).
- [ ] Gamepad input works alongside keyboard input — no exclusive mode.
- [ ] The focus ring works in every panel: main menu, settings tabs, pause menu, ship HUD, station HUD, editor panels.
- [ ] Non-gamepad players see no focus ring. The focus ring only renders when a gamepad is detected as active input.
- [ ] `controller_deadzone` setting is wired to filter analog stick input.

### 6. Exhaustive keybind table in settings UI

- [ ] The Controls tab in settings shows **every** `InputAction` variant in a scrollable table, grouped by category (Movement, Combat, Interaction, Editor, OnBoard, Reserved).
- [ ] Each row shows: action name (human-readable), current keybind (rendered as display string), and a "Rebind" button.
- [ ] The table is exhaustive — no `InputAction` variant is hidden. The "Reserved" group is collapsible.
- [ ] Key rebind flow (from S31): click Rebind → enter capture mode → press key → validate (no duplicate conflict warning) → accept or cancel.
- [ ] Unbound actions show "—" with a yellow warning indicator.
- [ ] Reset to defaults per-group and for all.
- [ ] Test: every `InputAction` variant is reachable in the Controls tab. Assert `InputAction::iter()` count matches the settings UI row count.
- [ ] The keybind display string table (`KeyCode` → `&str`) must be exhaustive. New `KeyCode` variants added by Bevy upgrades are caught at compile time by a `match` that returns `Result<&str, UnknownKey>`.

### 7. Text scaling

- [ ] `text_scale` (f32, 0.5–3.0) from `AccessibilitySettings` multiplies all UI font sizes.
- [ ] Implement as a `UiScale` resource (or Bevy's `UiScale` if available in S70's feather setup) that all text and layout nodes read.
- [ ] The scale applies globally to: menu text, HUD labels, dialog text, captions, tooltips, keybind table, panel headers.
- [ ] Does NOT apply to: 3D text in the game world (billboard labels), debug overlays, or the editor UI (editor has its own zoom).
- [ ] Test: set `text_scale = 2.0` → all UI text is twice as large. Set to `0.5` → half size. Set to `1.0` → default.

### 8. High-contrast UI mode

- [ ] `high_contrast_ui` (bool) from `AccessibilitySettings`.
- [ ] When enabled: swap to a high-contrast palette with documented minimum contrast ratio (WCAG AA: 4.5:1 for text, 3:1 for large text / UI components).
- [ ] High-contrast palette: light background (`#F5F5F5`), dark text (`#1A1A1A`), high-saturation accent colours with thick (2 px) borders on all interactive elements.
- [ ] Focus ring becomes more prominent (4 px, dashed, high-contrast colour).
- [ ] Panel backgrounds use solid fills instead of semi-transparent overlays.
- [ ] Test: toggle `high_contrast_ui` on → all UI elements have sufficient contrast against their backgrounds. Automated: sample pixel pairs from known UI regions and assert contrast ratio ≥ 4.5:1.

### 9. Settings completeness gate test

- [ ] Implement `settings_consumer_registry()` — a function returning a `HashMap<&'static str, &'static str>` mapping every settings field path to a consumer description.
- [ ] Implement `test_all_settings_have_consumers()` — recursively walks the `Settings` struct's fields (use `reflection` or a manual macro that enumerates fields), and for each field asserts the registry contains an entry.
- [ ] The test must cover all sub-structs: `AudioSettings`, `VideoSettings`, `ControlSettings`, `GameplaySettings`, `AccessibilitySettings`, `NetworkSettings`.
- [ ] Adding a new field to any settings struct without adding a registry entry causes a test failure.
- [ ] The registry is the source of truth for "this field is wired." Settings UI (`settings_ui`) is NOT a valid consumer — the UI reads the field to display it, but a *different* system must actually use the value.
- [ ] Exception: fields whose sole purpose is display configuration (e.g., `show_fps`) can list `settings_ui` as the consumer IF `settings_ui` is the one toggling the FPS overlay visibility. Document the exception.
- [ ] This test runs in CI as part of `make check` → `cargo test -p reachlock-client`.

## Acceptance gates

```bash
# Settings completeness gate
cargo test -p reachlock-client settings_consumer_registry  # all fields have consumers

# reduce_motion
cargo run -p reachlock-client                              # toggle reduce_motion on: shake stops,
                                                            # animations skip, parallax slows
# Semantic palette
cargo run -p reachlock-client                              # set colorblind_mode to Deuteranopia
                                                            # → every state indicator is still
                                                            # readable by glyph alone
# Captions
# Enable subtitles → NPC speaks → captions appear at bottom-center
# Disable subtitles → captions disappear
# Voice chat (S29): speak → transcript appears as caption

# Gamepad
# Connect a gamepad → focus ring appears → navigate menus, settings, HUD

# Exhaustive keybind table
# Open Settings → Controls tab → every InputAction is listed, rebindable
# No action is hidden or missing

# Text scaling
# Set text_scale to 2.0 → all UI text doubles → panels still fit on screen

# High-contrast UI
# Toggle high_contrast_ui on → all text meets WCAG AA contrast ratio

make check
```

Manual verification path: open game → Settings → Accessibility tab → toggle every option → verify each changes behaviour in-game → toggle colourblind mode → verify glyphs render → open Controls tab → verify every action is listed → rebind an action → verify the new key works → quit → relaunch → verify settings persist.

## Non-goals

- Custom colour themes (the high-contrast mode picks one of two fixed palettes)
- Language/locale (S31 non-goal — still deferred)
- Cloud-synced settings (S31 non-goal — still deferred)
- Per-save settings (settings are global, not per save file)
- Screen reader / TTS for UI navigation (that is a separate infrastructure sprint)
- Switch-style adaptive controller support (hardware-level — out of scope)
- Rebindable mouse buttons beyond M1/M2 (mouse has a fixed button count in Bevy's `MouseButton` enum)
- Controller vibration / haptic feedback (requires hardware API integration not yet planned)
- Caption customisation beyond on/off and size (position, colour, background opacity deferred)
- Full keyboard-only accessibility for the editor (editor is power-user tooling, not required to be a11y-complete for this sprint)

## Gotchas

- **The completeness gate is the point of this sprint.** Do not ship a settings audit without the conformance test. C2 happened because there was no machine-enforceable contract. The gate test (`test_all_settings_have_consumers`) is deliverable 9, but define its interface in Freeze first before wiring any individual setting.
- **Semantic palette ≠ colourblind mode.** Colourblind mode remaps hues; the semantic palette ensures glyphs exist regardless of colour mode. They compose: colourblind mode swaps the colour, the glyph remains. Don't conflate the two.
- **`controller_deadzone` is genuinely a placeholder.** Wire it (filter analog stick input < deadzone) but the full gamepad deliverable in this sprint is about the focus ring and button mapping, not analogue precision. The placeholder note in the struct stays until gamepad is fully polished.
- **Adding `UiUp`/`UiDown`/`UiLeft`/`UiRight` to `InputAction`** expands the enum. This is fine — S31 explicitly reserved the right to add variants. These new actions get defaults (`ArrowUp` etc.) but also default to gamepad D-pad/left stick. The keybind table must show them in a "Navigation" group.
- **The captions overlay competes with the S38 deliberation theater panel.** If both are visible, captions remain bottom-center and the deliberation panel is above them. No overlap — the deliberation panel has a fixed height and captions draw below it.
- **`text_scale` vs `ui_scale`.** S31 has both `ui_scale` (in `VideoSettings`) and `text_scale` (in `AccessibilitySettings`). `ui_scale` scales the entire UI layout (buttons, panels, spacing). `text_scale` scales only font sizes. They multiply: `effective_font_size = base_size * ui_scale * text_scale`. If `text_scale` is `2.0` and `ui_scale` is `0.8`, fonts end up at `1.6×`. Document this interaction in the settings UI tooltip.
- **The focus ring must not interfere with the editor.** The editor (S67/S68) uses `bevy_egui` which has its own focus model. The gamepad focus ring in this sprint applies to the S70 feather UI panels only. `bevy_egui` panels are excluded. Document this in the focus ring implementation.
- **If S70 hasn't landed yet, do not start S71.** The feather UI framework (focus rings, panel abstraction, `UiScale` resource, consistent button/widget pattern) is a hard dependency. S71 implements *on top of* S70's primitives. If S70 is delayed, park S71.
- **`KeyCode` display string table must be exhaustive.** Same pattern as S31's gotcha: `KeyCode` doesn't derive `Serialize`/`Deserialize` natively. The display string conversion is a `match KeyCode` — new Bevy `KeyCode` variants will cause a compile error if the match is non-exhaustive. Add a `KeyCodeDisplay` impl in `settings.rs` with `_ => Ok("?")` fallback only after verifying the variant doesn't exist in current Bevy. CI's `-D warnings` catches unknown variants at compile time.
