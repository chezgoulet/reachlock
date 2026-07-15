//! Parallax starfield background (spec §5, §14 Mode 3). In the 3D flight view
//! the depth is real: stars are scattered on a large sphere shell around the
//! origin, so the ship's own motion produces the parallax for free — no
//! per-frame drift system needed. Positions come from a fibonacci sphere
//! (even, deterministic) with per-star brightness from `generate_starfield`.

use bevy::prelude::*;
use reachlock_core::generator::system::{generate_starfield, Fidelity};

/// Radius of the star shell (world units) — far enough to read as "infinitely
/// distant" relative to the playable volume.
const SHELL: f32 = 6000.0;
/// Golden angle for fibonacci-sphere distribution.
const GOLDEN_ANGLE: f32 = 2.399_963_2;

/// Spawns the starfield as emissive points on a sphere shell. Each star is a
/// `ModeScope(SpaceFlight)` entity so it tears down with the rest of the scene.
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    starfield_seed: u64,
) {
    use crate::states::{GameMode, ModeScope};

    let points = generate_starfield(starfield_seed, Fidelity::Full);
    let n = points.len().max(1);
    let dot = meshes.add(Sphere::new(6.0));
    let star_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        emissive: LinearRgba::rgb(2.0, 2.0, 2.2),
        unlit: true,
        ..default()
    });

    commands
        .spawn((
            Transform::default(),
            Visibility::default(),
            ModeScope(GameMode::SpaceFlight),
        ))
        .with_children(|parent| {
            for (i, point) in points.iter().enumerate() {
                let y = 1.0 - (i as f32 / n as f32) * 2.0;
                let radius_xz = (1.0 - y * y).max(0.0).sqrt();
                let theta = i as f32 * GOLDEN_ANGLE;
                let dir = Vec3::new(theta.cos() * radius_xz, y, theta.sin() * radius_xz);
                let b = (point.brightness as f32 / 255.0).clamp(0.2, 1.0);
                parent.spawn((
                    Mesh3d(dot.clone()),
                    MeshMaterial3d(star_mat.clone()),
                    Transform::from_translation(dir * SHELL).with_scale(Vec3::splat(b * 2.0)),
                ));
            }
        });
}
