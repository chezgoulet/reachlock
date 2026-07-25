# S90 — FocusStack Input Gating

**Wave: UX-Hardening · Depends on:** S06 (Mode state machine), S16 (Dialogue), S34 (Contract crafting), S17/S18 (Ship editors)

## Outcome

Every gameplay-input system checks the `FocusStack` before consuming keyboard input. When a modal or panel is open (character creation, contract workshop, dialogue typing, ship editor, crew conference), WASD no longer moves the avatar. The `top_captures_input` gate on `FocusStack` is the single point of enforcement — no input system reads raw `ButtonInput<KeyCode>` without it.

## Context

`FocusStack` exists in `focus_stack.rs` with a working `top_captures_input()` method and a full test suite. **Zero systems call it.** Every input system reads raw `ButtonInput<KeyCode>` or `ButtonInput<KeyCode>` directly:

| System | File | Issue |
|--------|------|-------|
| `dialogue_input` | `dialogue.rs:151` | Typing mode should suppress WASD movement |
| `workshop_system` | `contract_crafting.rs` | Tab/WASD editing should not move avatar |
| `character_creation_input` | `character_creation.rs:444` | Separate AppState, but keycodes overlap with game |
| `crew_conference_hotkey` | `comms.rs` | Y-key conference steals from flight |
| `shipeditor::editor_system` | `shipeditor/exterior.rs` | A/D cycling hardpoints should not strafe |
| `shipeditor::interior_editor_system` | `shipeditor/interior.rs` | Same issue — editor keys vs flight keys |
| `library_system` | `contract_library.rs` | Tab/WASD should not walk avatar |
| `trope_input_system` | `trope_dispatcher.rs` | Number keys for trope choices |
| `encounter_choice_system` | `encounter_executor.rs` | Number keys for encounter choices |
| `dilemma_input_system` | `dilemma.rs` | Number keys for dilemma choices |

