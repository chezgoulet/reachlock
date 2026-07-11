//! Spike deliverable #1 (spec §2, WASM Build Risk): the full plugin stack —
//! bevy + bevy_rapier2d + bevy_prototype_lyon + bevy_audio — on both native
//! and wasm32-unknown-unknown, driving one of everything through the
//! seed → core generator → bridge → Bevy pipeline.

mod bridge;

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "ReachLock v2 — plugin stack spike".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(ShapePlugin)
        .add_plugins(RapierPhysicsPlugin::<()>::pixels_per_meter(100.0))
        .add_systems(Startup, setup)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
) {
    commands.spawn(Camera2d);

    // Generator → bridge → Bevy mesh: a seeded hull, rendered.
    let hull = reachlock_core::generator::generate_hull(0x5EED_0001);
    commands.spawn((
        Mesh2d(meshes.add(bridge::mesh_from_generated(&hull))),
        MeshMaterial2d(materials.add(Color::srgb(0.35, 0.55, 0.85))),
        Transform::from_xyz(-200.0, 100.0, 0.0),
    ));

    // Lyon: a stroked hexagon, proving bevy_prototype_lyon renders.
    let hex = shapes::RegularPolygon {
        sides: 6,
        feature: shapes::RegularPolygonFeature::Radius(60.0),
        ..default()
    };
    commands.spawn((
        ShapeBuilder::with(&hex)
            .fill(Color::srgb(0.2, 0.3, 0.2))
            .stroke((Color::srgb(0.6, 0.9, 0.6), 4.0))
            .build(),
        Transform::from_xyz(200.0, 100.0, 0.0),
    ));

    // Rapier: a dynamic ball dropping onto a fixed floor.
    commands.spawn((
        RigidBody::Fixed,
        Collider::cuboid(400.0, 20.0),
        Transform::from_xyz(0.0, -200.0, 0.0),
    ));
    commands.spawn((
        RigidBody::Dynamic,
        Collider::ball(20.0),
        Restitution::coefficient(0.8),
        Transform::from_xyz(0.0, 150.0, 0.0),
    ));

    // Audio: a seeded phrase from the core generator, through bevy_audio.
    let music = reachlock_core::generator::generate_music(
        0x5EED_0001,
        reachlock_core::generator::Mood::Calm,
        2,
    );
    commands.spawn(AudioPlayer(
        audio_sources.add(bridge::audio_from_generated(&music)),
    ));
}
