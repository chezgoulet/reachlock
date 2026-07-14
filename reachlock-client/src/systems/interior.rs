//! Interior renderer + walking avatar (spec §14 Mode 1/2; S06 placeholders).
//! One renderer serves both Landed (station interior) and On-Board (ship
//! interior): both are a `GeneratedLayout` of rooms rendered as flat rects
//! with door markers, and a player square you walk with WASD. S07/S08 build
//! the real content on top of this.
//!
//! Coordinates are 1:1 grid→world units; the camera follows the avatar so
//! scale is irrelevant. The flying `SpaceFlight` scene uses entirely
//! separate world coordinates and the `PlayerShip` entity.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use reachlock_core::content::{ContentPayload, NpcSpawn};
use reachlock_core::generator::hull::HullClass;
use reachlock_core::generator::station::generate_station;
use reachlock_core::generator::{GeneratedLayout, Room, RoomKind};
use reachlock_core::util::rng::SeededRng;

use crate::states::{CurrentLocation, GameMode, ModeScope, SceneRegistry};
use crate::systems::content_index::ContentIndex;
use crate::systems::crew::{CrewFigure, CrewRoster};
use crate::systems::interaction::{InteractKind, Interactable, Npc};
use crate::systems::mode::PlayerAvatar;

/// Seed for the player's hull interior. Must match the player-ship generation
/// seed in `systems/setup.rs` so the On-Board scene is the ship you fly.
pub const HULL_INTERIOR_SEED: u64 = 0x5EED_0001 ^ 0x51119;

/// Walk speed in world units per second (interior scale).
const WALK_SPEED: f32 = 140.0;
/// Avatar square half-extent in world units.
const AVATAR_HALF: f32 = 7.0;

/// The layout currently walked, so the walk + transition systems can test
/// room membership without regenerating it.
#[derive(Resource, Default, Clone, Debug)]
pub struct CurrentInterior {
    pub layout: Option<GeneratedLayout>,
}

/// A unit quad (1×1, centered) shared by every room rect and the avatar.
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

fn room_label(kind: RoomKind) -> &'static str {
    match kind {
        RoomKind::Hangar => "AIRLOCK",
        RoomKind::Corridor => "",
        RoomKind::Bridge => "COCKPIT",
        RoomKind::Reactor => "ENGINEERING",
        RoomKind::Quarters => "QUARTERS",
        RoomKind::Bar => "GALLEY",
        RoomKind::Market => "MARKET",
        RoomKind::Shipyard => "CARGO",
    }
}

/// Build the interior scene for a Landed or On-Board mode. Reuses the same
/// code path: a `GeneratedLayout` → flat rects. Skips rebuild when re-entering
/// the same mode we never tore down (the pause round-trip).
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

    let layout = match &mode {
        GameMode::OnBoard => reachlock_core::generator::station::generate_hull_interior(
            HULL_INTERIOR_SEED,
            HullClass::Corvette,
        ),
        GameMode::Landed => {
            let kind = location
                .station_kind
                .unwrap_or(reachlock_core::generator::station::StationKind::Trade);
            generate_station(location.station_seed, kind, 2).layout
        }
        _ => return,
    };

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
    let start_x = (hangar.x + hangar.width / 2) as f32;
    let start_y = (hangar.y + hangar.height / 2) as f32;

    for room in &layout.rooms {
        let cx = (room.x + room.width / 2) as f32;
        let cy = (room.y + room.height / 2) as f32;
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(room_color(room.kind))),
            Transform::from_xyz(cx, cy, 0.0).with_scale(Vec3::new(
                room.width as f32,
                room.height as f32,
                1.0,
            )),
            ModeScope(mode),
        ));
        let label = room_label(room.kind);
        if !label.is_empty() {
            commands.spawn((
                Text::new(label),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(Color::srgb(0.9, 0.92, 0.95)),
                Transform::from_xyz(cx, cy, 1.0),
                ModeScope(mode),
            ));
        }
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
        quad.clone(),
    );

    // Door markers — small lighter rects at each connector.
    for door in &layout.doors {
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(Color::srgb(0.8, 0.8, 0.6))),
            Transform::from_xyz(door.x as f32, door.y as f32, 0.5)
                .with_scale(Vec3::new(6.0, 6.0, 1.0)),
            ModeScope(mode),
        ));
    }

    commands.spawn((
        PlayerAvatar,
        Mesh2d(quad.clone()),
        MeshMaterial2d(materials.add(Color::srgb(0.95, 0.95, 0.4))),
        Transform::from_xyz(start_x, start_y, 1.0).with_scale(Vec3::new(
            AVATAR_HALF * 2.0,
            AVATAR_HALF * 2.0,
            1.0,
        )),
        ModeScope(mode),
    ));

    interior.layout = Some(layout);
    registry.scene = Some(mode);
}

