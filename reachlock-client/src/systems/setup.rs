//! World initialization (spec §5, §14 Mode 3; spec §10 Override System):
//! one seed produces a whole `GeneratedSystem` — star, orbits, asteroid
//! fields, station slots, one gate, a starfield — and this module renders all
//! of it as the 3D `SpaceFlight` scene ("Star Fox 64" view, spec §14 Mode 3).
//! The generator's 2D layout is laid out on the XZ plane (y up); flight is
//! full 6-DOF around it.
//!
//! The player's hull is resolved through the content pipeline first: an
//! authored override (spec §10) renders in place of the generated corvette
//! when one applies, and an authored GLTF (`SHIP_GLTF`) renders in place of
//! the procedural extrusion when present — the bridge treats all three the
//! same at the call site.
//!
//! The flying `PlayerShip` is intentionally NOT tagged `ModeScope`: it persists
//! across mode switches so its transform survives the full loop. Every other
//! scene entity is `ModeScope(GameMode::SpaceFlight)` and torn down on exit.

use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
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
/// overrides (spec §10 acceptance demo).
const PLAYER_HULL_ID: &str = "loup_garou";

/// Authored flight model. When an artist drops a `.glb` under the client's
/// `assets/` and names it here, the ship renders as that GLTF scene instead of
/// the procedural extrusion (spec §14: "full GLTF ship"). `None` keeps the
/// offline-first procedural fallback so the game always shows a ship.
pub const SHIP_GLTF: Option<&str> = None;

/// Marks the system gate, so `systems/jump.rs` can test jump proximity.
#[derive(Component)]
pub struct Gate;

/// Map a generator plane position (fixed-point XY) onto the flight world's XZ
/// plane at height `y`.
fn plane(pos: FixedVec2, y: f32) -> Vec3 {
    Vec3::new(pos.x.to_f32(), y, pos.y.to_f32())
}

/// Builds (or rebuilds) the 3D `SpaceFlight` scene. Skips when re-entering a
/// mode we never tore down (the pause round-trip). The `PlayerShip`, lights and
/// ambient audio are spawned only once — they persist across the session.
#[allow(clippy::too_many_arguments)]
pub fn enter_spaceflight(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    asset_server: Res<AssetServer>,
    content_index: Res<ContentIndex>,
    location: ResMut<CurrentLocation>,
    mut registry: ResMut<SceneRegistry>,
    ship: Query<Entity, With<PlayerShip>>,
    mode_entities: Query<Entity, With<ModeScope>>,
) {
    if registry.scene == Some(GameMode::SpaceFlight) {
        return; // came back from pause; scene already present
    }

    for entity in &mode_entities {
        commands.entity(entity).despawn();
    }

    let seed = location.system_seed;
    let palette = generate_palette(seed);
    let system = generate_system(seed, SYSTEM_BIOME, Fidelity::Full);

    // 3D lighting: a keylight plus soft ambient so hulls read without textures.
    commands.insert_resource(GlobalAmbientLight {
        color: bridge::color_from_palette(palette.accent),
        brightness: 220.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            ..default()
        },
        Transform::from_xyz(400.0, 800.0, 300.0).looking_at(Vec3::ZERO, Vec3::Y),
        ModeScope(GameMode::SpaceFlight),
    ));

    starfield::spawn(
        &mut commands,
        &mut meshes,
        &mut materials,
        system.starfield_seed,
    );

    if ship.is_empty() {
        spawn_player_ship(
            &mut commands,
            &mut meshes,
            &mut materials,
            &asset_server,
            &palette,
            &content_index,
            seed,
        );
    }

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
        spawn_planet(
            &mut commands,
            &mut meshes,
            &mut materials,
            &mut images,
            orbit,
        );
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
    spawn_gate_marker(
        &mut commands,
        &mut meshes,
        &mut materials,
        &palette,
        system.gate_position,
    );

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

