# S98 — Dialogue Portrait, Mood & History

**Wave: UX-Polish · Depends on:** S16 (Dialogue UI), S13 (Soul system)

## Outcome

The dialogue panel shows the NPC's character portrait, a mood indicator (glyph + color), and scrollable conversation history in addition to the current line. The panel text is reformatted to give each element its own visual hierarchy.

## Context

Current dialogue rendering (`panel_text` in `dialogue.rs:481-531`):

```
Boris — engineer · mood: STABLE

"Hello, traveler."

1. Ask about the station
2. Trade rumors
9. say something else…
```

Missing:
1. **Portrait** — `portrait_id` exists on every `SoulFile` but no sprite is ever rendered
2. **Mood visualization** — mood is a single word (`STABLE`, `ANXIOUS`). The `SoulState.mood` field exists but there's no visual (glyph, color bar, animation)
3. **Conversation history** — `active.history: Vec<DialogueTurn>` is populated but never displayed. Only the current NPC line is shown.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/dialogue.rs` | `panel_text()` — reformat, add history rendering |
| `reachlock-client/src/systems/hud.rs` | Dialogue panel entity — may need a child for portrait |
| `reachlock-client/src/systems/soul.rs` | `SoulRegistry`, `SoulState` — mood data source |
| `reachlock-core/src/soul/types.rs` | `SoulFile`, `Mood`, `portrait_id` |
| `reachlock-core/src/generator/sprite.rs` | `generate_character_sprite()` — for portrait rendering |

## Freeze first

### Mood → glyph mapping

```rust
pub fn mood_glyph(mood: Mood) -> (&'static str, Color) {
    match mood {
        Mood::Stable => ("◆", Color::srgb(0.7, 0.7, 0.7)),
        Mood::Happy => ("★", Color::srgb(0.3, 0.9, 0.4)),
        Mood::Tense => ("◆", Color::srgb(0.9, 0.7, 0.2)),
        Mood::Grieving => ("▼", Color::srgb(0.5, 0.5, 0.9)),
        Mood::Suspicious => ("◈", Color::srgb(0.9, 0.5, 0.1)),
        Mood::Grateful => ("★", Color::srgb(0.2, 0.8, 0.8)),
        Mood::Anxious => ("◆", Color::srgb(0.9, 0.6, 0.1)),
        Mood::Protective => ("▲", Color::srgb(0.9, 0.3, 0.3)),
        Mood::Defensive => ("◈", Color::srgb(0.8, 0.3, 0.3)),
        Mood::Focused => ("■", Color::srgb(0.5, 0.7, 0.9)),
        Mood::Withdrawn => ("▼", Color::srgb(0.5, 0.5, 0.5)),
    }
}
```

### Reformatted panel text layout

```
┌─────────────────────────────────────────┐
│ [portrait]  Boris                       │
│ 48x64 px    Engineer · ◆ STABLE         │
│                                         │
│ ── CONVERSATION ──                      │
│ Player: Hello.                          │
│ Boris: "What brings you here?"          │
│ Player: Just passing through.           │
│                                         │
│ Boris: "Fair enough. Stay out of        │
│         trouble."                       │
│                                         │
│ 1. Ask about the station                │
│ 2. Trade rumors                         │
│ 9. say something else…                  │
└─────────────────────────────────────────┘
```

The history shows the last 5 turns (or all, if <5). The current NPC line is the last line in history. Player lines are prefixed with name, NPC lines are prefixed with name + quotation marks.

## Deliverables

### 1. Add mood glyph + color to panel header

- [ ] In `panel_text` (`dialogue.rs:481`): replace `"mood: STABLE"` with mood glyph + mood name
- [ ] Use `mood_glyph()` to get glyph and color
- [ ] Render glyph + mood name with appropriate color (via `Color` — but note: text coloring in Bevy's `Text` component requires `TextColor` on a separate entity, or use rich text sections)

### 2. Add conversation history to panel

- [ ] After the header, render a separator line `"── CONVERSATION ──"`
- [ ] Iterate `active.history` (last 5 items) and render each as:
  - For NPC lines: `"{name}: "{line}""`
  - For player lines: `"Player: {line}"`
- [ ] The current NPC line (latest) is part of history — don't duplicate it

### 3. Add portrait rendering

- [ ] If `soul.portrait_id` is set and a sprite asset exists for it: render as a Bevy `Sprite` next to the panel text
- [ ] If no `portrait_id`: use the generic `portrait_{species}` sprite
- [ ] If no sprite assets exist at all: render a Unicode placeholder based on species (e.g., `"👤"`)
- [ ] Portrait position: left side of the dialogue panel, 48×64 pixels, scaled 2× from the 32×48 generator output

### 3a. Fallback: ASCII placeholder when no sprites available

- [ ] If neither portrait asset nor sprite generator can render: show species name as a colored Unicode block:
  ```
  Human: "◎"
  Android: "⬡"
  Robot: "▣"
  Voidborn: "◇"
  Xenotype: "◈"
  ```

### 4. Handle thinking state

- [ ] When `active.thinking` is true: show mood glyph + name + "is considering…"
- [ ] Add a simple animation indicator — three dots that cycle `.`, `..`, `...` each frame (using `Time` or a simple frame counter)

### 5. History truncation

- [ ] Show at most 5 history turns
- [ ] Show most recent turn first (bottom of history = newest)
- [ ] Long lines: truncate at 60 chars with `"…"`

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Talk to a soul-backed NPC → see portrait, mood glyph, role
# 2. Have a conversation → history scrolls with new lines
# 3. Trigger a mood change (via soul event) → mood glyph updates
# 4. Go offline → use free-text edge → NPC "thinking" shows cycling dots

make check
```

## Non-goals

- Voice synthesis integration (S62 TTS — separate)
- Portrait animation (idle bob, blink)
- Different portrait expressions per mood
- History persistence across sessions (history is in-memory only)
- Clickable/scrolling history (just display-only text)

## Gotchas

- **Bevy `Text` has no per-character coloring within a single `Text` component in Bevy 0.18.** Use multiple `Text` spans within a `Text` entity, or use the `TextSection` API. Verify the Bevy version's text API before implementing colored mood text.
- **Portrait as a separate sprite entity.** The dialogue panel is a `Text` entity at position `top: 120px, left: 8px`. The portrait would be a separate `Sprite` entity at `top: 120px, left: 56px` (next to the text). Despawn both when the panel closes.
- **`SoulState.mood` is an enum, not a string.** Use `state.mood.as_str()` or match on the variant. The `Mood` type is in `reachlock_core::soul::types`.
- **History is already populated.** `dialogue_input` pushes `DialogueTurn` entries. `panel_text` just needs to read them. No new data collection needed.
- **The free-text edge creates a dialogue turn** (`submit_utterance` pushes player's utterance as history). The LLM response also pushes the NPC reply. History covers both authored and free-text paths.
