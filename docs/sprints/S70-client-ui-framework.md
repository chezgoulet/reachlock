# S70 — Client UI Framework

**Spec:** New (widget kit, focus stack, panel z-order, bevy_ui + bevy_egui split) ·
**Wave C · Depends on:** S31 (settings UI), S34 (contract workshop/library — egui surface)

## Outcome

The entire client UI layer — HUD, menus, interaction panels, settings, market,
and tool surfaces — migrates from raw `Text` entities to real bevy_ui widgets
with interaction, focus rings, tooltips, and gamepad navigation. A `FocusStack`
resource replaces the `if ui.open { return }` convention. Six independent
`*Visible` booleans collapse into the `ActivePanel` z-stack with mutual
exclusion. The split framework is applied: bevy_ui + bevy_feathers for the
game HUD, menus, and diegetic panels; bevy_egui (created/destroyed per panel
open/close) for authoring and dense-data surfaces (contract workshop, contract
library, ship editor, rule builder). The shell creates two stacks gated by
`ActivePanel` — when a tool panel is open, egui owns input; otherwise, bevy_ui
owns the HUD and menus.

**Closes:** C1, C4(part), C10, C11, C13, C15

## Context

- **No UI layer exists (C1).** The 23k LOC client has zero `Interaction`
  components, zero `Button` bundles, zero `ImageNode` usage — all UI is
  rendered as `Text` entities with keyboard-driven state machines inside
  monolithic system functions. There are no reusable widget primitives.
- **Main menu is keyboard-only text (C4).** No Quit button, no click support,
  seed is displayed but not editable or copyable.
- **`settings_ui::row_count()` is hardcoded per tab (C10).** Adding a setting
  drifts the row count and breaks Tab/Arrow-up navigation silently.
- **Focus is a convention, not a stack (C11).** Every panel guards with
  `if ui.open { return }`. Two panels can be "open" simultaneously; they leak
  input to each other. The pause menu and settings panel fight for Esc.
- **Six independent `*Visible` booleans (C13).** `ReputationPanelVisible`,
  `CulturePanelVisible`, `DiscoveryPanelVisible`, `CareerPanelVisible`,
  `MissionBoardVisible`, `SignatureCollectorVisible` — no z-order, no mutual
  exclusion. Closing one: another was accidentally opened.
- **Market/ship editors share one text entity (C15).** Three panels multiplex
  onto `hud.rs:391-417`'s single `&mut Text` query, which works only because
  `ActivePanel` is mutually exclusive at the enum level — but the rendering
  surface is a single text string, not composable UI nodes.
- **`ActivePanel` already exists** (`interaction.rs:103`) but it only gates
  panel rendering — it does not manage focus, z-ordering, or input routing.
  Its 15+ variants include `ContractWorkshop` and `ContractLibrary` which need
  egui; the rest are bevy_ui-native.
- **bevy_ui_widgets, bevy_feathers, bevy_input_focus, bevy_a11y, bevy_picking**
  are transitive deps of bevy 0.18. The widget kit is built on these, not added
  as new dependencies — no new Cargo.toml entries needed.
- **egui is for tools only.** bevy_egui is not initialized at startup. When
  `ActivePanel` is `ContractWorkshop`, `ContractLibrary`, `ShipExterior`, or
  `ShipInterior`, the shell spawns an egui `EguiContext` within a bevy_ui node
  and routes all input to it. On panel close, the egui context is destroyed.
  This means egui panels have zero overhead when closed.
- **Offline-first:** every widget and panel works identically with no server.
  Online adds synced contract libraries and shared workshop sessions, but the
  UI itself is local.

## Freeze first

### `FocusStack` resource

A LIFO stack of focus layers. Only the top layer receives keyboard/gamepad
input. Opening a panel pushes; closing it pops. Replaces `if ui.open { return }`
throughout the codebase.

