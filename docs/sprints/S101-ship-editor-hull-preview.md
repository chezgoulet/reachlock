# S101 — Ship Editor 2D Hull Preview

**Wave: UX-Polish · Depends on:** S17 (Ship Exterior Editor), S18 (Ship Interior Editor), S04 (Hull generator)

## Outcome

The ship exterior editor shows a 2D preview of the hull configuration — a wireframe or silhouette rendered next to the editing panel. The interior editor shows a 2D grid preview of room placement on the current deck. Both update in real-time as the player cycles through options.

## Context

The ship editors currently render all data as text key-value pairs:

```rust
// editor_panel_text returns strings like:
"Hull: frame_corvette"
"Hardpoint 0: Laser Cannon (Tier 3)"
"Plating Fore: 512 mass"
```

The hull frame generator (`reachlock_core::generator::hull`) produces `HullFrame` with hardpoint positions, armor zone polygons, engine mount points, and decal slots. The generator output is data — not a rendered mesh. A 2D preview would:

1. Draw the hull outline (a polygon from `frame.hull_shape` vertices)
2. Mark hardpoint positions (small squares at `frame.hardpoints[i].position`)
3. Mark engine mount (triangle at `frame.engine_mount`)
4. Color-code armor zones (tinted polygons for fore/aft/port/starboard zones)

For the interior editor:
1. Draw a grid of room cells (each cell = a room template)
2. Color-code rooms by type (cockpit=blue, reactor=orange, medbay=green, etc.)
3. Show door connections between adjacent rooms

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/shipeditor/exterior.rs` | Exterior editor — add 2D preview spawn |
| `reachlock-client/src/systems/shipeditor/interior.rs` | Interior editor — add grid preview spawn |
| `reachlock-core/src/generator/hull.rs` | `HullFrame`, hardpoint positions, zone polygons |
| `reachlock-core/src/generator/ship.rs` | `ShipInterior`, decks, room layout |
| `reachlock-client/src/pixel.rs` | 2D rendering helpers |

## Freeze first

### Exterior preview geometry

```rust
/// 2D preview data for the exterior editor.
pub struct HullPreview {
    /// Vertices of the hull outline, in screen space (scaled and offset).
    pub outline: Vec<Vec2>,
    /// Hardpoint markers: (position, size_class)
    pub hardpoints: Vec<(Vec2, HardpointSize)>,
    /// Engine mount position.
    pub engine: Vec2,
    /// Armor zones: (polygon vertices, zone_id)
    pub armor_zones: Vec<(Vec<Vec2>, String)>,
    /// Decal slots: (position, slot_id)
    pub decal_slots: Vec<(Vec2, String)>,
}

/// Convert a HullFrame to screen-space preview geometry.
/// Scale: hull width fits in 160px preview area. Origin at center.
pub fn hull_to_preview(frame: &HullFrame) -> HullPreview {
    let scale = 160.0 / frame.bounding_width().max(1.0) as f32;
    // ... transform hull_shape vertices, hardpoint positions, etc.
}
```

### Interior preview grid

```rust
/// 2D grid preview for the interior editor.
pub struct InteriorPreview {
    /// Grid of (x, y) → RoomKind (None = empty cell)
    pub cells: Vec<Vec<Option<RoomKind>>>,
    /// Door connections: ((x1, y1), (x2, y2))
    pub doors: Vec<((usize, usize), (usize, usize))>,
    pub grid_width: usize,
    pub grid_height: usize,
    pub cell_size: f32,  // px per cell
}

