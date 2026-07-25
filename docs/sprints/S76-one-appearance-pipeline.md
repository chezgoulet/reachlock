# S76 — One Appearance Pipeline

**Spec:** New (pixel::Look → CharacterLookConfig conversion, crew_look() deletion, reusable widget, SoulFile.look) ·
**Wave D (character & open world)** · **Depends on:** — (standalone, no UI dependency)

## Outcome

The two parallel appearance systems are unified into one. `pixel::Look` becomes a thin Bevy-side renderer that derives from `core::generator::sprite::CharacterLookConfig` via a `From` impl. The hardcoded `crew_look()` match (`pixel.rs:419-500`) is deleted — the seven Loup-Garou crew members get their look from authored soul files instead. The `SpriteViewer` controls in the editor are extracted into a reusable `CharacterAppearanceEditor` widget so the editor previewer and the in-game character creator render the same UI from the same config. Authored souls carry `look: Option<CharacterLookConfig>`; procedural NPCs derive their look from seed.

**Closes:** X3 (two parallel appearance systems), X4 (crew_look() hardcoded match)
**Also closes:** — the appearance half of the character-identity gap
**Part of Wave D — start-now, no dependencies**

## Context

- **X3 (High):** Two systems model the same concepts with zero shared code. `core::generator::sprite` produces `CharacterLookConfig → CharacterSprite` — deterministic, parametric, serializable, used only by the editor previewer. `client::pixel` produces `Look` — a Bevy-side type with `BodyKind` enum (5 variants), `Hair` enum (7 variants), `bevy::Color` for all channels — used by the entire game. They share no struct, no conversion, no test. A player customizes their look via `CharacterLookConfig` (the generator) while the game renders them as `Look` (the pixel system). The two never meet.
- **X4 (High):** `crew_look()` at `pixel.rs:419-500` is a `match` over seven hardcoded lore ids (`"tib"`, `"tove"`, `"keene"`, `"bardo"`, `"prudence"`, `"risc"`, `"boris"`) returning hand-authored palette tuples. It is the only path from character identity to rendered appearance. When crew become data-driven (S77), this match cannot survive — authored souls define their own look.
- **The recommendation (CHARACTER-CREATION-PLAN.md §3.3):** unify on `core::generator::sprite::CharacterLookConfig`. It is deterministic, already parametric, already has an editing UI, lives in the pure crate (so the server and CLI can reason about appearance), and serializes. `pixel::Look` becomes a thin Bevy-side renderer. `crew_look()` dissolves into authored look configs on soul files.
- **Offline-first:** the reusable widget is a local bevy_ui widget. It works identically with no server. Online adds the ability to sync look configs via presence messages, but the widget itself is always local.
- **The `SpriteViewer` in the editor** (`editor/editors/character_sprite.rs`, 479 lines) is a working character-creator prototype: sliders, species switch, seed reroll, walk-cycle preview. Its controls must become a reusable widget that the editor and the in-game creator both instantiate.

## Freeze first

### `From<CharacterLookConfig> for Look` conversion (`reachlock-client/src/systems/pixel.rs`)

```rust
impl From<CharacterLookConfig> for Look {
    fn from(config: CharacterLookConfig) -> Self {
        // Map species string → BodyKind
        // Map hair_style u8 → Hair variant
        // Map [u8;3] color triples → bevy::Color (sRGBA)
        // Map chassis/visor/accessory fields
        // For fields where config is None, derive from a deterministic
        // hash of the entity id + species (same seeding strategy used
        // by procedural NPC generation).
        todo!()
    }
}
```

The mapping table must cover all known species strings registered in the game:
"Human", "Android", "Robot", "Voidborn", "Xenotype". Any unrecognised species
string falls back to `BodyKind::Human`.

### `SoulFile.look: Option<CharacterLookConfig>` (`reachlock-core/src/soul/types.rs`)

```rust
pub struct SoulFile {
    // … existing fields …
    pub look: Option<CharacterLookConfig>,
    // … existing fields …
}
```

`None` means "derive look from seed at runtime" — the current behaviour for
procedural NPCs. `Some(config)` pins the look explicitly, as authored souls
and player characters do.

### Reusable widget signature (`reachlock-editor/src/widgets/character_appearance.rs`)

New module. The existing `SpriteViewer` in `editor/editors/character_sprite.rs`
is split: the rendering/preview side stays in the editor, the *controls* for
editing a `CharacterLookConfig` move to a shared widget.

```rust
/// A reusable widget for editing a CharacterLookConfig. Used by the editor
/// previewer and by the in-game character creator (S78).
///
/// Takes a &mut CharacterLookConfig and renders controls inline. Returns
/// true if any value changed.
pub fn character_appearance_editor(
    ui: &mut egui::Ui,
    config: &mut CharacterLookConfig,
    seed: &mut u64,                    // for "Reroll" button
) -> bool {
    // Species dropdown
    // Hair style selector (7 styles, visual preview thumbnails)
    // Color pickers: hair, skin, shirt, pants, jacket, chassis, visor
    // Reroll button (randomizes un-pinned fields)
    // Walk cycle toggle (run animation preview)
    let changed = false;
    changed
}
```