```rust
#[derive(Resource, Default)]
pub struct FocusStack {
    layers: Vec<FocusLayer>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FocusLayer {
    /// The world and HUD (default, always present).
    World,
    /// An interaction panel (market, dialogue, helm, nav, …).
    Panel(ActivePanel),
    /// The pause menu overlay.
    Pause,
    /// The settings UI (opened from main menu or pause menu).
    Settings,
    /// An egui tool panel (contract workshop, library, ship editor).
    Tool(ActivePanel),
    /// Modal dialog (confirmation, key rebind capture, text edit).
    Modal,
}

impl FocusStack {
    pub fn push(&mut self, layer: FocusLayer) { … }
    pub fn pop(&mut self) -> Option<FocusLayer> { … }
    pub fn top(&self) -> &FocusLayer { … }
    pub fn pop_until(&mut self, target: FocusLayer) { … }
    /// True if the given layer is the top (can receive input).
    pub fn is_active(&self, layer: FocusLayer) -> bool { … }
}
```

All `if ui.open { return }` guards become `if !focus.is_active(FocusLayer::X) { return }`.
`FocusStack` is the single source of truth for "who owns the keyboard."

### `ActivePanel` z-order constants

Which panels close when another opens. Defined in a `const` table, not scattered
across systems:

```rust
/// The six *Visible booleans (C13) are gone. Panels are mutually exclusive by
/// category. `open` closes any sibling in the same category; `open` across
/// categories is allowed (e.g., HUD stays visible while a panel is open).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelCategory {
    /// HUD overlays: never close each other.
    Hud,
    /// Interaction consoles (helm, engineering, nav, log, fuel, gunner, …).
    /// Opening one closes any other console.
    Console,
    /// Information panels (reputation, culture, discovery, career, mission board).
    /// Opening one closes any other info panel.
    InfoPanel,
    /// Shops and trade (market).
    Shop,
    /// Editors (ship exterior, interior).
    Editor,
    /// Authoring tools (contract workshop, contract library).
    Tool,
    /// System panels (settings, pause).
    System,
}

impl ActivePanel {
    pub fn category(&self) -> PanelCategory {
        match self {
            ActivePanel::Market => PanelCategory::Shop,
            ActivePanel::Helm | ActivePanel::Engineering | … => PanelCategory::Console,
            ActivePanel::ShipExterior | ActivePanel::ShipInterior => PanelCategory::Editor,
            ActivePanel::ContractWorkshop | ActivePanel::ContractLibrary => PanelCategory::Tool,
            ActivePanel::Dialogue(_) | ActivePanel::Order(_) => PanelCategory::Console,
            ActivePanel::None => PanelCategory::Hud,
            _ => PanelCategory::InfoPanel,
        }
    }

    /// Which panels should be closed when this one opens. Based on category:
    /// within-category closes siblings; cross-category leaves them open.
    pub fn closes(&self) -> &[ActivePanel] { … }
}
```

### Widget kit trait/type definitions

The public API surface of the widget kit. Implementation follows in
deliverable 1.

```rust
// reachlock-client/src/widget_kit/mod.rs
pub mod button;
pub mod toggle;
pub mod slider;
pub mod dropdown;
pub mod list;
pub mod scroll_area;
pub mod text_input;
pub mod tooltip;

/// Theme colours read from bevy_feathers `StyleHandle` / theme token.
pub struct WidgetTheme;

/// Spawn a styled bevy_ui button entity. Returns the entity id.
pub fn button(
    commands: &mut Commands,
    label: &str,
    on_activate: impl Fn(&mut Commands) + Send + 'static,
) -> Entity { … }

/// Spawn a toggle (on/off switch). Returns entity + a toggle handle.
pub fn toggle(
    commands: &mut Commands,
    label: &str,
    value: bool,
    on_change: impl Fn(bool, &mut Commands) + Send + 'static,
) -> (Entity, ToggleHandle) { … }

/// Spawn a labeled slider. Returns entity + handle for reading current value.
pub fn slider(
    commands: &mut Commands,
    label: &str,
    min: f32, max: f32,
    value: f32,
    on_change: impl Fn(f32, &mut Commands) + Send + 'static,
) -> (Entity, SliderHandle) { … }

pub struct ToggleHandle(Entity);
pub struct SliderHandle(Entity);
```

