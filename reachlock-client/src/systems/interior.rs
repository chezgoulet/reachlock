//! Interior renderer + walking avatar (spec §14 Mode 1 "Stardew × Zelda ×
//! Pokémon" / Mode 2 "FTL × Trust"). One renderer serves both Landed
//! (station interior) and On-Board (ship interior): a `GeneratedLayout` of
//! rooms rendered as walled floors with door apertures, palette-tinted per
//! location so two stations never look alike, dressed with seeded per-kind
//! props (bar tables, bunks, crates, the reactor core), and populated by
//! wandering NPCs. The player avatar has a facing nose and a walk bob; the
//! nearest interactable gets a pulsing highlight ring.
//!
//! Movement is door-honest: walkable space is each room inset by the wall
//! thickness, plus apertures at the layout's doors — you cross between rooms
//! where the doors are, not through walls (Zelda, not noclip).
//!
//! Coordinates are 1:1 grid→world units; the camera follows the avatar
//! zoomed to a Zelda-ish framing (`mode.rs` / `ship::manage_cameras`). The
//! flying `SpaceFlight` scene uses entirely separate world coordinates and
//! the `PlayerShip` entity.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use reachlock_core::content::{ContentPayload, NpcSpawn};
use reachlock_core::generator::hull::HullClass;
use reachlock_core::generator::station::generate_station;
use reachlock_core::generator::{GeneratedLayout, Room, RoomKind};
use reachlock_core::util::color::generate_palette;
use reachlock_core::util::rng::SeededRng;

use crate::bridge;
use crate::states::{CurrentLocation, GameMode, ModeScope, SceneRegistry};
use crate::systems::content_index::ContentIndex;
use crate::systems::crew::{CrewFigure, CrewNav, CrewRoster};
use crate::systems::interaction::{InteractKind, Interactable, InteractionPrompt, Npc};
use crate::systems::mode::PlayerAvatar;

/// Seed for the player's hull interior. Must match the player-ship generation
/// seed in `systems/setup.rs` so the On-Board scene is the ship you fly.
pub const HULL_INTERIOR_SEED: u64 = 0x5EED_0001 ^ 0x51119;

/// Walk speed in world units per second (interior scale).
const WALK_SPEED: f32 = 110.0;
/// Avatar body half-extent in world units.
const AVATAR_HALF: f32 = 7.0;
/// Wall thickness: floors are inset this much inside each room rect, and the
/// dark band between adjacent floors is exactly the un-walkable wall.
pub const WALL: f32 = 3.0;
/// Radius around a door point that stays walkable across the wall band.
pub const DOOR_R: f32 = 12.0;
/// NPC wander speed, world units per second.
const WANDER_SPEED: f32 = 32.0;

/// The layout currently walked, so the walk + transition systems can test
/// room membership without regenerating it.
#[derive(Resource, Default, Clone, Debug)]
pub struct CurrentInterior {
    pub layout: Option<GeneratedLayout>,
}

/// The avatar's live motion state, written by `walk_avatar` and read by
/// `animate_avatar` (bob + nose) and the camera lookahead.
#[derive(Component)]
pub struct AvatarMotion {
    pub facing: Vec2,
    pub moving: bool,
    pub phase: f32,
}

impl Default for AvatarMotion {
    fn default() -> Self {
        AvatarMotion {
            facing: Vec2::new(0.0, -1.0),
            moving: false,
            phase: 0.0,
        }
    }
}

/// The avatar's body quad (bobbed while walking).
#[derive(Component)]
pub struct AvatarBody;

/// The avatar's facing "nose" quad.
#[derive(Component)]
pub struct AvatarNose;

/// An NPC that idles around its room. Tiny xorshift state keeps each one's
/// path deterministic-ish without threading a resource through.
#[derive(Component)]
pub struct Wanderer {
    pub room: Room,
    pub target: Vec2,
    pub pause: f32,
    pub rng: u64,
}

/// The pulsing ring marking the nearest interactable.
#[derive(Component)]
pub struct InteractGlow;

/// A unit quad (1×1, centered) shared by every rect in the scene.
fn unit_quad() -> Mesh {
    let positions = vec![
        [-0.5, -0.5, 0.0],
        [0.5, -0.5, 0.0],
        [0.5, 0.5, 0.0],
        [-0.5, 0.5, 0.0],
    ];
    let indices = Indices::U32(vec![0, 1, 2, 0, 2, 3]);
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(indices);
    mesh
}

