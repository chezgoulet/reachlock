//! World initialization: one system's worth of generated space — the
//! player's hull, a station, a planet — all from seeds through the bridge.

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use reachlock_core::generator::{self, hull::HullClass, station::StationKind};
use reachlock_core::seed::types::Biome;
use reachlock_core::util::color::generate_palette;

use crate::bridge;
use crate::systems::ship::{PlayerShip, ShipSystems};

pub const SYSTEM_SEED: u64 = 0x5EED_0001;

pub fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
) {
    let palette = generate_palette(SYSTEM_SEED);

    // The player's ship: generated hull, dynamic body, damped drift.
    let hull = generator::hull::generate_hull_class(SYSTEM_SEED ^ 0x51119, HullClass::Corvette);
    commands.spawn((
        PlayerShip,
        Mesh2d(meshes.add(bridge::mesh_from_generated(&hull))),
        MeshMaterial2d(materials.add(bridge::color_from_palette(palette.primary))),
        Transform::from_xyz(0.0, 0.0, 1.0),
        RigidBody::Dynamic,
        Collider::ball(20.0),
        Velocity::default(),
        ExternalForce::default(),
        Damping {
            linear_damping: 0.6,
            angular_damping: 4.0,
        },
    ));

    // A trade station hanging in the distance.
    let station = generator::generate_station(SYSTEM_SEED ^ 0x57A710, StationKind::Trade, 2);
    commands.spawn((
        Mesh2d(meshes.add(bridge::mesh_from_generated(&station.exterior))),
        MeshMaterial2d(materials.add(bridge::color_from_palette(palette.structure))),
        Transform::from_xyz(500.0, 300.0, 0.0),
        RigidBody::Fixed,
        Collider::ball(120.0),
    ));

    // A planet: generated disc + fbm surface texture as a sprite backdrop.
    let planet = generator::generate_planet(SYSTEM_SEED ^ 0x914A57, 100, Biome::Frontier);
    commands.spawn((
        Sprite {
            image: images.add(bridge::image_from_generated(&planet.surface)),
            custom_size: Some(Vec2::splat(400.0)),
            ..default()
        },
        Transform::from_xyz(-700.0, -400.0, -1.0),
    ));

    // A lyon-drawn nav beacon ring around the station, proving ShapePlugin
    // shares the scene with mesh rendering.
    let ring = shapes::RegularPolygon {
        sides: 32,
        feature: shapes::RegularPolygonFeature::Radius(160.0),
        ..default()
    };
    commands.spawn((
        ShapeBuilder::with(&ring)
            .stroke((bridge::color_from_palette(palette.accent), 2.0))
            .build(),
        Transform::from_xyz(500.0, 300.0, 0.5),
    ));

    // System ambience from the music generator.
    let music = generator::generate_music(SYSTEM_SEED, generator::Mood::Calm, 8);
    commands.spawn(AudioPlayer(
        audio_sources.add(bridge::audio_from_generated(&music)),
    ));

    commands.insert_resource(ShipSystems::default());
}
