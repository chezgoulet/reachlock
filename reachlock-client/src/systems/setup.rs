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
    shipcfg: Res<crate::systems::shipeditor::ShipConfig>,
    location: ResMut<CurrentLocation>,
    mut registry: ResMut<SceneRegistry>,
    ship: Query<Entity, With<PlayerShip>>,
    mode_entities: Query<(Entity, &ModeScope)>,
) {
    if registry.scene == Some(GameMode::SpaceFlight) {
        return; // came back from pause; scene already present
    }

    // S09d: returning to the helm from walking the ship mid-flight. The
    // space scene never went away — drop the interior overlay and hand the
    // world straight back to the pilot.
    if registry.space_alive {
        for (entity, scope) in &mode_entities {
            if scope.0 != GameMode::SpaceFlight {
                commands.entity(entity).despawn();
            }
        }
        registry.scene = Some(GameMode::SpaceFlight);
        return;
    }

    for (entity, _) in &mode_entities {
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
    starfield::spawn_dust(
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
            &shipcfg,
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
    registry.space_alive = true;
}

/// The player's ship. Priority: S17 applied exterior config (composed
/// through the SAME `compose_hull` the editor preview renders — the S17
/// gotcha) → authored GLTF (`SHIP_GLTF`) → authored hull mesh (content
/// override) → generated corvette, extruded to a 3D solid.
/// NOT tagged `ModeScope`: the ship persists across the mode loop.
#[allow(clippy::too_many_arguments)]
fn spawn_player_ship(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    asset_server: &AssetServer,
    palette: &Palette,
    content_index: &ContentIndex,
    shipcfg: &crate::systems::shipeditor::ShipConfig,
    seed: u64,
) {
    // S17: a configured exterior replaces the stock hull entirely — mesh
    // and paint both come from the composed config.
    let configured = shipcfg.config.as_ref().map(|config| {
        let frame = crate::systems::shipeditor::frame_for(content_index, &config.hull_id);
        reachlock_core::editor::exterior::compose_hull(config, &frame)
    });

    let params = SeedParams {
        object_id: PLAYER_HULL_ID.into(),
        universe: UniverseTier::Classic,
        now: 0,
    };
    let hull = match &configured {
        Some(composed) => composed.mesh.clone(),
        None => match resolve(&content_index.files, &params) {
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
        },
    };

    let collider_radius = bounding_radius(&hull);
    let mut ship = commands.spawn((
        PlayerShip,
        Transform::from_xyz(0.0, 0.0, 0.0),
        Visibility::default(),
        RigidBody::Dynamic,
        GravityScale(0.0),
        Collider::ball(collider_radius),
        ActiveEvents::COLLISION_EVENTS,
        // S19: the player is in the PLAYER collision group so its own bolts
        // (which filter that group out) can never hit it.
        CollisionGroups::new(
            crate::systems::combat::G_PLAYER,
            Group::ALL & !crate::systems::combat::G_PLAYER_PROJ,
        ),
        Velocity::default(),
        ExternalForce::default(),
        Damping {
            // Zero linear damping: `ship::control` sets the velocity
            // directly each frame (arcade model) — rapier damping would
            // silently tax the speed cap it computes.
            linear_damping: 0.0,
            angular_damping: 5.0,
        },
        crate::systems::ship::Hull {
            hp: 1024,
            max: 1024,
        },
    ));
    if let Some(composed) = &configured {
        // S17: the configured exterior, extruded to a solid — silhouette
        // and attachments straight from `compose_hull`, painted with the
        // resolved primary (paint slots resolved at render, per spec §19).
        ship.insert((
            Mesh3d(meshes.add(bridge::mesh3d_from_generated(
                &composed.mesh,
                collider_radius * 0.35,
            ))),
            MeshMaterial3d(materials.add(bridge::standard_material_from_palette(
                composed.paint.primary,
            ))),
        ));
    } else if let Some(path) = SHIP_GLTF {
        // Authored models are assumed exported nose-forward (-Z), wings in
        // the XZ plane.
        let scene = asset_server.load(GltfAssetLabel::Scene(0).from_asset(path));
        ship.insert(SceneRoot(scene));
    } else {
        // The Loup-Garou (docs/LORE.md §IV): a Class-J working corvette
        // composed from primitives — fuselage + nose cone, glass canopy,
        // swept delta wings with accent stripes and tip fins, twin engine
        // nacelles, and the chin-mounted mass driver. Everything scales off
        // the physics radius so the model matches the collider, and the
        // palette keeps the seed identity (hull tinted primary, trim in
        // accent). Nose = -Z under the chase-cam.
        spawn_loup_garou_model(&mut ship, meshes, materials, palette, collider_radius);
    }

    // Engine exhaust: emissive cones welded to the two nacelle nozzles,
    // stretched by `ship::engine_glow` with thrust/boost. Children of the
    // hull so they inherit the ship's pose and visibility.
    let flame_len = collider_radius * 1.0;
    let base_z = collider_radius * 0.93;
    ship.with_children(|parent| {
        for side in [-1.0f32, 1.0] {
            parent.spawn((
                Mesh3d(meshes.add(Cone {
                    radius: collider_radius * 0.16,
                    height: flame_len,
                })),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 0.6, 0.2, 0.85),
                    emissive: LinearRgba::rgb(5.0, 2.2, 0.5),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                // Cone tip points +Y by default; rotate it to point +Z
                // (astern).
                Transform::from_xyz(
                    side * collider_radius * 0.34,
                    -0.03 * collider_radius,
                    base_z,
                )
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::new(1.0, 0.01, 1.0)),
                crate::systems::ship::EngineExhaust {
                    base_z,
                    length: flame_len,
                },
                Visibility::Hidden,
            ));
        }
    });
}

