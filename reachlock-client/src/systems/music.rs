//! Procedural audio engine (S48): real-time seeded music via fundsp.
//! Core `MusicIntent` generator in `reachlock_core::generator::music`.
//! This module bridges game state into `MusicParams` and holds the
//! music engine resources. The fundsp streaming audio graph itself
//! is a follow-up — `tick_music` currently logs intent generation.

use bevy::prelude::*;

use reachlock_core::generator::music::{
    generate_music_intent, music_intensity, music_mood_for_context, Mood,
};
use reachlock_core::util::Fixed;

use crate::settings::Settings;
use crate::states::{CurrentLocation, GameMode};
use crate::systems::combat::{EnemyShip, SpawnedEncounters};
use crate::systems::ship::ShipSystems;

/// Parameters that will drive the fundsp audio graph.
#[derive(Resource)]
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

/// The running audio engine state.
#[derive(Resource)]
pub struct MusicEngine {
    pub active_mood: Mood,
    pub initialized: bool,
    pub last_intensity: Fixed,
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

/// Read game state and update MusicParams every frame.
#[allow(clippy::too_many_arguments)]
pub fn sync_music_params(
    encounters: Res<SpawnedEncounters>,
    enemies: Query<&EnemyShip>,
    systems: Res<ShipSystems>,
    location: Res<CurrentLocation>,
    mode: Res<State<GameMode>>,
    settings: Res<Settings>,
    mut params: ResMut<MusicParams>,
    mut engine: ResMut<MusicEngine>,
) {
    let combat_active = encounters.seed.is_some() && enemies.iter().count() > 0;
    let hull_hp = systems.hull_hp.0.clamp(0, 1024);
    let hull_damage_pct = ((1024 - hull_hp) * 100 / 1024) as u8;
    let is_docked = location.is_docked;
    let in_derelict = location.hostile_location_id.is_some();

    let target_mood = music_mood_for_context(combat_active, hull_damage_pct, in_derelict, is_docked);
    let intensity = music_intensity(combat_active, hull_damage_pct, target_mood);
    let intensity_f = intensity.0 as f32 / Fixed::SCALE as f32;
    let music_gain = settings.audio.master_volume * settings.audio.music_volume;

    let (melody, bass, drone, rhythm, tempo, cutoff): (f32, f32, f32, f32, f32, f32) =
        match target_mood {
            Mood::Calm => (0.3, 0.1, 0.1, 0.0, 1.0, 8000.0),
            Mood::Tense => (0.4, 0.2, 0.05, 0.1, 1.2, 4000.0),
            Mood::Combat => (0.25, 0.3, 0.05, 0.4, 1.5, 2000.0),
            Mood::Derelict => (0.1, 0.15, 0.3, 0.0, 0.7, 600.0),
        };

    let mod_factor = 0.5 + 0.5 * intensity_f;
    let filter_factor = 1.0 - intensity_f * 0.6;

    params.intensity = intensity_f;
    params.melody_gain = melody * mod_factor * music_gain;
    params.bass_gain = bass * mod_factor * music_gain;
    params.drone_gain = drone * mod_factor * music_gain;
    params.rhythm_gain = rhythm * mod_factor * music_gain;
    params.master_gain = music_gain;
    params.tempo_scale = tempo * (0.8 + 0.4 * intensity_f);
    params.filter_cutoff = cutoff * filter_factor;
    params.distortion = intensity_f * 0.3;

    if *mode == GameMode::Paused {
        params.master_gain *= 0.3;
    }
    if settings.audio.mute_when_unfocused {
        params.master_gain = 0.0;
    }

    if engine.active_mood != target_mood {
        engine.active_mood = target_mood;
        engine.music_seed = engine.last_intensity.0 as u64;
    }
    engine.last_intensity = intensity;
}

/// Spawn the music resources.
pub fn setup_music(mut commands: Commands) {
    commands.init_resource::<MusicParams>();
    commands.init_resource::<MusicEngine>();
}

/// Tick the music engine — logs MusicIntent generation.
/// The fundsp streaming audio graph (Sequencer + Shared params + cpal
/// thread) ships as a follow-up. See S48 milestone 4 in the brief.
pub fn tick_music(mut engine: ResMut<MusicEngine>) {
    if !engine.initialized {
        engine.initialized = true;
        engine.music_seed = 4242;
        let intent = generate_music_intent(engine.music_seed, engine.active_mood, 8);
        info!(
            "Music engine ready: mood={:?}, {} notes, {} bpm — fundsp streaming graph TBD",
            engine.active_mood,
            intent.notes.len(),
            intent.bpm
        );
    }
}
