# S78 — The Creation Flow

**Spec:** New (character creation UI, New Game / Continue split) ·
**Wave D (character & open world) · Depends on:** S70 (client UI framework), S75 (player identity in core), S76 (one appearance pipeline), S77 (decouple the Loup-Garou)

**Closes:** X2 (no New Game), C4(part) (main menu: New Game vs Continue)

## Outcome

The main menu offers a real **New Game** that enters a six-step character creation flow (Identity → Appearance → Origin → Ship & crew → Galaxy seed → Confirm) before launching into the game world. **Continue** loads an existing save directly — `load_save` is split out of `Startup` so the two paths diverge. Each step is skippable with a "Randomize" button that delegates to the existing seeded generators (sprite, soul, ship) so every screen has a valid one-click answer. Full keyboard + mouse + gamepad input per S70/S71. The character created here persists through save, reload, and play.

## Context

- **The player has no identity (X1, S75 closes).** `SaveFile` now holds a `PlayerCharacter` with name, pronouns, species, `CharacterLookConfig`, origin/background id, and a `SoulFile`. That identity exists in core. This sprint surfaces it.
- **No New Game exists (X2).** `AppState` is `MainMenu | InGame`. `load_save` runs in `Startup` (`main.rs:218-221`), chain-loaded before the menu renders. Every launch loads the same save — there is no way to start fresh.
- **S75 froze the contract.** `PlayerCharacter` is a `SaveFile` field (`Option` for migration). The wire shape carries name/species/look. `CharacterLookConfig` is the canonical appearance type. This sprint reads those shapes but does not change them.
- **S76 unified appearance.** `pixel::Look` derives from `CharacterLookConfig`. `crew_look()` is gone. The `SpriteViewer`'s controls exist as a reusable widget. This sprint embeds that widget in the Appearance step.
- **S77 decoupled the Loup-Garou.** `CrewRoster` is data-driven from a crew package. `deck_of()`/`deck_zero_g()` use the live `ShipInterior`. Starting location comes from the origin package. The `include_str!` is gone from `reachlock-core`. The Loup-Garou is one authored origin among many — this sprint's Origin step lists it as an option.
- **The menu currently shows a seed it won't let you edit** (`menu.rs`). Character creation surfaces the galaxy seed (step 5) as a Typable, shareable string. "The seed IS the game" becomes true for the player — they can type a friend's seed and see the same galaxy.
- **Offline-first:** every step works identically with no server. The galaxy seed step works offline (seeds are local); origin content comes from local `mods/`.

## Freeze first

### `AppState::CharacterCreation` variant

```rust
// reachlock-client/src/states.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, States)]
pub enum AppState {
    #[default]
    MainMenu,
    CharacterCreation,  // NEW — entered from New Game
    InGame,
}
```

Transition: `MainMenu → CharacterCreation` on "New Game". `CharacterCreation → InGame` on "Confirm". `CharacterCreation → MainMenu` on "Cancel" (or Esc from any incomplete step).

### Character creation step enum

```rust
// reachlock-client/src/systems/character_creation.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CreationStep {
    Identity,
    Appearance,
    Origin,
    ShipAndCrew,
    GalaxySeed,
    Confirm,
}

impl CreationStep {
    /// How many steps total (for progress indicator).
    pub const COUNT: usize = 6;

    /// Index in the step sequence (0-based).
    pub fn index(self) -> usize { self as usize }

    /// Next step, if not the last.
    pub fn next(self) -> Option<Self> { … }

    /// Previous step, if not the first.
    pub fn prev(self) -> Option<Self> { … }
}
```

### Character creation state resource

```rust
#[derive(Resource, Default)]
pub struct CharacterCreationState {
    pub step: CreationStep,
    pub identity: IdentityDraft,
    pub look: CharacterLookConfig,
    pub origin_id: Option<String>,
    pub ship_seed: Option<Seed>,
    pub galaxy_seed: String,
}

#[derive(Default)]
pub struct IdentityDraft {
    pub name: String,
    pub pronouns: String,
    pub species: Species,  // Human / Android / Robot / Voidborn / Xenotype
}
```

### `load_save` split contract

