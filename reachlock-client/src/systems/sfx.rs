//! SFX player (S49): plays deterministic sound effects from game events.
//! SFX are generated deterministically from seed + kind, then played
//! through bevy_audio as WAV blobs (same path as legacy generated music).

use bevy::audio::{AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use reachlock_core::generator::sfx::{generate_sfx, SfxEvent};

use crate::settings::Settings;

/// Queue of pending SFX events. Game systems push to this; the SFX system
/// drains it every frame and plays the corresponding audio.
#[derive(Resource, Default)]
pub struct SfxQueue(pub Vec<SfxEvent>);

/// Spawn a SFX player entity.
pub fn setup_sfx(mut commands: Commands) {
    commands.init_resource::<SfxQueue>();
}

/// Drain the SFX queue and play each event through bevy_audio.
pub fn process_sfx(
    mut queue: ResMut<SfxQueue>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    settings: Res<Settings>,
    mut commands: Commands,
) {
    let gain = settings.audio.master_volume * settings.audio.sfx_volume;
    let events = std::mem::take(&mut queue.0);
    for evt in events {
        let audio = generate_sfx(evt.seed, evt.kind);
        let wav = crate::bridge::audio_from_generated(&audio);
        let source = audio_sources.add(wav);
        commands.spawn((
            AudioPlayer(source),
            PlaybackSettings {
                volume: Volume::Linear(gain * evt.gain),
                ..Default::default()
            },
        ));
    }
}

/// Helper to queue an SFX event from game systems.
#[allow(dead_code)]
pub fn play_sfx(queue: &mut SfxQueue, kind: reachlock_core::generator::sfx::SfxKind, seed: u64) {
    queue.0.push(SfxEvent::new(kind, seed));
}
