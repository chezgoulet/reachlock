# S75 — Player Identity in Core

**Spec:** New (PlayerCharacter, SoulFile revision, SaveFile extension, wire-shape update) ·
**Wave D (character & open world)** · **Depends on:** — (standalone, no UI dependency)

## Outcome

The player has an identity that the engine knows about at all. A `PlayerCharacter` struct lives in `reachlock-core` holding id, name, pronouns, species, `CharacterLookConfig`, origin/background id, and a `SoulFile`. The `SaveFile` carries `character: Option<PlayerCharacter>` so existing saves migrate gracefully. The presence wire shape includes name/species/look so remote peers can render the player as a person rather than a ship. Determinism goldens cover appearance and soul generation for the new types. This is a protocol revision (iron rule #4): the wire-shape test and golden manifest are updated with deliberate commits.

**Closes:** X1 (the player has no identity)
**Also closes:** — SS3.1 decision (player gets a soul, yes)
**Part of Wave D — start-now, no dependencies**

## Context

- **X1 (Critical):** The player has zero identity. `SaveFile` (`inventory.rs`) holds inventory, location, universe, soul states, hull config, interior layout — no character at all. The wire (`core/network/messages.rs`) carries `player_id: String` and nothing else. Presence (S23) syncs remote *ships*, never remote *people*. The avatar is a hardcoded lookup (`interior.rs:543-553`).
- **Section 3.1 decision:** The player gets a soul. This is the highest-leverage design decision in the plan — NPCs form persistent relationships with the player, co-deliberation gains a real participant, crew can have breaking points *about the player's choices*, and the trope/dilemma/storyline engines get a subject to reference. Cost: `SoulFile` is a frozen wire shape, so the revision is deliberate and done in this sprint before anything depends on the old shape.
- **The player character is separate from SoulFile** — `PlayerCharacter` wraps a soul rather than being a variant of it. This keeps SoulFile's wire shape stable for NPC interoperabity. The new composite `PlayerCharacter` is a separate frozen shape with its own wire test.
- **Offline-first:** Everything works identically with no server. The player character is persisted in the local save. Online adds presence broadcasting of the new fields.
- **Generators are involved:** Appearance generation (`CharacterLookConfig → CharacterSprite`) and soul generation (`generate_soul`) must have determinism goldens extended to cover the new round-trips (iron rule #3).

## Freeze first

### `PlayerCharacter` struct (`reachlock-core/src/identity.rs`)

New file. The player is a person — name, pronouns, a body, a past, and a soul like everyone else.

```rust
/// The player character's identity. Not a SoulFile variant — a wrapper
/// that pairs an identity record with a full soul, so SoulFile's wire
/// shape stays stable for NPC interop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerCharacter {
    pub id: EntityId,
    pub name: String,
    pub pronouns: String,           // "they/them", "she/her", etc.
    pub species: String,            // Human, Android, Robot, Voidborn, Xenotype
    pub look: CharacterLookConfig,
    pub origin_id: String,          // key into the origin catalog (S79)
    pub background_id: String,      // key into the background catalog
    pub soul: SoulFile,
}

/// EntityId is a newtype over u64 with serialization that matches the
/// existing soul entity id scheme. Reuse the soul module's id type.
```

Add `pub mod identity;` to `reachlock-core/src/lib.rs`.

### `SaveFile.character` field (`reachlock-client/src/systems/inventory.rs`)

```rust
pub struct SaveFile {
    // … existing fields …
    pub character: Option<PlayerCharacter>,
    // … existing fields …
}
```

`Option` so saves from before S75 deserialize with `character: None` and the game
presents the character-creation flow (S78) as a first-time setup path.

### Presence wire shape update (`reachlock-core/src/network/messages.rs`)

```rust
/// Presence announcement — broadcast by servers to all peers when a player
/// enters or leaves a system. Previously only carried player_id + ship state.
/// Now also carries the player's visible identity for remote rendering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PresenceMessage {
    pub player_id: String,
    pub name: Option<String>,       // None if not yet created
    pub species: Option<String>,
    pub look: Option<CharacterLookConfig>,
    pub ship: ShipPresence,         // existing
}

impl PresenceMessage {
    pub fn new_from_character(character: &PlayerCharacter, ship: ShipPresence) -> Self {
        Self {
            player_id: character.id.to_string(),
            name: Some(character.name.clone()),
            species: Some(character.species.clone()),
            look: Some(character.look.clone()),
            ship,
        }
    }
}
```

Update the existing `wire_shape_presence_message` test to include the new optional
fields — this is a protocol revision per iron rule #4.

### Determinism goldens extension (`reachlock-core/src/determinism.rs`)

- Golden entry: `generate_character_sprite` with a pinned `CharacterLookConfig` and seed, asserting the output `CharacterSprite` pixel buffer is bit-identical.
- Golden entry: `generate_soul` with a pinned seed and species, asserting `SoulFile` fields are stable. Add a second golden for the same seed + species + appearance pin to cover player-character soul generation.
- Golden entry: round-trip serialize/deserialize `PlayerCharacter` through RON and JSON (the two serialization formats used by content and wire respectively).

## Deliverables

### 1. `reachlock-core/src/identity.rs` — PlayerCharacter struct

- [ ] Define `EntityId` newtype (`u64`) with `Serialize`/`Deserialize`/`Clone`/`Copy`/`Debug`/`PartialEq`/`Eq`/`Hash`. Match the existing soul entity id scheme.
- [ ] Define `PlayerCharacter` with all fields from Freeze first. Derive `Serialize`, `Deserialize`, `Clone`, `Debug`, `PartialEq`.
- [ ] Add `pub mod identity;` to `lib.rs`.
- [ ] Add a `new` constructor that takes all fields and a `from_soul(entity_id, soul, name, pronouns, species, look, origin_id, background_id)` convenience constructor.
- [ ] Implement `Default` with placeholder values (for test use — "Rook", "they/them", species "Human", random seed `Seed::new(0)`, etc.).
- [ ] Unit tests: `PlayerCharacter` round-trips through RON and JSON. `EntityId` serializes as a JSON number ≤ 2^53.

### 2. SoulFile revision — PlayerCharacter compatibility

- [ ] **Decision check:** If the team chooses to make `PlayerCharacter` a `SoulFile` variant instead of a wrapper (alternative in §3.1), add `PlayerCharacterVariant` to `SoulFile` and update the wire-shape test tag comment. If the team chooses the wrapper approach (recommended in `CHARACTER-CREATION-PLAN.md` §3.1), no change to `SoulFile` — `PlayerCharacter` is its own struct. Either way, the decision must be recorded in a comment in `identity.rs:1`.
- [ ] Document the decision boundary in `identity.rs` module doc: "PlayerCharacter is a wrapper, not a SoulFile variant, because SoulFile's wire shape is frozen for NPC interop. The player's soul is accessible at `pc.soul`."

### 3. SaveFile extension — `character` field

- [ ] Add `character: Option<PlayerCharacter>` to `SaveFile` in `reachlock-client/src/systems/inventory.rs`.
- [ ] Add migration path: `SaveFile::load` deserializes with serde default for `character` (`None`). The save version field is not bumped — `Option` is backward-compatible.
- [ ] Update `SaveFile::default()` to include `character: None`.
- [ ] Update the save/load round-trip test to include a `PlayerCharacter` in the save and assert it survives.
- [ ] A test that a pre-S75 save (no `character` field) deserializes with `character: None` and the game continues without crashing.

### 4. Presence wire shape — name/species/look

- [ ] Update `PresenceMessage` in `reachlock-core/src/network/messages.rs` with optional `name`, `species`, and `look` fields.
- [ ] Add `new_from_character` constructor.
- [ ] Update the existing `wire_shape_presence_message` test: include a `PresenceMessage` with `name: Some("Rook".into())`, `species: Some("Human".into())`, `look: Some(…)` and verify the serialized bytes match. This is the protocol revision — tag the commit message accordingly.
- [ ] Update the `wire_shape_presence_message` test assertion for the `name: None` case too, to prove backward compatibility.
- [ ] Client-side: when sending a `PresenceMessage` in online mode, populate the three new fields from `SaveFile.character` if present.

### 5. Determinism goldens

- [ ] Add a new golden file `determinism/goldens/appearance_player.ron` — pinned `CharacterLookConfig` + seed, expected `CharacterSprite`.
- [ ] Add a new golden file `determinism/goldens/soul_player.ron` — pinned seed + species + look pin, expected `SoulFile`.
- [ ] Add golden for `PlayerCharacter` RON round-trip and JSON round-trip.
- [ ] Register all three in the determinism manifest (`determinism.rs` entries array) so `make check` + cross-platform CI runs them.
- [ ] Rerun `cargo test -p reachlock-core determinism::` and commit the golden files. The commit message must say "S75: add player-identity goldens — manifest changed."

## Acceptance gates

```
cargo test -p reachlock-core identity::          # PlayerCharacter unit tests pass
cargo test -p reachlock-core network::messages::wire_shape  # presence message protocol test
cargo test -p reachlock-core determinism::         # goldens pass (3 new entries)
cargo test -p reachlock-core determinism::regenerate  # regression goldens match
cargo test -p reachlock-client inventory::         # SaveFile round-trip with character
# Manual: deserialize a pre-S75 save (remove character field from a .ron save)
#       → `character: None`, game runs normally
cargo run -p reachlock-client                      # existing saves load without character
make check
```

Manual: load an existing save (no character) → game starts in current state. Load a new save with a `PlayerCharacter` → character persists. Online: two clients in the same system → presence messages carry name/species/look.

## Non-goals

- The creation flow itself (S78 — depends on S70's client UI framework)
- Rendering the player character in-world (S76 handles the appearance pipeline; S78 spawns the character entity)
- Character origin/background mechanics (S79)
- Remote-character rendering in multiplayer (deferred — the wire shape carries the data but no client renders it yet)
- Crew relationships or co-deliberation with the player's soul (S80)
- Save migration UI (a save with `character: None` silently falls through to New Game / character creation)

## Gotchas

- `EntityId` must serialize to a JSON number ≤ 2^53 (JSON float survival — iron rule from the gotcha ledger). The `Seed::new` mask pattern is the precedent: apply the same constraint if `EntityId` is a newtype.
- `SoulFile` is a frozen wire shape (iron rule #4). If the team chooses to add a `PlayerCharacterVariant` to `SoulFile` instead of the wrapper approach, the wire-shape test must be updated AND the commit message must tag it as a protocol revision. The CHARACTER-CREATION-PLAN.md §3.1 recommends the wrapper — if you follow it, SoulFile changes are zero.
- The presence wire shape test (`wire_shape_presence_message`) currently asserts a specific byte string. Adding optional fields shifts the bytes. This is an intentional protocol revision — every test that asserts the serialized form must be updated in the same commit, not deferred.
- `generate_soul` takes a species string. The golden must pin the species to a known value (e.g., `"Human"`). If the species enum in `soul/types.rs` changes, this golden breaks — that is correct and deliberate (the golden catches the change).
- `CharacterLookConfig` is in `core/generator/sprite.rs`. It has optional fields. When building a `PlayerCharacter` from a seed (no explicit look), set all options to `None` so the generator fills them deterministically. The golden test should cover both pinned and seed-derived looks.
- `SaveFile` serializes as RON. The `Optional` field `character` serializes as `character: Some(…)` or `character: None`. Serde's `#[serde(default)]` on the field ensures pre-S75 saves (no `character` key) deserialize to `None`. Test this explicitly — serde's default for `Option<T>` is `None`, but only if the field is absent; if the field is present with a null value, serde rejects it. RON doesn't write null, so the absence case is what matters.
- Branch: `sprint-v2/s75-player-identity-in-core`, cut from `testing`.
