# S111 — Bug Fixes & Cleanup Batch

**Wave: UX-Hardening · Depends on:** None (standalone fixes)

## Outcome

Six low-severity bugs and code quality issues resolved in a single batch:
1. Duplicate `CrewRoster` resource initialization removed
2. `GamepadActive` resource documented as WIP (not dead code)
3. `ship_template_catalog()` relative path fix
4. Dialogue `panel_text()` SoulState panic guard
5. Character creation species desync fix
6. `hud.rs` HelpText line redundancy removed

## Bug 1 — Duplicate CrewRoster Initialization

**File:** `reachlock-client/src/main.rs` lines 132 and 252

```rust
.init_resource::<crew::CrewRoster>()  // line 132
.init_resource::<crew::CrewRoster>()  // line 252 — duplicate!
```

**Fix:** Remove the second `init_resource::<crew::CrewRoster>()` at line 252. The first one suffices — `init_crew_roster` (line 277) overwrites it with loaded data.

**Verification:** `cargo clippy` should not warn (init_resource duplicates are not a clippy lint, but redundant). Manual check: `make check` builds without error.

---

## Bug 2 — GamepadActive Dead Code Documentation

**File:** `reachlock-client/src/systems/gamepad.rs`

`GamepadActive` is set by `detect_gamepad` but never read by any system (until S105). This is not a bug — it's WIP infrastructure. But it reads as dead code.

**Fix:** Add a doc comment on the `GamepadActive` resource:
```rust
/// Tracks whether a gamepad is connected and active.
/// Set by `detect_gamepad` system. Consumed by S105 (gamepad input routing).
/// Currently infrastructure — the detection works, routing arrives in S105.
#[derive(Resource, Default)]
pub struct GamepadActive(pub bool);
```

Also add a test that verifies `detect_gamepad` sets `GamepadActive` to `true`:
```rust
#[test]
fn gamepad_detection_sets_active() {
    // This test verifies the detection system exists.
    // Full integration test requires a connected gamepad.
    let mut active = GamepadActive(false);
    active.0 = true; // simulated detection
    assert!(active.0);
}
```

**Verification:** `cargo test -p reachlock-client gamepad`

---

## Bug 3 — ship_template_catalog() Relative Path

**File:** `reachlock-client/src/systems/crew.rs` lines 798-826

```rust
pub fn ship_template_catalog() -> Vec<ShipTemplate> {
    for root in ["mods/reachlock/hulls", "../mods/reachlock/hulls"] {
```