This widget lives in `reachlock-editor/src/widgets/` (not in core — it depends
on egui). The editor re-exports it; client side (S78) can depend on
`reachlock-editor` for the widget, or duplicate the function with its own UI
framework if S70 chooses bevy_ui over egui.

## Deliverables

### 1. `CharacterLookConfig` → `Look` conversion

- [ ] Implement `From<CharacterLookConfig> for Look` in `reachlock-client/src/systems/pixel.rs`.
- [ ] Species mapping table: `"Human" → BodyKind::Human`, `"Android" → BodyKind::Android`, `"Robot" → BodyKind::Robot`, `"Voidborn" → BodyKind::Voidborn`, `"Xenotype" → BodyKind::Xenotype`. Add a `From<&str>` or `TryFrom<&str>` on `BodyKind` for the reverse direction.
- [ ] Hair mapping: the 7 `CharacterLookConfig` hair style indices (0-6) map to the 7 `Hair` enum variants. Define an explicit match — no arithmetic cast.
- [ ] Color mapping: `[u8; 3]` (RGB) → `bevy::Color::srgba(r, g, b, 1.0)`. Clip to valid range (though the generator already produces 0-255).
- [ ] For optional config fields (`hair_color`, `skin_color`, `shirt_color`, `pants_color`, `jacket_color`, `chassis_color`, `visor_color`): if `None`, derive from `FastRng::seed_from(entity_id.wrapping_add(species_seed_offset))`. Document the derivation formula in a comment so it can be reproduced server-side.
- [ ] Add a test: for every `(BodyKind, Hair)` pair, build a `CharacterLookConfig` with pinned colors, convert to `Look`, assert all fields are populated and the `BodyKind`/`Hair` variants match. Run this as a table-driven test.
- [ ] Add a test: `CharacterLookConfig` with all `None` fields converts to a valid `Look` (seed-derived path).
- [ ] Add a test: unrecognised species string converts to `BodyKind::Human` (graceful fallback).

### 2. Delete `crew_look()` (`reachlock-client/src/systems/pixel.rs:419-500`)

- [ ] Remove the `crew_look()` function entirely — the ~80-line `match` over 7 lore ids.
- [ ] Find every call site of `crew_look(id)` in the codebase. There is exactly one: `interior.rs:543-553` (the avatar render call). Replace it with the new look-resolution path: souls loaded from the content index carry `look: Some(CharacterLookConfig)`. The avatar lookup becomes `soul.look.unwrap_or_else(|| derive_look_from_seed(soul.id, &soul.species))`.
- [ ] Verify no remaining references to `crew_look` via `grep -r crew_look reachlock-client/src/`. There should be zero.
- [ ] Remove `"tib"`, `"tove"`, `"keene"`, `"bardo"`, `"prudence"`, `"risc"`, `"boris"` string literals from pixel.rs if they were only used as crew_look keys. (The soul files themselves still carry these ids — the ids remain, the palette mapping code is what goes.)

### 3. `SoulFile.look` field

- [ ] Add `look: Option<CharacterLookConfig>` to `SoulFile` in `reachlock-core/src/soul/types.rs`. Default to `None`.
- [ ] Update the SoulFile wire-shape test to include the new field. The serialized form changes — this is a protocol revision per iron rule #4. Tag the commit message.
- [ ] Update `generate_soul` in `core/generator/soul.rs`: after generating the soul, if `look` is `None`, leave it `None` (procedural NPCs derive from seed at render time). Add a new variant `generate_soul_with_look(seed, species, look_config)` that pins the look.
- [ ] Update determinism goldens: the existing `generate_soul` golden will change because `SoulFile` gained a field. Recapture goldens deliberately per iron rule #3. The commit message must say "S76: add SoulFile.look — manifest changed."
- [ ] Add a golden entry for `generate_soul_with_look` with a pinned `CharacterLookConfig`.

### 4. Reusable `CharacterAppearanceEditor` widget

