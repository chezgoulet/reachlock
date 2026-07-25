# S97 — Character Creation Sprite Preview

**Wave: UX-Polish · Depends on:** S78 (Character creation flow), S76 (CharacterLookConfig / sprite generator)

## Outcome

The character creation flow shows a live sprite preview of the captain next to the text panel. The sprite updates in real-time as the player changes species, hair style, colors, and jacket. The sprite generator from `reachlock_core::generator::sprite` (which is seed-deterministic) renders via a Bevy 2D `Sprite` entity.

## Context

The character creation flow currently renders everything as plain text:

```rust
// character_creation.rs:272-311
lines.push(format!("Species: {}", species));
lines.push(format!("Hair style: {}", hair));
lines.push(format!("Hair color: #{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
```

There is zero visual feedback for the character's appearance. The player is designing a character they cannot see.

The sprite generator exists in core and IS used:
- The editor has a `CharacterLookConfig` editor (`editor/src/editors/character_sprite.rs`) with full species/hair/color controls
- `editor/src/editors/widgets.rs` has `character_appearance_editor()` — a reusable egui widget
- `generator::sprite` has `generate_character_sprite(config, seed) -> GeneratedSprite`
- `character_creation.rs` already constructs a `CharacterLookConfig` and passes it through `build_player_soul()`

The generator returns pixel data (width, height, palette, indices). The bridge layer (`client/src/bridge.rs`) converts these to Bevy assets.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/character_creation.rs` | Creation flow — add sprite spawn/render |
| `reachlock-core/src/generator/sprite.rs` | Sprite generator (no changes needed) |
| `reachlock-client/src/bridge.rs` | `sprite_to_image()` — converts generator output to Bevy `Image` |
| `reachlock-client/src/systems/menu.rs` | Reference — camera setup for 2D rendering |

## Freeze first

### Sprite entity component

```rust
#[derive(Component)]
pub struct CharacterPreviewSprite;
```

### Sprite position

```rust
// Right side of the creation UI, vertically centered.
// Creation UI is at top 10%, left 10%, 80%x80% (absolute).
// Sprite goes at top 25%, left 68%, size ~128x192 px.
// Generated sprites are typically 32x48 px at 1×; scale 4× to 128x192.
```

### Bridge function

```rust
// In bridge.rs (or a new function):
pub fn character_sprite_to_image(
    sprite: &reachlock_core::generator::sprite::GeneratedSprite,
) -> Image {
    let scale = 4;
    let w = sprite.width as u32 * scale;
    let h = sprite.height as u32 * scale;
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let sx = (x / scale) as usize;
            let sy = (y / scale) as usize;
            let idx = sprite.pixels[sy * sprite.width as usize + sx];
            let color = sprite.palette[idx as usize];
            pixels.extend_from_slice(&color);
            pixels.push(255);
        }
    }
    Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}
```

## Deliverables

### 1. Add bridge function for character sprite → Image

- [ ] Add `character_sprite_to_image()` to `bridge.rs` (or a new `sprite_bridge.rs`)
- [ ] Handle the nearest-neighbor scale-up (4× recommended for 32×48 → 128×192)

### 2. Add sprite entity to creation UI

- [ ] In `spawn_creation_ui` (`character_creation.rs:173`):
  - Spawn a second child entity with `CharacterPreviewSprite` marker
  - Initialize it with the default `CharacterLookConfig` generated sprite
- [ ] Position: absolute, `top: 25%, left: 68%` — right side of the creation UI panel

### 3. Update sprite on config change

- [ ] In `character_creation_input` (`character_creation.rs:444`): whenever `creation.look` changes (species, hair, colors, jacket), regenerate and update the sprite
- [ ] In `randomize_step`: after randomizing appearance, regenerate sprite
- [ ] In `advance` (step change): when entering Appearance step, ensure sprite is visible

### 4. Handle species changes

- [ ] When species changes (keys 1-5 on Identity step): update `creation.look.species` AND regenerate sprite
- [ ] Already happening in `character_creation_input:506-507` — verify

### 5. Visibility per step

- [ ] On Identity step: show generic placeholder sprite based on current species (auto-generated look)
- [ ] On Appearance step: show the live preview updating with every adjustment
- [ ] On Origin / ShipCrew / GalaxySeed / Confirm steps: continue showing the sprite alongside the text

### 6. Despawn on exit

- [ ] `despawn_creation_ui` already despawns the `CreationUiRoot` entity and its children — verify the sprite child is cleaned up

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Start new game → on Identity step, see a sprite (auto-generated from species)
# 2. Press 1-5 to cycle species → sprite changes species appearance
# 3. Advance to Appearance → press P to cycle pronouns → (no sprite change)
# 4. Press R to randomize → sprite updates immediately
# 5. Advance through all steps → sprite stays visible alongside text
# 6. Launch game → character sprite in save matches the preview

make check
```

## Non-goals

- Sprite animation (idle/walk/etc.) — this is a static preview
- Real-time hair/color editing via arrow keys (that's an editor feature, not the creation flow)
- Species-specific idle poses
- Background/scene behind the sprite

## Gotchas

- **GeneratedSprite uses indices and a palette.** Each pixel is a palette index (u8), not a direct RGBA value. The bridge function must do the palette lookup.
- **Nearest-neighbor scaling.** The generator produces 32×48 pixel art. Scaling to 128×192 with bilinear filtering would blur it. Use nearest-neighbor (integer division as shown in the freeze section).
- **`creation.look.species` is a String, not Species enum.** After re-randomizing, the sprite species string must match the identity species string. In `randomize_step:553-554`, both are set together. Verify they stay in sync.
- **The generated sprite is seed-deterministic.** Use `creation.sprite_seed` (already in the resource) as the seed. Each re-roll increments `sprite_seed`.
- **Bevy `Image` creation is synchronous.** No async loading needed — the sprite generator runs in ~1ms.
- **Robot species has different color channels** (chassis + visor, not hair + skin). The generator handles this automatically — just pass the `CharacterLookConfig`.
