# S93 — Help Mode Overlay

**Wave: UX-Hardening · Depends on:** S31 (Settings/HelpTextCache), S72 (diegetic help toggle)

## Outcome

Pressing F1 shows an overlay with every keybind for the current game mode (SpaceFlight or Interior), rendered as a formatted key→action table. The overlay is dismissable with F1 again or Esc. The previous behavior — spawning a text label that says "Press F1 for help" — is replaced.

## Context

The current help system does exactly one thing:

```rust
// help.rs:34
GameMode::SpaceFlight => vec!["Press F1 for help".into()],
GameMode::Landed | GameMode::OnBoard => vec!["Press F1 for help".into()],
```

This is a tautology. The `HelpTextCache` (from S31) already contains the full per-mode keybinding strings in `flight` and `interior` fields. The HUD footer already renders a condensed version via `HelpText`. The help overlay should show the EXPANDED version — every action, its key, and its group.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/help.rs` | Replace entire file |
| `reachlock-client/src/settings.rs` | `HelpTextCache`, `InputAction::all()`, `InputAction::group()`, `KeyBind::display()` |
| `reachlock-client/src/main.rs` | Registration — already registered |

## Freeze first

### Help overlay data structure

```rust
/// One row in the help overlay.
struct HelpRow {
    action: &'static str,      // "Thrust forward"
    key: String,               // "W"
    group: &'static str,       // "Movement"
}
```

### Per-mode row sets

The overlay shows ALL actions for the current mode, filtered to the groups active in that mode:

| Mode | Groups shown |
|------|--------------|
| SpaceFlight | Movement, Combat, Interaction |
| Landed | Interaction, Landed Combat |
| OnBoard | Interaction |

Plus always: Navigation (F1=help, Esc=pause, etc.)

## Deliverables

### 1. Replace `help.rs` entirely

- [ ] Delete the current implementation (`HelpMode`, `toggle_help_mode`, `spawn_help_labels`, `despawn_help_labels`)
- [ ] Replace with a new implementation:

```rust
use bevy::prelude::*;
use crate::settings::{HelpTextCache, InputAction, KeyBind, Settings};
use crate::states::GameMode;

#[derive(Resource, Default)]
pub struct HelpMode {
    pub active: bool,
}

#[derive(Component)]
pub struct HelpOverlay;

#[derive(Component)]
pub struct HelpOverlayText;

/// Toggle the help overlay on F1, dismiss with Esc.
pub fn toggle_help_mode(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut help: ResMut<HelpMode>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenHelp)) {
        help.active = !help.active;
    }
    if help.active && keys.just_pressed(KeyCode::Escape) {
        help.active = false;
    }
}

/// Spawn/despawn overlay entities.
pub fn sync_help_overlay(
    help: Res<HelpMode>,
    mut commands: Commands,
    overlay_q: Query<Entity, With<HelpOverlay>>,
) {
    if help.active && overlay_q.is_empty() {
        commands.spawn((
            HelpOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.06, 0.92)),
            ZIndex(20),
        ));
        commands.spawn((
            HelpOverlayText,
            Text::new(""),
            TextFont { font_size: 13.0, ..default() },
            TextColor(Color::srgb(0.85, 0.9, 0.95)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(10.0),
                left: Val::Percent(15.0),
                width: Val::Percent(70.0),
                ..default()
            },
        ));
    } else if !help.active && !overlay_q.is_empty() {
        for e in &overlay_q {
            commands.entity(e).despawn();
        }
    }
}

/// Build the per-mode help text.
pub fn render_help_overlay(
    help: Res<HelpMode>,
    mode: Option<Res<State<GameMode>>>,
    settings: Res<Settings>,
    mut query: Query<&mut Text, With<HelpOverlayText>>,
) {
    if !help.active {
        return;
    }
    if let Ok(mut text) = query.single_mut() {
        let mode = mode.map(|m| **m).unwrap_or(GameMode::SpaceFlight);
        let groups = match mode {
            GameMode::SpaceFlight => &["Movement", "Combat", "Interaction"][..],
            GameMode::Landed => &["Interaction", "Landed Combat"][..],
            GameMode::OnBoard => &["Interaction"][..],
            _ => &["Interaction"][..],
        };

        let mut lines = vec![format!("── HELP: {:?} ──", mode)];
        lines.push(String::new());

        for group in groups {
            lines.push(group.to_string());
            for action in InputAction::all() {
                if action.group() == *group {
                    let key = settings.key_display(*action);
                    lines.push(format!("  {:<20} {}", key, action.label()));
                }
            }
            lines.push(String::new());
        }
        lines.push("F1 / Esc — close help".to_string());
        **text = lines.join("\n");
    }
}
```

### 2. Register systems

- [ ] In `main.rs`, replace old help system registrations with:
  - `toggle_help_mode.run_if(in_state(AppState::InGame))`
  - `sync_help_overlay.run_if(in_state(AppState::InGame))`
  - `render_help_overlay.run_if(in_state(AppState::InGame))`

### 3. Push FocusLayer on open

- [ ] In `toggle_help_mode`: when `help.active` switches to true, push `FocusLayer::Tool(...)` or a new `FocusLayer::Help` variant
- [ ] When false, pop the layer

### 4. Remove old HUD "Press F1 for help" line

- [ ] In `hud.rs:539-545` — the `HelpText` query writes `"Press F1 for help"`. After this sprint, remove that line (it's already in the footer help bar; HelpText should show the help bar, not a redundant "press F1" message).
- [ ] Keep `HelpText` for the condensed footer bar — just remove the redundant `"Press F1 for help"` text from the `HelpText` slot (it should still show the condensed keybinding bar from `HelpTextCache`).

### 5. Test

- [ ] Press F1 in SpaceFlight → overlay shows all Movement, Combat, Interaction keys
- [ ] Press F1 in Landed → overlay shows Interaction + Landed Combat keys
- [ ] Press Esc → overlay dismisses
- [ ] Remap a key in settings → help overlay shows the new key

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual: F1 in flight → see "W thrust forward, A strafe left" etc.
# F1 while docked → see "E interact, I inventory" etc.
# Esc closes overlay
# Remap Boost to Tab in settings → F1 shows "Tab Boost"

make check
```

## Non-goals

- Searchable/filterable help entries
- Interactive tutorial mode
- Per-console help (gunner, scanner, etc.)
- Help for the editor (editor already has its own F1 help window)
- Context-sensitive help that highlights only relevant actions based on proximity

## Gotchas

- **`GameMode` is a sub-state of `AppState::InGame`.** On the main menu, `Res<State<GameMode>>` does not exist — use `Option<Res<State<GameMode>>>` (as `in_spaceflight` and `in_any_interior` do).
- **`toggle_help_mode` uses `KeyCode::Escape` directly for dismiss**, not `settings.key(InputAction::Pause)`. This is intentional: Esc dismisses overlay; Esc again pauses. If we used the same action, one Esc would dismiss AND pause simultaneously.
- **Help overlay must render ABOVE panels.** Use `ZIndex(20)` to ensure it draws over factions/discovery/career panels (ZIndex 7) and narrative panels (ZIndex 8).
- **The HUD footer already shows a condensed help bar.** Do not remove it. The overlay is the expanded version shown on demand.
