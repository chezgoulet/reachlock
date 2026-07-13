//! World initialization (spec §5, §14 Mode 3; spec §10 Override System):
//! one seed produces a whole `GeneratedSystem` — star, orbits, asteroid
//! fields, station slots, one gate, a starfield — and this module renders
//! all of it as the `SpaceFlight` scene. The player's hull is resolved
//! through the content pipeline first: an authored override (spec §10)
//! renders in place of the generated corvette when one applies.
//!
//! The flying `PlayerShip` is intentionally NOT tagged `ModeScope`: it
//! persists across mode switches (Landed/OnBoard/Paused) so its transform —
//! and therefore position — survives the full loop. Every other scene entity
//! is `ModeScope(GameMode::SpaceFlight)` and is torn down on mode exit by
//! `systems/mode::teardown_mode`.

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
use reachlock_core::content::{resolve, ContentPayload, Resolved, SeedParams};
use reachlock_core::generator::hull::HullClass;
use reachlock_core::generator::system::{
    generate_system, AsteroidField, Fidelity, Orbit, StationSlot,
};
use reachlock_core::generator::{self, FixedVec2, GeneratedMesh};
use reachlock_core::seed::types::Biome;
use reachlock_core::universe::tier::UniverseTier;
use reachlock_core::util::color::{generate_palette, Palette};
use reachlock_core::util::rng::{Fixed, SeededRng};
use reachlock_core::util::trig::{icos, isin};

use crate::bridge;
use crate::states::{CurrentLocation, GameMode, ModeScope, SceneRegistry};
use crate::systems::content_index::ContentIndex;
use crate::systems::docking::Dockable;
use crate::systems::sensors::{Contact, KnownContacts};
use crate::systems::ship::{PlayerShip, ShipSystems};
use crate::systems::starfield;

pub const SYSTEM_SEED: u64 = 0x5EED_0001;
pub const SYSTEM_BIOME: Biome = Biome::Frontier;

/// Object id the authored Loup-Garou hull (`content/hulls/loup_garou.ron`)
/// overrides (spec §10 acceptance demo: "the authored Loup-Garou hull
/// replaces the generated corvette").
const PLAYER_HULL_ID: &str = "loup_garou";

/// Marks the system gate, so `systems/jump.rs` can test jump proximity.
#[derive(Component)]
pub struct Gate;

/// Builds (or rebuilds) the `SpaceFlight` scene. Skips entirely when
/// re-entering a mode we never tore down (the pause round-trip). The
/// `PlayerShip` and ambient audio are spawned only once — they persist
/// across the whole session.
#[allow(clippy::too_many_arguments)]
pub fn enter_spaceflight(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    content_index: Res<ContentIndex>,
    location: ResMut<CurrentLocation>,
    mut registry: ResMut<SceneRegistry>,
    ship: Query<Entity, With<PlayerShip>>,
    mode_entities: Query<Entity, With<ModeScope>>,
) {
    if registry.scene == Some(GameMode::SpaceFlight) {
        return; // came back from pause; scene already present
    }

    // Tearing down the previous scene (Landed/OnBoard) before building space.
    for entity in &mode_entities {
        commands.entity(entity).despawn();
    }

    // S09 (S02 integrator carry-over): the world is rebuilt from the
    // *current* system seed, not a hardcoded constant. Jumping sets
    // `location.system_seed` to the destination so re-entering SpaceFlight
    // regenerates into the new system.
    let seed = location.system_seed;
    let palette = generate_palette(seed);
    let system = generate_system(seed, SYSTEM_BIOME, Fidelity::Full);

    starfield::spawn(
        &mut commands,
        &mut meshes,
        &mut materials,
        system.starfield_seed,
    );

    spawn_player_ship(
        &mut commands,
        &mut meshes,
        &mut materials,
        &palette,
        &content_index,
        seed,
    );

    for (index, slot) in system.station_slots.iter().enumerate() {
        spawn_station(
            &mut commands,
            &mut meshes,
            &mut materials,
            &palette,
            slot,
            index,
        );
    }
    for orbit in &system.orbits {
        spawn_planet(&mut commands, &mut images, orbit);
    }
    for (index, field) in system.asteroid_fields.iter().enumerate() {
        spawn_asteroid_field(
            &mut commands,
            &mut meshes,
            &mut materials,
            &palette,
            seed,
            index,
            field,
        );
    }
    spawn_gate_marker(&mut commands, &palette, system.gate_position);

    // System ambience from the music generator. Spawned once (the ship
    // query is empty only on the very first entry).
    if ship.is_empty() {
        let music = generator::generate_music(seed, generator::Mood::Calm, 8);
        commands.spawn(AudioPlayer(
            audio_sources.add(bridge::audio_from_generated(&music)),
        ));
        commands.insert_resource(ShipSystems::default());
        commands.insert_resource(KnownContacts::default());
    }

    registry.scene = Some(GameMode::SpaceFlight);
}