## Deliverables

### 1. Widget kit — reusable bevy_ui primitives

Build a `widget_kit` module under `reachlock-client/src/` with these widgets.
Each widget is a pure function that spawns a bevy_ui entity subtree, wires
`Interaction` components for hover/press/release, and emits a callback or
event on activation. Styling comes from bevy_feathers `StyleHandle` lookups
(tokens, not hardcoded `Val::Px` literals).

- [ ] **Button**: `Node` + `Button` + `Text` + `Interaction`. Background colour
      changes on hover (lighten), press (darken), and disabled (grey). Accepts
      `on_activate: impl Fn(&mut Commands)`.
- [ ] **Toggle**: Two-state switch. Visual: filled/empty track + thumb that
      slides. Keyboard: Enter to flip, or A/D. Returns `ToggleHandle` for
      reading current value. Accepts `on_change: impl Fn(bool, &mut Commands)`.
- [ ] **Dropdown**: Select from N options. Renders as a button showing current
      selection; on activate, spawns a temporary overlay list. A/D or ↑/↓ to
      cycle; Enter to confirm; Esc to cancel.
- [ ] **Scrollable List**: Vertical list with scrollbar. Items are dynamically
      spawned children. Scrollbar thumb tracks content offset. Mouse wheel and
      keyboard (↑/↓/PageUp/PageDown) scroll. The list queries its children
      heights and clips overflow via `Overflow::clip()`.
- [ ] **TextInput**: Single-line text field. On focus, captures typed characters.
      Backspace deletes, Enter commits, Esc cancels. Cursor shown as a blinking
      bar (`|`). Accepts `on_commit: impl Fn(String, &mut Commands)`.
- [ ] **Slider**: Horizontal bar with a draggable thumb. A/D or ←/→ adjust by
      step (default 1/20 of range). Mouse drag also supported. Visual: track
      + filled portion + thumb. Returns `SliderHandle`. Accepts
      `on_change: impl Fn(f32, &mut Commands)`.
- [ ] Widget tests: each widget spawns correctly, responds to `Interaction`
      events, and the callback fires with the correct value. Test in a headless
      Bevy app with `bevy::ecs::system::RunSystemOnce`.

### 2. Focus stack — `FocusStack` resource

- [ ] `FocusStack` resource with `push`/`pop`/`top`/`is_active`/`pop_until`.
- [ ] Initialised at startup with `FocusLayer::World` on the stack.
- [ ] Migrate all `if ui.open { return }` guards in `menu.rs`, `pause.rs`,
      `settings_ui.rs`, `onboard.rs`, `dialogue.rs`, `shipeditor/*.rs`,
      `contract_crafting.rs`, `contract_library.rs` to
      `focus_stack.is_active(FocusLayer::X)` checks.
- [ ] Opening any panel -> `focus_stack.push(FocusLayer::Panel(…))`.
      Closing -> `focus_stack.pop()`.
- [ ] `pop_until(FocusLayer::World)` on Esc to close everything and return to
      the game HUD (the new universal Esc handler replaces the scattered
      panel-by-panel Esc logic).
- [ ] Test: push two layers, verify `is_active` returns true only for the top;
      pop, verify the previous layer becomes active; `pop_until` removes
      everything above the target.

### 3. Panel z-order + mutual exclusion

- [ ] Remove all six `*Visible` boolean resources from `main.rs`:
      `ReputationPanelVisible`, `CulturePanelVisible`, `DiscoveryPanelVisible`,
      `CareerPanelVisible`, `MissionBoardVisible`, `SignatureCollectorVisible`.
      Their functionality (toggle open/close, render) moves into the
      `ActivePanel` enum + category-based mutual exclusion.
