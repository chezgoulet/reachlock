# S100 — Console Station Diegetic Frames

**Wave: UX-Polish · Depends on:** S09b (OnBoard consoles), S70 (Client UI framework)

## Outcome

The ship's console stations (Gunner, Scanner, Miner, Power) render with diegetic visual frames — panel borders, gauge bars, and label headers — instead of plain text. The text content is unchanged; the visual container around it gives each console a distinct look that matches a ship cockpit.

## Context

Current console rendering (`onboard.rs` — `onboard_panels` / `onboard_ship_consoles`):

```
GUNNER
  Weapon: Laser
  Heat: 0%
  Target: none
```

```
SCANNER
  Range: 5000
  Objects: 3
```

These are plain text panels spawned at absolute positions. They work functionally but look like debug output.

The fix adds:
1. A frame border around each console text
2. A gauge bar for values that have a max (heat, mining progress, power level)
3. A console-specific header with label
4. Optional: background tint matching the console role (red for gunner, blue for scanner, yellow for miner, green for power)

All rendering uses Bevy's 2D primitives: `Sprite` rectangles for borders/bars, `Text` for labels.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/onboard.rs` | Console panel spawning + rendering |
| `reachlock-client/src/systems/hud.rs` | Reference — text panel spawn pattern |
| `reachlock-client/src/pixel.rs` | `BodyKind`, color constants |

## Freeze first

### Console frame dimensions

```rust
/// Dimensions and colors for each console type.
pub struct ConsoleFrame {
    pub label: &'static str,
    pub width: f32,       // pixels
    pub height: f32,      // pixels
    pub bg_color: Color,  // background tint
    pub accent_color: Color, // border and header color
}

pub fn frame_for_console(kind: InteractKind) -> ConsoleFrame {
    match kind {
        InteractKind::Gunner => ConsoleFrame {
            label: "GUNNER", width: 280.0, height: 200.0,
            bg_color: Color::srgba(0.1, 0.02, 0.02, 0.85),
            accent_color: Color::srgb(0.9, 0.3, 0.3),
        },
        InteractKind::Scanner => ConsoleFrame {
            label: "SCANNER", width: 300.0, height: 220.0,
            bg_color: Color::srgba(0.02, 0.02, 0.1, 0.85),
            accent_color: Color::srgb(0.3, 0.5, 0.9),
        },
        InteractKind::Miner => ConsoleFrame {
            label: "MINING", width: 260.0, height: 180.0,
            bg_color: Color::srgba(0.1, 0.08, 0.01, 0.85),
            accent_color: Color::srgb(0.9, 0.7, 0.2),
        },
        InteractKind::Power => ConsoleFrame {
            label: "POWER", width: 240.0, height: 240.0,
            bg_color: Color::srgba(0.02, 0.08, 0.02, 0.85),
            accent_color: Color::srgb(0.3, 0.8, 0.3),
        },
        _ => ConsoleFrame {
            label: "CONSOLE", width: 200.0, height: 120.0,
            bg_color: Color::srgba(0.05, 0.05, 0.05, 0.85),
            accent_color: Color::srgb(0.5, 0.5, 0.5),
        },
    }
}
```

### Gauge bar rendering

```rust
/// Render a horizontal gauge bar: [████████░░░░] 80%
/// Returns a text line for inclusion in the panel text.
fn gauge_bar(label: &str, value: i64, max: i64, width_chars: usize) -> String {
    let pct = (value as f32 / max.max(1) as f32).clamp(0.0, 1.0);
    let filled = (pct * width_chars as f32).round() as usize;
    let empty = width_chars - filled;
    format!(
        "{}: [{}{}] {}%",
        label,
        "█".repeat(filled),
        "░".repeat(empty),
        (pct * 100.0).round() as i32
    )
}
```

## Deliverables

### 1. Add console frame constants

- [ ] Add `frame_for_console(InteractKind) -> ConsoleFrame` to `onboard.rs`
- [ ] Add `ConsoleFrame` struct with width, height, bg/accent colors

### 2. Add gauge bar helper

- [ ] Add `gauge_bar(label, value, max, width_chars) -> String` to `onboard.rs`

### 3. Update console text rendering

- [ ] Gunner console: replace plain text with:
  ```
  ╔══════════ GUNNER ══════════╗
  ║  Weapon: {kind}            ║
  ║  {gauge_bar("Heat", heat, 100, 12)} ║
  ║  Target: {name}            ║
  ║  Ammo: {ammo}              ║
  ║  {gauge_bar("Shield", shield, 100, 12)} ║
  ╚════════════════════════════╝
  ```
- [ ] Scanner: formatted with scan range gauge, contact list
- [ ] Miner: formatted with beam intensity gauge, resource name, collection progress
- [ ] Power: formatted with system power bars, total output gauge

### 4. Add background color to console entities

- [ ] Each console spawn gets a `BackgroundColor(frames.bg_color)` on its `Node`
- [ ] The accent color is used for header text in `TextColor`

### 5. Position consoles within the frame

- [ ] Consoles are currently at absolute positions (defined in `onboard_panels` or `spawn_onboard_panels`)
- [ ] Frame size does NOT change position — the text entity stays at the same absolute position. The background color fills the `Node` dimensions.

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Board ship → walk to gunner console → see framed panel with red accent
# 2. Fire weapons → heat gauge fills
# 3. Walk to scanner → blue-framed panel with contact list
# 4. Walk to miner → yellow-framed panel with beam progress
# 5. Walk to power station → green-framed panel with system bars

make check
```

## Non-goals

- Interactive console buttons (click-to-select subsystems)
- Animated gauge transitions (instant update is fine)
- Custom fonts or pixel-art console frames (Unicode box-drawing chars are sufficient)
- Console sounds on gauge change
- Different frame styles per hull type

## Gotchas

- **The gauge bar is text-based Unicode blocks (█, ░).** This means it inherits the `TextColor` and `TextFont` of the parent entity. No separate sprite rendering needed.
- **Console positions are set by `onboard.rs` on spawn.** Don't move them — just add the `BackgroundColor` and `width`/`height` to the existing `Node` component.
- **`Node` width/height in Bevy 0.18 uses `Val::Px()`.** The frame width/height should match or exceed the text content. A typical console with 12 lines at 14px font needs ~168px height; use 200px to be safe.
- **Wide characters.** The Unicode box-drawing characters (╔═╗║╚╝) are narrow in most monospace fonts. If the Bevy default font renders them at double width, use ASCII fallback: `+=== HEADER ===+` and `| text here |`.
