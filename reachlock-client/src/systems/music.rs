//! Procedural audio engine (S48): real-time seeded music via fundsp.
//! The core `MusicIntent` lives in `reachlock_core::generator::music`;
//! this module bridges it into Bevy's audio output.

use bevy::prelude::*;

use reachlock_core::generator::music::{generate_music_intent, Mood};
use reachlock_core::util::Fixed;

use crate::settings::Settings;

/// Parameters broadcast to the fundsp graph via Shared<T> atomic variables.
/// Game systems write to these; the audio thread reads them per-sample.
#[derive(Resource)]
#[allow(dead_code)]
pub struct MusicParams {
    pub intensity: f32,
    pub melody_gain: f32,
    pub bass_gain: f32,
    pub drone_gain: f32,
    pub rhythm_gain: f32,
    pub master_gain: f32,
    pub filter_cutoff: f32,
    pub distortion: f32,
    pub tempo_scale: f32,
}

impl Default for MusicParams {
    fn default() -> Self {
        MusicParams {
            intensity: 0.0,
            melody_gain: 0.3,
            bass_gain: 0.15,
            drone_gain: 0.1,
            rhythm_gain: 0.0,
            master_gain: 0.5,
            filter_cutoff: 20000.0,
            distortion: 0.0,
            tempo_scale: 1.0,
        }
    }
}

/// The running audio engine state. On native, this holds the fundsp graph.
/// On WASM, it falls back to legacy WAV-based music generation.
#[derive(Resource)]
pub struct MusicEngine {
    /// Currently active mood — drives crossfade decisions.
    pub active_mood: Mood,
    /// Whether the engine is initialized (fundsp spike passed).
    pub initialized: bool,
    /// Last computed intensity value (for smoothing).
    pub last_intensity: Fixed,
    /// Seed used for the current music generation.
    pub music_seed: u64,
}

impl Default for MusicEngine {
    fn default() -> Self {
        MusicEngine {
            active_mood: Mood::Calm,
            initialized: false,
            last_intensity: Fixed::from_int(0),
            music_seed: 0,
        }
    }
}

/// Read game state and update MusicParams every frame. This is the bridge
/// between gameplay and the audio engine.
pub fn sync_music_params(
    _params: Res<MusicParams>,
    engine: Res<MusicEngine>,
    _settings: Res<Settings>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    // TODOs:
    // - Read combat active from PlayerTargeting / SpawnedEncounters
    // - Read hull_damage_pct from ShipSystems::hull_hp
    // - Read current location for docked/derelict state
    // - Call music_mood_for_context() → target mood
    // - Call music_intensity() → intensity Fixed
    // - Write MusicParams fields (gain, cutoff, distortion, tempo)
    // - Trigger mood crossfade on change
    // - Duck music on menu/pause/dialogue
    // - Apply settings.audio.master_volume * music_volume

    // For now: log intensity placeholder.
    if keys.just_pressed(KeyCode::KeyM) {
        info!(
            "Music: mood={:?}, intensity={}, initialized={}",
            engine.active_mood, engine.last_intensity.0, engine.initialized
        );
    }
}

/// Spawn the music resources on game start.
pub fn setup_music(mut commands: Commands) {
    commands.init_resource::<MusicParams>();
    commands.init_resource::<MusicEngine>();
    info!("Music engine resources initialized");
}

/// Tick the music engine each frame (placeholder — fundsp integration TBD).
pub fn tick_music(mut engine: ResMut<MusicEngine>) {
    // TODO: advance sample clock, schedule notes, mix layers
    if !engine.initialized {
        // Mark initialized after first tick (fundsp spike would go here).
        engine.initialized = true;
        engine.music_seed = 4242;
        let intent = generate_music_intent(engine.music_seed, engine.active_mood, 8);
        info!("MusicIntent generated: {} notes, {} bpm", intent.notes.len(), intent.bpm);
    }
}
