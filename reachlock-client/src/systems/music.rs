//! Procedural audio engine (S48): real-time seeded music via fundsp.
//! Core `MusicIntent` generator in `reachlock_core::generator::music`.
//! This module bridges game state into `MusicParams` and runs the
//! fundsp streaming engine on a cpal audio thread (native only).

use bevy::prelude::*;

use reachlock_core::generator::music::{
    generate_music_intent, music_intensity, music_mood_for_context, Mood,
};
use reachlock_core::util::Fixed;

use crate::settings::Settings;
use crate::states::{CurrentLocation, GameMode};
use crate::systems::combat::{EnemyShip, SpawnedEncounters};
use crate::systems::ship::ShipSystems;

/// Parameters that drive the fundsp audio graph.
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
    pub mood_changed: bool,
}

impl Default for MusicEngine {
    fn default() -> Self {
        MusicEngine {
            active_mood: Mood::Calm,
            initialized: false,
            last_intensity: Fixed::from_int(0),
            music_seed: 0,
            mood_changed: false,
        }
    }
}

// -----------------------------------------------------------------------
// StreamingEngine — different on native vs WASM
// -----------------------------------------------------------------------
mod engine {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use fundsp::prelude64::*;
    use fundsp::sequencer::{Fade, ReplayMode, Sequencer};
    use fundsp::shared::Shared;

    use bevy::prelude::Resource;
    use reachlock_core::generator::music::MusicIntent;

    use super::MusicEngine;

    /// The native fundsp streaming engine.
    #[derive(Resource)]
    pub struct StreamingEngine {
        pub sequencer: Sequencer,
        tempo_s: Shared,
        gain_s: Shared,
        intensity_s: Shared,
        _handle: std::thread::JoinHandle<()>,
    }

    impl StreamingEngine {
        pub fn new() -> Self {
            let tempo_s = shared(1.0);
            let gain_s = shared(0.5);
            let intensity_s = shared(0.0);

            let mut sequencer = Sequencer::new(0, 2, ReplayMode::None);

            let drone = pink() >> mul(0.05f32) >> pan(0.0);
            sequencer
                .push_relative(0.0, f64::INFINITY, Fade::Smooth, 1.0, 1.0, Box::new(drone));

            let gain_s2 = gain_s.clone();

            // Need to get the backend in the closure but keep frontend here.
            // Workaround: clone the sequencer before backend split.
            let mut seq_for_thread = sequencer.clone();
            let handle = std::thread::spawn(move || {
                let host = cpal::default_host();
                let device = match host.default_output_device() {
                    Some(d) => d,
                    None => {
                        log::warn!("Audio: no output device available");
                        return;
                    }
                };
                let config = match device.default_output_config() {
                    Ok(c) => c,
                    Err(e) => {
                        log::warn!("Audio: failed to get default config: {e}");
                        return;
                    }
                };
                let sample_rate = config.sample_rate().0 as f64;

                let mut backend = seq_for_thread.backend();
                backend.set_sample_rate(sample_rate);

                let mut net = Net::wrap(Box::new(backend));
                net.allocate();

                let stream = match device.build_output_stream(
                    &config.into(),
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        for frame in data.chunks_mut(2) {
                            let (l, r) = net.get_stereo();
                            let g = gain_s2.value();
                            frame[0] = l * g;
                            if frame.len() > 1 {
                                frame[1] = r * g;
                            }
                        }
                    },
                    |err| log::error!("Audio stream error: {err}"),
                    None,
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Audio: failed to build output stream: {e}");
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    log::error!("Audio: failed to play stream: {e}");
                    return;
                }
                std::thread::park();
            });

            StreamingEngine {
                sequencer,
                tempo_s,
                gain_s,
                intensity_s,
                _handle: handle,
            }
        }