/// Convert a ShipDeck's room layout to a preview grid.
pub fn interior_to_grid(deck: &ShipDeck, cell_size: f32) -> InteriorPreview {
    // ... snap rooms to grid cells ...
}
```

### Preview drawer (Gizmo-based)

Use Bevy's `Gizmos` for 2D drawing (wireframe lines, filled polygons):

```rust
fn draw_hull_preview(
    gizmos: Gizmos,
    preview: &HullPreview,
    position: Vec2,      // bottom-left of preview area in world space
) {
    // Draw outline polygon (white lines)
    // Draw hardpoint markers (small filled squares, colored by size)
    // Draw engine mount (triangle, red)
    // Draw armor zones (translucent filled polygons, colored by zone)
}
```

## Deliverables

### 1. Add HullPreview struct and conversion

- [ ] Add `hull_to_preview(frame: &HullFrame) -> HullPreview` to `shipeditor/exterior.rs` (or a new `shipeditor/preview.rs`)
- [ ] Scale hull to fit in a 200×160 pixel preview area
- [ ] Offset hardpoints, engine, armor zones by the same scale+offset

### 2. Add InteriorPreview struct and conversion

- [ ] Add `interior_to_grid(deck: &ShipDeck) -> InteriorPreview`
- [ ] Snap rooms to grid cells based on their position/size
- [ ] Detect door connections

### 3. Add gizmo rendering systems

- [ ] `draw_hull_preview` system: runs when `ActivePanel::ShipExterior`, renders gizmo lines/polygons
- [ ] `draw_interior_preview` system: runs when `ActivePanel::ShipInterior`, renders grid cells + door lines
- [ ] Preview position: right side of the editor panel (world-space x offset of ~320px from panel origin)
- [ ] Preview updates when the editor state changes (on every option cycle)

### 4. Color mapping for room types

```rust
fn room_color(kind: RoomKind) -> Color {
    match kind {
        RoomKind::Cockpit => Color::srgb(0.3, 0.6, 0.9),
        RoomKind::Reactor => Color::srgb(0.9, 0.5, 0.2),
        RoomKind::MedBay => Color::srgb(0.3, 0.8, 0.4),
        RoomKind::Bridge => Color::srgb(0.5, 0.5, 0.9),
        RoomKind::Scanner => Color::srgb(0.4, 0.7, 0.9),
        RoomKind::TechBay => Color::srgb(0.7, 0.7, 0.3),
        RoomKind::Quarters => Color::srgb(0.5, 0.5, 0.5),
        RoomKind::Bar => Color::srgb(0.8, 0.6, 0.3),
        RoomKind::Cryo => Color::srgb(0.6, 0.8, 0.9),
        RoomKind::Hangar => Color::srgb(0.6, 0.6, 0.3),
        RoomKind::Corridor => Color::srgb(0.3, 0.3, 0.3),
        _ => Color::srgb(0.4, 0.4, 0.4),
    }
}
```

### 5. Update preview on editor state change

- [ ] Exterior: when hull frame changes, recalculate `HullPreview`
- [ ] Exterior: when hardpoint/plating/paint changes, update markers
- [ ] Interior: when deck or room layout changes, recalculate `InteriorPreview`
- [ ] Store the preview data as a `Local` in the draw system (recalculated each frame or cached when editor state changes)

### 6. Register systems

- [ ] Add `draw_hull_preview` and `draw_interior_preview` to the Update schedule
- [ ] Run only when the corresponding `ActivePanel` is open

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Dock at station → open shipyard → see hull outline with hardpoint markers
# 2. Cycle through hull frames → preview updates
# 3. Change hardpoint → hardpoint marker changes color/size
# 4. Open interior editor → see grid with room cells and doors
# 5. Cycle deck → grid updates

make check
```

## Non-goals

- 3D preview (wireframe is 2D top-down / side profile)
- Rotation/zoom of preview
- Clickable preview (click hardpoint → select it in editor)
- Exact-to-scale render (preview is approximate, showing relative positions)
- Rendering the actual hull mesh polygons (just the outline + markers)

## Gotchas

- **Bevy 0.18 Gizmos API.** Use `Gizmos` for 2D / 3D wireframe drawing. The `gizmos.line_2d()` and `gizmos.rect_2d()` methods render immediately (not through ECS entities). For filled polygons, draw multiple line segments or use a thin `Sprite` rect.
- **Gizmos draw in world space, not screen space.** The preview system must set the gizmo position relative to the camera. For interior mode (2D camera), world space ≈ screen space at the camera's transform. Apply a translation offset to position the preview to the right of the editor panel.
- **`Gizmos` requires a `GizmoConfigStore` resource.** It's provided by Bevy's `GizmoPlugin`. Verify it's registered in `main.rs` (it may be registered by `bevy_prototype_lyon` or the default Bevy plugins).
- **Hardpoint size class visualization.** Use `Small` = 6px square, `Medium` = 10px square, `Large` = 14px square. Color by slot type: weapon=red, utility=yellow, scanner=blue.
- **Fallback when no gizmos available.** If Bevy 0.18 doesn't support 2D gizmos cleanly, fall back to rendering via `Sprite` entities (small colored rectangles at computed positions). This is more code but always works.