fn room_color(kind: RoomKind) -> Color {
    match kind {
        RoomKind::Hangar => Color::srgb(0.4, 0.5, 0.7),
        RoomKind::Corridor => Color::srgb(0.25, 0.25, 0.28),
        RoomKind::Bridge => Color::srgb(0.3, 0.6, 0.5),
        RoomKind::Reactor => Color::srgb(0.7, 0.4, 0.3),
        RoomKind::Quarters => Color::srgb(0.5, 0.4, 0.6),
        RoomKind::Bar => Color::srgb(0.6, 0.5, 0.3),
        RoomKind::Market => Color::srgb(0.5, 0.6, 0.4),
        RoomKind::Shipyard => Color::srgb(0.4, 0.55, 0.6),
    }
}

/// Blend `base` toward `tint` by `t` in srgb space — gives every location its
/// palette identity without losing the per-kind readability.
fn tinted(base: Color, tint: Color, t: f32) -> Color {
    let a = base.to_srgba();
    let b = tint.to_srgba();
    Color::srgb(
        a.red + (b.red - a.red) * t,
        a.green + (b.green - a.green) * t,
        a.blue + (b.blue - a.blue) * t,
    )
}

/// Room label, phrased for the mode: a station concourse reads like a town
/// (spec Mode 1: markets, bars, repair bays, admin), a ship like a ship
/// (spec Mode 2: cockpit, engineering, galley).
fn room_label(mode: GameMode, kind: RoomKind) -> &'static str {
    let onboard = mode == GameMode::OnBoard;
    match kind {
        RoomKind::Hangar => {
            if onboard {
                "AIRLOCK"
            } else {
                "DOCK"
            }
        }
        RoomKind::Corridor => "",
        RoomKind::Bridge => {
            if onboard {
                "COCKPIT"
            } else {
                "ADMIN"
            }
        }
        RoomKind::Reactor => {
            if onboard {
                "ENGINEERING"
            } else {
                "POWER PLANT"
            }
        }
        RoomKind::Quarters => {
            if onboard {
                "QUARTERS"
            } else {
                "HABS"
            }
        }
        RoomKind::Bar => {
            if onboard {
                "GALLEY"
            } else {
                "BAR"
            }
        }
        RoomKind::Market => "MARKET",
        RoomKind::Shipyard => {
            if onboard {
                "CARGO"
            } else {
                "REPAIR BAY"
            }
        }
    }
}

fn room_center_point(room: &Room) -> Vec2 {
    Vec2::new(
        (room.x + room.width / 2) as f32,
        (room.y + room.height / 2) as f32,
    )
}