/// The player's ship. Priority: authored GLTF (`SHIP_GLTF`) → authored hull
/// mesh (content override) → generated corvette, extruded to a 3D solid.
/// NOT tagged `ModeScope`: the ship persists across the mode loop.
#[allow(clippy::too_many_arguments)]
fn spawn_player_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    palette: &Palette,
    content_index: &ContentIndex,
    seed: u64,
) {
    let params = SeedParams {
        object_id: PLAYER_HULL_ID.into(),
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
    let depth = (collider_radius * 0.4).max(2.0);
    let mut ship = commands.spawn((
        PlayerShip,
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::default(),
        RigidBody::Dynamic,
        GravityScale(0.0),
        Collider::ball(collider_radius),
        Velocity::default(),
        ExternalForce::default(),
        Damping {
            linear_damping: 0.3,
            angular_damping: 5.0,
        },
    ));
    if let Some(path) = SHIP_GLTF {
        let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(path));
        ship.insert(SceneRoot(scene));
    } else {
        ship.insert((
            Mesh3d(meshes.add(bridge::mesh3d_from_generated(&hull, depth))),
            MeshMaterial3d(materials.add(bridge::standard_material_from_palette(palette.primary))),
        ));
    }
}

fn spawn_station(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    palette: &Palette,
    slot: &StationSlot,
    index: usize,
) {
    let station = generator::generate_station(slot.seed, slot.kind, 2);
    let radius = bounding_radius(&station.exterior);
    commands.spawn((
        Mesh3d(meshes.add(bridge::mesh3d_from_generated(
            &station.exterior,
            radius * 0.5,
        ))),
        MeshMaterial3d(materials.add(bridge::standard_material_from_palette(palette.structure))),
        Transform::from_translation(plane(slot.position, 0.0)),
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

fn spawn_planet(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    orbit: &Orbit,
) {
    let planet = generator::generate_planet(orbit.seed, orbit.planet_radius, orbit.biome);
    let r = orbit.planet_radius as f32;
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(r))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(images.add(bridge::image_from_generated(&planet.surface))),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_translation(plane(orbit.position, 0.0)),
        Contact,
        ModeScope(GameMode::SpaceFlight),
    ));
}

fn asteroid_field_seed(system_seed: u64, index: usize) -> u64 {
    system_seed ^ 0xA57E_A01D_0000_0000 ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

fn spawn_asteroid_field(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
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
        // Scatter rocks a little off the plane so the field reads as a volume.
        let y = (rng.next_below(120) as f32) - 60.0;

        let radius = bounding_radius(&mesh);
        commands.spawn((
            Mesh3d(meshes.add(bridge::mesh3d_from_generated(&mesh, radius * 0.8))),
            MeshMaterial3d(
                materials.add(bridge::standard_material_from_palette(palette.structure)),
            ),
            Transform::from_translation(plane(position, y)),
            RigidBody::Fixed,
            Collider::ball(radius),
            Contact,
            ModeScope(GameMode::SpaceFlight),
        ));
    }
}

/// A glowing torus marking the gate — the one landmark every system guarantees
/// (spec §5: "exactly one gate").
fn spawn_gate_marker(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    palette: &Palette,
    position: FixedVec2,
) {
    commands.spawn((
        Mesh3d(meshes.add(Torus::new(150.0, 165.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: bridge::color_from_palette(palette.accent),
            emissive: {
                let c = bridge::color_from_palette(palette.accent).to_linear();
                LinearRgba::rgb(c.red * 2.0, c.green * 2.0, c.blue * 2.0)
            },
            ..default()
        })),
        Transform::from_translation(plane(position, 0.0)),
        Contact,
        ModeScope(GameMode::SpaceFlight),
        Gate,
    ));
}

fn polar_offset(radius: i64, turn: u16) -> FixedVec2 {
    let r = Fixed::from_int(radius);
    FixedVec2 {
        x: Fixed(r.0 * icos(turn) as i64 / 32768),
        y: Fixed(r.0 * isin(turn) as i64 / 32768),
    }
}

/// Rough render-only collision radius: the farthest vertex from the mesh's
/// local origin. `to_f32` is fine here — colliders are render/physics geometry.
fn bounding_radius(mesh: &GeneratedMesh) -> f32 {
    mesh.vertices
        .iter()
        .map(|v| v.x.to_f32().abs().max(v.y.to_f32().abs()))
        .fold(0.0_f32, f32::max)
        .max(2.0)
}