- [ ] Create `reachlock-editor/src/widgets/character_appearance.rs`.
- [ ] Extract the control UI from `editor/editors/character_sprite.rs` `SpriteViewer` — the species dropdown, hair style selector, 7 color pickers, and reroll button. Leave the walk-cycle animation preview canvas in the editor (it depends on the editor's rendering context).
- [ ] The widget takes `&mut CharacterLookConfig` and `&mut u64` (seed for reroll). Returns `bool` (changed).
- [ ] Species dropdown: `egui::ComboBox` with the 5 known species strings.
- [ ] Hair style selector: 7 buttons or a `egui::ComboBox` with labels `"Bald"`, `"Short"`, `"Long"`, `"Ponytail"`, `"Braids"`, `"Mohawk"`, `"Sidecut"`.
- [ ] Color pickers: each pinned color gets an `egui::color_picker::color_edit_button_srgb`. Each unpinned color gets a checkbox "(auto)" that, when checked, sets the option to `None` (seed-derived).
- [ ] Reroll button: increments the seed by 1 and sets all un-pinned fields to `None` so the generator re-derives them.
- [ ] Register the new module in `reachlock-editor/src/widgets/mod.rs`. If no `mod.rs` exists, create one.
- [ ] Refactor `editor/editors/character_sprite.rs` to call `character_appearance_editor(&mut ui, &mut config, &mut seed)` instead of inline controls. The `SpriteViewer` retains only the preview canvas and the walk-cycle animation.

### 5. Authored soul look configs (S59 crew souls)

- [ ] The seven Loup-Garou crew soul files (`mods/reachlock/souls/loup_garou_*.ron`) gain `look: Some(…)` with the palette values that were previously hardcoded in `crew_look()`. Extract the RGB triples from the deleted match arm for each crew member.
- [ ] Validate round-trip: load each soul file, assert `look` is `Some`, convert to `Look`, assert the palette matches the original hardcoded values.

### 6. Round-trip test — every Species/BodyKind pair

- [ ] Table-driven test in `pixel.rs` or a new test module: for each `BodyKind` variant, for each `Hair` variant, build a `CharacterLookConfig` with pinned species/hair/colors → convert to `Look` → assert `Look.body_kind == BodyKind`, `Look.hair == Hair`, `Look.hair_color == expected`, etc.
- [ ] Table-driven test: the same `CharacterLookConfig` applied to the editor renderer and the client renderer produces visually identical pixel output. This requires rendering to a pixel buffer from both sides — use the existing `CharacterSprite::render_to_buffer` (or equivalent) in core, and a `Look::render_to_buffer` in client.

## Acceptance gates

```
cargo test -p reachlock-core soul::types::wire_shape  # SoulFile wire shape updated
cargo test -p reachlock-core determinism::               # goldens recaptured (SoulFile.look, generate_soul_with_look)
cargo test -p reachlock-client pixel::look_from_config   # conversion tests pass
cargo test -p reachlock-client pixel::round_trip          # every Species/BodyKind round-trips
cargo test -p reachlock-client pixel::crew_look_gone      # zero references to crew_look()
cargo test -p reachlock-editor character_appearance::     # widget unit tests
# Manual: open the editor → Character Sprite preview → controls still work
# Manual: build client → avatar renders with look from authored soul
grep -r "crew_look" reachlock-client/src/                 # must return nothing
make check
```

Manual: open the editor, edit a character sprite, save the `.ron`. Load that `.ron` in the game client — the avatar renders identically. Reroll in the editor → seed changes → game renders the new look. Authored soul without `look` field renders from seed (procedural NPC).

## Non-goals

- The in-game character creator screen itself (S78 — depends on S70's client UI framework)
- Walk-cycle animation for the player character (the existing walk cycle stays; no new animation work)
- The `SpriteViewer` preview canvas extract (that's an editor concern; the widget is just the controls)
- Network sync of look configs beyond the presence message shape (presence already got the fields in S75; this sprint makes them renderable)
- Any change to the Ship interior avatar system beyond replacing the `crew_look()` call site

## Gotchas

- `CharacterLookConfig` is in `reachlock-core`. It cannot depend on `bevy::Color`. The `From` impl lives in `reachlock-client` where `bevy::Color` is available — do not move the conversion into core.
- The `crew_look()` call site in `interior.rs:543-553` uses `pixel::crew_look("tib")` to get the avatar `Look`. After deletion, the avatar system must load the soul from the content index and extract its `look` field. If souls haven't loaded yet (startup ordering), guard with `Option`. The avatar system already handles `Option`-based lookups — the existing code has a fallback path.
- The `SoulFile` wire shape test asserts the full serialized bytes. Adding `look: Option<CharacterLookConfig>` changes those bytes. This is a protocol revision. The test must be updated in the same commit. If the `CharacterLookConfig` struct changes in the future, the wire-shape test catches it — correct and deliberate.
- The reusable widget lives in `reachlock-editor` and depends on `egui`. The in-game character creator (S78) may use `bevy_ui` instead of `egui` (per S70's split framework decision). If S70 chooses bevy_ui for game panels, the appearance controls must be duplicated or adapted for bevy_ui. If S70 chooses bevy_egui for creator panels, the widget is used directly. This sprint creates the *editor* widget; the S78 author adapts or reimplements for the game's UI framework.
- The `From` conversion's seed-derivation for unpinned colors must match the procedural NPC's derivation. The canonical seed formula is: `entity_id.wrapping_add(match species { "Human" => 0, "Android" => 1, "Robot" => 2, "Voidborn" => 3, "Xenotype" => 4, _ => 0 })`. Document this in `pixel.rs` so it becomes a protocol if other renderers (server thumbnails, CLI preview) need to reproduce the look.
- `generate_soul_with_look` is a new function. It must have its own determinism golden. The existing `generate_soul` golden will change because `SoulFile.look` changes the serialized form even when `None` — RON serialises `look: None` where previously there was no `look` field. This is expected. Recapture the golden and commit with the manifest-change note.
- Branch: `sprint-v2/s76-one-appearance-pipeline`, cut from `testing`.