/// Build the interior scene for a Landed or On-Board mode. Reuses the same
/// code path: a `GeneratedLayout` → walled floors. Skips rebuild when
/// re-entering the same mode we never tore down (the pause round-trip).
#[allow(clippy::too_many_arguments)]
pub fn enter_interior(
    mode: Res<State<GameMode>>,
    location: Res<CurrentLocation>,
    content: Res<ContentIndex>,
    roster: Res<CrewRoster>,
    mut registry: ResMut<SceneRegistry>,
    mut interior: ResMut<CurrentInterior>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mode_entities: Query<Entity, With<ModeScope>>,
) {
    let mode = **mode;
    if registry.scene.as_ref() == Some(&mode) {
        return; // came back from pause; scene already present
    }

    // Tear down the previous scene (SpaceFlight / other interior) before
    // building this one.
    for entity in &mode_entities {
        commands.entity(entity).despawn();
    }

    let (layout, scene_seed) = match &mode {
        GameMode::OnBoard => (
            reachlock_core::generator::station::generate_hull_interior(
                HULL_INTERIOR_SEED,
                HullClass::Corvette,
            ),
            HULL_INTERIOR_SEED,
        ),
        GameMode::Landed => {
            let kind = location
                .station_kind
                .unwrap_or(reachlock_core::generator::station::StationKind::Trade);
            (
                generate_station(location.station_seed, kind, 2).layout,
                location.station_seed,
            )
        }
        _ => return,
    };
    let palette = generate_palette(scene_seed);
    let accent = bridge::color_from_palette(palette.accent);
    let structure = bridge::color_from_palette(palette.structure);

    // NPCs: authored spawns if a content station matches this seed, else a
    // seeded filler set (bar → patron, market → vendor). Each entry is
    // (room_index, name, dialogue_lines).
    let npcs: Vec<(usize, String, Vec<String>)> =
        match content.find_station_by_seed(location.station_seed) {
            Some(f) => match &f.payload {
                ContentPayload::Station { npc_spawns, .. } => npc_spawns
                    .iter()
                    .map(|n: &NpcSpawn| (n.room_index, n.name.clone(), n.dialogue.clone()))
                    .collect(),
                _ => Vec::new(),
            },
            None => generated_fillers(location.station_seed, &layout),
        };

    let quad = meshes.add(unit_quad());

    // Start the avatar in the hangar (the airlock / board point).
    let hangar = layout
        .rooms
        .iter()
        .find(|r| r.kind == RoomKind::Hangar)
        .cloned()
        .unwrap_or(Room {
            kind: RoomKind::Hangar,
            x: 0,
            y: 0,
            width: 48,
            height: 32,
        });
    let start = room_center_point(&hangar);

    // Rooms: a dark wall slab per room rect with the floor inset by WALL —
    // where two rooms adjoin, the two wall strips form the party wall, and
    // walkability (below) matches the picture exactly.
    let wall_color = Color::srgb(0.07, 0.08, 0.11);
    for room in &layout.rooms {
        let c = room_center_point(room);
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(wall_color)),
            Transform::from_xyz(c.x, c.y, 0.0).with_scale(Vec3::new(
                room.width as f32,
                room.height as f32,
                1.0,
            )),
            ModeScope(mode),
        ));
        let floor = tinted(room_color(room.kind), structure, 0.3);
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(floor)),
            Transform::from_xyz(c.x, c.y, 0.1).with_scale(Vec3::new(
                room.width as f32 - WALL * 2.0,
                room.height as f32 - WALL * 2.0,
                1.0,
            )),
            ModeScope(mode),
        ));
        let label = room_label(mode, room.kind);
        if !label.is_empty() {
            commands.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(Color::srgba(0.95, 0.96, 1.0, 0.55)),
                Transform::from_xyz(c.x, c.y + room.height as f32 * 0.5 - 9.0, 2.0),
                ModeScope(mode),
            ));
        }
        spawn_props(
            &mut commands,
            &mut materials,
            mode,
            room,
            scene_seed,
            accent,
            &quad,
        );
    }

    // Door apertures: floor-colored bridges across the party wall, so doors
    // read as the openings they are.
    for door in &layout.doors {
        let floor = layout
            .rooms
            .get(door.to as usize)
            .map(|r| tinted(room_color(r.kind), structure, 0.3))
            .unwrap_or(wall_color);
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(floor)),
            Transform::from_xyz(door.x as f32, door.y as f32, 0.15).with_scale(Vec3::new(
                DOOR_R * 1.6,
                WALL * 2.0 + 2.0,
                1.0,
            )),
            ModeScope(mode),
        ));
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(accent.with_alpha(0.5))),
            Transform::from_xyz(door.x as f32, door.y as f32, 0.16).with_scale(Vec3::new(
                DOOR_R * 1.6,
                1.2,
                1.0,
            )),
            ModeScope(mode),
        ));
    }

    // NPCs (authored or seeded fillers) + a market counter marker.
    // On-Board also puts the crew at their stations and drops the ship's
    // consoles as `Interactable`s.
    spawn_actors(
        &mut commands,
        &mut materials,
        mode,
        &layout,
        &npcs,
        &roster,
        scene_seed,
        quad.clone(),
    );

    // The interaction highlight ring (moved onto the nearest interactable by
    // `highlight_interactable`).
    commands.spawn((
        InteractGlow,
        Mesh2d(quad.clone()),
        MeshMaterial2d(materials.add(Color::srgba(1.0, 0.95, 0.55, 0.3))),
        Transform::from_xyz(start.x, start.y, 0.9),
        Visibility::Hidden,
        ModeScope(mode),
    ));

    // The avatar: an unscaled root (so children keep their own sizes) with a
    // body quad that bobs while walking and a nose quad showing facing.
    commands
        .spawn((
            PlayerAvatar,
            AvatarMotion::default(),
            Transform::from_xyz(start.x, start.y, 1.0),
            Visibility::default(),
            ModeScope(mode),
        ))
        .with_children(|parent| {
            parent.spawn((
                AvatarBody,
                Mesh2d(quad.clone()),
                MeshMaterial2d(materials.add(Color::srgb(0.95, 0.95, 0.4))),
                Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(
                    AVATAR_HALF * 2.0,
                    AVATAR_HALF * 2.0,
                    1.0,
                )),
            ));
            parent.spawn((
                AvatarNose,
                Mesh2d(quad.clone()),
                MeshMaterial2d(materials.add(Color::srgb(0.6, 0.55, 0.2))),
                Transform::from_xyz(0.0, -AVATAR_HALF, 0.05).with_scale(Vec3::new(5.0, 5.0, 1.0)),
            ));
        });

    interior.layout = Some(layout);
    registry.scene = Some(mode);
}

