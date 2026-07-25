# S92 — Inventory Panel

**Wave: UX-Hardening · Depends on:** S07 (Inventory resource), S70 (Client UI framework)

## Outcome

The `OpenInventory` keybind (KeyI) actually opens an inventory panel. Players can view their credits, cargo items, and equipped weapon. The panel renders as a formatted text overlay (consistent with the existing faction/discovery/career panels), with visibility toggled by `Visibility::Hidden` / `Visibility::Visible`.

## Context

`OpenInventory` exists in `InputAction` with a default binding of `KeyI`. The `PlayerInventory` resource exists with `credits`, `capacity`, `cargo` (BTreeMap<GoodId, u32>), and `equipped_weapon`. But there is no panel spawn, no toggle, no render system. The keybind is a dead letter.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/settings.rs` | `OpenInventory` action definition (line ~639) |
| `reachlock-client/src/systems/inventory.rs` | `PlayerInventory` resource (line 27) |
| `reachlock-client/src/systems/factions.rs` | Reference panel pattern — copy structure from here |
| `reachlock-client/src/systems/contract.rs` | `ShipLog` — not needed, just noting cargo exists here too |
| `reachlock-client/src/main.rs` | Register new systems |

## Freeze first

### Panel visibility toggle resource

```rust
/// Marker resource — inventory panel visible state.
#[derive(Resource, Default)]
pub struct InventoryPanelVisible(pub bool);
```

### Marker component for the text entity

```rust
#[derive(Component)]
pub struct InventoryPanel;
```

### Panel spawn data (absolute positioning)

```rust
// Left-center, below the main HUD but distinct from the left-side panel stack.
// Use: top 200px, left 8px (below the career/factions/discovery panels at top 120px).
```

## Deliverables

### 1. Add InventoryPanelVisible resource

- [ ] Add to `inventory.rs`:
```rust
#[derive(Resource, Default)]
pub struct InventoryPanelVisible(pub bool);
```

### 2. Add toggle system

- [ ] In `inventory.rs`, add:
```rust
pub fn inventory_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<InventoryPanelVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenInventory)) {
        visible.0 = !visible.0;
    }
}
```

### 3. Add spawn system

- [ ] In `inventory.rs`, add:
```rust
pub fn spawn_inventory_panel(mut commands: Commands) {
    commands.spawn((
        InventoryPanel,
        Text::new(""),
        TextFont { font_size: 14.0, ..default() },
        TextColor(Color::srgb(0.9, 0.9, 0.75)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(200.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}
```

### 4. Add render system

- [ ] In `inventory.rs`, add:
```rust
pub fn render_inventory_panel(
    visible: Res<InventoryPanelVisible>,
    inventory: Res<PlayerInventory>,
    mut query: Query<(&mut Text, &mut Visibility), With<InventoryPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── INVENTORY ──".to_string()];
            lines.push(format!("Credits: {}", inventory.credits));
            lines.push(format!("Capacity: {} / {}", inventory.cargo_units(), inventory.capacity));
            lines.push(String::new());
            lines.push("Cargo:".to_string());
            if inventory.cargo.is_empty() {
                lines.push("  (empty)".to_string());
            } else {
                for (good, qty) in &inventory.cargo {
                    lines.push(format!("  {}: {}", good.0, qty));
                }
            }
            if let Some((weapon, stats)) = &inventory.equipped_weapon {
                lines.push(String::new());
                lines.push(format!("Equipped: {:?}", weapon));
                lines.push(format!("  Damage: {:.1}", stats.0.get(&reachlock_core::item::types::Stat::Damage).copied().unwrap_or(0) as f32 / 1024.0));
            }
            **text = lines.join("\n");
        } else {
            *vis = Visibility::Hidden;
        }
    }
}
```

### 5. Register in main.rs

- [ ] Add `spawn_inventory_panel` to the `OnEnter(AppState::InGame)` system chain
- [ ] Add `inventory_panel_toggle` and `render_inventory_panel` to the per-frame Update chain under `in_state(AppState::InGame)`

### 6. Init resource

- [ ] Add `.init_resource::<InventoryPanelVisible>()` to the init block in `main.rs`

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual: Press I in-game → inventory panel shows credits + cargo + equipped weapon
# Press I again → panel hides
# Buy goods at market → press I → cargo list updates
# Equip a weapon → press I → weapon section shows

make check
```

## Non-goals

- Item icons or visual item grid
- Drag-drop inventory management
- Equipment comparison tooltips
- Cargo transfer UI (buy/sell happens through the market panel)
- Inventory sorting or filtering

## Gotchas

- **`GoodId` is a newtype around String.** Display it with `good.0` or implement `Display` on the spot. `"{:?}"` will include quotes — prefer the plain `good.0` access.
- **`equipped_weapon` is `Option<(MeleeWeapon, ItemStats)>`.** The `ItemStats` is a `BTreeMap<Stat, i64>`. Use the `Damage` stat for the summary line; other stats can be added later.
- **Panel position.** `top: 200px, left: 8px` puts it below the faction/discovery/career panel region (120px). This avoids overlap but still leaves room for other panels. If the faction panel is open AND inventory is open, both are visible (different positions).
- **Auto-close from S91.** The inventory panel is in `PanelGroup::InfoPanel` (shared with factions, discovery, career, log). Opening inventory should close other InfoPanel siblings. Add `close_panels_in_group(InfoPanel, ...)` to the toggle function.