- [ ] `ActivePanel` gains `category()` and `closes()` as defined in Freeze first.
      When `ActivePanel` changes to a new variant, `closes()` returns a list of
      panels to close — implemented as `OnSet<ActivePanel>` observer or an
      `active_panel_changed` system that clears conflicting panels.
- [ ] Remove the shared-text-entity pattern (`hud.rs:391-417`). Each panel that
      was sharing `MarketPanel`/`DialoguePanel`/etc. now spawns its own bevy_ui
      entity subtree when opened and despawns when closed. The HUD runs
      `spawn_*_panel` / `despawn_*_panel` functions driven by `ActivePanel`
      changes.
- [ ] Add a `ZStack` component that sets `Node::z_index` based on
      `PanelCategory`. Consoles render above the HUD. Info panels render above
      consoles. Tools render above everything. This ensures proper visual
      ordering regardless of spawn order.

### 4. Main menu port — bevy_ui buttons

- [ ] Replace `menu.rs`'s `Text::new(menu_text(…))` with real bevy_ui widgets.
      The menu is a centred `Node` column with:
      - Title text "REACHLOCK" (styled, large font)
      - Editable system seed display (TextInput widget, pre-filled with
        `SYSTEM_SEED` as hex, copyable via Ctrl+C)
      - **New Game** button (pops seed editor for naming the save)
      - **Continue** button (greyed out if no save exists)
      - **Settings** button (opens settings panel via FocusStack)
      - **Quit** button (exits the application — new, C4 closure)
      - Button style: hover highlight, press feedback, focus ring.
- [ ] Keyboard nav: ↑/↓ cycle buttons, Enter activates, Tab skips between
      seed field and buttons. Gamepad: D-pad up/down, A to confirm.
- [ ] Seed editor: TextInput with hex-only validation. Changing the seed
      updates the preview text and the `SYSTEM_SEED`-equivalent resource.
- [ ] **Continue** button: queries for a save file. If none exists, the button
      is disabled (dimmed, no focus target).
- [ ] **Quit** button: emits `AppExit` event.
- [ ] Click support: buttons respond to mouse click via `Interaction`.

### 5. Settings UI port — real widgets

- [ ] Replace `settings_ui.rs`'s monolithic `render()` function that builds a
      single `String` with real widget instances per row. Each tab's rows are
      spawned as a bevy_ui entity subtree on tab switch.
- [ ] Tab strip: clickable tabs + Tab/Shift+Tab keyboard cycling.
- [ ] Audio tab: `Slider` for master/music/sfx/voice volume, `Toggle` for mute
      when unfocused. Volume sliders trigger the same preview tone on change.
- [ ] Video tab: `Dropdown` for resolution presets, `Toggle` for fullscreen/vsync/
      show fps, `Slider` for render scale / UI scale.
- [ ] Controls tab: `Slider` for mouse sensitivity / controller deadzone,
      `Toggle` for invert Y. Keybind rows use a custom keybind widget: shows
      current key name, on activate enters capture mode (FocusStack pushes
      `Modal`), displays "Press a key…". Conflict detection preserves existing
      S31 behaviour.
- [ ] Gameplay tab: `Toggle` for aim assist / auto dock / tutorial hints,
      `Slider` for combat log verbosity, autosave interval.
- [ ] Accessibility tab: `Dropdown` for colorblind mode (None/Protanopia/
      Deuteranopia/Tritanopia), `Slider` for text scale / screen shake /
      subtitle size, `Toggle` for high contrast / subtitles / hold to interact.
- [ ] Network tab: `TextInput` for server URL, `Toggle` for auto-connect / show
      latency.
- [ ] Apply & Reset buttons at the bottom. Apply writes to `Settings` resource
      + disk. Reset per-tab restores defaults for that tab. Reset All resets
      everything. Confirmation modal (FocusStack pushes `Modal`) before reset.