/// Seeded per-kind set dressing so rooms read as places, not colored rects:
/// tables in the bar, bunks in quarters, crates in cargo, a glowing core in
/// the reactor, a landing pad in the hangar, a viewport strip on the bridge.
#[allow(clippy::too_many_arguments)]
fn spawn_props(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mode: GameMode,
    room: &Room,
    scene_seed: u64,
    accent: Color,
    quad: &Handle<Mesh>,
) {
    let mut rng = SeededRng::new(scene_seed ^ ((room.x as u64) << 20) ^ (room.y as u64));
    let c = room_center_point(room);
    let prop = |commands: &mut Commands,
                materials: &mut ResMut<Assets<ColorMaterial>>,
                x: f32,
                y: f32,
                w: f32,
                h: f32,
                color: Color| {
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(color)),
            Transform::from_xyz(x, y, 0.6).with_scale(Vec3::new(w, h, 1.0)),
            ModeScope(mode),
        ));
    };
    // Seeded offset inside the room, keeping `margin` clear of the walls.
    let jitter = |rng: &mut SeededRng, margin: f32| {
        let w = (room.width as f32 - margin * 2.0).max(1.0);
        let h = (room.height as f32 - margin * 2.0).max(1.0);
        Vec2::new(
            room.x as f32 + margin + (rng.next_below(w as u64 + 1) as f32),
            room.y as f32 + margin + (rng.next_below(h as u64 + 1) as f32),
        )
    };
    match room.kind {
        RoomKind::Bar => {
            for _ in 0..2 + rng.next_below(2) {
                let p = jitter(&mut rng, 10.0);
                prop(
                    commands,
                    materials,
                    p.x,
                    p.y,
                    9.0,
                    9.0,
                    Color::srgb(0.45, 0.32, 0.2),
                );
            }
        }
        RoomKind::Quarters => {
            for i in 0..2 + rng.next_below(2) {
                prop(
                    commands,
                    materials,
                    room.x as f32 + 12.0 + (i as f32) * 16.0,
                    room.y as f32 + room.height as f32 - 9.0,
                    12.0,
                    6.0,
                    Color::srgb(0.35, 0.38, 0.55),
                );
            }
        }
        RoomKind::Shipyard | RoomKind::Market => {
            for _ in 0..3 {
                let p = jitter(&mut rng, 9.0);
                prop(
                    commands,
                    materials,
                    p.x,
                    p.y,
                    8.0,
                    8.0,
                    Color::srgb(0.5, 0.45, 0.3),
                );
            }
        }
        RoomKind::Reactor => {
            prop(commands, materials, c.x, c.y + 10.0, 14.0, 14.0, accent);
            prop(
                commands,
                materials,
                c.x,
                c.y + 10.0,
                8.0,
                8.0,
                Color::srgb(1.0, 0.9, 0.6),
            );
        }
        RoomKind::Bridge => {
            // Viewport strip along the far wall.
            prop(
                commands,
                materials,
                c.x,
                room.y as f32 + room.height as f32 - WALL - 3.0,
                room.width as f32 * 0.6,
                4.0,
                Color::srgb(0.1, 0.16, 0.3),
            );
        }
        RoomKind::Hangar => {
            // Landing pad ring.
            prop(
                commands,
                materials,
                c.x,
                c.y,
                room.width as f32 * 0.45,
                room.height as f32 * 0.45,
                Color::srgba(0.0, 0.0, 0.0, 0.25),
            );
            prop(
                commands,
                materials,
                c.x,
                c.y,
                4.0,
                4.0,
                accent.with_alpha(0.6),
            );
        }
        RoomKind::Corridor => {
            // Guide stripe down the spine.
            prop(
                commands,
                materials,
                c.x,
                c.y,
                room.width as f32 - WALL * 4.0,
                1.2,
                accent.with_alpha(0.25),
            );
        }
    }
}

