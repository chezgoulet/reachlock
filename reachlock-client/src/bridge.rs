//! Core → Bevy conversion layer (spec §5, Bridge Layer). Thin by design:
//! plain data in, Bevy asset out. The bridge never inspects gameplay state
//! and never cares whether the data was generated or authored.

use bevy::asset::RenderAssetUsages;
use bevy::audio::AudioSource;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use reachlock_core::generator::GeneratedMesh;

pub fn mesh_from_generated(gen: &GeneratedMesh) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        gen.vertices
            .iter()
            .map(|v| [v.x.to_f32(), v.y.to_f32(), 0.0])
            .collect::<Vec<_>>(),
    );
    mesh.insert_indices(Indices::U32(gen.indices.clone()));
    mesh
}

/// GeneratedAudio → bevy AudioSource, via core's WAV container encoding
/// (bevy_audio is built with the "wav" feature).
pub fn audio_from_generated(audio: &reachlock_core::generator::GeneratedAudio) -> AudioSource {
    AudioSource {
        bytes: reachlock_core::generator::music::to_wav_bytes(audio).into(),
    }
}