/// Blend `a` toward `b` by `t` in srgb — palette tinting for hull materials.
fn blend(a: Color, b: Color, t: f32) -> Color {
    let a = a.to_srgba();
    let b = b.to_srgba();
    Color::srgb(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
    )
}

/// Compose the Loup-Garou's flight model as children of the ship root.
/// `r` is the collider radius; every dimension hangs off it.
fn spawn_loup_garou_model(
    ship: &mut EntityCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    palette: &Palette,
    r: f32,
) {
    let primary = bridge::color_from_palette(palette.primary);
    let accent = bridge::color_from_palette(palette.accent);
    let accent_srgba = accent.to_srgba();

    let hull = materials.add(StandardMaterial {
        base_color: blend(Color::srgb(0.42, 0.45, 0.52), primary, 0.30),
        metallic: 0.75,
        perceptual_roughness: 0.38,
        ..default()
    });
    let trim = materials.add(StandardMaterial {
        base_color: blend(Color::srgb(0.19, 0.20, 0.24), primary, 0.15),
        metallic: 0.85,
        perceptual_roughness: 0.45,
        ..default()
    });
    let stripe = materials.add(StandardMaterial {
        base_color: accent,
        metallic: 0.40,
        perceptual_roughness: 0.50,
        emissive: LinearRgba::rgb(
            accent_srgba.red * 0.35,
            accent_srgba.green * 0.35,
            accent_srgba.blue * 0.35,
        ),
        ..default()
    });
    let canopy = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.16, 0.24),
        metallic: 0.20,
        perceptual_roughness: 0.05,
        emissive: LinearRgba::rgb(0.12, 0.42, 0.60),
        ..default()
    });
    let burn = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.65, 0.25),
        emissive: LinearRgba::rgb(1.6, 0.7, 0.15),
        unlit: true,
        ..default()
    });

    // Cylinders extrude along +Y; lay them along the hull axis (Z).
    let along_z = Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
    // Cone tips point +Y; aim the nose cone forward (-Z).
    let tip_forward = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    // Delta wings sweep their outboard edge aft.
    let sweep = 0.30f32;

    ship.with_children(|parent| {
        let mut part = |mesh: Mesh, mat: &Handle<StandardMaterial>, tx: Transform| {
            parent.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mat.clone()), tx));
        };

        // Fuselage: main hull box, tapering nose cone, raised dorsal spine.
        part(
            Cuboid::new(0.46 * r, 0.28 * r, 1.2 * r).into(),
            &hull,
            Transform::from_xyz(0.0, 0.0, -0.15 * r),
        );
        part(
            Cone {
                radius: 0.20 * r,
                height: 0.55 * r,
            }
            .into(),
            &hull,
            Transform::from_xyz(0.0, 0.0, -r).with_rotation(tip_forward),
        );
        part(
            Cuboid::new(0.16 * r, 0.10 * r, 0.9 * r).into(),
            &trim,
            Transform::from_xyz(0.0, 0.16 * r, 0.1 * r),
        );

        // Canopy: smoked glass over the cockpit, lit faintly from inside.
        part(
            Sphere::new(0.15 * r).into(),
            &canopy,
            Transform::from_xyz(0.0, 0.17 * r, -0.35 * r).with_scale(Vec3::new(0.9, 0.55, 1.6)),
        );

        // Forward-mounted rotary mass driver, under the chin.
        part(
            Cylinder::new(0.045 * r, 0.8 * r).into(),
            &trim,
            Transform::from_xyz(0.0, -0.10 * r, -0.9 * r).with_rotation(along_z),
        );

        // Aft engine block bridging the nacelles.
        part(
            Cuboid::new(0.62 * r, 0.30 * r, 0.5 * r).into(),
            &trim,
            Transform::from_xyz(0.0, 0.0, 0.55 * r),
        );

        for side in [-1.0f32, 1.0] {
            let wing_rot = Quat::from_rotation_y(-side * sweep);
            // Delta wing with an accent stripe along its chord and a
            // vertical tip fin.
            part(
                Cuboid::new(0.95 * r, 0.05 * r, 0.45 * r).into(),
                &hull,
                Transform::from_xyz(side * 0.65 * r, -0.02 * r, 0.35 * r).with_rotation(wing_rot),
            );
            part(
                Cuboid::new(0.92 * r, 0.055 * r, 0.09 * r).into(),
                &stripe,
                Transform::from_xyz(side * 0.66 * r, -0.02 * r, 0.47 * r).with_rotation(wing_rot),
            );
            part(
                Cuboid::new(0.05 * r, 0.30 * r, 0.35 * r).into(),
                &stripe,
                Transform::from_xyz(side * 1.05 * r, 0.10 * r, 0.52 * r),
            );
            // Engine nacelle with an always-lit nozzle disc (the idle burn
            // the exhaust cones grow out of under thrust).
            part(
                Cylinder::new(0.13 * r, 0.75 * r).into(),
                &trim,
                Transform::from_xyz(side * 0.34 * r, -0.03 * r, 0.55 * r).with_rotation(along_z),
            );
            part(
                Cylinder::new(0.10 * r, 0.03 * r).into(),
                &burn,
                Transform::from_xyz(side * 0.34 * r, -0.03 * r, 0.93 * r).with_rotation(along_z),
            );
        }

        // Comms whip on the dorsal spine.
        part(
            Cylinder::new(0.012 * r, 0.35 * r).into(),
            &trim,
            Transform::from_xyz(0.08 * r, 0.32 * r, 0.15 * r),
        );
    });
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
            ActiveEvents::COLLISION_EVENTS,
            Contact,
            crate::systems::ship::Asteroid {
                ore: (radius as i64 * 20).clamp(40, 400),
            },
            crate::systems::ship::Hull {
                hp: (radius as i64 * 20).clamp(40, 400),
                max: (radius as i64 * 20).clamp(40, 400),
            },
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