/// WASD walking with wall collision. Each axis resolves independently so the
/// avatar slides along walls; crossing between rooms only works through door
/// apertures (`walkable`).
pub fn walk_avatar(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    interior: Res<CurrentInterior>,
    mut avatar: Query<(&mut Transform, &mut AvatarMotion), With<PlayerAvatar>>,
) {
    let Some(layout) = &interior.layout else {
        return;
    };
    let Ok((mut transform, mut motion)) = avatar.single_mut() else {
        return;
    };

    let mut dir = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        dir.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        dir.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
        dir.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
        dir.x += 1.0;
    }
    if dir == Vec2::ZERO {
        motion.moving = false;
        return;
    }
    let dir = dir.normalize();
    motion.moving = true;
    motion.facing = dir;
    motion.phase += time.delta_secs() * 11.0;
    let step = WALK_SPEED * time.delta_secs();
    let dx = dir.x * step;
    let dy = dir.y * step;

    let cur = transform.translation;
    if walkable(cur.x + dx, cur.y, layout) {
        transform.translation.x += dx;
    }
    if walkable(transform.translation.x, cur.y + dy, layout) {
        transform.translation.y += dy;
    }
}

/// Bob the avatar body while walking and keep the nose on the facing side.
pub fn animate_avatar(
    motion: Query<&AvatarMotion, With<PlayerAvatar>>,
    mut body: Query<&mut Transform, (With<AvatarBody>, Without<AvatarNose>)>,
    mut nose: Query<&mut Transform, (With<AvatarNose>, Without<AvatarBody>)>,
) {
    let Ok(motion) = motion.single() else {
        return;
    };
    if let Ok(mut body) = body.single_mut() {
        let bob = if motion.moving {
            1.0 + motion.phase.sin() * 0.06
        } else {
            1.0
        };
        body.scale = Vec3::new(AVATAR_HALF * 2.0 / bob, AVATAR_HALF * 2.0 * bob, 1.0);
    }
    if let Ok(mut nose) = nose.single_mut() {
        let offset = motion.facing * AVATAR_HALF;
        nose.translation.x = offset.x;
        nose.translation.y = offset.y;
    }
}

/// Idle NPCs drift around their own room: pick a spot, amble to it, pause,
/// repeat. The station breathes instead of standing at attention.
pub fn wander_npcs(time: Res<Time>, mut npcs: Query<(&mut Transform, &mut Wanderer)>) {
    let dt = time.delta_secs();
    for (mut tx, mut w) in &mut npcs {
        let pos = tx.translation.truncate();
        let to = w.target - pos;
        if to.length() > 2.0 {
            let step = to.normalize() * WANDER_SPEED * dt;
            tx.translation.x += step.x;
            tx.translation.y += step.y;
            continue;
        }
        w.pause -= dt;
        if w.pause > 0.0 {
            continue;
        }
        // xorshift: cheap deterministic-per-NPC randomness.
        let mut x = w.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        w.rng = x;
        let margin = 10.0_f32;
        let rw = (w.room.width as f32 - margin * 2.0).max(1.0);
        let rh = (w.room.height as f32 - margin * 2.0).max(1.0);
        w.target = Vec2::new(
            w.room.x as f32 + margin + (x % 1000) as f32 / 1000.0 * rw,
            w.room.y as f32 + margin + ((x >> 10) % 1000) as f32 / 1000.0 * rh,
        );
        w.pause = 1.5 + ((x >> 20) % 100) as f32 / 40.0;
    }
}

