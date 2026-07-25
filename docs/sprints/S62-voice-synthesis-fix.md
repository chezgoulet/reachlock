# S62 — Voice Synthesis Fix

**Spec:** §29 (voice chat) · **Wave 17 (Client Polish) · Depends on:** S61

## Outcome

NPCs speak audible synthesized lines during dialogue. The `voice_native_placeholder_handle()` no-op thread is replaced with a real TTS thread that generates WAV audio from text and routes it through `bevy_audio`. The voice synthesis is deterministic — same seed + same text = same audio — enabling offline parity.

## Context

- `reachlock-client/src/systems/voice/mod.rs:346` creates `voice_native_placeholder_handle()` — an empty `JoinHandle<()>` that does nothing. The comment acknowledges this as a placeholder.
- The `VoiceManager` runs a native thread for WebRTC voice chat (real-time player voice). NPC synthesis is a separate concern — it generates audio from text, not from a microphone.
- The spec (§29) defines voice chat for P2P player communication. NPC voice synthesis is a related but distinct gameplay feature: story dialogue, crew banter, station announcements, ship AI voice.
- Deterministic audio generation is an iron rule requirement for gameplay values. Voice synthesis output must be seed-consistent so the same dialogue line sounds the same across platforms.

## Freeze first

1. TTS runs locally on the client machine — no cloud TTS API, no server-side synthesis. This preserves offline-first (iron rule #6).
2. The synthesis thread communicates with the Bevy main thread via a crossbeam channel: synthesis requests go in, WAV buffers come out.
3. Only NPC voice lines are synthesized (not player voice — that's real WebRTC audio). Player voice remains a separate path.

## Deliverables

### 1. TTS thread replacement

- [ ] **Remove `voice_native_placeholder_handle()`** at `voice/mod.rs:346` — delete the no-op function.
- [ ] **Add `espeak-rs` or `piper-rs` dependency** to `reachlock-client/Cargo.toml` behind a `tts` feature flag (native-only, excluded from WASM build). Espeak is preferred: it's small, fast, deterministic, and available on all platforms via system packages. Piper provides higher quality but requires model downloads.
- [ ] **TtsThread struct** — owns the TTS engine handle and a channel receiver. Spawned on the native thread (same as the existing WebRTC thread pattern). Methods:
  - `synthesize(text, voice_params) -> Vec<f32>` — generates audio samples synchronously
  - Returns WAV samples (PCM f32, 22050 Hz mono)
- [ ] **Voice params from soul file** — the `VoiceParams` struct from the soul (pitch, speed, accent) maps to espeak parameters:
  - `pitch` (0-1024) → espeak pitch adjustment (-50 to +50 semitones, mapped)
  - `speed` (0-1024) → espeak speed (80 to 450 wpm)
  - `accent` (string) → espeak voice variant ("en-us", "en-gb", etc.)
  - `Seed` → deterministic noise floor for the output (ensures same params + same text = same audio)

### 2. Bevy audio integration

- [ ] **Audio queue resource** — `SynthesizedSpeechQueue` holding pending WAV buffers. The synthesis thread pushes completed buffers here. A Bevy system pops buffers and spawns `AudioSource` components.
- [ ] **`bevy_audio` playback** — each completed synthesis request becomes a `bevy_audio::AudioSource` (from raw PCM samples) and is played via `commands.spawn(AudioBundle { source, settings: PlaybackSettings { spatial: true, .. } })`. Spatial audio: the source is positioned at the NPC's location in the game world.
- [ ] **Channel capacity** — queue holds max 8 pending speech buffers. If the queue is full, synthesis requests are dropped (the player hears silence instead of old dialogue). This prevents memory growth from rapid NPC dialogue.

### 3. Dialogue integration

- [ ] **NpcLine → synthesis trigger** — when a dialogue node of type `NpcLine` is activated, the dialogue system sends a synthesis request to the TTS thread with:
  - Text from the dialogue node
  - Voice params from the NPC's soul file
  - NPC's current game position (for spatial audio)
- [ ] **Synthesis timing** — the dialogue advance waits for synthesis to complete before showing the "continue" prompt. This ensures the voice line plays before the player can click through. Maximum wait: 5 seconds (fallback to text-only if synthesis fails).
- [ ] **Subtitle fallback** — when TTS is not available (WASM build, platform without espeak, synthesis failure), the dialogue proceeds with text only. A small indicator shows "🔇 Voice not available" on the first dialogue node.

### 4. Deterministic synthesis test

- [ ] **Test** — synthesize the same text with the same seed and voice params twice. The resulting audio buffer must be bit-identical. This test runs on native builds only (excluded from WASM).
- [ ] **Determinism** — the synthesis engine must use a seeded noise floor for any non-deterministic components (small variations in sample timing). In practice, espeak produces identical output for identical input, so the test should pass trivially.

### 5. Platform handling

- [ ] **Linux** — espeak available via `espeak-ng` system package. Cargo feature `tts` enables the espeak crate. Check at runtime: if espeak is not installed, fall back to text-only with a warning.
- [ ] **macOS** — espeak via Homebrew (`brew install espeak-ng`). Same runtime check.
- [ ] **Windows** — espeak via the espeak crate (bundled DLL). Runtime check for the DLL.
- [ ] **WASM** — TTS is unavailable. Fall back to text-only dialogue. All code behind `#[cfg(not(target_arch = "wasm32"))]`.

## Acceptance gates

```
cargo test -p reachlock-client --features tts
# Synthesis: same text + same seed + same params = identical audio
# Integration: play a dialogue with NPC → synthesized audio plays
# Queue: rapid speech requests do not overflow
# Fallback: without tts feature → text-only, no crash

Manual: dock at Sorrow Station → talk to Doss → hear his gravelly voice → talk to Boris → hear his flat robotic tone → hear spatial audio position changing as you move → disable espeak → dialogue continues with text only + "Voice not available" indicator
```

## Non-goals

Real-time WebRTC voice chat improvements (that's the existing WebRTC thread for player voice, which works). Lip-sync animation during dialogue (Phase 4). Voice emotion modulation (happy/sad/angry voice — espeak supports limited prosody control, but full emotional range is Phase 4).

## Gotchas

- The `espeak-ng` system library is large (~5MB on disk) and not available on WASM. The `tts` feature flag keeps it out of WASM builds. `make check` must still pass without the `tts` feature.
- Espeak's pitch and speed parameters approximate the soul's `VoiceParams` but do not perfectly match. Document the mapping function in code with the expected tolerances. Example: `soul.pitch` (0-1024) → `espeak_pitch` (range 0-99, map with `(pitch * 99 / 1024) - 50` for semitone adjustment).
- Synthesis latency: espeak generates ~100ms of speech per 10ms of real-time. A 5-second sentence takes ~500ms to synthesize. The dialogue "wait for synthesis" must be async on the Bevy thread — do NOT block the main thread. Use the existing async channel pattern from the WebRTC thread.
- If espeak panics or crashes (corrupted voice data, unsupported language), the TTS thread should catch the panic, log the error, and enter a fallback mode (pass-through with empty audio). Never crash the game client from the TTS thread.
- The `determinism` test for synthesis requires the espeak library at test time. Gate the test with `#[cfg(all(test, feature = "tts"))]`.