/// WASD walking with AABB wall collision against the union of room rects.
/// Each axis is resolved independently so the avatar slides along walls and
/// passes through doorways (adjacent rooms share edges via the corridor).
pub fn walk_avatar(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    interior: Res<CurrentInterior>,
    mut avatar: Query<&mut Transform, With<PlayerAvatar>>,
) {
    let Some(layout) = &interior.layout else {
        return;
    };
    let Ok(mut transform) = avatar.single_mut() else {
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
        return;
    }
    let dir = dir.normalize();
    let step = WALK_SPEED * time.delta_secs();
    let dx = dir.x * step;
    let dy = dir.y * step;

    let cur = transform.translation;
    if inside_any(cur.x + dx, cur.y, layout) {
        transform.translation.x += dx;
    }
    if inside_any(transform.translation.x, cur.y + dy, layout) {
        transform.translation.y += dy;
    }
}

/// Is the point inside at least one room rect? Defines walkable space.
fn inside_any(x: f32, y: f32, layout: &GeneratedLayout) -> bool {
    layout.rooms.iter().any(|r| {
        x >= r.x as f32
            && x <= (r.x + r.width) as f32
            && y >= r.y as f32
            && y <= (r.y + r.height) as f32
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

/// Spawn every NPC as a colored figure, name label, `Interactable(Talk)`,
/// and `Npc` (carries the dialogue the panel reads), all `ModeScope(mode)`.
/// Also drop a `Shop` marker in the first Market room so the market verb has
/// something to target.
fn spawn_actors(
    commands: &mut Commands,
    materials: &mut ResMut<Assets<ColorMaterial>>,
    mode: GameMode,
    layout: &GeneratedLayout,
    npcs: &[(usize, String, Vec<String>)],
    roster: &CrewRoster,
    quad: Handle<Mesh>,
) {
    for (room_index, name, dialogue) in npcs {
        let Some(room) = layout.rooms.get(*room_index) else {
            continue;
        };
        let cx = (room.x + room.width / 2) as f32;
        let cy = (room.y + room.height / 2) as f32;
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(Color::srgb(0.85, 0.6, 0.5))),
            Transform::from_xyz(cx, cy, 1.0).with_scale(Vec3::new(10.0, 10.0, 1.0)),
            ModeScope(mode),
            Interactable {
                label: name.clone(),
                kind: InteractKind::Talk,
            },
            Npc {
                name: name.clone(),
                dialogue: dialogue.clone(),
            },
        ));
        commands.spawn((
            Text::new(name.clone()),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(Color::srgb(0.95, 0.9, 0.8)),
            Transform::from_xyz(cx, cy - 9.0, 1.1),
            ModeScope(mode),
        ));
    }

    // Market counter marker (the shop verb target).
    if let Some(market) = layout.rooms.iter().find(|r| r.kind == RoomKind::Market) {
        let cx = (market.x + market.width / 2) as f32;
        let cy = (market.y + market.height / 2) as f32;
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(Color::srgb(0.4, 0.7, 0.5))),
            Transform::from_xyz(cx, cy, 0.6).with_scale(Vec3::new(20.0, 10.0, 1.0)),
            ModeScope(mode),
            Interactable {
                label: "MARKET".to_string(),
                kind: InteractKind::Shop,
            },
        ));
    }

    // On-Board: crew at their stations + the ship's consoles.
    if mode == GameMode::OnBoard {
        spawn_crew(commands, materials, layout, roster, &quad);
        spawn_consoles(commands, materials, layout, &quad);
    }
}

/// Place each roster member as a colored figure + name label + `CrewFigure`
/// tag (so the shift system can find the entity) + a `Crew` `Interactable`
/// (so you can order them). All `ModeScope(mode)`; they respawn on board.
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
        commands.spawn((
            Mesh2d(quad.clone()),
            MeshMaterial2d(materials.add(Color::srgb(0.6, 0.7, 0.95))),
            Transform::from_xyz(center.x, center.y, 1.0).with_scale(Vec3::new(10.0, 10.0, 1.0)),
            ModeScope(GameMode::OnBoard),
            CrewFigure(m.id.clone()),
            Interactable {
                label: m.name.clone(),
                kind: InteractKind::Crew,
            },
        ));
        commands.spawn((
            Text::new(m.name.clone()),
            TextFont {
                font_size: 9.0,
                ..default()
            },
            TextColor(Color::srgb(0.9, 0.95, 1.0)),
            Transform::from_xyz(center.x, center.y - 9.0, 1.1),
            ModeScope(GameMode::OnBoard),
        ));
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
}

/// Center of the first room of a given kind, if the layout has one.
pub(crate) fn room_center(layout: &GeneratedLayout, kind: RoomKind) -> Option<Vec2> {
    layout
        .rooms
        .iter()
        .find(|r| r.kind == kind)
        .map(|r| Vec2::new((r.x + r.width / 2) as f32, (r.y + r.height / 2) as f32))
}