/// Park the highlight ring on the nearest interactable (from
/// `InteractionPrompt`) with a soft pulse; hide it when nothing's in reach.
pub fn highlight_interactable(
    time: Res<Time>,
    prompt: Res<InteractionPrompt>,
    mut glow: Query<(&mut Transform, &mut Visibility), With<InteractGlow>>,
) {
    let Ok((mut tx, mut vis)) = glow.single_mut() else {
        return;
    };
    match prompt.target {
        Some(pos) => {
            *vis = Visibility::Visible;
            tx.translation.x = pos.x;
            tx.translation.y = pos.y;
            let pulse = 16.0 + (time.elapsed_secs() * 5.0).sin() * 2.0;
            tx.scale = Vec3::new(pulse, pulse, 1.0);
        }
        None => *vis = Visibility::Hidden,
    }
}

/// Is the point in walkable space: inside a room's floor (the rect inset by
/// `WALL`), or within a door aperture bridging two rooms?
pub fn walkable(x: f32, y: f32, layout: &GeneratedLayout) -> bool {
    let in_floor = layout.rooms.iter().any(|r| {
        x >= r.x as f32 + WALL
            && x <= (r.x + r.width) as f32 - WALL
            && y >= r.y as f32 + WALL
            && y <= (r.y + r.height) as f32 - WALL
    });
    if in_floor {
        return true;
    }
    // Door apertures cross the wall band — but never past the union of the
    // rooms themselves (no slipping outside the hull at an edge door).
    let in_any_room = layout.rooms.iter().any(|r| {
        x >= r.x as f32
            && x <= (r.x + r.width) as f32
            && y >= r.y as f32
            && y <= (r.y + r.height) as f32
    });
    in_any_room
        && layout.doors.iter().any(|d| {
            let dx = x - d.x as f32;
            let dy = y - d.y as f32;
            dx * dx + dy * dy <= DOOR_R * DOOR_R
        })
}

/// Seeded filler NPCs for a *generated* station (authored stations supply
/// their own via `npc_spawns`). Bars get a patron, markets a vendor, 1–2
/// per qualifying room, named from a small pool so two stations differ.
fn generated_fillers(seed: u64, layout: &GeneratedLayout) -> Vec<(usize, String, Vec<String>)> {
    let mut rng = SeededRng::new(seed ^ 0xBEEF);
    let names = ["Pell", "Mara", "Voss", "Juno", "Dax", "Rill"];
    let mut out = Vec::new();
    for (i, room) in layout.rooms.iter().enumerate() {
        let (role, lines) = match room.kind {
            RoomKind::Bar => ("patron", vec!["*nods at you*".to_string()]),
            RoomKind::Market => ("vendor", vec!["Looking to trade?".to_string()]),
            RoomKind::Quarters => ("crew", vec!["Off-shift. Don't slack.".to_string()]),
            _ => continue,
        };
        let count = 1 + (rng.next_below(2) as usize); // 1..=2
        for _ in 0..count {
            let name = names[rng.next_below(names.len() as u64) as usize];
            let full = format!("{name} the {role}");
            out.push((i, full, lines.clone()));
        }
    }
    out
}

