//! Planet generation (spec §5): disc mesh + fbm-shaded surface texture.

use super::{FixedVec2, GeneratedMesh, GeneratedTexture};
use crate::seed::types::Biome;
use crate::util::color::{generate_palette, ColorRgba8};
use crate::util::noise::fbm;
use crate::util::rng::Fixed;
use crate::util::trig::{icos, isin};

pub struct GeneratedPlanet {
    pub disc: GeneratedMesh,
    pub surface: GeneratedTexture,
}

/// `radius` in whole world units; texture is a fixed 64×64 RGBA image the
/// bridge maps onto the disc.
pub fn generate_planet(seed: u64, radius: i64, biome: Biome) -> GeneratedPlanet {
    GeneratedPlanet {
        disc: disc_mesh(radius),
        surface: surface_texture(seed, biome),
    }
}

fn disc_mesh(radius: i64) -> GeneratedMesh {
    const SEGMENTS: usize = 32;
    let r = Fixed::from_int(radius);
    let mut vertices = Vec::with_capacity(SEGMENTS + 1);
    vertices.push(FixedVec2 {
        x: Fixed(0),
        y: Fixed(0),
    });
    for i in 0..SEGMENTS {
        let turn = (i as u64 * 65536 / SEGMENTS as u64) as u16;
        vertices.push(FixedVec2 {
            x: Fixed(r.0 * icos(turn) as i64 / 32768),
            y: Fixed(r.0 * isin(turn) as i64 / 32768),
        });
    }
    let mut indices = Vec::with_capacity(SEGMENTS * 3);
    for i in 0..SEGMENTS {
        let a = 1 + i as u32;
        let b = 1 + ((i + 1) % SEGMENTS) as u32;
        indices.extend_from_slice(&[0, a, b]);
    }
    GeneratedMesh { vertices, indices }
}

const TEX_SIZE: u32 = 64;

/// Height thresholds (fbm output in [-32768, 32768]) per biome, dark→light.
fn bands(biome: Biome) -> [i32; 3] {
    match biome {
        Biome::Core => [-8000, 4000, 20000],
        Biome::Frontier => [-4000, 8000, 24000],
        Biome::Nebula => [-12000, 0, 16000],
        Biome::Derelict => [0, 12000, 26000],
        Biome::DeepSpace => [-16000, -4000, 12000],
    }
}

fn surface_texture(seed: u64, biome: Biome) -> GeneratedTexture {
    let palette = generate_palette(seed);
    let thresholds = bands(biome);
    let shades: [ColorRgba8; 4] = [
        palette.structure,
        palette.primary,
        palette.accent,
        ColorRgba8 {
            r: 240,
            g: 240,
            b: 235,
            a: 255,
        },
    ];

    let mut pixels = Vec::with_capacity((TEX_SIZE * TEX_SIZE * 4) as usize);
    for y in 0..TEX_SIZE {
        for x in 0..TEX_SIZE {
            // Sample fbm at 4 lattice cells across the texture.
            let h = fbm(seed, x as i64 * 16, y as i64 * 16, 4);
            let band = thresholds.iter().position(|&t| h < t).unwrap_or(3);
            let c = shades[band];
            pixels.extend_from_slice(&[c.r, c.g, c.b, c.a]);
        }
    }
    GeneratedTexture {
        width: TEX_SIZE,
        height: TEX_SIZE,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_planet(3, 100, Biome::Frontier);
        let b = generate_planet(3, 100, Biome::Frontier);
        assert_eq!(a.surface, b.surface);
        assert_eq!(a.disc, b.disc);
    }

    #[test]
    fn biomes_differ() {
        let a = generate_planet(3, 100, Biome::Frontier);
        let b = generate_planet(3, 100, Biome::DeepSpace);
        assert_ne!(a.surface, b.surface);
    }

    #[test]
    fn texture_is_full_rgba() {
        let planet = generate_planet(3, 100, Biome::Core);
        assert_eq!(planet.surface.pixels.len(), (64 * 64 * 4) as usize);
    }
}
