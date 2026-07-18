# S48 — Procedural Audio Engine (fundsp)

**Spec:** §5 Procedural Generation (music generator), §10 Content Override
System, §14 Modes (mode-aware audio) · **Wave: Phase 4 (Audio Pass)** ·
**Depends on:** S01 (content pipeline), S05 (item generator — item audio
cues), S06 (mode state machine — mode-specific music), S09 (space flight
— flight/combat events)

## Outcome

A real-time, seeded, dynamic music engine replaces the static `generate_music`
WAV renderer. Music is authored as a `MusicIntent` in core — a deterministic
note sequence bound to a seed, mood, and intensity — and rendered continuously
by the fundsp library in the client. Three moods stream seamlessly (one at a
time, crossfaded on transition). Game events (combat, hull damage, docking,
jump) modulate intensity, tempo, and filter parameters in real time. Authored
themes can override or augment the procedural melody — the generator riffs on
a furnished theme, varying it deterministically from a seed. Authored
recordings (WAV/OGG) can replace the fundsp engine entirely at any layer via
the content pipeline override system.

## Context

- **Current state:** `generate_music(seed, mood, duration)` produces a
  pre-rendered mono square-wave melody as a `Vec<i16>`. It plays once on scene
  enter via Bevy's `AudioPlayer`. No dynamic reaction to game state, no
  crossfades, no layered instrumentation. The bridge wraps it as a WAV blob
  and delegates decoding to `bevy_audio`.
- **fundsp 0.23** is a pure-Rust audio DSP and synthesis library with
  composable graph notation, real-time `Sequencer` crossfade engine,
  `Shared` atomic variables for per-frame parameter control, and support
  for 12+ oscillator types, 30+ filter types, noise generators, envelopes,
  reverb, and `no_std` / WASM compatibility. `bevy_procedural_audio`
  bridges fundsp directly into Bevy's audio output.
- **The spec's content override system** (§10) already has a proven pattern:
  `ContentPayload::Procedural { seed }` vs `ContentPayload::HandCrafted {
  asset_id }`. Music follows the same pipeline — a system/station/moment
  resolves its music source at scene load time, and the client renders what it
  receives.
- **This is Phase 4 infrastructure.** The game is feature-complete through
  S47; this sprint replaces the placeholder audio with the real system.
  It is self-contained — no other sprint blocks on it, and it modifies only
  the generator + bridge + a new client system. Existing gameplay is
  unaffected during development: the fundsp engine runs alongside the old
  WAV path until fully validated.
- **WASM fallback:** Fundsp's dependency tree (SIMD `wide`, `typenum`,
  `numeric-array`) is expected to compile for `wasm32-unknown-unknown`,
  but this is validated in Phase 1. If the tree chokes on WASM, the web
  build uses the legacy `generate_music` WAV renderer. Fundsp is
  native-only for the desktop build. The acceptance gate treats WASM
  failure as a warning, not a blocker — all other gates still close.
  The MusicIntent interface is identical on both targets; only the
  downstream renderer differs.

## Freeze first

### Core types (`reachlock-core/src/generator/music.rs`)

All types are deterministic, pure, integer-math. No fundsp dependency in core.

