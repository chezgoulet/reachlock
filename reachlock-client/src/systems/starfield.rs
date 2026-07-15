//! Parallax starfield background (spec §5, §14 Mode 3). In the 3D flight view
//! the depth is real: stars are scattered on a large sphere shell around the
//! origin, so the ship's own motion produces the parallax for free — no
//! per-frame drift system needed. Positions come from a fibonacci sphere
//! (even, deterministic) with per-star brightness from `generate_starfield`.
//!
//! The shell alone can't sell motion, though: it reads as an infinitely
//! distant backdrop, and the system's planets/stations are thousands of units
//! apart. The near-field dust layer below fills the gap — motes that wrap
//! around the ship in a fixed box give every meter of flight visible
//! parallax, and stretch into streaks at speed (the SF64 boost lines).

use bevy::prelude::*;
use bevy_rapier3d::prelude::Velocity;
use reachlock_core::generator::system::{generate_starfield, Fidelity};
use reachlock_core::util::rng::SeededRng;

use crate::systems::ship::PlayerShip;

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

// --- near-field dust ---------------------------------------------------

/// Motes in the wrap box around the ship.
const DUST_COUNT: u32 = 240;
/// Half-extent of the wrap box, world units. Big enough that respawns at the
/// far face aren't noticeable, small enough that the box is always populated.
const DUST_RANGE: f32 = 420.0;
/// Speed→streak factor: at boost (~270 u/s) motes stretch ~8× into lines.
const STREAK_SCALE: f32 = 0.03;

#[derive(Component)]
pub struct DustMote;

/// Wrap one offset component into `[-range, range)` — the modular arithmetic
/// that teleports a mote the ship has flown past back in front of it.
pub fn wrap_component(v: f32, range: f32) -> f32 {
    (v + range).rem_euclid(range * 2.0) - range
}

/// Scatter the dust motes around the origin (they immediately re-wrap into
/// the box around wherever the ship is). ModeScope'd with the rest of the
/// flight scene.
pub fn spawn_dust(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    seed: u64,
) {
    use crate::states::{GameMode, ModeScope};

    let mut rng = SeededRng::new(seed ^ 0xD057_D057);
    let dot = meshes.add(Sphere::new(0.8));
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.75, 0.8, 0.95, 0.8),
        emissive: LinearRgba::rgb(0.5, 0.55, 0.75),
        unlit: true,
        ..default()
    });
    let span = (DUST_RANGE * 2.0) as u64;
    for _ in 0..DUST_COUNT {
        let mut coord = || (rng.next_below(span) as f32) - DUST_RANGE;
        let p = Vec3::new(coord(), coord(), coord());
        commands.spawn((
            Mesh3d(dot.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(p),
            DustMote,
            ModeScope(GameMode::SpaceFlight),
        ));
    }
}

/// Keep the dust box centered on the ship by wrapping motes that fall out
/// the back around to the front, and stretch them into streaks along the
/// flight vector at speed. The motes never move in world space — all the
/// parallax is the ship's own motion, which is exactly the point.
#[allow(clippy::type_complexity)]
pub fn dust_parallax(
    ship: Query<(&Transform, &Velocity), With<PlayerShip>>,
    mut motes: Query<&mut Transform, (With<DustMote>, Without<PlayerShip>)>,
) {
    let Ok((ship, velocity)) = ship.single() else {
        return;
    };
    let speed = velocity.linear.length();
    let stretch = (speed * STREAK_SCALE).clamp(1.0, 14.0);
    let streak_rot = if stretch > 1.0 {
        Quat::from_rotation_arc(Vec3::Z, velocity.linear.normalize_or(Vec3::Z))
    } else {
        Quat::IDENTITY
    };
    for mut tx in &mut motes {
        let off = tx.translation - ship.translation;
        tx.translation = ship.translation
            + Vec3::new(
                wrap_component(off.x, DUST_RANGE),
                wrap_component(off.y, DUST_RANGE),
                wrap_component(off.z, DUST_RANGE),
            );
        tx.rotation = streak_rot;
        tx.scale = Vec3::new(1.0, 1.0, stretch);
    }
}

#[cfg(test)]
mod tests {
    use super::wrap_component;

    #[test]
    fn wrap_keeps_offsets_in_box() {
        // In-range values pass through untouched.
        assert_eq!(wrap_component(10.0, 420.0), 10.0);
        assert_eq!(wrap_component(-400.0, 420.0), -400.0);
        // A mote left 100 units behind the far face re-enters at the front.
        assert_eq!(wrap_component(520.0, 420.0), -320.0);
        assert_eq!(wrap_component(-520.0, 420.0), 320.0);
        // Many box-lengths out still lands inside.
        let w = wrap_component(420.0 * 7.0 + 33.0, 420.0);
        assert!((-420.0..420.0).contains(&w), "{w}");
    }
}