- [ ] `C10 fix`: `row_count()` is gone — the widget kit's dynamic list
      tracks its own children. Adding a setting adds a widget to the tab's
      spawn function; the list automatically knows its length.

### 6. Market panel port — scrollable buy/sell grid

- [ ] Replace `market_panel_text()` with a bevy_ui market panel. Spawned when
      `ActivePanel::Market` becomes active; despawned when market closes.
- [ ] Header row: credits + cargo capacity count.
- [ ] Scrollable list of goods. Each row shows: good name, buy price, sell price,
      held quantity. The selected row has a highlight background.
- [ ] Quantity selector at the bottom (TextInput or stepper buttons +/-).
- [ ] Buy / Sell buttons (real buttons, not B/N keys — keyboard shortcuts still
      work as an alternative).
- [ ] Keyboard: ↑/↓ navigate rows, ←/→ adjust quantity, Enter buy, Shift+Enter
      sell, Esc close.
- [ ] Click: click a row to select, click Buy/Sell buttons to trade.

### 7. HUD port — styled bevy_ui nodes

- [ ] Replace `spawn_hud`'s raw `Text` entities with bevy_ui `Node` containers
      with background/border styling.
- [ ] Fuel readout: a coloured bar + percentage text. Bar fills to fuel level.
- [ ] Speed indicator: numeric readout + thrust indicator (▲ icon).
- [ ] Hull integrity: a horizontal bar (green → yellow → red gradient).
- [ ] Location banner: styled panel top-centre with system/station name.
- [ ] Ship log: scrollable panel bottom-left, newest entries at bottom.
- [ ] Deliberation overlay: styled card with crew member name + context summary
      + ellipsis animation.
- [ ] Help text: styled line top-left, rebuilt on settings change (preserves
      `HelpTextCache` pattern).
- [ ] Interaction prompt: styled "[E] Mara" bottom-centre with key icon.
- [ ] Offline badge: red badge top-right, shown only in online mode when
      disconnected (preserves existing logic).
- [ ] Dialogue panel: styled card with NPC portrait placeholder + text.
- [ ] Pause overlay: centred panel with Resume/Settings/Quit buttons.

### 8. egui tool panels — conditional context

- [ ] bevy_egui is NOT added to `DefaultPlugins`. Instead, the shell creates an
      `EguiContext` entity when `ActivePanel` is `ContractWorkshop`,
      `ContractLibrary`, `ShipExterior`, or `ShipInterior`. On panel close, the
      entity is despawned and egui context destroyed.
- [ ] A new `egui_bridge` module handles context lifecycle:
      ```rust
      pub fn sync_egui_context(
          mut commands: Commands,
          panel: Res<ActivePanel>,
          query: Query<Entity, With<EguiManaged>>,
      ) {
          let needs_egui = matches!(*panel,
              ActivePanel::ContractWorkshop | ActivePanel::ContractLibrary |
              ActivePanel::ShipExterior | ActivePanel::ShipInterior
          );
          if needs_egui && query.is_empty() { spawn_egui(&mut commands); }
          if !needs_egui && !query.is_empty() { despawn_egui(&mut commands, &query); }
      }
      ```
- [ ] bevy_egui is a non-optional dependency in `Cargo.toml` — it is always
      compiled, but zero cost when no egui context exists (the `EguiPlugin` is
      not added; the `EguiContext` is spawned manually).
- [ ] Each tool panel's system checks `FocusStack::is_active(FocusLayer::Tool(…))`
      to gate input. Inside the egui frame, the panel uses
      `egui::CentralPanel` / `egui::SidePanel` as before, but now the
      surrounding bevy_ui node clips and positions the egui render target.
- [ ] Wire the systems in `main.rs`:
      ```
      .add_systems(Update, egui_bridge::sync_egui_context.run_if(in_state(AppState::InGame)))
      ```
