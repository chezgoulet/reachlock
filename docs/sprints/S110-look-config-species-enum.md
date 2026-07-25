# S110 — CharacterLookConfig Species: String → Enum

**Wave: UX-Hardening · Depends on:** S76 (Sprite generator / CharacterLookConfig)

## Outcome

`CharacterLookConfig.species` is changed from `String` to `reachlock_core::soul::types::Species` enum. All string comparisons (`config.species == "Robot"`) are replaced with enum matching. This eliminates a class of bugs where a typo or case mismatch silently skips species-specific rendering (Robot chassis/visor vs Human hair/skin).

## Context

The `CharacterLookConfig` struct uses `String` for species:

```rust
// generator/src/sprite.rs
pub struct CharacterLookConfig {
    pub species: String,  // "Human", "Android", "Robot", "Voidborn", "Xenotype"
    pub hair_style: Option<u8>,
    // ...
}
```

Downstream code uses string equality:
```rust
// editor/src/editors/widgets.rs:207
let is_robot = config.species == "Robot";

// character_creation.rs:555
creation.look.species = SPECIES_NAMES[creation.identity.species].to_string();
```

If someone writes `"robot"` (lowercase) or `"roboto"` (typo), the species-specific rendering silently fails. The `Species` enum already exists in `soul::types` — use it.

### Key files

| File | Role |
|------|------|
| `reachlock-core/src/generator/sprite.rs` | `CharacterLookConfig` — change `species` field type |
| `reachlock-core/src/soul/types.rs` | `Species` enum — no changes needed |
| `reachlock-editor/src/editors/widgets.rs` | `character_appearance_editor` — use enum matching |
| `reachlock-client/src/systems/character_creation.rs` | Creation flow — use enum |
| `reachlock-core/src/identity.rs` | `PlayerCharacter` — may use `look.species` |

## Freeze first

### Type change

```rust
// Before:
pub struct CharacterLookConfig {
    pub species: String,
    // ...
}

// After:
pub struct CharacterLookConfig {
    pub species: Species,
    // ...
}
```

### Serialization compatibility

`Species` already derives `Serialize + Deserialize`. No RON/JSON migration needed — the RON files already store species as the enum discriminant (e.g., `species: Human`), which is the same format as the enum variant.

### Function signature changes

Every function that sets or reads `config.species` must use `Species` instead of `String`:

```rust
// Before:
pub fn CharacterLookConfig::seed_derived(species_name: &str) -> Self { ... }

// After:
pub fn CharacterLookConfig::seed_derived(species: Species) -> Self { ... }
```

## Deliverables

### 1. Change CharacterLookConfig.species type

- [ ] In `reachlock-core/src/generator/sprite.rs`: change `pub species: String` to `pub species: Species`
- [ ] Add `use crate::soul::types::Species;` import
- [ ] Update `seed_derived(species_name: &str)` → `seed_derived(species: Species)`
- [ ] Update any internal functions that use `config.species` as a string

### 2. Update editor widgets

- [ ] In `editor/src/editors/widgets.rs`:
  - Replace `config.species == "Robot"` with `config.species == Species::Robot`
  - Replace `config.species = name.to_string()` with enum matching on the species name
  - Update `SPECIES_NAMES` to return a `(Species, &str)` tuple: `[(Species::Human, "Human"), ...]`
- [ ] The `character_appearance_editor` dropdown now cycles `Species` enum variants, not string names

### 3. Update character creation

- [ ] In `character_creation.rs`:
  - Replace `creation.look.species = SPECIES_NAMES[i].to_string()` with `creation.look.species = SPECIES[i]` where `SPECIES` is an array of `Species` values
  - In `randomize_step`: derive species from the `rng` index into the `SPECIES` array
  - In `build_player_soul`: the `species_name` match already exists — use it to set `species` directly
- [ ] The `IdentityDraft` already uses `species: usize` (index). No change needed there.

### 4. Update identity.rs

- [ ] In `reachlock-core/src/identity.rs`: `PlayerCharacter.look: CharacterLookConfig` — no code change needed, just ensure the type propagates
- [ ] Any code that reads `player_char.look.species` as a string must be updated

### 5. Update bridge/rendering code

- [ ] In `bridge.rs` or wherever sprites are rendered: any `if look.species == "Robot"` must change to `if look.species == Species::Robot`

### 6. Run determinism checks

- [ ] `cargo run -p reachlock-cli -- determinism check` — verify no golden manifests changed
- [ ] If the sprite generator output changes (because the enum serializes differently), recapture goldens following the CLI instructions

### 7. Test

- [ ] All existing character creation tests pass
- [ ] Editor character sprite viewer still works
- [ ] Species-specific rendering (Robot chassis, Voidborn bioluminescence) still activates correctly

## Acceptance gates

```bash
cargo test -p reachlock-core
cargo test -p reachlock-editor
cargo test -p reachlock-client
cargo clippy -- -D warnings

# Determinism gate (if golden changed)
cargo run -p reachlock-cli -- determinism check

make check
```

## Non-goals

- Changing `IdentityDraft.species` from `usize` to `Species` (it's an index for the creation flow — fine as-is)
- Adding new species variants
- Changing the sprite generator's species-specific logic

## Gotchas

- **`CharacterLookConfig` is a core type.** It lives in `reachlock-core`. Changing its field type affects the editor, the client, and the CLI. Compile all three crates (`cargo build --workspace`) after the change.
- **Serialization round-trip.** `Species` is `#[derive(Serialize, Deserialize)]`. RON serializes enum variants as `Human`, `Android`, etc. — the same as the old string values `"Human"`, `"Android"`. Existing RON files should load correctly. Verify with a test:
  ```rust
  #[test]
  fn species_enum_round_trips_in_look_config() {
      let cfg = CharacterLookConfig { species: Species::Robot, ..Default::default() };
      let ron = ron::to_string(&cfg).unwrap();
      let back: CharacterLookConfig = ron::from_str(&ron).unwrap();
      assert_eq!(back.species, Species::Robot);
  }
  ```
- **`seed_derived` callers.** Find every call site of `CharacterLookConfig::seed_derived(...)`. Change the argument from `&str` to `Species`. The caller is typically in `character_creation.rs` or `crew.rs` — both have easy access to `Species` since they already do `match species_name { ... }` somewhere.
- **`PlayerCharacter` serialization in save files.** `PlayerCharacter` includes `look: CharacterLookConfig` which now has `species: Species` instead of `species: String`. Old save files with `species: "Human"` may not deserialize. Test with an existing save file. If it breaks, add a custom `Deserialize` that accepts both enum variants and string representations.