/// The player's ship: an authored hull if the content pipeline resolves
/// one for `PLAYER_HULL_ID`, otherwise the generated corvette (spec §10,
/// "the bridge cannot tell the difference" — this is the one place that
/// makes the choice; `bridge::mesh_from_generated` treats both identically).
/// NOT tagged `ModeScope`: the ship persists across the mode loop.
fn spawn_player_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    palette: &Palette,
    content_index: &ContentIndex,
    seed: u64,
) {
    let params = SeedParams {
        object_id: PLAYER_HULL_ID.into(),
        // Offline client, no server-negotiated tier yet — Classic is the
        // rules-only, no-inference default (mirrors reachlock-server's
        // `auth::default_universe`).
        universe: UniverseTier::Classic,
        now: 0,
    };
    let hull = match resolve(&content_index.files, &params) {
        Resolved::Authored(file) => match file.payload {
            ContentPayload::Hull(mesh) => mesh,
            other => {
                warn!(
                    "content override for {PLAYER_HULL_ID} is not a hull payload \
                         ({other:?}); falling back to the generated corvette"
                );
                generator::hull::generate_hull_class(seed ^ 0x51119, HullClass::Corvette)
            }
        },
        Resolved::Procedural => {
            generator::hull::generate_hull_class(seed ^ 0x51119, HullClass::Corvette)
        }
    };

    let collider_radius = bounding_radius(&hull);
    commands.spawn((
        PlayerShip,
        Mesh2d(meshes.add(bridge::mesh_from_generated(&hull))),
        MeshMaterial2d(materials.add(bridge::color_from_palette(palette.primary))),
        Transform::from_xyz(0.0, 0.0, 1.0),
        RigidBody::Dynamic,
        Collider::ball(collider_radius),
        Velocity::default(),
        ExternalForce::default(),
        Damping {
            linear_damping: 0.6,
            angular_damping: 4.0,
        },
    ));
}

fn spawn_station(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    palette: &Palette,
    slot: &StationSlot,
    index: usize,
) {
    let station = generator::generate_station(slot.seed, slot.kind, 2);
    let radius = bounding_radius(&station.exterior);
    commands.spawn((
        Mesh2d(meshes.add(bridge::mesh_from_generated(&station.exterior))),
        MeshMaterial2d(materials.add(bridge::color_from_palette(palette.structure))),
        Transform::from_xyz(slot.position.x.to_f32(), slot.position.y.to_f32(), 0.0),
        RigidBody::Fixed,
        Collider::ball(radius),
        Dockable {
            seed: slot.seed,
            kind: slot.kind,
            station_id: format!("station-{index}"),
        },
        Contact,
        ModeScope(GameMode::SpaceFlight),
    ));
}