```rust
/// A single note event in a music sequence. All values are fixed-point
/// or integer — no floats. This is the core's deterministic output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoteEvent {
    /// Scale degree index (0 = root). Relative to tonic, not absolute frequency.
    pub degree: u8,
    /// Octave offset from the base register. 0 = natural register, 1 = one up.
    pub octave: u8,
    /// Velocity in 0..=127 MIDI territory. Keeps the abstraction familiar.
    pub velocity: u8,
    /// Start time in ticks from sequence start. 1 tick = 1/24 beat.
    pub start_tick: u32,
    /// Duration in ticks. 0 = rest.
    pub duration_ticks: u32,
}

/// Which pentatonic/scale mode to draw notes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    MinorPentatonic,  // 1·♭3·4·5·♭7  (default — current behavior)
    MajorPentatonic,  // 1·2·3·5·6
    Dorian,           // 1·2·♭3·4·5·6·♭7
    Octatonic,        // 1·2·♭3·4·♭5·♭6·6·7 — alien/derelict feel
}

/// Bitmask for musical layers. A MusicIntent carries which layers are active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerMask(pub u8);

impl LayerMask {
    pub const MELODY: u8  = 1 << 0;
    pub const BASS:   u8  = 1 << 1;
    pub const DRONE:  u8  = 1 << 2;
    pub const RHYTHM: u8  = 1 << 3;
}

/// Musical mood. Deterministic input to the generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mood {
    Calm,       // slow, spacious, pentatonic
    Tense,      // faster, narrower range, more chromatic
    Derelict,   // droning, sparse, octatonic or atonal
    Combat,     // urgent, percussive, rhythmic bass, dissonance
}

/// A deterministic music sequence — the core generator's output.
/// This is what gets recaptured in determinism.rs goldens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicIntent {
    pub seed: u64,
    pub mood: Mood,
    pub scale: Scale,
    pub bpm: u32,               // beats per minute, integer
    pub root_hz: u32,           // tonic frequency in Hz, integer
    pub active_layers: LayerMask,
    pub notes: Vec<NoteEvent>,  // all layers merged, sorted by start_tick
    pub bar_length: u16,        // ticks per bar — for phrase boundaries
}

/// An authored theme — content that constrains (not replaces) the generator.
/// Loaded from `content/music/themes/<id>.ron` via the content pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    /// Theme name — used to derive the seed for variation.
    pub id: String,
    /// The original, unvaried note sequence. Notes are scale degrees (not
    /// absolute pitches) so the theme can be transposed by the generator.
    pub notes: Vec<NoteEvent>,
    /// Preferred scale for this theme.
    pub scale: Scale,
    /// Preferred tempo range the generator should stay within.
    pub bpm_range: (u32, u32),
    /// Which variation operators the generator may apply.
    pub allowed_variations: VariationMask,
}

/// Bitmask for allowed variation operators when riffing on a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariationMask(pub u16);

impl VariationMask {
    pub const TRANSPOSE:       u16 = 1 << 0;  // move up/down an octave
    pub const PASSING_TONES:   u16 = 1 << 1;  // insert neighbor notes
    pub const RHYTHMIC_SHIFT:  u16 = 1 << 2;  // nudge note starts
    pub const REPETITION:      u16 = 1 << 3;  // stutter notes
    pub const ARTICULATION:    u16 = 1 << 4;  // slur/accent some notes
    pub const PHRASE_SWAP:     u16 = 1 << 5;  // reorder phrases
    pub const REST_INSERTION:  u16 = 1 << 6;  // insert pauses
    pub const SUBSTITUTION:    u16 = 1 << 7;  // swap degrees in same chord
    pub const ORNAMENTATION:   u16 = 1 << 8;  // trills, grace notes
}

/// What the content pipeline resolves for a music source.
#[derive(Debug, Clone)]
pub enum MusicSource {
    /// Procedurally generated from seed + mood + optional theme.
    Procedural { seed: u64, mood: Mood, theme: Option<Theme> },
    /// Authored recording — skip the fundsp engine entirely. The bridge
    /// loads the file and plays it through the standard Bevy audio path.
    HandCrafted { asset_path: String },
}
```

### Generator functions (`reachlock-core/src/generator/music.rs`)