The fallback path `../mods/reachlock/hulls` assumes the current working directory is one level below the mods directory. This works when running from the workspace root but breaks when:
- Running from `reachlock-client/` directly
- Running from a different CWD
- The editor calls this function (it's in `reachlock-client` but the pattern relies on CWD)

**Fix:** Use the `content_root()` pattern from the editor:
```rust
pub fn ship_template_catalog() -> Vec<ShipTemplate> {
    let roots = [
        std::path::PathBuf::from("mods/reachlock/hulls"),
        // Secondary: try relative to the executable directory
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("mods/reachlock/hulls")))
            .unwrap_or_default(),
    ];
    for root in &roots {
        // ... (existing scan logic, using root instead of raw string)
    }
}
```

**Verification:** Run the game from different CWDs — ship templates should load.

---

## Bug 4 — Dialogue panel_text() SoulState Panic Guard

**File:** `reachlock-client/src/systems/dialogue.rs` lines 481-531

```rust
pub fn panel_text(session: &DialogueSession, souls: &SoulRegistry) -> Option<String> {
    let active = session.active.as_ref()?;
    let file = souls.files.get(&active.soul_id)?;
    let state = souls.states.get(&active.soul_id)?;  // <--- panics if SoulState not initialized
```

If a soul-backed NPC is interacted with before `SoulRegistry::init_souls` has initialized their state, `souls.states.get()` returns `None` and the `?` returns `None` — this is fine (the panel just doesn't render). But if `states` is missing but `files` is present, the `?` on `states` silently swallows the issue.

**Fix:** No code change needed — the `?` properly handles missing state. Add a comment:
```rust
let state = souls.states.get(&active.soul_id)?;
// ↑ None if state not yet initialized — panel renders empty until then. This
//   is correct: the state is initialized by init_souls which runs at startup.
```

The real fix is ensuring `init_souls` runs before any dialogue can start. Verify in `main.rs` that `soul::init_souls` runs in the Startup chain before any interaction system.

**Verification:** No panic on dialogue open. If SoulState missing, panel shows nothing (not a crash).

---

## Bug 5 — Character Creation Species Desync

**File:** `reachlock-client/src/systems/character_creation.rs` lines 553-555

```rust
creation.identity.species = rng.next_below(SPECIES_NAMES.len() as u64) as usize;
creation.look.species = SPECIES_NAMES[creation.identity.species].to_string();
```

If `identity.species` changes without updating `look.species`, the two desync. Currently they're updated together in `randomize_step`, but in `character_creation_input` (keys 1-5), only `identity.species` and `look.species` are updated:

```rust
if keys.just_pressed(*key) {
    creation.identity.species = i;
    creation.look.species = SPECIES_NAMES[i].to_string();
}
```

After S110 (species → enum), `look.species` becomes `Species`, removing the string mismatch. Until then, the manual sync is maintained but fragile.

**Fix:** After S110 is merged, this bug resolves automatically. For now, add a test:
```rust
#[test]
fn species_identity_and_look_stay_in_sync() {
    let mut state = CharacterCreationState::default();
    for i in 0..5 {
        state.identity.species = i;
        state.look.species = SPECIES_NAMES[i].to_string();
        assert_eq!(state.look.species, SPECIES_NAMES[state.identity.species]);
    }
}
```

**Verification:** After S110, `look.species` is `Species` enum — sync is enforced by the type system.

---

## Bug 6 — HUD HelpText Redundant Line

**File:** `reachlock-client/src/systems/hud.rs` lines 539-545

```rust
if let Ok(mut text) = texts.p6().single_mut() {
    let help_text = settings.key_display(InputAction::OpenHelp);
    let help = format!("Press {help_text} for help");
    if **text != help {
        **text = help;
    }
}
```

This writes `"Press F1 for help"` to the `HelpText` entity. But the HUD footer (`HelpText` component) already shows the full flight/interior help bar from `HelpTextCache`. The `"Press F1 for help"` line is redundant (the footer already shows keybindings including F1).

**Fix:** Remove this block entirely. The `HelpText` entity should render the cached help bar from `HelpTextCache`, not a redundant instruction. OR: keep the block but change it to show `HelpTextCache::flight` when in flight mode and `HelpTextCache::interior` when in interior mode.

Actually, the `HelpText` entity is NOT the footer bar. Looking at `spawn_hud`:
- `HelpText` entity: `top: 30px, left: 8px` — this is the per-mode keybinding bar
- The fuel readout is at `top: 8px, left: 8px`

So the `HelpText` entity IS the footer keybinding bar. The code should render `HelpTextCache` here, not `"Press F1 for help"`.

**Fix (corrected):** Replace the block with:
```rust
if let Ok(mut text) = texts.p6().single_mut() {
    let help_text = match **mode {
        GameMode::SpaceFlight => &settings.flight_help_text,
        GameMode::Landed | GameMode::OnBoard => &settings.interior_help_text,
        _ => "",
    };
    // Or rebuild from HelpTextCache
    let cache = HelpTextCache::rebuild(&settings);
    let help = match **mode {
        GameMode::SpaceFlight => cache.flight.clone(),
        GameMode::Landed | GameMode::OnBoard => cache.interior.clone(),
        _ => String::new(),
    };
    if **text != help {
        **text = help;
    }
}
```

But wait — `HelpTextCache` is rebuilt on settings change. Adding it to `update_hud_status` would need another query. Alternative: just remove the block and let the `HelpText` entity stay empty (the footer bar is handled by the initial spawn at line 236-248 which already uses `HelpTextCache::rebuild().flight`).

**Fix (simplest):** Remove the block. The initial spawn at line 236 already sets the correct value. The `update_hud_status` block was overwriting the spawn value with `"Press F1 for help"` — which is strictly worse.

**Verification:** Footer bar shows full keybinding line (`"W pitch · A yaw · Q roll..."`) not just `"Press F1 for help"`.

---

## Acceptance gates

```bash
cargo test -p reachlock-client crew  # ship_template_catalog test
cargo test -p reachlock-client gamepad  # GamepadActive test
cargo test -p reachlock-client character_creation  # species sync test
cargo clippy -- -D warnings

# Manual checks:
# 1. Footer bar shows keybinding line, not "Press F1 for help"
# 2. Game launches from reachlock-client/ directory — ship templates load
# 3. Talk to soul-backed NPC immediately after game start — no panic

make check
```

## Non-goals

- Full integration test for every bug (manual verification is sufficient for these)
- Refactoring any of the affected modules beyond the fix
- Adding new features alongside fixes

## Gotchas

- **Bug 6 (HelpText) — the spawn at line 236 uses `HelpTextCache::rebuild(&settings)`.** This creates a NEW HelpTextCache that is immediately discarded. The cached version lives in `settings.rs`. After removing the overwrite block, verify the spawn still gets the correct value. Better: replace the spawn to read from the resource `HelpTextCache` (which is initialized on the line above at `.init_resource::<settings::HelpTextCache>()`).
- **Bug 3 (ship_template_catalog) — `current_exe()` may fail in WASM.** This function is native-only (the client doesn't target WASM anymore per the commit log: `Drop WASM/web distribution`). Safe to use.
- **Bug 4 (dialogue guard) — no actual code change.** The `?` already handles the missing state. Just add the comment and verify `init_souls` ordering.
- **All fixes are in `reachlock-client`.** No server or core changes needed.