- `Startup` system at `main.rs:218` runs `load_content_index` → `init_souls` (via content dispatcher) but **no longer** calls `load_save`.
- `load_save` runs on the **Continue** path: `AppState::MainMenu → Continue button → load_save → AppState::InGame`.
- `New Game` path: `AppState::MainMenu → New Game button → AppState::CharacterCreation`. At Confirm, the save is initialised with the player character, the chosen galaxy seed, and the origin's starting conditions. No `load_save` call.

## Deliverables

### 1. New Game / Continue split (`main.rs`, `menu.rs`, `states.rs`, `inventory.rs`)

- [ ] Add `AppState::CharacterCreation` to the `AppState` enum in `states.rs`. Add `CharacterCreation` to all state-scoped systems that need it (e.g., camera, input, pause block).
- [ ] Remove `load_save` from the `Startup` chain in `main.rs:218-221`. It currently runs after `init_souls`. Replace with a no-op or remove the line.
- [ ] Move `load_save` into a system triggered by the **Continue** action:
      ```rust
      fn continue_game(
          mut commands: Commands,
          save: Res<Option<SaveFile>>,
          mut next_state: ResMut<NextState<AppState>>,
      ) {
          if save.is_some() {
              load_save(&mut commands, …);
              next_state.set(AppState::InGame);
          }
      }
      ```
- [ ] **New Game** button in `menu.rs` transitions to `AppState::CharacterCreation`. Creates a fresh `CharacterCreationState` resource (defaults).
- [ ] **Continue** button: greyed out / disabled if no save file exists (queried at menu open). On click, calls `load_save` and transitions to `InGame`.
- [ ] Settings/Quit buttons remain unchanged from S70.
- [ ] Test: `AppState::MainMenu → New Game → CharacterCreation`. `AppState::MainMenu → Continue → InGame` (with existing save). No save → Continue disabled.

**Gate:** `make check`. Launch game → New Game transitions to creation flow. Launch game with existing save → Continue loads directly into game.

### 2. Identity step (`character_creation.rs`)

- [ ] **Name:** TextInput widget (from S70). Placeholder "Captain's name…". Max 32 chars. Hex-only galaxy seed names noted in tooltip.
- [ ] **Pronouns:** Dropdown (they/them, she/her, he/him, it/its, xe/xem, custom). Custom opens a TextInput.
- [ ] **Species:** Radio-style card selector showing Human, Android, Robot, Voidborn, Xenotype. Each card has a short lore blurb (from `editor/ai.rs:91-99`). Selecting a species updates the preview look (step 2 preview panel reflects species choice).
- [ ] **Randomize** button: generates random name (from a name table or seeded), random pronouns, random species. Uses `SeededRng` from the galaxy seed so "Randomize" with the same seed produces the same identity.
- [ ] **Validation:** name non-empty. If empty, the Next button is disabled with tooltip "Enter a captain name".
- [ ] Navigation: Next advances to Appearance. Previous goes to main menu (confirm dialog "Discard character?" with Yes/No modal per S70).

**Gate:** Enter name → select species → Next → appearance step shows. Randomize → fields populate.

### 3. Appearance step (S76 widget embedded)

- [ ] Embed the `SpriteViewer`'s live-preview controls as a reusable bevy_ui widget. The S76 unification delivered this: a `CharacterLookConfig` editor component that renders sliders/swatches for hair, skin, shirt, pants, jacket, chassis, visor colours, plus hair style selector.
- [ ] Live preview on a **walk cycle** animation: a small sprite viewport (200×200 px) showing the character's walk animation with current look applied. The `generate_character_sprite` function produces frame data; this sprint renders it as an animated bevy_ui `ImageNode` (or a minimal sprite animation loop).
- [ ] Previous step updates the preview: species selection from step 1 changes the base body type rendered here.
- [ ] **Randomize** button: re-rolls all appearance parameters from the seed. A new look appears in the preview. Randomize does not change the species or identity from step 1.
- [ ] **Reset** button: returns to the original randomised look (the one from first entering the step).
- [ ] Navigation: Next → Origin. Previous → Identity (preserves all selections).

**Gate:** Hover species from step 1 → appearance preview updates. Randomize → new look in preview. Walk cycle plays.

### 4. Origin step — background card selector