```rust
/// Generate a deterministic note sequence from a seed and mood. No theme.
/// This replaces the current `generate_music` (which is kept for
/// backward-compat CLI). The output is a pure, testable data structure.
pub fn generate_music_intent(seed: u64, mood: Mood, duration_bars: u16)
    -> MusicIntent;

/// Generate a variation on an authored theme. Same seed + same theme
/// always produces the same note sequence. The theme is the "head" —
/// the generator varies it using the allowed operators from the
/// VariationMask and restates the pure theme every `recap_every` bars.
pub fn generate_themed_music(seed: u64, mood: Mood, theme: &Theme,
    duration_bars: u16, recap_every: u16) -> MusicIntent;

/// Select the mood and scale for a given game context. Pure function —
/// reads data structures, not game state. The client calls this, passing
/// the relevant game values.
pub fn music_mood_for_context(
    combat_active: bool,
    hull_damage_pct: u8,     // 0-100
    in_derelict: bool,
    is_docked: bool,
) -> Mood;

/// Calculate intensity from game state — the client uses this to drive
/// fundsp parameters (tempo, filter, distortion, gain, mix).
pub fn music_intensity(
    combat_active: bool,
    hull_damage_pct: u8,
    mood: Mood,
) -> Fixed;   // 0..1024
```

### Content schema (frozen)

Theme files live in `content/music/themes/<id>.ron`. The schema:

```ron
// content/music/themes/calm_exploration.ron
{
    "id": "calm_exploration",
    "scale": "MinorPentatonic",
    "bpm_range": [60, 80],
    "notes": [
        // Phrase 1 — rising call
        { "degree": 3, "octave": 0, "velocity": 80,  "start_tick": 0,   "duration_ticks": 12 },
        { "degree": 5, "octave": 0, "velocity": 72,  "start_tick": 12,  "duration_ticks": 12 },
        { "degree": 6, "octave": 0, "velocity": 72,  "start_tick": 24,  "duration_ticks": 12 },
        { "degree": 5, "octave": 0, "velocity": 64,  "start_tick": 36,  "duration_ticks": 12 },
        { "degree": 3, "octave": 0, "velocity": 72,  "start_tick": 48,  "duration_ticks": 24 },
        // rest
        { "degree": 0, "octave": 0, "velocity": 0,   "start_tick": 72,  "duration_ticks": 12 },

        // Phrase 2 — answering fall
        { "degree": 6, "octave": 0, "velocity": 80,  "start_tick": 84,  "duration_ticks": 12 },
        { "degree": 8, "octave": 0, "velocity": 72,  "start_tick": 96,  "duration_ticks": 12 },
        { "degree": 10, "octave": 0, "velocity": 80, "start_tick": 108, "duration_ticks": 24 },
        { "degree": 8, "octave": 0, "velocity": 64,  "start_tick": 132, "duration_ticks": 12 },
        { "degree": 6, "octave": 0, "velocity": 60,  "start_tick": 144, "duration_ticks": 12 },
    ],
    "allowed_variations": 511,  // all bits set — full variation
}
```

### Client types (`reachlock-client/src/systems/music.rs`)

```rust
use fundsp::prelude::*;

/// The running fundsp audio engine. Lives as a Bevy Resource.
/// Uses fundsp's frontend/backend split for thread-safe mutation.
pub struct StreamingMusicEngine {
    /// Fundsp frontend — mutated by game systems.
    net: Net,
    /// Fundsp backend — consumed by the audio thread (bevy_procedural_audio).
    backend: NetBackend,
    /// Currently active mood, for crossfade management.
    active_mood: Mood,
    /// Current MusicIntent being rendered.
    current_intent: MusicIntent,
    /// Sample counter — drives note scheduling.
    sample_clock: u64,
    /// Sample rate (from fundsp output — 44100 default).
    sample_rate: f32,
}

/// Parameters broadcast to the fundsp graph via Shared<T> atomic variables.
/// Game systems write to these; the audio thread reads them per-sample.
pub struct MusicParams {
    pub intensity: Shared<f32>,        // 0.0–1.0
    pub melody_gain: Shared<f32>,
    pub bass_gain: Shared<f32>,
    pub drone_gain: Shared<f32>,
    pub rhythm_gain: Shared<f32>,
    pub master_gain: Shared<f32>,      // respects settings.audio.master_volume
    pub filter_cutoff: Shared<f32>,    // global lowpass opening
    pub distortion: Shared<f32>,       // waveshaper drive amount
    pub tempo_scale: Shared<f32>,      // multiplier on base BPM
}

/// Marker component for the music emitter entity.
#[derive(Component)]
pub struct MusicEmitter;
```