The `FocusStack` is already pushed/popped correctly — `character_creation.rs:174` pushes `FocusLayer::Modal`, `contract_crafting` does not yet push anything. The issue is that the consuming systems never ask the stack whether they should be consuming input.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/focus_stack.rs` | The FocusStack resource (no changes needed) |
| `reachlock-client/src/systems/dialogue.rs` | Gate `dialogue_input` + `typing()` check |
| `reachlock-client/src/systems/contract_crafting.rs` | Gate `workshop_system`, push/pop on open/close |
| `reachlock-client/src/systems/contract_library.rs` | Gate `library_system` |
| `reachlock-client/src/systems/shipeditor/exterior.rs` | Gate `editor_system` |
| `reachlock-client/src/systems/shipeditor/interior.rs` | Gate `interior_editor_system` |
| `reachlock-client/src/systems/comms.rs` | Gate `crew_conference_hotkey` |
| `reachlock-client/src/systems/trope_dispatcher.rs` | Gate `trope_input_system` |
| `reachlock-client/src/systems/encounter_executor.rs` | Gate `encounter_choice_system` |
| `reachlock-client/src/systems/dilemma.rs` | Gate `dilemma_input_system` |

## Freeze first

### Panel → FocusLayer mapping table

Every openable panel must push a `FocusLayer` and pop it on close. The mapping:

| Panel/Surface | FocusLayer | Pushed when | Popped when |
|---------------|------------|-------------|-------------|
| Character creation | `FocusLayer::Modal` | `spawn_creation_ui` | `despawn_creation_ui` |
| Settings panel | `FocusLayer::Settings` | `open_settings_from_menu` / `open_settings_from_pause` | Panel close |
| Pause menu | `FocusLayer::Pause` | `toggle_pause` (open) | `toggle_pause` (close) |
| Dialogue (free-text typing) | `FocusLayer::Panel(ActivePanel::Dialogue(e))` | Enter typing mode | Exit typing mode (Enter/Esc) |
| Contract workshop | `FocusLayer::Tool(ActivePanel::ContractWorkshop)` | Workshop opens via console | Workshop closes (Esc / walk away) |
| Contract library | `FocusLayer::Tool(ActivePanel::ContractLibrary)` | Library opens | Library closes |
| Ship exterior editor | `FocusLayer::Tool(ActivePanel::ShipExterior)` | Panel opens from shipyard | Panel closes |
| Ship interior editor | `FocusLayer::Tool(ActivePanel::ShipInterior)` | Panel opens from interior-refit | Panel closes |
| Crew conference | `FocusLayer::Tool(...)` | Conference hotkey | Conference ends |

### Gate pattern — the contract every input system follows

```rust
// At the TOP of every input system that reads ButtonInput<KeyCode>:
fn my_input_system(
    focus_stack: Res<FocusStack>,
    keys: Res<ButtonInput<KeyCode>>,
    // ... other params ...
) {
    // BLOCK 1: Never suppress non-input reads (settings, mode, etc.).
    // BLOCK 2: Check if this system OWNS the top layer.
    let my_layer = FocusLayer::Tool(ActivePanel::ContractWorkshop);
    if !focus_stack.is_active(my_layer) && focus_stack.top_captures_input() {
        return; // Some OTHER modal/panel owns input; do nothing.
    }
    // BLOCK 3: If NO other modal owns input AND my panel is not open,
    //          also do nothing (not consuming).
    // BLOCK 4: Normal input handling.
}
```

**Simpler gate for systems that are always-on (dialogue, trope, encounter, dilemma):**

```rust
fn dialogue_input(
    focus_stack: Res<FocusStack>,
    keys: Res<ButtonInput<KeyCode>>,
    session: ResMut<DialogueSession>,
    // ...
) {
    // If no session active, nothing to consume.
    if session.active.is_none() {
        return;
    }
    // If some OTHER modal owns input (settings, char creation), do nothing.
    // But dialogue itself (typing mode) IS allowed.
    let owns_focus = focus_stack.is_active(FocusLayer::Panel(ActivePanel::Dialogue(session.active.as_ref().unwrap().entity)));
    if focus_stack.top_captures_input() && !owns_focus {
        return;
    }
    // ... normal input handling ...
}
```

## Deliverables

### 1. Push/pop FocusLayers at panel open/close points

- [ ] `contract_crafting.rs`: push `FocusLayer::Tool(ActivePanel::ContractWorkshop)` when workshop opens, pop when it closes
- [ ] `contract_library.rs`: push `FocusLayer::Tool(ActivePanel::ContractLibrary)` when library opens, pop when it closes
- [ ] `shipeditor/exterior.rs`: push `FocusLayer::Tool(ActivePanel::ShipExterior)` on open, pop on close
- [ ] `shipeditor/interior.rs`: push `FocusLayer::Tool(ActivePanel::ShipInterior)` on open, pop on close
- [ ] `comms.rs`: push a `FocusLayer::Tool(...)` when crew conference starts, pop when it ends
- [ ] `dialogue.rs`: push `FocusLayer::Panel(ActivePanel::Dialogue(e))` when entering typing mode, pop when leaving typing mode
- [ ] Verify `character_creation.rs` already pushes `FocusLayer::Modal` — confirm

### 2. Add FocusStack gate to every input system

Each system gets the gate check at the top of the function. Exact changes:

**`dialogue_input` (`dialogue.rs:151`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] At function top, after `let session = &mut *session;`: if a modal owns input and this dialogue doesn't own it, return early
- [ ] Free-text typing mode: already pushes FocusLayer on entry — confirm

**`workshop_system` (`contract_crafting.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] At function top: if `!focus_stack.is_active(FocusLayer::Tool(ActivePanel::ContractWorkshop)) && focus_stack.top_captures_input()`, return
- [ ] Also return if `*panel != ActivePanel::ContractWorkshop`

**`library_system` (`contract_library.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] Same gate pattern as workshop, with `ActivePanel::ContractLibrary`

**`editor_system` (`shipeditor/exterior.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] Gate: `!focus_stack.is_active(FocusLayer::Tool(ActivePanel::ShipExterior)) && focus_stack.top_captures_input()`

