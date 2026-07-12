//! Parallax starfield background (spec §5, §14 Mode 3): a seeded point
//! cloud drawn far behind the scene. Every star is parented under one
//! `StarfieldLayer` root entity so a single system can drift the whole
//! layer at a fraction of the camera's motion — a cheap depth cue that
//! reads as "impossibly far away" without per-star bookkeeping.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use reachlock_core::generator::system::{generate_starfield, Fidelity};

/// Marker for a starfield root entity, carrying how much of the camera's
/// motion it tracks (low = reads as distant).
#[derive(Component)]
pub struct StarfieldLayer {
    pub factor: f32,
}

const STARFIELD_Z: f32 = -50.0;
const PARALLAX_FACTOR: f32 = 0.15;
const STAR_RADIUS: f32 = 1.5;
const STAR_SIDES: usize = 6;

/// Spawns the whole starfield (spec §17 fidelity knob applies here too) as
/// children of one `StarfieldLayer` root. Positions come straight from
/// `generate_starfield`'s fixed-point output — converted to f32 only here,
/// at the render boundary.
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    starfield_seed: u64,
) {
    let points = generate_starfield(starfield_seed, Fidelity::Full);
    let dot = meshes.add(dot_mesh());

    commands
        .spawn((
            StarfieldLayer {
                factor: PARALLAX_FACTOR,
            },
            Transform::from_xyz(0.0, 0.0, STARFIELD_Z),
            Visibility::default(),
        ))
        .with_children(|parent| {
            for point in &points {
                let b = point.brightness as f32 / 255.0;
                let color = Color::srgba(
                    (point.tint.r as f32 / 255.0) * b,
                    (point.tint.g as f32 / 255.0) * b,
                    (point.tint.b as f32 / 255.0) * b,
                    1.0,
                );
                parent.spawn((
                    Mesh2d(dot.clone()),
                    MeshMaterial2d(materials.add(color)),
                    Transform::from_xyz(point.position.x.to_f32(), point.position.y.to_f32(), 0.0),
                ));
            }
        });
}

/// A tiny triangle-fan hexagon, shared by every star in the layer — purely
/// cosmetic geometry (not gameplay state), so plain floats are fine here.
fn dot_mesh() -> Mesh {
    let mut positions = Vec::with_capacity(STAR_SIDES + 1);
    positions.push([0.0, 0.0, 0.0]);
    for i in 0..STAR_SIDES {
        let angle = i as f32 / STAR_SIDES as f32 * std::f32::consts::TAU;
        positions.push([angle.cos() * STAR_RADIUS, angle.sin() * STAR_RADIUS, 0.0]);
    }
    let mut indices = Vec::with_capacity(STAR_SIDES * 3);
    for i in 0..STAR_SIDES {
        let a = 1 + i as u32;
        let b = 1 + ((i + 1) % STAR_SIDES) as u32;
        indices.extend_from_slice(&[0, a, b]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Drifts every `StarfieldLayer` at a fraction of the camera's position.
pub fn parallax(
    camera: Query<&Transform, (With<Camera2d>, Without<StarfieldLayer>)>,
    mut layers: Query<(&StarfieldLayer, &mut Transform)>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    for (layer, mut transform) in &mut layers {
        transform.translation.x = camera.translation.x * layer.factor;
        transform.translation.y = camera.translation.y * layer.factor;
    }
}
