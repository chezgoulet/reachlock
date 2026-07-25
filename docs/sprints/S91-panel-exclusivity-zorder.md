# S91 — Panel Mutual Exclusion & Z-Ordering

**Wave: UX-Hardening · Depends on:** S90 (FocusStack input gating)

## Outcome

No two overlapping panels can be open simultaneously. Opening a panel that shares screen space with another panel auto-closes the sibling. Panels have z-order from a single source of truth, not from spawn order.

## Context

Twelve independently-toggled text panels render at overlapping positions:

| Panel | Top | Left | Shares region with |
|-------|-----|------|-------------------|
| Career | 120px | 8px | Factions, Discovery, Log, Mission Board, Dialogue, Dilemma, Encounter, Trope |
| Factions | 120px | 8px | Same |
| Discovery | 120px | 8px | Same |
| Captain's Log | 120px | 8px | Same |
| Mission Board | 120px | 8px | Same |
| Dialogue | 120px | 8px | Same |
| Dilemma | 100px | 8px | Same (slightly offset) |
| Encounter | 100px | 8px | Same |
| Trope | 100px | 8px | Same |
| Market | 120px | 360px | Right-side panels |
| Contract Workshop | egui | centered | Owns whole screen |
| Contract Library | egui | centered | Owns whole screen |

Each panel uses a `*Visible: bool` resource and a `Visibility` component toggle. Nothing prevents opening the career panel while factions is open — they stack, obscure each other, and both consume input.

The fix has two parts:
1. **Mutual exclusion groups** — panels in the same group auto-close siblings.
2. **Z-order table** — a single place defines draw order, replacing implicit spawn-order z-index.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/states.rs` | New — `PanelGroup` enum, z-order table |
| `reachlock-client/src/systems/factions.rs` | `ReputationPanelVisible` → add group close |
| `reachlock-client/src/systems/discovery.rs` | `DiscoveryPanelVisible` → add group close |
| `reachlock-client/src/systems/career.rs` | `CareerPanelVisible` → add group close |
| `reachlock-client/src/systems/log_ui.rs` | `LogViewerVisible` → add group close |
| `reachlock-client/src/systems/mission_board.rs` | Mission board toggle → consistency |
| `reachlock-client/src/systems/hud.rs` | Dialogue/Dilemma/Encounter/Trope panels |
| `reachlock-client/src/systems/market.rs` | `MarketState` consistency |
| `reachlock-client/src/main.rs` | Z-order application on spawn |

## Freeze first

### PanelGroup enum

```rust
/// Panel groups define which panels share screen space.
/// Opening any panel in a group auto-closes all other panels in that group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelGroup {
    /// Left-side overlay panels (120px top, 8px left)
    InfoPanel,
    /// Narrative popups (trope, dilemma, encounter) — 100px top, 8px left
    Narrative,
    /// Right-side market/editor panels (120px top, 360px left)
    Trade,
    /// Full-screen egui panels (contract workshop, library)
    Workshop,
    /// No group — this panel lives alone (settings modal, pause overlay)
    None,
}