/// Spawn every NPC as a figure root (Interactable + Npc + Wanderer) with a
/// body quad and a name label as children, scattered in its room rather than
/// stacked at the center. Also drop a `Shop` counter in the first Market
/// room, and — On-Board — the crew and the ship's consoles.
#[allow(clippy::too_many_arguments)]
fn spawn_actors(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mode: GameMode,
    layout: &GeneratedLayout,
    npcs: &[(usize, String, Vec<String>)],
    roster: &CrewRoster,
    scene_seed: u64,
    quad: Handle<Mesh>,
) {
    let mut rng = SeededRng::new(scene_seed ^ 0x5CA7);
    for (room_index, name, dialogue) in npcs {
        let Some(room) = layout.rooms.get(*room_index) else {
            continue;
        };
        let margin = 12.0;
        let rw = (room.width as f32 - margin * 2.0).max(1.0) as u64;
        let rh = (room.height as f32 - margin * 2.0).max(1.0) as u64;
        let px = room.x as f32 + margin + rng.next_below(rw + 1) as f32;
        let py = room.y as f32 + margin + rng.next_below(rh + 1) as f32;
        let hue = 0.55 + (rng.next_below(40) as f32) / 100.0;
        spawn_figure(
            commands,
            materials,
            &quad,
            mode,
            Vec2::new(px, py),
            name,
            Color::srgb(0.85, hue, 0.5),
            (
                Interactable {
                    label: name.clone(),
                    kind: InteractKind::Talk,
                },
                Npc {
                    name: name.clone(),
                    dialogue: dialogue.clone(),
                },
                Wanderer {
                    room: room.clone(),
                    target: Vec2::new(px, py),
                    pause: (rng.next_below(30) as f32) / 10.0,
                    rng: scene_seed ^ ((*room_index as u64) << 32) ^ rng.next_below(u64::MAX - 1),
                },
            ),
        );
    }

    // Market counter marker (the shop verb target).
    if let Some(market) = layout.rooms.iter().find(|r| r.kind == RoomKind::Market) {
        let c = room_center_point(market);
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(Color::srgb(0.4, 0.7, 0.5))),
            Transform::from_xyz(c.x, c.y, 0.65).with_scale(Vec3::new(20.0, 10.0, 1.0)),
            ModeScope(mode),
            Interactable {
                label: "MARKET".to_string(),
                kind: InteractKind::Shop,
            },
        ));
        commands.spawn((
            Text2d::new("¤"),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(Color::srgb(0.85, 1.0, 0.9)),
            Transform::from_xyz(c.x, c.y, 0.66),
            ModeScope(mode),
        ));
    }

    // On-Board: crew at their stations + the ship's consoles.
    if mode == GameMode::OnBoard {
        spawn_crew(commands, materials, layout, roster, &quad);
        spawn_consoles(commands, materials, layout, &quad);
    }
}

/// Spawn a walking figure: unscaled root carrying the game components (so
/// interaction distance and movement work on the root), with the body quad
/// and name label as children that follow it around.
#[allow(clippy::too_many_arguments)]
fn spawn_figure(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    quad: &Handle<Mesh>,
    mode: GameMode,
    pos: Vec2,
    name: &str,
    color: Color,
    extra: impl Bundle,
) -> Entity {
    commands
        .spawn((
            Transform::from_xyz(pos.x, pos.y, 1.0),
            Visibility::default(),
            ModeScope(mode),
            extra,
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh2d(quad.clone()),
                MeshMaterial2d(materials.add(color)),
                Transform::from_scale(Vec3::new(10.0, 10.0, 1.0)),
            ));
            parent.spawn((
                Text2d::new(name.to_string()),
                TextFont {
                    font_size: 7.0,
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.9, 0.8)),
                Transform::from_xyz(0.0, -11.0, 0.1),
            ));
        })
        .id()
}

/// Place each roster member as a walking figure + `CrewFigure` tag (so the
/// shift system can find the entity) + a `Crew` `Interactable` (so you can
/// order them). All `ModeScope`; they respawn on board.
fn spawn_crew(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    layout: &GeneratedLayout,
    roster: &CrewRoster,
    quad: &Handle<Mesh>,
) {
    for m in &roster.members {
        let Some(center) = room_center(layout, m.current_room) else {
            continue;
        };
        spawn_figure(
            commands,
            materials,
            quad,
            GameMode::OnBoard,
            center,
            &m.name,
            Color::srgb(0.6, 0.7, 0.95),
            (
                CrewFigure(m.id.clone()),
                CrewNav::default(),
                Interactable {
                    label: m.name.clone(),
                    kind: InteractKind::Crew,
                },
            ),
        );
    }
}