fn spawn_planet(commands: &mut Commands, images: &mut Assets<Image>, orbit: &Orbit) {
    let planet = generator::generate_planet(orbit.seed, orbit.planet_radius, orbit.biome);
    commands.spawn((
        Sprite {
            image: images.add(bridge::image_from_generated(&planet.surface)),
            custom_size: Some(Vec2::splat(orbit.planet_radius as f32 * 2.0)),
            ..default()
        },
        Transform::from_xyz(orbit.position.x.to_f32(), orbit.position.y.to_f32(), -1.0),
        Contact,
        ModeScope(GameMode::SpaceFlight),
    ));
}

/// Derives a stable per-field seed from the world seed and the field's
/// index in `GeneratedSystem::asteroid_fields`. `AsteroidField` itself
/// carries no seed (it's a frozen S04 field list) — deriving it here keeps
/// rock placement deterministic without touching core.
fn asteroid_field_seed(system_seed: u64, index: usize) -> u64 {
    system_seed ^ 0xA57E_A01D_0000_0000 ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// Scatters `field.density` `HullClass::Rock` hulls inside `field.radius`
/// of `field.center` (spec §5 deliverable: "asteroid clusters (reuse the
/// hull generator's `HullClass::Rock` ... `density` = rock count)").
fn spawn_asteroid_field(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    palette: &Palette,
    system_seed: u64,
    index: usize,
    field: &AsteroidField,
) {
    let mut rng = SeededRng::new(asteroid_field_seed(system_seed, index));
    for _ in 0..field.density {
        let rock_seed = rng.next_u64();
        let mesh = generator::hull::generate_hull_class(rock_seed, HullClass::Rock);

        let offset_radius = rng.next_below(field.radius.max(1) as u64) as i64;
        let turn = rng.next_below(65536) as u16;
        let offset = polar_offset(offset_radius, turn);
        let position = FixedVec2 {
            x: Fixed(field.center.x.0 + offset.x.0),
            y: Fixed(field.center.y.0 + offset.y.0),
        };

        let radius = bounding_radius(&mesh);
        commands.spawn((
            Mesh2d(meshes.add(bridge::mesh_from_generated(&mesh))),
            MeshMaterial2d(materials.add(bridge::color_from_palette(palette.structure))),
            Transform::from_xyz(position.x.to_f32(), position.y.to_f32(), 0.0),
            RigidBody::Fixed,
            Collider::ball(radius),
            Contact,
            ModeScope(GameMode::SpaceFlight),
        ));
    }
}

/// A lyon-drawn ring marking the gate — keeps `ShapePlugin` sharing the
/// scene with mesh rendering, now anchored to the one landmark every
/// system guarantees (spec §5: "exactly one gate").
fn spawn_gate_marker(commands: &mut Commands, palette: &Palette, position: FixedVec2) {
    let ring = shapes::RegularPolygon {
        sides: 32,
        feature: shapes::RegularPolygonFeature::Radius(160.0),
        ..default()
    };
    commands.spawn((
        ShapeBuilder::with(&ring)
            .stroke((bridge::color_from_palette(palette.accent), 2.0))
            .build(),
        Transform::from_xyz(position.x.to_f32(), position.y.to_f32(), 0.5),
        Contact,
        ModeScope(GameMode::SpaceFlight),
        Gate,
    ));
}

/// Same polar construction as `generator::system`'s private helper — kept
/// local since that one isn't part of the public contract, and this is a
/// render-side placement detail (rock scatter), not a gameplay one.
fn polar_offset(radius: i64, turn: u16) -> FixedVec2 {
    let r = Fixed::from_int(radius);
    FixedVec2 {
        x: Fixed(r.0 * icos(turn) as i64 / 32768),
        y: Fixed(r.0 * isin(turn) as i64 / 32768),
    }
}

/// Rough render-only collision radius: the farthest vertex from the mesh's
/// local origin. `to_f32` is fine here — colliders are render/physics
/// geometry, not gameplay state.
fn bounding_radius(mesh: &GeneratedMesh) -> f32 {
    mesh.vertices
        .iter()
        .map(|v| v.x.to_f32().abs().max(v.y.to_f32().abs()))
        .fold(0.0_f32, f32::max)
        .max(2.0)
}
