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

/// Wrap raw mono PCM in a WAV header so bevy_audio's decoder accepts it.
pub fn audio_from_samples(samples: &[i16]) -> AudioSource {
    const SAMPLE_RATE: u32 = 44100;
    let data_len = (samples.len() * 2) as u32;

    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }

    AudioSource {
        bytes: bytes.into(),
    }
}