        pub fn update(&mut self, tempo: f64, gain: f64, intensity: f64) {
            self.tempo_s.set(tempo as f32);
            self.gain_s.set(gain as f32);
            self.intensity_s.set(intensity as f32);
        }
    }

    /// Schedule all NoteEvents into the Sequencer.
    pub fn schedule_intent(
        seq: &mut Sequencer,
        intent: &MusicIntent,
        _engine: &MusicEngine,
    ) {
        let bpm = intent.bpm as f64;
        let tick_sec = 60.0 / bpm / 24.0;
        let total_dur = intent
            .notes
            .last()
            .map(|n| n.start_tick + n.duration_ticks)
            .unwrap_or(96) as f64
            * tick_sec
            + 1.0;

        // Melody notes.
        for note in &intent.notes {
            if note.velocity == 0 {
                continue;
            }
            let freq = degree_freq(note.degree, note.octave, intent.root_hz);
            let start_sec = note.start_tick as f64 * tick_sec;
            let dur_sec = note.duration_ticks as f64 * tick_sec;
            let vel = note.velocity as f32 / 127.0;

            let graph = (sine_hz(freq as f32) * (vel * 0.2f32)) >> pan(0.0);
            seq.push_relative(
                start_sec,
                dur_sec + 0.03,
                Fade::Smooth,
                0.005,
                0.01,
                Box::new(graph),
            );
        }

        // Bass drone.
        let bass_root = intent.root_hz as f64 * 0.5;
        let bass = (triangle_hz(bass_root as f32) * 0.12f32) >> pan(0.0);
        seq.push_relative(0.0, total_dur, Fade::Smooth, 0.5, 0.5, Box::new(bass));

        // Rhythm — noise bursts at each quarter note.
        let beat_interval = 60.0 / bpm;
        let total_beats = (total_dur / beat_interval).ceil() as u32;
        for beat in 0..total_beats {
            let beat_sec = beat as f64 * beat_interval;
            if beat_sec > total_dur {
                break;
            }
            let hat = (noise() * 0.06f32) >> pan(0.0);
            seq.push_relative(
                beat_sec,
                beat_sec + 0.04,
                Fade::Smooth,
                0.0,
                0.02,
                Box::new(hat),
            );
        }
    }

    fn degree_freq(degree: u8, octave: u8, root_hz: u32) -> f64 {
        let semitones = (degree as i32 + octave as i32 * 12 - 12)
            .clamp(0, 108) as u32;
        let ratio = 2.0f64.powf(semitones as f64 / 12.0);
        root_hz as f64 * ratio
    }
}

pub use engine::*;

// -----------------------------------------------------------------------
// Game-state bridge systems
// -----------------------------------------------------------------------

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
        engine.mood_changed = true;
        engine.active_mood = target_mood;
        engine.music_seed = engine.last_intensity.0 as u64;
    }
    engine.last_intensity = intensity;
}

/// Spawn the music resources. On native, also starts the fundsp cpal thread.
pub fn setup_music(mut commands: Commands) {
    commands.init_resource::<MusicParams>();
    commands.init_resource::<MusicEngine>();
    commands.insert_resource(StreamingEngine::new());
}

/// Tick the music engine — pushes NoteEvents into the fundsp Sequencer.
pub fn tick_music(
    mut engine: ResMut<MusicEngine>,
    params: Res<MusicParams>,
    mut streaming: ResMut<StreamingEngine>,
) {
    streaming.update(
        params.tempo_scale as f64,
        params.master_gain as f64,
        params.intensity as f64,
    );

    if !engine.initialized {
        engine.initialized = true;
        engine.music_seed = 4242;
        let intent = generate_music_intent(engine.music_seed, engine.active_mood, 8);
        info!(
            "Music engine: mood={:?}, {} notes, {} bpm",
            engine.active_mood,
            intent.notes.len(),
            intent.bpm
        );
        schedule_intent(&mut streaming.sequencer, &intent, &engine);
    }

    if engine.mood_changed {
        engine.mood_changed = false;
        let intent = generate_music_intent(engine.music_seed, engine.active_mood, 8);
        schedule_intent(&mut streaming.sequencer, &intent, &engine);
    }
}