- [ ] Existing egui panels (contract_crafting, contract_library, shipeditor)
      remain largely unchanged — they gain a `FocusStack::is_active` gate and
      their systems move behind the egui bridge lifecycle.

### 9. Gamepad + mouse + keyboard — universal input

- [ ] All bevy_ui widgets accept gamepad input. `bevy_input_focus` provides a
      focus ring: D-pad up/down moves focus, A button confirms, B button
      cancels. The `FocusStack` resource's top layer determines which widget
      tree receives gamepad events.
- [ ] `bevy_a11y` is wired for screen-reader support: widgets set
      `AccessibilityNode` with appropriate labels and roles.
- [ ] `bevy_picking` is wired so all buttons respond to pointer clicks.
      `PickingBackend` is set to the default (pointer-emulation). The market
      grid supports click-to-select and click-buy.
- [ ] Gamepad guide button opens the pause menu (same as Esc).
- [ ] Test: run the client with `--gamepad` (a `VirtualGamepad`-based test
      harness in the test suite) and verify that every menu and panel is
      navigable without a keyboard.

### 10. Tooltips — hover/focus overlay

- [ ] A `Tooltip` component + `TooltipTarget` component pair.
      `Tooltip` carries the text; `TooltipTarget` marks a widget as having a
      tooltip. A `tooltip_system` runs after HUD rendering, checks for hovered
      or focused widgets with `TooltipTarget`, and spawns a floating
      `Node` + `Text` overlay near the widget.
- [ ] Tooltip styling: small font, subtle background, slight transparency,
      auto-positioned to avoid screen edges.
- [ ] Tooltip delay: 500 ms hover before showing; instant on focus (for
      keyboard/gamepad users).
- [ ] Tooltip dismiss: move cursor away, lose focus, or press any key.
- [ ] Apply `TooltipTarget` to all main menu buttons, settings rows with
      abbreviated labels, market goods, and HUD elements that have more
      state than their icon/brief text shows.
- [ ] Test: spawn a button with `TooltipTarget`, simulate hover for 600 ms,
      assert tooltip entity exists; simulate cursor move, assert tooltip
      despawned.

## Acceptance gates

```
cargo test -p reachlock-client widget_kit::   # widget tests pass
cargo test -p reachlock_client focus_stack::  # FocusStack unit tests
cargo test -p reachlock_client tooltip::      # tooltip lifecycle test
cargo run -p reachlock-client                 # main menu shows buttons, seed editable, Quit present
# Click "New Game" → character creation flow (if S78 is in, else continue)
# Click "Settings" → settings panel renders with real widgets
# Open market (interact with shop NPC) → scrollable list, buy/sell buttons work
# Open pause menu → Resume/Settings/Quit buttons
# Open contract workshop → egui context appears; close → egui context gone
# Gamepad: navigate main menu with D-pad, launch game
# No six *Visible booleans remain in main.rs
make check
```

Manual: launch game → main menu shows styled buttons → change seed → see hex
edit → click Settings → tab through Audio/Video/Controls → adjust volume slider
→ hear preview tone → apply → close → pause → Esc → Quit. Relaunch → Continue
button present (save exists).

## Non-goals

- VDOM or immediate-mode UI for game UI (bevy_ui is retained-mode; this is
  deliberate — tool panels use egui for dense-data surfaces, but game UI stays
  in bevy_ui)
- Full controller rebinding (the `ControllerSettings` fields exist in S31's
  `ControlSettings` but full gamepad mapping is separate)
- Theme/skin system (colours come from bevy_feathers tokens; swapping themes
  is a separate sprint)
- Localization/locale (widgets render static strings; text extraction is a
  separate infrastructure sprint)
- Animation system for UI (transitions, slide-in panels — future work;
  this sprint is static placement)
- HUD damage/threat feedback hierarchy (C8 — S71 adds semantic palette and
  the feedback layer)