- [ ] Query `OriginRegistry` (populated by content dispatcher from S81 or directly from `mods/`) for available origins. Each origin is rendered as a **card** showing:
  - Origin name and icon (faction icon or career path glyph)
  - Starting career path and rank
  - Faction standing deltas (positive AND negative)
  - Starting credits
  - Ship template name and class
  - Crew count (how many crew the origin brings)
  - Known systems / starting location name
  - One-line flavour text ("A Compact deserter with a grudge and a rustbucket")
- [ ] Selected origin card has a highlight border. Selecting a different origin updates the visible card.
- [ ] A summary panel to the side (or below) lists **what you gain** and **what you lose** (closed doors: `conflicting_paths` that this origin's career blocks).
- [ ] **Randomize** button: picks a random origin from the available set. Does not re-randomize on subsequent clicks unless the player clicks again.
- [ ] Navigation: Next → Ship & crew. Previous → Appearance.

**Gate:** At least one origin loads (the "Loup-Garou veteran" from S77). Select origin → summary shows grants/conflicts. Randomize → different origin selected. Next → ship & crew step shows the origin's starting vessel.

### 5. Ship & crew step

- [ ] Shows the starting vessel from the selected origin: ship class, hull seed preview (a rendered sprite or stats card), and any crew the origin brings.
- [ ] If the origin grants no ship, show a default starter ship option.
- [ ] **Re-roll ship seed:** a button that cycles the `ship_seed` and regenerates the hull preview. The ship's class remains the same (dictated by origin) but visual variant/procedural details change.
- [ ] Crew list: shows any crew the origin packages. Each crew member shown as a small card: name, species, role, one-line trait. No editing here — this is informational. Crew recruitment is S80.
- [ ] Navigation: Next → Galaxy seed. Previous → Origin.

**Gate:** Ship preview renders. Re-roll changes hull appearance. Crew list matches origin definition.

### 6. Galaxy seed step

- [ ] Prominent TextInput showing the galaxy seed. Pre-filled with a fresh random seed on first entry. The seed determines the entire procedural galaxy — systems, stations, factions, encounters.
- [ ] **Why the seed matters** — a short blurb: "The seed IS the galaxy. Share this with a friend to explore the same stars."
- [ ] Validation: must be a valid `Seed` (≤ 2^53 integer, or hex string). Invalid input shows a red error border.
- [ ] **Randomize** button (or "New Seed"): generates a new random seed. The identity and appearance don't change — only the galaxy seed.
- [ ] Navigation: Next → Confirm. Previous → Ship & crew.

**Gate:** Seed displays. Edit → type invalid → error shown. Randomize → new seed. Next → confirm step.

### 7. Confirm step — summary card

- [ ] A scrollable summary showing the entire opening position:
  - **Identity:** name, pronouns, species
  - **Appearance:** small portrait sprite (static frame from walk cycle)
  - **Origin:** name, career path, rank, starting credits
  - **Ship:** class, hull preview
  - **Crew:** count and names
  - **Galaxy seed:** shown as hex
  - **Starting location:** system/station name from origin
- [ ] **Launch** button (primary action): finalises character creation.
  - Creates a `SaveFile` with the `PlayerCharacter` populated.
  - Applies origin conditions: sets career path + rank, applies faction deltas, grants credits, equips starting gear, positions the player at the origin's starting location.
  - Generates the galaxy from the seed (calls `generate_system` if not cached).
  - Transitions to `AppState::InGame`.
- [ ] **Back** button: returns to Galaxy seed step. All choices preserved.
- [ ] **Cancel** button: returns to Main Menu with "Discard this character?" confirmation modal.
- [ ] Keyboard: Enter launches. Esc goes back. Shift+Esc cancels to menu.

**Gate:** Launch → game starts at the correct system with correct ship, credits, crew, career, faction standing. Save → reload → character data fully intact.

### 8. Persistence — character survives save/reload

- [ ] On Launch (Confirm step), write a `SaveFile` with the full `PlayerCharacter` and the galaxy seed.
- [ ] `load_save` (Continue path) restores the `PlayerCharacter` into a `Res<PlayerCharacter>` or `Res<SaveFile>`.
- [ ] The HUD/UI reads from this resource for the captain's name, species, and appearance in dialogue panels, crew interactions, and the character sheet.
- [ ] Test: create character → save → exit → Continue → verify all identity fields, appearance, origin, ship seed, galaxy seed match on reload.

**Gate:** `create → play → save → reload → character persists intact`.

### 9. Step progress indicator

- [ ] A horizontal step indicator at the top of the creation screen: 6 circles/dots, current step highlighted, completed steps filled. Clickable to jump back (except Galaxy seed → Confirm changes, which needs a "changes to seed will regenerate the galaxy" confirm).
- [ ] The indicator shows step labels on hover (tooltip per S70).

## Acceptance gates

```
# New Game / Continue split
cargo test -p reachlock-client states::app_state_transitions

# Creation flow step navigation
cargo test -p reachlock-client character_creation::step_navigation
cargo test -p reachlock-client character_creation::identity_validation
cargo test -p reachlock-client character_creation::randomize_all_steps

# Origin selection (requires S81 content dispatch or direct lookup)
cargo test -p reachlock-client character_creation::origin_selection

# Galaxy seed validation
cargo test -p reachlock-client character_creation::seed_validation

# Persistence round-trip
cargo test -p reachlock-client character_creation::save_reload_roundtrip

make check
```

Manual:
1. Launch → main menu shows New Game + Continue → New Game → Identity step → fill name → Next → Appearance → randomize → Next → Origin → select "Loup-Garou veteran" → Next → Ship & crew → preview renders → Next → Galaxy seed → enter a friend's seed → Next → Confirm → summary shows all choices → Launch → game starts at Aethon with the Loup-Garou and canonical crew
2. Save → exit → Continue → character name, appearance, ship, location identical
3. New Game → Cancel at any step → confirm dialog → returns to main menu
4. Continue with no save → button is greyed out

## Non-goals

- Character re-creation mid-game / appearance change clinic — future sprint
- Full character sheet UI in-game (the identity readout in HUD is minimal — a full sheet with biography, stats, and history is separate)
- Crew recruitment or management (S80)
- Origin creation or editing (S79)
- Multiplayer remote-character rendering (deferred to Wave E)
- Tutorial or onboarding (S72)
- Accessibility pass beyond S70/S71 baseline (contrast, colourblind-safe faction colours — S71)

## Gotchas

- `load_save` removal from `Startup` is the riskiest single change. It currently runs after `init_souls` and before the menu. Any system that assumes save data is available at startup will break. Audit every `Startup` system in `main.rs` for `Res<SaveFile>` or `Res<Option<SaveFile>>` reads. The pattern after this sprint: save data is available only AFTER Continue is clicked. New Game creates a fresh save from scratch.
- The `CharacterCreationState` resource is created on New Game and despawned (or reset) on Launch/Cancel. Systems in `InGame` must not assume it exists — it is only valid during `CharacterCreation` state.
- Galaxy seed step: changing the seed after already having confirmed once (e.g., going back from Confirm to Galaxy Seed) invalidates the generated galaxy. If the player changes the seed and returns to Confirm, the generated systems from the old seed are stale. Either regenerate on Confirm click (may cause a delay) or warn "Changing the seed will regenerate the entire galaxy" with a confirmation modal. Recommend the latter — regeneration is expensive.
- The origin step reads from `OriginRegistry` which is populated by the content dispatcher (S81). If S81 hasn't shipped yet, populate it with a hardcoded single origin ("Loup-Garou veteran") for development. Document this in the PR: "Temporary hardcoded origin pending S81 content dispatch."
- The walk-cycle preview in the Appearance step requires the `generate_character_sprite` animation frames. The core generator produces individual frames; this sprint must drive a simple timer-based animation loop in bevy_ui. If bevy_ui `ImageNode` doesn't support animation, use a `TextureAtlas` sprite on a separate camera layer overlaid on the creation UI.
- Step indicator clickable to jump back: changing Identity after seeing Confirm is fine (no side effects). Changing Galaxy Seed after Confirm invalidates galaxy data — gate this with a modal. Changing Origin after Ship & crew may change the ship — the Ship step must re-read the origin and update its display. The `CharacterCreationState` resource is mutable at every step; the Confirm step reads it as-is. No step "commits" until Launch.