## Deliverables

### 1. Core MusicIntent generator

- [ ] Define `NoteEvent`, `Scale`, `LayerMask`, `Mood`, `MusicIntent`,
  `Theme`, `VariationMask`, `MusicSource` in `reachlock-core/src/generator/music.rs`.
- [ ] Implement `generate_music_intent(seed, mood, duration_bars) → MusicIntent`.
      Seed drives note selection, octave, velocity, and rhythmic placement. Notes
      drawn from the scale, avoiding consecutive identical degrees (better
      variety than current random walk).
- [ ] Implement `generate_themed_music(seed, mood, theme, duration, recap) →
      MusicIntent`. The generator uses `SeededRng` to permute phrase order, shift
      rhythms, insert passing tones, and apply ornaments based on
      `VariationMask`. The pure theme is restated every `recap_every` bars.
- [ ] Implement `music_mood_for_context(...)` — pure function mapping game
      context to mood.
- [ ] Implement `music_intensity(...)` — returns a `Fixed` (0..1024).
- [ ] Keep `generate_music(seed, mood, duration) → GeneratedAudio` unchanged
      (backward compat for CLI `gen music --wav`). Add a deprecation note.
- [ ] Unit tests: deterministic output (`generate_music_intent(seed, _, _) ==
      generate_music_intent(seed, _, _)`), moods differ, theme variation is
      deterministic, themed music restates the original theme at the right bar.
- [ ] Extend `determinism.rs`: capture goldens for `generate_music_intent` and
      `generate_themed_music` across x86_64/aarch64/wasm32.

### 2. Fundsp integration (client)

- [ ] Add `fundsp = { version = "0.23", default-features = false }` to
      `reachlock-client/Cargo.toml`. The `no_std`-compatible subset is
      sufficient (no `files`, no `fft`). Add `bevy_procedural_audio` (or
      `bevy_fundsp`) for the Bevy audio bridge.
- [ ] Verify WASM build: `fundsp` compiles with `wasm32-unknown-unknown`.
      **This is the Phase 1 spike gate — if it fails, the WASM fallback
      activates: the web build uses the legacy `generate_music` WAV renderer
      and fundsp is native-only for the desktop build.** The MusicIntent
      interface is identical on both targets; only the downstream renderer
      differs. WASM failure is a warning, not a blocker.
- [ ] Spike a mono fundsp stream: `sine() + noise() * 0.3` piped through
      `bevy_procedural_audio` → audible continuous output. Validate latency
      and CPU overhead (target: < 2% frame budget at 60 fps).
- [ ] Replace the startup music spawn in `setup.rs:enter_spaceflight` with
      a `MusicEmitter` entity that the fundsp engine renders to.

### 3. Streaming music engine (client)

- [ ] Build `StreamingMusicEngine` resource with fundsp graph per layer:
  - **Melody**: soft saw/sine oscillator, driven by note events from the
    current `MusicIntent`. Notes are gated by a note-on/note-off sequencer.
  - **Bass**: triangle wave sub-oscillator, locked to the root note of the
    current bar. Rhythm follows the drum track.
  - **Drone**: filtered pink noise with slow LFO sweep on cutoff.
  - **Rhythm**: noise bursts sequenced via fundsp `Sequencer`.
- [ ] All layers are mixed via `bus()` and passed through a master chain:
      `lowpass_hz(filter_cutoff) >> dbell_hz(mid_freq, q, gain) >> dcblock()`.
- [ ] `MusicParams` atomic variables drive all dynamic parameters. The engine
      writes sample data to `bevy_procedural_audio`'s output buffer each frame.