/// Mapping: every toggleable panel → its PanelGroup
/// New panels register here.
pub fn panel_group_for(panel: &ActivePanel) -> PanelGroup {
    match panel {
        ActivePanel::None => PanelGroup::None,
        ActivePanel::Dialogue(_) => PanelGroup::InfoPanel,
        ActivePanel::Market => PanelGroup::Trade,
        ActivePanel::Dilemma => PanelGroup::Narrative,
        ActivePanel::Encounter => PanelGroup::Narrative,
        ActivePanel::TropePopup => PanelGroup::Narrative,
        // Workshop/library are not ActivePanel variants — they use sibling active-flags
        _ => PanelGroup::None,
    }
}
```

### Group-close function signature

```rust
/// Close every panel in `group` except `keep`.
/// Call this BEFORE opening a new panel.
pub fn close_panels_in_group(
    group: PanelGroup,
    keep: ActivePanel,
    panels: &mut ActivePanel,
    // Visibility flags for non-ActivePanel panels:
    career_visible: &mut bool,
    factions_visible: &mut bool,
    discovery_visible: &mut bool,
    log_visible: &mut bool,
    mission_visible: &mut bool,
    market_active: &mut bool,
) { ... }
```

### Z-order table

```rust
/// Closest to the player = highest number.
/// Render order: spawn highest first, lowest last (overdraw).
pub fn z_order_for(target: &str) -> i32 {
    match target {
        "background" => 0,
        "starfield" => 1,
        "space_scene" => 2,
        "ship_hud_footer" => 3,      // bottom help bar
        "ship_hud_top_left" => 4,    // fuel/speed/threats
        "ship_hud_top_right" => 5,   // FPS/latency/offline
        "ship_hud_top_center" => 6,  // location banner
        "panel_info" => 7,           // Career / Factions / Discovery / Log
        "panel_narrative" => 8,      // Trope / Dilemma / Encounter
        "panel_trade" => 9,          // Market
        "deliberation_overlay" => 10,
        "dialogue_panel" => 11,
        "settings_modal" => 12,
        "pause_modal" => 13,
        "onboarding_modal" => 14,
        _ => 5,
    }
}
```

## Deliverables

### 1. Define PanelGroup enum and registration table

- [ ] Add `PanelGroup` enum to `reachlock-client/src/states.rs` (near `GameMode`/`ActivePanel`)
- [ ] Add `panel_group_for(panel: &ActivePanel) -> PanelGroup` function
- [ ] Add `z_order_for(label: &str) -> i32` function

### 2. Build group-close function

- [ ] Implement `close_panels_in_group` in a new file `reachlock-client/src/systems/panel_manager.rs`
- [ ] Function takes the group being activated + the ActivePanel being opened
- [ ] Iterates all known panel visibility flags and sets them to false if they're in the same group and not the panel being opened
- [ ] Returns nothing (mutates resources in-place)

### 3. Wire group-close into every panel toggle

For each toggle system (`reputation_panel_toggle`, `discovery_panel_toggle`, `career_panel_toggle`, `captains_log_toggle`, `mission_board_toggle`, `market_system` open, etc.):

- [ ] `factions.rs:23` — before setting `visible.0 = true`, call `close_panels_in_group(InfoPanel, ...)`
- [ ] `discovery.rs:68` — same
- [ ] `career.rs:28` — same
- [ ] `log_ui.rs` (`captains_log_toggle`) — same
- [ ] `mission_board.rs` (`mission_board_toggle`) — same
- [ ] `market.rs` (`market_system`) — before market open, close Trade group

### 4. Apply z-order to panel spawns

- [ ] In `hud.rs` `spawn_hud`: add `ZIndex(z_order_for("ship_hud_footer"))` etc. to each text entity
- [ ] In `factions.rs` `spawn_reputation_panel`: add `ZIndex(z_order_for("panel_info"))`
- [ ] In `discovery.rs` `spawn_discovery_panel`: add `ZIndex(z_order_for("panel_info"))`
- [ ] In `career.rs` `spawn_career_panel`: add `ZIndex(z_order_for("panel_info"))`
- [ ] In `log_ui.rs`: add `ZIndex(z_order_for("panel_info"))`
- [ ] For narrative panels (trope/dilemma/encounter): add `ZIndex(z_order_for("panel_narrative"))`
- [ ] For market: add `ZIndex(z_order_for("panel_trade"))`

### 5. Register new panel_manager module

- [ ] Add `mod panel_manager;` to `client/src/systems/mod.rs`
- [ ] Add `pub mod panel_manager;` to the use list in `client/src/main.rs`

### 6. Test

- [ ] Unit test: `close_panels_in_group` closes siblings but leaves `keep` open
- [ ] Unit test: every `ActivePanel` variant maps to a valid `PanelGroup`
- [ ] Unit test: `z_order_for` returns distinct values for every string key

## Acceptance gates

```bash
cargo test -p reachlock-client panel_manager
cargo test -p reachlock-client states::panel_group
cargo clippy -p reachlock-client -- -D warnings

# Manual: Open career panel (O) → verify renders. Open factions (N) → career closes, factions shows.
# Open discovery (H) → factions closes, discovery shows.
# Close all → open market (K) → market shows. Open career → market stays (different groups).
# Narrative: A trope fires → renders. A dilemma fires → trope closes, dilemma shows.
make check
```

## Non-goals

- Perfect overlap detection (just group-based mutual exclusion)
- Animated panel transitions (that's visual polish, not correctness)
- Resizable / draggable panels
- Panel history / back stack
- Tabbed panel container (separate sprint — this is the minimum viable fix)

## Gotchas

- **`ActivePanel::Dialogue(e)` carries an Entity.** The `keep` in `close_panels_in_group` must handle `ActivePanel::Dialogue(_)` as "any dialogue" — not match exact entities. Two different NPC dialogues should still close each other.
- **Narrative panels are event-driven, not toggled.** Trope/dilemma/encounter panels are opened by the engine, not the player pressing a key. Close previous narratives when a new one fires, but don't prevent the player from pressing Esc to dismiss them.
- **`ActivePanel` enum has `Unknown` variant.** Map it to `PanelGroup::None` in the registration function to avoid panics.
- **Market uses `ActivePanel::Market` AND internal state.** Make sure closing the market via group-close also resets the internal `MarketState` if needed.
- **ZIndex is retrospective.** Panels already spawned without `ZIndex` get implicit ZIndex 0. Adding ZIndex to one panel without adjusting others will reorder the stack unexpectedly. Apply ZIndex to ALL spawned HUD/panel text entities in one commit.
