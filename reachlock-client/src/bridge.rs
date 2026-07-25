//! Core → Bevy conversion layer (spec §5, Bridge Layer). Thin by design:
//! plain data in, Bevy asset out. The bridge never inspects gameplay state
//! and never cares whether the data was generated or authored.

use bevy::asset::RenderAssetUsages;
use bevy::audio::AudioSource;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use reachlock_core::generator::GeneratedMesh;

/// GeneratedMesh → a solid 3D bevy Mesh by extruding the flat hull outline
/// along Z (spec §14 Mode 3: the flight view is 3D). The generated hull is a
/// flat XY triangle list; we lift it into a low-poly solid with a front face,
/// a back face, and side walls around the silhouette, then bake flat normals
/// so it reads as a faceted ship without a texture.
///
/// This is the offline-first procedural fallback for the flight model. When an
/// authored `.glb` is present (`setup::SHIP_GLTF`), a GLTF scene renders in its
/// place — the bridge treats both identically at the call site.
pub fn mesh3d_from_generated(gen: &GeneratedMesh, depth: f32) -> Mesh {
    let hd = depth * 0.5;
    let n = gen.vertices.len();
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(n * 2);
    for v in &gen.vertices {
        positions.push([v.x.to_f32(), v.y.to_f32(), hd]);
    }
    for v in &gen.vertices {
        positions.push([v.x.to_f32(), v.y.to_f32(), -hd]);
    }

    let mut indices: Vec<u32> = Vec::new();
    // Front face: the hull triangles as authored (facing +Z).
    for tri in gen.indices.chunks_exact(3) {
        indices.extend_from_slice(&[tri[0], tri[1], tri[2]]);
    }
    // Back face: same triangles, reversed winding, on the -Z copy.
    let off = n as u32;
    for tri in gen.indices.chunks_exact(3) {
        indices.extend_from_slice(&[off + tri[0], off + tri[2], off + tri[1]]);
    }
    // Side walls: quads along every silhouette edge (an edge used by exactly
    // one triangle). Undirected edge counting finds the outline.
    let mut edge_count: std::collections::HashMap<(u32, u32), i32> =
        std::collections::HashMap::new();
    for tri in gen.indices.chunks_exact(3) {
        for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = if a < b { (a, b) } else { (b, a) };
            *edge_count.entry(key).or_insert(0) += 1;
        }
    }
    for tri in gen.indices.chunks_exact(3) {
        for &(a, b) in &[(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = if a < b { (a, b) } else { (b, a) };
            if edge_count.get(&key) == Some(&1) {
                let (fa, fb) = (a, b);
                let (ba, bb) = (off + a, off + b);
                indices.extend_from_slice(&[fa, ba, fb, fb, ba, bb]);
            }
        }
    }

    let uvs: Vec<[f32; 2]> = vec![[0.0, 0.0]; positions.len()];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh.duplicate_vertices();
    mesh.compute_flat_normals();
    mesh
}

/// Core palette color → bevy `StandardMaterial` (unlit-ish PBR body used for
/// hulls/stations/rocks in the 3D flight scene).
pub fn standard_material_from_palette(
    c: reachlock_core::util::color::ColorRgba8,
) -> bevy::pbr::StandardMaterial {
    bevy::pbr::StandardMaterial {
        base_color: Color::srgba_u8(c.r, c.g, c.b, c.a),
        perceptual_roughness: 0.7,
        metallic: 0.1,
        ..Default::default()
    }
}

/// GeneratedTexture → bevy Image (RGBA8, nearest filtering — procedural
/// pixels should stay crisp).
pub fn image_from_generated(tex: &reachlock_core::generator::GeneratedTexture) -> Image {
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let mut image = Image::new(
        Extent3d {
            width: tex.width,
            height: tex.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        tex.pixels.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.sampler = ImageSampler::nearest();
    image
}

/// Core palette color → bevy Color.
pub fn color_from_palette(c: reachlock_core::util::color::ColorRgba8) -> Color {
    Color::srgba_u8(c.r, c.g, c.b, c.a)
}

/// GeneratedAudio → bevy AudioSource, via core's WAV container encoding
/// (bevy_audio is built with the "wav" feature).
pub fn audio_from_generated(audio: &reachlock_core::generator::GeneratedAudio) -> AudioSource {
    AudioSource {
        bytes: reachlock_core::generator::music::to_wav_bytes(audio).into(),
    }
}