- [ ] Implement mood crossfade: when mood changes, `Sequencer` starts a new
      event with `Fade::Smooth` fade-in while fading out the previous event.
      Crossfade duration: 1–2 seconds.
- [ ] Implement note scheduling: `StreamingMusicEngine` reads `NoteEvent`s
      from `MusicIntent`, converts `start_tick` to sample offset, and gates
      oscillators via ADSR envelopes.

### 4. Dynamic music reaction

- [ ] A new system `sync_music_params` runs every frame in `Update`:
  - Reads `PlayerTargeting` / `SpawnedEncounters` → combat active
  - Reads `ShipSystems::hull_hp` → damage percentage
  - Reads `CurrentLocation` → station presence, system danger level
  - Reads `Settings` → master/music volume
  - Calls `music_mood_for_context()` → target mood
  - Calls `music_intensity()` → intensity `Fixed`
  - Writes `MusicParams` shared variables (intensity, gains, filter, tempo)
- [ ] When target mood differs from active mood, trigger a mood transition:
      generate a new `MusicIntent` for the target mood and push it to the
      fundsp `Sequencer` with crossfade.
- [ ] Music ducks (gain drops to 20%) when the main menu or pause menu is
      active, and when dialogue text is being displayed (S16 typing state).

### 5. Authored themes (content pipeline)

- [ ] Define the `Theme` RON schema (see Freeze first). Deserialize from
      `content/music/themes/<id>.ron`.
- [ ] Extend `ContentIndex` to include a `themes: HashMap<String, Theme>`.
      Load themes at startup alongside other content.
- [ ] Implement `MusicOverride` in the content pipeline: when a system/station/
      moment resolves its music source, check for a `ContentPayload::HandCrafted`
      with an asset path → `MusicSource::HandCrafted`. Otherwise,
      `MusicSource::Procedural` with seed, mood, and optional theme.
- [ ] Authored recordings (`MusicSource::HandCrafted`): the bridge loads the
      file via `asset_server.load(path)` and plays it through standard
      `AudioPlayer`. The fundsp engine idles (or renders a subtle ambient bed
      underneath if the authored file is mono/stereo).
- [ ] Test: authored theme + seed 0xDEAD → deterministic variation. Authored
      recording → skips fundsp entirely, plays the file.

### 6. Settings integration

- [ ] Music respects `settings.audio.master_volume * settings.audio.music_volume`
      through the `master_gain` shared variable every frame.
- [ ] SFX volume is plumbed as a separate `Shared<f32>` for future SFX sprint.
- [ ] `mute_when_unfocused` ducks master gain to 0.0 when the window loses focus.

## Acceptance gates

```
# Core determinism
cargo test -p reachlock-core generator::music::  # deterministic intent, theme variation
cargo run -p reachlock-cli -- determinism         # goldens match x86_64/aarch64/wasm32

# Client build
cargo build -p reachlock-client                   # fundsp compiles
cargo build -p reachlock-client --target wasm32-unknown-unknown  # WASM builds
                                                     (warning only — see WASM fallback)

# Fundsp spike (Phase 1 gate)
cargo test -p reachlock-client music::spike       # sine+noise renders N samples non-silent

# Streaming — manual
# Launch the game → hear continuous music (not a one-shot WAV)
# Fly into combat → music becomes tense, tempo increases
# Dock at a station → music transitions to calm
# Take hull damage → distortion + lowpass modulation audible
# Open settings, change music volume → volume changes in real time
# Place a theme file in content/music/themes/ → themed music renders

# Content override — manual
# Place an authored WAV as override for a station → station plays that WAV
# Remove the override → station returns to procedural music

make check  # fmt, clippy -D warnings, determinism gate, WASM (warning only)
```

## Non-goals

- **Sound effects (SFX):** Weapons fire, explosions, UI beeps, engine hum,
  footsteps. These are a separate sprint (S49 or later). The `sfx_volume`
  setting is plumbed but no SFX system exists.