/// Drop the ship's consoles as `Interactable`s in their rooms:
/// Helm + Nav on the Bridge, Engineering in the Reactor, Log in Quarters.
/// Offsets keep multiple consoles in one room from overlapping.
fn spawn_consoles(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    layout: &GeneratedLayout,
    quad: &Handle<Mesh>,
) {
    if let Some(bridge) = room_center(layout, RoomKind::Bridge) {
        console(
            commands,
            materials,
            quad,
            bridge.x - 12.0,
            bridge.y,
            "HELM",
            InteractKind::Helm,
        );
        console(
            commands,
            materials,
            quad,
            bridge.x + 12.0,
            bridge.y,
            "NAV",
            InteractKind::Nav,
        );
        // S09b: gunner + scanner consoles on the bridge (spec §22). Staggered
        // in +y so they don't overlap HELM/NAV.
        console(
            commands,
            materials,
            quad,
            bridge.x - 12.0,
            bridge.y + 14.0,
            "GUNNER",
            InteractKind::Gunner,
        );
        console(
            commands,
            materials,
            quad,
            bridge.x + 12.0,
            bridge.y + 14.0,
            "SCANNER",
            InteractKind::Scanner,
        );
    }
    if let Some(reactor) = room_center(layout, RoomKind::Reactor) {
        console(
            commands,
            materials,
            quad,
            reactor.x,
            reactor.y,
            "ENG",
            InteractKind::Engineering,
        );
        // S09b: power routing + mining rig consoles in the reactor.
        console(
            commands,
            materials,
            quad,
            reactor.x - 14.0,
            reactor.y + 14.0,
            "POWER",
            InteractKind::Power,
        );
        console(
            commands,
            materials,
            quad,
            reactor.x + 14.0,
            reactor.y + 14.0,
            "MINER",
            InteractKind::Miner,
        );
    }
    if let Some(quarters) = room_center(layout, RoomKind::Quarters) {
        console(
            commands,
            materials,
            quad,
            quarters.x,
            quarters.y,
            "LOG",
            InteractKind::Log,
        );
    }
}

fn console(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    quad: &Handle<Mesh>,
    x: f32,
    y: f32,
    label: &str,
    kind: InteractKind,
) {
    commands.spawn((
        Mesh2d(quad.clone()),
        MeshMaterial2d(materials.add(Color::srgb(0.3, 0.45, 0.55))),
        Transform::from_xyz(x, y, 0.7).with_scale(Vec3::new(14.0, 10.0, 1.0)),
        ModeScope(GameMode::OnBoard),
        Interactable {
            label: label.to_string(),
            kind,
        },
    ));
    commands.spawn((
        Text2d::new(label.to_string()),
        TextFont {
            font_size: 6.0,
            ..default()
        },
        TextColor(Color::srgba(0.75, 0.9, 1.0, 0.8)),
        Transform::from_xyz(x, y + 8.0, 0.71),
        ModeScope(GameMode::OnBoard),
    ));
}

/// Center of the first room of a given kind, if the layout has one.
pub(crate) fn room_center(layout: &GeneratedLayout, kind: RoomKind) -> Option<Vec2> {
    layout
        .rooms
        .iter()
        .find(|r| r.kind == kind)
        .map(room_center_point)
}

#[cfg(test)]
mod tests {
    use super::{walkable, DOOR_R, WALL};
    use reachlock_core::generator::{Door, GeneratedLayout, Room, RoomKind};

    /// Two rooms sharing the edge y=32 with one door at (24, 32).
    fn two_rooms() -> GeneratedLayout {
        GeneratedLayout {
            rooms: vec![
                Room {
                    kind: RoomKind::Hangar,
                    x: 0,
                    y: 0,
                    width: 48,
                    height: 32,
                },
                Room {
                    kind: RoomKind::Corridor,
                    x: 0,
                    y: 32,
                    width: 96,
                    height: 16,
                },
            ],
            doors: vec![Door {
                from: 0,
                to: 1,
                x: 24,
                y: 32,
            }],
        }
    }

    #[test]
    fn room_centers_are_walkable() {
        let l = two_rooms();
        assert!(walkable(24.0, 16.0, &l));
        assert!(walkable(48.0, 40.0, &l));
    }

    #[test]
    fn walls_block_including_shared_edges_away_from_doors() {
        let l = two_rooms();
        // Outside everything.
        assert!(!walkable(-10.0, 10.0, &l));
        // Inside the wall band on an exterior edge.
        assert!(!walkable(1.0, 16.0, &l));
        // On the shared edge, but far from the door: blocked (no noclip
        // through party walls).
        assert!(!walkable(80.0, 32.0, &l));
    }

    #[test]
    fn door_aperture_is_walkable_but_only_near_the_door() {
        let l = two_rooms();
        // On the shared edge at the door: open.
        assert!(walkable(24.0, 32.0, &l));
        // A step to the side, still within DOOR_R: open.
        assert!(walkable(24.0 + DOOR_R - 1.0, 32.0, &l));
        // Beyond the aperture: wall.
        assert!(!walkable(24.0 + DOOR_R + 2.0 + WALL, 32.0, &l));
    }
}
