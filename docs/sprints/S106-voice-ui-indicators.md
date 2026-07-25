# S106 — Voice UI Indicators

**Wave: UX-QoL · Depends on:** S29 (Voice chat), S62 (Voice synthesis)

## Outcome

The player can see when:
1. **They are transmitting** — a PTT indicator appears when the push-to-talk key is held
2. **Someone else is speaking** — the speaker's name appears in a small overlay
3. **The mic device is active** — current mic device name shown in a tooltip
4. **Voice connection state** — connected / connecting / failed icon

All indicators are minimal (2-3 characters + text) positioned near the HUD status line. No new complex panel — just status badges.

## Context

Voice chat works (WebRTC signaling, Opus encoding, spatial audio rendering) but the player has zero visual feedback:
- Press V (PTT) → nothing on screen tells you the mic is hot
- Someone speaks → spatial audio plays but no HUD badge for who's talking
- Mic cycle (F7) → device changes silently, player doesn't know which device is active
- Voice thread failure → logged to `log::warn!`, player never sees it

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/voice/mod.rs` | Voice manager, PTT, mic cycle |
| `reachlock-client/src/systems/hud.rs` | HUD status line — add voice badges |
| `reachlock-client/src/systems/presence.rs` | `RemoteShip` — player_ids for speaker identification |
| `reachlock-client/src/settings.rs` | `Settings.audio.voice_input_device` |

## Freeze first

### Voice status indicator data

```rust
/// Voice HUD state, updated each frame.
#[derive(Resource, Default)]
pub struct VoiceHudState {
    /// Whether the player is holding PTT this frame.
    pub transmitting: bool,
    /// Players currently sending audio (who is speaking right now).
    pub current_speakers: Vec<String>,
    /// Current mic device name (empty if default/unset).
    pub mic_device: String,
    /// Voice thread status.
    pub voice_available: bool,   // false if the thread failed to start
    pub voice_connected: bool,   // true when at least one peer is connected
}
```

### HUD badge format

Badges render in the top-right corner, below the FPS/latency/offline badges:

| Condition | Badge |
|-----------|-------|
| PTT held, voice available | `"🔴 TX"` (red) |
| PTT held, voice unavailable | `"TX (no mic)"` (grey) |
| Speaker active (each) | `"📢 Boris"` (green) |
| Voice thread failed | `"🔇 Voice disabled"` (yellow) |
| No peers connected | `"🎤 Ready"` (dim grey) |

If no voice activity: no badge (clean HUD).

## Deliverables

### 1. Add VoiceHudState resource

- [ ] Add to `voice/mod.rs`:
```rust
#[derive(Resource, Default)]
pub struct VoiceHudState {
    pub transmitting: bool,
    pub current_speakers: Vec<String>,
    pub mic_device: String,
    pub voice_available: bool,
    pub voice_connected: bool,
}
```

### 2. Update VoiceHudState each frame

- [ ] In `ptt_system` (`voice/mod.rs:369`): set `hud_state.transmitting = held_on_this_frame`
- [ ] In `audio_feed_voice` (`voice/mod.rs:293`): when processing audio frames, push the `player_id` to `hud_state.current_speakers`
- [ ] In `start_voice_thread` (`voice/mod.rs:407`): on success, set `voice_available = true`; on failure, set `false`
- [ ] In `process_voice_signals` (`voice/mod.rs:195`): on `PeerConnected`, set `voice_connected = true`; on `PeerClosed`, clear connected if no peers remain
- [ ] In `mic_cycle_system` (`voice/mod.rs:385`): update `mic_device` with the new device name

### 3. Render voice badge in HUD

- [ ] In `hud.rs`: add a new `Query<&mut Text, With<VoiceBadge>>` 
- [ ] In `spawn_hud` (`hud.rs:113`): spawn a voice badge text entity at:
  - `position_type: Absolute`
  - `top: 48px, right: 8px` (below the latency display at 36px, offline at 24px)
  - `font_size: 12.0`
  - `TextColor: srgb(0.6, 0.6, 0.6)` (default dim)
- [ ] In `update_hud_status` or a new `update_voice_hud` system: set the text to the appropriate badge

### 4. Badge logic

```rust
fn voice_badge_text(state: &VoiceHudState) -> String {
    if !state.voice_available {
        return "🔇 Voice disabled".to_string();
    }
    if state.transmitting {
        return "🔴 TX".to_string();
    }
    if !state.current_speakers.is_empty() {
        let names = state.current_speakers.iter().take(2).cloned().collect::<Vec<_>>().join(", ");
        return format!("📢 {names}");
    }
    // Show mic device on hover or always visible
    String::new() // clean: no ongoing voice activity
}
```

### 5. Wire mic device display

- [ ] Add a tooltip or always-visible line showing current mic device
- [ ] Format: `"🎤 {device_name}"` — shown in dim grey when no other voice activity
- [ ] Updated on mic cycle (F7)

### 6. Speaker tracking edge case handling

- [ ] Clear `current_speakers` each frame (audio is frame-by-frame PCM chunks)
- [ ] Only add speakers who sent audio THIS frame
- [ ] Deduplicate: if same player_id appears multiple times, show once

### 7. Clean up on voice disabled

- [ ] When `voice_available == false`, show `"🔇 Voice disabled"` permanently (dim yellow)
- [ ] When transitioning from false → true, switch to clean state

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Launch game → in flight, no voice badge visible
# 2. Press and hold V (PTT) → 🔴 TX appears, disappears on release
# 3. Have another player speak → 📢 {name} appears while they're speaking
# 4. Press F7 (cycle mic) → mic device name updates
# 5. If voice thread fails → 🔇 Voice disabled persists

make check
```

## Non-goals

- Voice volume visualization (VU meter)
- Mute individual players
- Voice activity threshold adjustment
- Spatial audio source markers in 3D space
- Voice channel selection

## Gotchas

- **`hud.rs` already has many ParamSet queries.** Adding another badge entity may exceed the tuple size limit. Consider adding a separate `update_voice_hud` system that takes its own query.
- **Speaker names come from `player_id` strings**, not display names. These are typically UUIDs or session IDs for remote players. For local testing with NPCs, speaker names come from `CrewMember.name`. Resolve player_id → display name via `RemoteShip` or a player name registry.
- **PTT detection is frame-level.** If `ptt_system` runs AFTER `update_voice_hud`, the transmitting flag may be one frame late. Run `update_voice_hud` after `ptt_system` using `.after()` or `.chain()`.
- **Voice badge position.** `top: 48px, right: 8px` is below latency (36px) and offline (24px). The FPS counter is at 24px top. Verify spacing doesn't overlap.
- **Unicode emoji rendering.** `🔴` (U+1F534), `📢` (U+1F4E2), `🎤` (U+1F3A4), `🔇` (U+1F507) may not render in Bevy's default font. Test on the target platform. Fallback: use ASCII alternatives: `[TX]`, `[SPK:Boris]`, `[MIC:default]`.