- **Voice output for LLM speech:** The `voice_volume` setting exists;
  actual TTS/speech synthesis is deferred to after S29 (voice chat).
- **Spatial audio / 3D audio positioning:** Mono/stereo output only. 3D
  audio (starboard explosions come from the right speaker) is deferred.
- **Music playback controls:** No "skip track," "pause music only," or
  playlist features. Music is continuous and adaptive — it is the game's
  ambient score.
- **Dynamic music composition from scratch:** The generator produces note
  sequences, not audio DSP graphs. Chord progressions, counterpoint, and
  full orchestration are non-goals for this sprint. The focus is on making
  the existing pentatonic melody richer and reactive.
- **Per-player music seeds:** Music is seeded from the system seed, not
  per-player. Two players in the same system hear the same procedural
  music (important for MMO: consistent ambient world).
- **Music modding tools:** No theme editor UI. Themes are authored as RON
  files. A visual theme editor belongs in S25 (content editor suite).
- **WASM fundsp on web:** If the fundsp dep tree fails to compile to
  `wasm32-unknown-unknown`, the web build falls back to the legacy WAV
  renderer. Fundsp is native-only. WASM acceptance is a warning-only gate.

## Gotchas

- **Fundsp dependencies transitively include `wide` (SIMD), `libm` (no_std
  math), `typenum` (type-level integers), and `numeric-array`.** The
  `bevy_procedural_audio` crate may add additional dependencies. Validate
  the full dependency tree compiles on wasm32 in Phase 1 BEFORE writing any
  production music code. If the tree is too heavy for WASM, fall back to
  the legacy WAV renderer for the web build and keep fundsp native-only.
  WASM failure is a warning, not a blocker — see acceptance gates.
- **Fundsp's pseudorandom phase system is deterministic per graph structure,
  not per seed.** To get deterministic output from a given seed, use
  `.seed(n)` on noise generators explicitly. Oscillators are seedable via
  `.phase(initial_phase)`. The note sequence determinism lives in core
  (testable via `MusicIntent`); audio determinism is best-effort (floats).
- **Audio thread vs game thread:** Fundsp's `Net` frontend/backend split
  handles this, but the `Shared<T>` variables use atomic operations — there
  is a one-frame latency between setting a parameter and hearing it change.
  This is imperceptible at 60 fps.
- **Mood transitions must not pop.** The `Sequencer` crossfade must ramp
  the outgoing graph down while ramping the incoming graph up, with a
  brief overlap. If the `Sequencer` is misconfigured, the audio output
  will clip or click. Test with extreme mood transitions (Calm → Combat).
- **The legacy `generate_music` remains in core for CLI compat.** The
  new `generate_music_intent` is additive, not a replacement. Both coexist
  in the same module. The CLI `gen music --wav` still uses the old path;
  `gen music --preview` uses the new path (prints MusicIntent as RON).
- **Theme RON files must survive the `r#"…"#` gotcha.** Rust raw strings
  die on `"#` sequences (hex colors in SVG). Theme files are pure RON
  with no hashes, so this isn't triggered — but the content importer
  should still use `r##"…"##` for safety.
- **Keep the existing `Mood` enum backward-compat.** The old `Calm`/`Tense`/
  `Derelict` variants must serialize to the same RON identifiers. Add
  `Combat` as a new variant — existing settings files that reference mood
  won't break.
- **The `content_index` must load themes at startup before the first music
  intent is generated.** Theme loading is a startup system in the same
  chain as `load_content_index`. If a theme file is missing, warn and
  fall back to theme-less generation (no panic).
- **WASM fallback is a first-class path, not an afterthought.** If the
  fundsp spike fails on WASM, the streaming engine code is conditionally
  compiled behind `#[cfg(not(target_arch = "wasm32"))]` gates. The
  fallback path bridges the same `MusicIntent` through the legacy WAV
  renderer — identical music sequence, different synthesis. The
  determinism tests (core) and the manual acceptance tests (client) both
  cover the fallback path.