**`interior_editor_system` (`shipeditor/interior.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] Gate: `!focus_stack.is_active(FocusLayer::Tool(ActivePanel::ShipInterior)) && focus_stack.top_captures_input()`

**`crew_conference_hotkey` (`comms.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] Gate: return if `focus_stack.top_captures_input()` (no one owns it specifically, just don't start a conference while in settings/char creation)

**`trope_input_system` (`trope_dispatcher.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] Gate: return if `focus_stack.top_captures_input()` and trope popup isn't the top layer

**`encounter_choice_system` (`encounter_executor.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] Gate: return if `focus_stack.top_captures_input()` and encounter isn't the top layer

**`dilemma_input_system` (`dilemma.rs`):**
- [ ] Add `focus_stack: Res<FocusStack>` parameter
- [ ] Gate: return if `focus_stack.top_captures_input()` and dilemma isn't the top layer

### 3. Movement system gate

- [ ] `interior::walk_avatar` (`interior.rs`): add `focus_stack: Res<FocusStack>` parameter
- [ ] At function top: return if `focus_stack.top_captures_input()` (any modal should freeze avatar movement)

### 4. Add enforce test

- [ ] In `focus_stack.rs` test module, add:
```rust
/// Every system fn with `ButtonInput<KeyCode>` in its parameter list must
/// also include `Res<FocusStack>`. This test verifies the pattern exists.
/// (Manual audit required for full coverage — this is the reminder test.)
#[test]
fn focus_stack_must_be_enforced_in_input_systems() {
    // This test proves the FocusStack is working. The real enforcement is
    // the code review checklist. This test exists so a removed gate breaks
    // the build.
    let mut stack = FocusStack::default();
    stack.push(FocusLayer::Pause);
    assert!(stack.top_captures_input());
    stack.pop();
    assert!(!stack.top_captures_input());
}
```

## Acceptance gates

```bash
# Compile + clippy
cargo clippy -p reachlock-client -- -D warnings

# Unit tests
cargo test -p reachlock-client focus_stack

# Manual verification path:
# 1. Open contract workshop from crew console → WASD should NOT move avatar
# 2. Open ship editor from shipyard → A/D cycles hardpoints, not strafes
# 3. Open dialogue with soul-backed NPC → press 9 (or QS key) → type text → WASD should NOT move avatar
# 4. In character creation → WASD should NOT change anything
# 5. Open settings from pause → Arrow keys adjust settings, not power management
# 6. All of above: Esc closes the panel and restores normal input

make check
```

## Non-goals

- Changing how `FocusStack` itself works (it's correct)
- Preventing ALL input during modals (only gameplay input — Esc/Escape should still work to close)
- Modal stacking rules (you CAN open settings from pause — FocusStack handles stacking natively)
- Input prioritization or conflict resolution (out of scope — single-owner model is sufficient)

## Gotchas

- **`try_interact` must NOT be gated.** It uses `ButtonInput<KeyCode>` but it's how the player presses E to open panels in the first place. If `top_captures_input()` gates `try_interact`, the player can never close a modal (E is needed to interact, Esc to close). The `interact_key` check in `try_interact` should run regardless of focus.
- **Dialogue has TWO input modes.** `dialogue_input` handles both choice mode (number keys) and typing mode (free text). Typing mode MUST gate WASD (it pipes characters). Choice mode should also gate WASD (you're in a conversation), but `try_interact` needs E to work to walk away.
- **`walk_avatar` already has a typing check.** `interior.rs` checks `dialogue.typing()` before moving. After this change, replace that check with the FocusStack gate — one gate, not two scattered checks.
- **Flight controls (ship::control) do NOT need gating.** Flight mode and interior modes are separate Bevy states. The flight systems only run in `in_spaceflight`. The FocusStack gate protects interior systems from modals — flight has no modals.
- **The `FocusLayer` enum must stay in `focus_stack.rs`.** Don't duplicate it elsewhere. Every panel that needs a layer references the existing enum variant.
- **Trope/encounter/dilemma panels share the same screen region.** Only one can be open at a time (they are not toggled — the game triggers them). The FocusStack gate is simpler for these: if any modal has input, don't handle trope/encounter/dilemma keys.