- Tutorial system (C5 — S72 adds onboarding and contextual hints)
- Deliberation theater / the signature moment (C6/C7 — S72)

## Gotchas

- `bevy_egui` and `bevy_ui` share the same camera. The egui render layer is
  painted on top of the bevy_ui layer. When both are visible (egui panel + HUD
  tooltip) the tooltip must render above the egui surface. Set the egui
  `EguiContext` z-index explicitly via the bevy_ui node that contains it, so
  HUD overlays can render higher.
- bevy_feathers `StyleHandle::label("button")` may not yet have registered
  tokens at startup if the theme asset hasn't loaded. Use a fallback style in
  widget constructors: `style.unwrap_or(default_button_style())`. The first
  frame of the main menu may flash default styling before the theme loads.
- The `FocusStack` replaces `if ui.open { return }` — every system that
  currently uses the pattern must be audited and migrated. Missed guards mean
  overlapping input. After the migration, run the client and try: open market,
  press Esc. If the menu closes but the market stays (or vice versa), a guard
  was missed. The universal Esc handler (`pop_until(World)`) prevents
  most of this class, but individual panel close handling (e.g., dialogue
  confirming "are you sure?") still needs explicit per-panel logic.
- `bevy_input_focus` drives focus via `bevy_ui::Focus` — but bevy_ui's
  built-in focus system is nascent. If `bevy_input_focus` does not compile
  with bevy 0.18, implement a minimal focus ring system inline: track a
  `FocusRing(Entity)` resource, render a `FocusHighlight` component on the
  focused entity, move focus on Tab/D-pad by querying widgets with
  `Focusable` marker and `NextFocus`/`PreviousFocus` links.
- C15 fix (shared text entity → per-panel spawned subtrees) means each panel
  now manages its own entity lifecycle. Systems that previously wrote text
  via `&mut Text` query must now update widgets differently: e.g., the market
  system updates the selected-row highlight by toggling a style component
  rather than rewriting a string. The widget kit's handle pattern
  (`ToggleHandle`, `SliderHandle`) gives each panel a way to set widget state
  without re-spawning.
- The six `*Visible` booleans (C13) being removed means every system that
  wrote to them (`reputation_panel_toggle`, `culture_panel_toggle`, etc.)
  must be rewritten to push/pop the `ActivePanel` + `FocusStack` instead.
  These toggle functions are currently bound to the same key (C14 — fixed in
  S64), so after S64 each already has its own unique keybind. The migration
  is: `visible.0 = !visible.0` → `if active_panel == X { active_panel = None }
  else { active_panel = X }`.
- egui context lifecycle: `EguiContext::new()` requires `&mut Commands` and
  the `EguiPlugin` must be registered at `App` build time. If the egui plugin
  can't be added lazily, register it at startup but keep the context entity
  despawned — the egui systems early-out when no context exists. Measure: a
  no-op egui frame costs ~0.1 ms on the render thread. Acceptable for a tool
  that is invisible during gameplay. If the cost is too high, gate egui
  `add_systems` behind a `RunCondition` that returns `false` when no tool
  panel is open, so the egui system functions never run.
- `bevy_picking` may conflict with bevy_ui's built-in click handling.
  If both process the same pointer event, buttons fire twice. Set
  `PickingPluginsSettings { is_ui: true }` so bevy_picking respects
  bevy_ui's pointer-capture region and doesn't double-fire.
- WASM build: the egui context lifecycle must compile for wasm32 even though
  egui panels are native-only. The contract workshop and ship editor are
  excluded from WASM (the existing `make check` WASM build excludes
  bevy_egui-dependent crates). The egui bridge's `sync_egui_context` may need
  `#[cfg(not(target_arch = "wasm32"))]` blocks or use `cfg!` for the egui
  import. Keep the import behind a feature flag if the compiler can't
  dead-code-eliminate the egui module.
