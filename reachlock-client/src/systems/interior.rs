//! Interior renderer + walking avatar (spec §14 Mode 1 "Stardew × Zelda ×
//! Pokémon" / Mode 2 "FTL × Trust"), pixel-art edition. One renderer serves
//! both Landed (station interior) and On-Board (ship interior):
//!
//! - The core `GeneratedLayout` (grid units) is scaled ×6 into pixel space —
//!   1 tile = 16px, rooms run 9–18 tiles across — so interiors breathe at
//!   Stardew scale instead of reading as a cramped diagram.
//! - Floors and walls are seeded pixel textures tiled per room kind (deck
//!   plate, planks, carpet, grating, poured concrete), palette-tinted per
//!   location so two stations never look alike.
//! - Furniture is painted at Terraria furniture dimensions (crates 26×22,
//!   tables 26×20, bunks 28×20, consoles 24×20) by `crate::pixel`.
//! - Every walking figure — avatar, NPCs, crew — is a 16×26 SNES-JRPG
//!   sprite (4 facings × 2 walk frames) with a drop shadow and a name tag,
//!   y-sorted so actors pass in front of and behind furniture correctly.
//!
//! Movement is door-honest: walkable space is each room inset by the wall
//! thickness, plus apertures at the layout's doors — you cross between rooms
//! where the doors are, not through walls (Zelda, not noclip).

use bevy::prelude::*;

use reachlock_core::content::{ContentPayload, NpcSpawn};
use reachlock_core::generator::station::generate_station;
use reachlock_core::generator::{Door, GeneratedLayout, Room, RoomKind};
use reachlock_core::util::color::generate_palette;
use reachlock_core::util::rng::SeededRng;

use crate::bridge;
use crate::pixel::{self, Look, TILE};
use crate::states::{CurrentLocation, GameMode, ModeScope, SceneRegistry};
use crate::systems::content_index::ContentIndex;
use crate::systems::crew::{CrewFigure, CrewNav, CrewRoster};
use crate::systems::interaction::{InteractKind, Interactable, InteractionPrompt, Npc};
use crate::systems::mode::PlayerAvatar;

/// Seed for the player's hull interior. Must match the player-ship generation
/// seed in `systems/setup.rs` so the On-Board scene is the ship you fly.
pub const HULL_INTERIOR_SEED: u64 = 0x5EED_0001 ^ 0x51119;

/// Core layout grid → pixel world. Grid unit 8 × 6 = 48px = 3 tiles, so a
/// generated 3–6-grid room becomes 9–18 tiles across.
pub const LAYOUT_SCALE: i32 = 6;

/// Walk speed, px/s (~6 tiles/s — A-Link-to-the-Past pace).
const WALK_SPEED: f32 = 104.0;
/// Wall thickness: exactly one tile. Floors inset this much inside each room
/// rect; the dark band between adjacent floors is the un-walkable wall.
pub const WALL: f32 = TILE;
/// Radius around a door point that stays walkable across the wall band
/// (2 tiles: a doorway, not a pinhole).
pub const DOOR_R: f32 = TILE * 2.0;
/// NPC wander speed, px/s (~2.5 tiles/s amble).
const WANDER_SPEED: f32 = 40.0;

/// The layout currently walked (already scaled to pixel space), so the walk
/// + transition systems can test room membership without regenerating it.
#[derive(Resource, Default, Clone, Debug)]
pub struct CurrentInterior {
    pub layout: Option<GeneratedLayout>,
    /// Zero-g deck (docs/SHIPS.md §5): humans move slow in mag boots,
    /// robots move fast. Always false in stations.
    pub zero_g: bool,
    /// The inter-deck ladder's pixel position on this deck (ship interiors
    /// only). Cross-deck crew routing targets it (S16B).
    pub ladder: Option<Vec2>,
}

/// Which of the ship's decks the On-Board scene shows, and where to place
/// the avatar on the next build (set by the ladder so climbing keeps your
/// position). Deck 0 is the boarding deck.
#[derive(Resource, Default)]
pub struct ActiveDeck {
    pub index: usize,
    pub spawn: Option<Vec2>,
}

/// Scale a core grid-unit layout into pixel space.
pub fn scale_layout(l: &GeneratedLayout, s: i32) -> GeneratedLayout {
    GeneratedLayout {
        rooms: l
            .rooms
            .iter()
            .map(|r| Room {
                kind: r.kind,
                x: r.x * s,
                y: r.y * s,
                width: r.width * s,
                height: r.height * s,
            })
            .collect(),
        doors: l
            .doors
            .iter()
            .map(|d| Door {
                from: d.from,
                to: d.to,
                x: d.x * s,
                y: d.y * s,
            })
            .collect(),
    }
}

/// Y-sorted depth for actors and furniture: lower on screen = in front.
/// Stays inside (0.5, 0.8) — above floors/thresholds, below name tags.
pub fn ysort(y: f32) -> f32 {
    0.5 + (20_000.0 - y).clamp(0.0, 25_000.0) * 1e-5
}

/// A walking figure's live animation state (avatar, NPC, or crew): facing +
/// walk phase derived from actual movement, consumed by `animate_figures`.
#[derive(Component)]
pub struct Figure {
    pub last: Vec2,
    pub dir: usize,
    pub phase: f32,
    pub moving: bool,
}

impl Figure {
    fn at(pos: Vec2) -> Self {
        Figure {
            last: pos,
            dir: pixel::DIR_DOWN,
            phase: 0.0,
            moving: false,
        }
    }
}

/// The figure's body sprite child: carries the 4×2 walk frame set.
#[derive(Component)]
pub struct FigureBody;

/// Walk frames for one figure, `[direction][frame]`.
#[derive(Component)]
pub struct CharacterSprite {
    pub frames: [[Handle<Image>; 2]; 4],
}

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

fn room_kind_color(kind: RoomKind) -> Color {
    match kind {
        RoomKind::Hangar => Color::srgb(0.45, 0.52, 0.62),
        RoomKind::Corridor => Color::srgb(0.42, 0.44, 0.48),
        RoomKind::Bridge => Color::srgb(0.38, 0.58, 0.55),
        RoomKind::Reactor => Color::srgb(0.6, 0.42, 0.36),
        RoomKind::Quarters => Color::srgb(0.5, 0.42, 0.55),
        RoomKind::Bar => Color::srgb(0.58, 0.45, 0.28),
        RoomKind::Market => Color::srgb(0.48, 0.56, 0.42),
        RoomKind::Shipyard => Color::srgb(0.45, 0.52, 0.56),
        RoomKind::Cockpit => Color::srgb(0.36, 0.56, 0.60),
        RoomKind::TechBay => Color::srgb(0.48, 0.50, 0.55),
        RoomKind::Scanner => Color::srgb(0.42, 0.46, 0.62),
        RoomKind::MedBay => Color::srgb(0.72, 0.76, 0.78),
        RoomKind::Cryo => Color::srgb(0.52, 0.62, 0.72),
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
                "BRIDGE"
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
        RoomKind::Cockpit => "COCKPIT",
        RoomKind::TechBay => "TECH BAY",
        RoomKind::Scanner => "SCANNER",
        RoomKind::MedBay => "MED BAY",
        RoomKind::Cryo => "CRYO",
    }
}

fn room_center_point(room: &Room) -> Vec2 {
    Vec2::new(
        (room.x + room.width / 2) as f32,
        (room.y + room.height / 2) as f32,
    )
}

/// A plain static pixel sprite at a position/z.
fn decal(commands: &mut Commands, image: Handle<Image>, mode: GameMode, x: f32, y: f32, z: f32) {
    commands.spawn((
        Sprite { image, ..default() },
        Transform::from_xyz(x, y, z),
        ModeScope(mode),
    ));
}

/// Build the interior scene for a Landed or On-Board mode. Skips rebuild when
/// re-entering the same mode we never tore down (the pause round-trip). Also
/// runs in `Update` (guarded by `in_any_interior`): the ladder switches decks
/// by clearing `SceneRegistry::scene`, and this rebuild picks it up.
#[allow(clippy::too_many_arguments)]
pub fn enter_interior(
    mode: Res<State<GameMode>>,
    location: Res<CurrentLocation>,
    content: Res<ContentIndex>,
    roster: Res<CrewRoster>,
    mut registry: ResMut<SceneRegistry>,
    mut interior: ResMut<CurrentInterior>,
    mut deck: ResMut<ActiveDeck>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mode_entities: Query<(Entity, &ModeScope)>,
) {
    let mode = **mode;
    if registry.scene.as_ref() == Some(&mode) {
        return; // came back from pause; scene already present
    }

    // S09d: boarding the ship *in flight* (leave the helm) keeps the space
    // scene spawned and simulating underneath the interior — the ship is a
    // place the crew walks while the world outside stays live, and the
    // station consoles render that live world (docs/SHIPS.md §1). Docked
    // boarding and landing tear space down as before.
    let keep_space = mode == GameMode::OnBoard && !location.is_docked && registry.space_alive;

    // Tear down the previous scene (SpaceFlight / other interior) before
    // building this one.
    for (entity, scope) in &mode_entities {
        if keep_space && scope.0 == GameMode::SpaceFlight {
            continue;
        }
        commands.entity(entity).despawn();
    }
    if !keep_space {
        registry.space_alive = false;
    }

    let mut zero_g = false;
    let mut ladder_px: Option<Vec2> = None;
    let (grid_layout, scene_seed) = match &mode {
        GameMode::OnBoard => {
            // The player ship is authored, not generated: the Loup-Garou's
            // two-deck plan (docs/SHIPS.md §6). `ActiveDeck` picks the deck;
            // the ladder toggles it.
            let ship = reachlock_core::generator::ship::loup_garou_interior();
            let d = deck.index.min(ship.decks.len() - 1);
            let deck_def = &ship.decks[d];
            zero_g = deck_def.zero_g;
            ladder_px = Some(Vec2::new(
                (deck_def.ladder.0 * LAYOUT_SCALE) as f32,
                (deck_def.ladder.1 * LAYOUT_SCALE) as f32,
            ));
            (deck_def.layout.clone(), HULL_INTERIOR_SEED)
        }
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
    let layout = scale_layout(&grid_layout, LAYOUT_SCALE);
    let palette = generate_palette(scene_seed);
    let accent = bridge::color_from_palette(palette.accent);
    let structure = bridge::color_from_palette(palette.structure);

    // NPCs: authored spawns if a content station matches this seed, else a
    // seeded filler set. Each entry is (room_index, name, dialogue_lines).
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

    // Start the avatar where the ladder put them, else in the hangar (the
    // airlock / board point; the first room on decks without one).
    let hangar = layout
        .rooms
        .iter()
        .find(|r| r.kind == RoomKind::Hangar)
        .or(layout.rooms.first())
        .cloned()
        .unwrap_or(Room {
            kind: RoomKind::Hangar,
            x: 0,
            y: 0,
            width: 48 * LAYOUT_SCALE,
            height: 32 * LAYOUT_SCALE,
        });
    let start = deck
        .spawn
        .take()
        .unwrap_or_else(|| room_center_point(&hangar));

    // Shared textures for this scene.
    let wall_tex = images.add(pixel::wall_texture(tinted(
        Color::srgb(0.5, 0.52, 0.6),
        structure,
        0.4,
    )));
    let shadow_tex = images.add(pixel::shadow_sprite());
    let mut floor_tex = std::collections::HashMap::new();

    // Rooms: a tiled wall slab per room rect with the tiled floor inset by
    // WALL — where two rooms adjoin, the two wall strips form the party
    // wall, and walkability (below) matches the picture exactly.
    for room in &layout.rooms {
        let c = room_center_point(room);
        let floor_color = tinted(room_kind_color(room.kind), structure, 0.35);
        let floor = floor_tex
            .entry(room.kind as u64)
            .or_insert_with(|| {
                images.add(pixel::floor_texture(
                    room.kind,
                    floor_color,
                    scene_seed ^ room.kind as u64,
                ))
            })
            .clone();
        commands.spawn((
            Sprite {
                image: wall_tex.clone(),
                custom_size: Some(Vec2::new(room.width as f32, room.height as f32)),
                image_mode: SpriteImageMode::Tiled {
                    tile_x: true,
                    tile_y: true,
                    stretch_value: 1.0,
                },
                ..default()
            },
            Transform::from_xyz(c.x, c.y, 0.0),
            ModeScope(mode),
        ));
        commands.spawn((
            Sprite {
                image: floor,
                custom_size: Some(Vec2::new(
                    room.width as f32 - WALL * 2.0,
                    room.height as f32 - WALL * 2.0,
                )),
                image_mode: SpriteImageMode::Tiled {
                    tile_x: true,
                    tile_y: true,
                    stretch_value: 1.0,
                },
                ..default()
            },
            Transform::from_xyz(c.x, c.y, 0.1),
            ModeScope(mode),
        ));
        let label = room_label(mode, room.kind);
        if !label.is_empty() {
            commands.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgba(0.95, 0.96, 1.0, 0.5)),
                Transform::from_xyz(c.x, c.y + room.height as f32 * 0.5 - 26.0, 2.0),
                ModeScope(mode),
            ));
        }
        spawn_props(&mut commands, &mut images, mode, room, scene_seed, accent);
    }

    // Door thresholds: hazard-striped plates bridging the party wall.
    let threshold_tex = images.add(pixel::threshold_sprite(
        tinted(Color::srgb(0.55, 0.56, 0.62), structure, 0.3),
        accent,
    ));
    for door in &layout.doors {
        decal(
            &mut commands,
            threshold_tex.clone(),
            mode,
            door.x as f32,
            door.y as f32,
            0.15,
        );
    }

    // NPCs + market counter; On-Board adds crew and the ship's consoles.
    spawn_actors(
        &mut commands,
        &mut images,
        mode,
        &layout,
        &npcs,
        &roster,
        scene_seed,
        &shadow_tex,
    );

    // The interaction highlight ring (moved onto the nearest interactable by
    // `highlight_interactable`).
    commands.spawn((
        InteractGlow,
        Sprite {
            image: images.add(pixel::ring_sprite()),
            ..default()
        },
        Transform::from_xyz(start.x, start.y, 0.49),
        Visibility::Hidden,
        ModeScope(mode),
    ));

    // The inter-deck ladder (On-Board only): interact to climb. Same grid
    // point on both decks, so climbing keeps your position.
    if let Some(lp) = ladder_px {
        let target = if deck.index == 0 {
            "up to the zero-g deck"
        } else {
            "down to the gravity deck"
        };
        commands.spawn((
            Sprite {
                image: images.add(pixel::ladder_sprite(accent)),
                ..default()
            },
            Transform::from_xyz(lp.x, lp.y, ysort(lp.y - 12.0)),
            ModeScope(mode),
            Interactable {
                label: format!("Ladder — {target}"),
                kind: InteractKind::Ladder,
            },
        ));
    }

    // The avatar: Tib, captain of the Loup-Garou (docs/LORE.md §V) — dark
    // hair, worn brown flight jacket.
    spawn_figure(
        &mut commands,
        &mut images,
        mode,
        start,
        "",
        pixel::crew_look("tib"),
        &shadow_tex,
        (PlayerAvatar,),
    );

    interior.layout = Some(layout);
    interior.zero_g = zero_g;
    interior.ladder = ladder_px;
    registry.scene = Some(mode);
}

/// Keep crew sprites in step with which deck each member is on (S16B
/// cross-deck routing): a member who climbed away loses their sprite; a
/// member who arrived appears at the ladder and walks on from there.
#[allow(clippy::too_many_arguments)]
pub fn sync_crew_deck_presence(
    mode: Option<Res<State<GameMode>>>,
    deck: Res<ActiveDeck>,
    interior: Res<CurrentInterior>,
    roster: Res<CrewRoster>,
    figures: Query<(Entity, &CrewFigure)>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
) {
    if !mode.is_some_and(|m| **m == GameMode::OnBoard) {
        return;
    }
    let Some(ladder) = interior.ladder else {
        return;
    };
    // Departures: the sprite's member is no longer on this deck.
    let mut present: Vec<&str> = Vec::new();
    for (entity, fig) in &figures {
        match roster.by_id(&fig.0) {
            Some(m) if m.deck == deck.index => present.push(&fig.0),
            _ => commands.entity(entity).despawn(),
        }
    }
    // Arrivals: members on this deck without a sprite step off the ladder.
    let shadow = images.add(pixel::shadow_sprite());
    for m in roster
        .members
        .iter()
        .filter(|m| m.deck == deck.index && !present.contains(&m.id.as_str()))
    {
        spawn_figure(
            &mut commands,
            &mut images,
            GameMode::OnBoard,
            ladder + Vec2::new(0.0, -20.0),
            &m.name,
            pixel::crew_look(&m.id),
            &shadow,
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

/// Seeded per-kind set dressing at Terraria furniture dimensions, y-sorted
/// so figures walk in front of and behind it.
fn spawn_props(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    mode: GameMode,
    room: &Room,
    scene_seed: u64,
    accent: Color,
) {
    let mut rng = SeededRng::new(scene_seed ^ ((room.x as u64) << 20) ^ (room.y as u64));
    let c = room_center_point(room);
    let sorted = |commands: &mut Commands, image: Handle<Image>, x: f32, y: f32| {
        commands.spawn((
            Sprite { image, ..default() },
            Transform::from_xyz(x, y, ysort(y - 8.0)),
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
            let table = images.add(pixel::table_sprite());
            for _ in 0..2 + rng.next_below(3) {
                let p = jitter(&mut rng, 44.0);
                sorted(commands, table.clone(), p.x, p.y);
            }
            let counter = images.add(pixel::counter_sprite(tinted(
                Color::srgb(0.5, 0.38, 0.24),
                accent,
                0.15,
            )));
            sorted(
                commands,
                counter,
                c.x,
                room.y as f32 + room.height as f32 - WALL - 18.0,
            );
        }
        RoomKind::Quarters => {
            let blankets = [
                Color::srgb(0.55, 0.25, 0.25),
                Color::srgb(0.25, 0.4, 0.55),
                Color::srgb(0.3, 0.5, 0.35),
            ];
            for i in 0..2 + rng.next_below(2) {
                let bunk = images.add(pixel::bunk_sprite(blankets[i as usize % 3]));
                sorted(
                    commands,
                    bunk,
                    room.x as f32 + 40.0 + (i as f32) * 44.0,
                    room.y as f32 + room.height as f32 - WALL - 16.0,
                );
            }
        }
        RoomKind::Shipyard | RoomKind::Market => {
            for _ in 0..3 + rng.next_below(2) {
                let p = jitter(&mut rng, 40.0);
                let crate_tex = images.add(pixel::crate_sprite(rng.next_below(1 << 30)));
                sorted(commands, crate_tex, p.x, p.y);
            }
        }
        RoomKind::Reactor => {
            let core = images.add(pixel::core_sprite(accent));
            sorted(commands, core, c.x, c.y + 30.0);
        }
        RoomKind::Bridge => {
            let viewport = images.add(pixel::viewport_sprite(scene_seed));
            decal(
                commands,
                viewport,
                mode,
                c.x,
                room.y as f32 + room.height as f32 - WALL - 10.0,
                0.4,
            );
        }
        // The cockpit: canopy stars and the pilot seat — walk up, E, and
        // you're flying her (docs/SHIPS.md §1: the pilot's station view IS
        // the Star Fox view).
        RoomKind::Cockpit => {
            let viewport = images.add(pixel::viewport_sprite(scene_seed ^ 0xC0C));
            decal(
                commands,
                viewport,
                mode,
                c.x,
                room.y as f32 + room.height as f32 - WALL - 10.0,
                0.4,
            );
            let y = room.y as f32 + room.height as f32 - WALL - 34.0;
            commands.spawn((
                Sprite {
                    image: images.add(pixel::seat_sprite(accent)),
                    ..default()
                },
                Transform::from_xyz(c.x, y, ysort(y - 10.0)),
                ModeScope(mode),
                Interactable {
                    label: "Pilot seat — take the helm".to_string(),
                    kind: InteractKind::TakeHelm,
                },
            ));
        }
        // Tech bay: the processing floor — shuttle on its pad, cargo
        // waiting to be broken down.
        RoomKind::TechBay => {
            let pad_x = room.x as f32 + room.width as f32 * 0.7;
            let pad_y = room.y as f32 + room.height as f32 * 0.4;
            let pad = images.add(pixel::pad_sprite(accent));
            decal(commands, pad, mode, pad_x, pad_y, 0.12);
            let shuttle = images.add(pixel::shuttle_sprite(accent));
            sorted(commands, shuttle, pad_x, pad_y);
            for _ in 0..4 + rng.next_below(3) {
                let p = jitter(&mut rng, 40.0);
                if (p - Vec2::new(pad_x, pad_y)).length() < 60.0 {
                    continue; // keep the pad clear
                }
                let crate_tex = images.add(pixel::crate_sprite(rng.next_below(1 << 30)));
                sorted(commands, crate_tex, p.x, p.y);
            }
        }
        // Med bay: clean bunks under pale light.
        RoomKind::MedBay => {
            let sheet = Color::srgb(0.85, 0.9, 0.92);
            for i in 0..2 {
                let bunk = images.add(pixel::bunk_sprite(sheet));
                sorted(
                    commands,
                    bunk,
                    room.x as f32 + 44.0 + (i as f32) * 48.0,
                    room.y as f32 + room.height as f32 - WALL - 16.0,
                );
            }
        }
        // Cryo chamber: ten pods in two ranks — one per possible sleeper
        // (docs/SHIPS.md §3). Empty while the crew is awake. On board every
        // pod is a place you can climb into — the jump-cryo loop's whole
        // point is the walk here beating the jump clock.
        RoomKind::Cryo => {
            let pod = images.add(pixel::cryo_pod_sprite(false));
            for row in 0..2 {
                for col in 0..5 {
                    let x = room.x as f32 + WALL + 20.0 + col as f32 * 22.0;
                    let y = room.y as f32 + WALL + 24.0 + row as f32 * 56.0;
                    if mode == GameMode::OnBoard {
                        commands.spawn((
                            Sprite {
                                image: pod.clone(),
                                ..default()
                            },
                            Transform::from_xyz(x, y, ysort(y)),
                            ModeScope(mode),
                            Interactable {
                                label: "Cryo pod".to_string(),
                                kind: InteractKind::CryoPod,
                            },
                        ));
                    } else {
                        sorted(commands, pod.clone(), x, y);
                    }
                }
            }
        }
        RoomKind::Scanner => {}
        RoomKind::Hangar => {
            let pad = images.add(pixel::pad_sprite(accent));
            decal(commands, pad, mode, c.x, c.y, 0.12);
            match mode {
                // Landed: the Loup-Garou sits on the pad. Boarding is a
                // thing you see and walk to, not a hidden keybind.
                GameMode::Landed => {
                    commands.spawn((
                        Sprite {
                            image: images.add(pixel::ship_sprite(accent)),
                            ..default()
                        },
                        Transform::from_xyz(c.x, c.y, ysort(c.y - 26.0)),
                        ModeScope(mode),
                        Interactable {
                            label: "Loup-Garou — board".to_string(),
                            kind: InteractKind::Board,
                        },
                    ));
                }
                // On board, the same room is the airlock: the hatch is the
                // way back out onto the dock.
                GameMode::OnBoard => {
                    let y = room.y as f32 + WALL + 20.0;
                    commands.spawn((
                        Sprite {
                            image: images.add(pixel::hatch_sprite(accent)),
                            ..default()
                        },
                        Transform::from_xyz(c.x, y, ysort(y - 13.0)),
                        ModeScope(mode),
                        Interactable {
                            label: "Airlock — disembark".to_string(),
                            kind: InteractKind::Disembark,
                        },
                    ));
                }
                _ => {}
            }
        }
        RoomKind::Corridor => {}
    }
    // The mandatory JRPG foliage, anywhere people linger.
    if matches!(
        room.kind,
        RoomKind::Bar | RoomKind::Market | RoomKind::Quarters | RoomKind::Bridge
    ) {
        let plant = images.add(pixel::plant_sprite(rng.next_below(1 << 30)));
        sorted(
            commands,
            plant,
            room.x as f32 + WALL + 14.0,
            room.y as f32 + WALL + 16.0,
        );
    }
}

/// WASD walking with wall collision. Each axis resolves independently so the
/// avatar slides along walls; crossing between rooms only works through door
/// apertures (`walkable`).
pub fn walk_avatar(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    interior: Res<CurrentInterior>,
    dialogue: Res<crate::systems::dialogue::DialogueSession>,
    plan: Res<crate::systems::cryojump::JumpPlan>,
    mut avatar: Query<&mut Transform, With<PlayerAvatar>>,
) {
    // S16: while typing a free-input line, WASD spells words, not steps.
    // S09e: a sealed cryo pod holds its sleeper until revival.
    if dialogue.typing() || plan.player_in_pod {
        return;
    }
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
    // Tib is human: mag boots on the zero-g deck (docs/SHIPS.md §5).
    let factor = crate::systems::crew::move_factor(pixel::BodyKind::Human, interior.zero_g);
    let step = WALK_SPEED * factor * time.delta_secs();
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

/// Animate every walking figure from its actual movement: derive facing and
/// walk phase from the position delta, pick the matching frame, and y-sort.
/// One system covers the avatar, NPCs, and crew alike.
pub fn animate_figures(
    time: Res<Time>,
    mut roots: Query<(&mut Figure, &mut Transform, &Children)>,
    mut bodies: Query<(&mut Sprite, &CharacterSprite), With<FigureBody>>,
) {
    for (mut fig, mut tx, children) in &mut roots {
        let pos = tx.translation.truncate();
        let delta = pos - fig.last;
        fig.last = pos;
        fig.moving = delta.length() > 0.05;
        if fig.moving {
            fig.dir = if delta.x.abs() > delta.y.abs() {
                if delta.x > 0.0 {
                    pixel::DIR_RIGHT
                } else {
                    pixel::DIR_LEFT
                }
            } else if delta.y > 0.0 {
                pixel::DIR_UP
            } else {
                pixel::DIR_DOWN
            };
            fig.phase += time.delta_secs() * 8.0;
        }
        tx.translation.z = ysort(pos.y);
        let frame = if fig.moving {
            (fig.phase as usize) % 2
        } else {
            0
        };
        for child in children {
            if let Ok((mut sprite, frames)) = bodies.get_mut(*child) {
                let want = frames.frames[fig.dir][frame].clone();
                if sprite.image != want {
                    sprite.image = want;
                }
            }
        }
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
        let margin = 36.0_f32;
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
            tx.translation.z = ysort(pos.y) - 0.01;
            let pulse = 1.0 + (time.elapsed_secs() * 5.0).sin() * 0.08;
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
/// their own via `npc_spawns`). Bars get patrons, markets vendors, 1–3 per
/// qualifying room, named from a small pool so two stations differ.
fn generated_fillers(seed: u64, layout: &GeneratedLayout) -> Vec<(usize, String, Vec<String>)> {
    let mut rng = SeededRng::new(seed ^ 0xBEEF);
    let names = [
        "Pell", "Mara", "Voss", "Juno", "Dax", "Rill", "Oona", "Brix",
    ];
    let mut out = Vec::new();
    for (i, room) in layout.rooms.iter().enumerate() {
        let (role, lines) = match room.kind {
            RoomKind::Bar => ("patron", vec!["*nods at you*".to_string()]),
            RoomKind::Market => ("vendor", vec!["Looking to trade?".to_string()]),
            RoomKind::Quarters => ("crew", vec!["Off-shift. Don't slack.".to_string()]),
            RoomKind::Shipyard => ("dockhand", vec!["Mind the crates.".to_string()]),
            _ => continue,
        };
        let count = 1 + (rng.next_below(3) as usize); // 1..=3
        for _ in 0..count {
            let name = names[rng.next_below(names.len() as u64) as usize];
            let full = format!("{name} the {role}");
            out.push((i, full, lines.clone()));
        }
    }
    out
}

/// Spawn every NPC as a walking pixel figure (Interactable + Npc + Wanderer)
/// scattered in its room rather than stacked at the center. Also drop a
/// `Shop` counter in the first Market room, and — On-Board — the crew and
/// the ship's consoles.
#[allow(clippy::too_many_arguments)]
fn spawn_actors(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    mode: GameMode,
    layout: &GeneratedLayout,
    npcs: &[(usize, String, Vec<String>)],
    roster: &CrewRoster,
    scene_seed: u64,
    shadow: &Handle<Image>,
) {
    let mut rng = SeededRng::new(scene_seed ^ 0x5CA7);
    for (room_index, name, dialogue) in npcs {
        let Some(room) = layout.rooms.get(*room_index) else {
            continue;
        };
        let margin = 44.0;
        let rw = (room.width as f32 - margin * 2.0).max(1.0) as u64;
        let rh = (room.height as f32 - margin * 2.0).max(1.0) as u64;
        let px = room.x as f32 + margin + rng.next_below(rw + 1) as f32;
        let py = room.y as f32 + margin + rng.next_below(rh + 1) as f32;
        let look_seed = rng.next_below(1 << 30);
        spawn_figure(
            commands,
            images,
            mode,
            Vec2::new(px, py),
            name,
            Look::seeded(look_seed),
            shadow,
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
                    rng: look_seed | 1,
                },
            ),
        );
    }

    // Market counter (the shop verb target).
    if let Some(market) = layout.rooms.iter().find(|r| r.kind == RoomKind::Market) {
        let c = room_center_point(market);
        let y = market.y as f32 + market.height as f32 - WALL - 18.0;
        commands.spawn((
            Sprite {
                image: images.add(pixel::counter_sprite(Color::srgb(0.35, 0.55, 0.45))),
                ..default()
            },
            Transform::from_xyz(c.x, y, ysort(y - 8.0)),
            ModeScope(mode),
            Interactable {
                label: "MARKET".to_string(),
                kind: InteractKind::Shop,
            },
        ));
    }

    // Shipyard terminal (S17): stations only — on board, the Shipyard room
    // kind doubles as the cargo hold and gets no refit verb.
    if mode == GameMode::Landed {
        if let Some(yard) = layout.rooms.iter().find(|r| r.kind == RoomKind::Shipyard) {
            let c = room_center_point(yard);
            let y = yard.y as f32 + yard.height as f32 - WALL - 18.0;
            commands.spawn((
                Sprite {
                    image: images.add(pixel::console_sprite(Color::srgb(0.85, 0.65, 0.25))),
                    ..default()
                },
                Transform::from_xyz(c.x, y, ysort(y - 8.0)),
                ModeScope(mode),
                Interactable {
                    label: "SHIPYARD".to_string(),
                    kind: InteractKind::Shipyard,
                },
            ));
        }
    }

    // On-Board: crew at their stations + the ship's consoles.
    if mode == GameMode::OnBoard {
        spawn_crew(commands, images, layout, roster, shadow);
        spawn_consoles(commands, images, layout);
    }
}

/// Spawn a walking pixel figure: unscaled root carrying the game components
/// (so interaction distance and movement work on the root), with the shadow,
/// the animated body sprite, and the name tag as children.
#[allow(clippy::too_many_arguments)]
fn spawn_figure(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    mode: GameMode,
    pos: Vec2,
    name: &str,
    look: Look,
    shadow: &Handle<Image>,
    extra: impl Bundle,
) {
    let frames = pixel::character_frames(images, look);
    let idle = frames[pixel::DIR_DOWN][0].clone();
    commands
        .spawn((
            Figure::at(pos),
            Transform::from_xyz(pos.x, pos.y, ysort(pos.y)),
            Visibility::default(),
            ModeScope(mode),
            extra,
        ))
        .with_children(|parent| {
            parent.spawn((
                Sprite {
                    image: shadow.clone(),
                    ..default()
                },
                Transform::from_xyz(0.0, -1.0, -0.002),
            ));
            parent.spawn((
                FigureBody,
                CharacterSprite { frames },
                Sprite {
                    image: idle,
                    ..default()
                },
                Transform::from_xyz(0.0, 10.0, 0.0),
            ));
            if !name.is_empty() {
                parent.spawn((
                    Text2d::new(name.to_string()),
                    TextFont {
                        font_size: 8.0,
                        ..default()
                    },
                    TextColor(Color::srgba(0.95, 0.9, 0.8, 0.9)),
                    Transform::from_xyz(0.0, 30.0, 0.05),
                ));
            }
        });
}

/// Place each roster member as a uniformed pixel figure + `CrewFigure` tag
/// (so the shift system can find the entity) + a `Crew` `Interactable` (so
/// you can order them). All `ModeScope`; they respawn on board.
fn spawn_crew(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    layout: &GeneratedLayout,
    roster: &CrewRoster,
    shadow: &Handle<Image>,
) {
    for m in &roster.members {
        // Crew spawn on the deck that has their room: Prudence and Boris
        // work Upstairs, the rest live Downstairs. A member whose room
        // isn't in this deck's layout is simply on the other deck.
        let Some(center) = room_center(layout, m.current_room) else {
            continue;
        };
        // Canonical lore looks (Tove's coveralls, the androids, BOR-IS).
        let look = pixel::crew_look(&m.id);
        spawn_figure(
            commands,
            images,
            GameMode::OnBoard,
            center,
            &m.name,
            look,
            shadow,
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

/// Drop the ship's consoles as glowing-screen pixel sprites in their rooms:
/// Helm + Nav + Gunner + Scanner on the Bridge, Engineering + Power + Miner
/// in the Reactor, Log in Quarters.
fn spawn_consoles(commands: &mut Commands, images: &mut Assets<Image>, layout: &GeneratedLayout) {
    let console = |commands: &mut Commands,
                   images: &mut Assets<Image>,
                   x: f32,
                   y: f32,
                   label: &str,
                   glow: Color,
                   kind: InteractKind| {
        commands.spawn((
            Sprite {
                image: images.add(pixel::console_sprite(glow)),
                ..default()
            },
            Transform::from_xyz(x, y, ysort(y - 8.0)),
            ModeScope(GameMode::OnBoard),
            Interactable {
                label: label.to_string(),
                kind,
            },
        ));
        commands.spawn((
            Text2d::new(label.to_string()),
            TextFont {
                font_size: 7.0,
                ..default()
            },
            TextColor(Color::srgba(0.75, 0.9, 1.0, 0.7)),
            Transform::from_xyz(x, y + 18.0, 1.9),
            ModeScope(GameMode::OnBoard),
        ));
    };
    let teal = Color::srgb(0.3, 0.85, 0.8);
    let amber = Color::srgb(0.95, 0.7, 0.3);
    let green = Color::srgb(0.7, 0.85, 0.5);
    // Stations live in their lore rooms (docs/SHIPS.md §6). Each deck's
    // layout only contains its own rooms, so every block below simply
    // no-ops on the other deck.
    // Cockpit (upper): flight systems beside the pilot seat.
    if let Some(cockpit) = room_center(layout, RoomKind::Cockpit) {
        console(
            commands,
            images,
            cockpit.x - 52.0,
            cockpit.y + 8.0,
            "HELM",
            teal,
            InteractKind::Helm,
        );
    }
    // Bridge (lower, under the cockpit): tactical, stellar nav, comms log.
    if let Some(bridge) = room_center(layout, RoomKind::Bridge) {
        console(
            commands,
            images,
            bridge.x - 52.0,
            bridge.y + 12.0,
            "GUNNER",
            amber,
            InteractKind::Gunner,
        );
        console(
            commands,
            images,
            bridge.x,
            bridge.y + 12.0,
            "NAV",
            teal,
            InteractKind::Nav,
        );
        console(
            commands,
            images,
            bridge.x + 52.0,
            bridge.y + 12.0,
            "LOG",
            green,
            InteractKind::Log,
        );
    }
    // The scanner array: its own console in its own room.
    if let Some(scanner) = room_center(layout, RoomKind::Scanner) {
        console(
            commands,
            images,
            scanner.x,
            scanner.y + 8.0,
            "SCANNER",
            teal,
            InteractKind::Scanner,
        );
    }
    // Engineering: power allocation, drive control, the mining rig.
    if let Some(reactor) = room_center(layout, RoomKind::Reactor) {
        console(
            commands,
            images,
            reactor.x - 44.0,
            reactor.y - 20.0,
            "ENG",
            amber,
            InteractKind::Engineering,
        );
        console(
            commands,
            images,
            reactor.x + 44.0,
            reactor.y - 20.0,
            "POWER",
            amber,
            InteractKind::Power,
        );
        console(
            commands,
            images,
            reactor.x,
            reactor.y - 44.0,
            "MINER",
            teal,
            InteractKind::Miner,
        );
    }
}

/// Center of the first room of a given kind, if the layout has one.
pub(crate) fn room_center(layout: &GeneratedLayout, kind: RoomKind) -> Option<Vec2> {
    layout
        .rooms
        .iter()
        .find(|r| r.kind == kind)
        .map(room_center_point)
}

/// Which deck holds the cryo chamber, and where a revived sleeper stands
/// when the pods open (the jump-cryo wake beat, SHIPS.md §3 step 4).
pub fn cryo_wake_spawn() -> Option<(usize, Vec2)> {
    let ship = reachlock_core::generator::ship::loup_garou_interior();
    for (deck_index, deck) in ship.decks.iter().enumerate() {
        let layout = scale_layout(&deck.layout, LAYOUT_SCALE);
        if let Some(center) = room_center(&layout, RoomKind::Cryo) {
            return Some((deck_index, center));
        }
    }
    None
}

/// Which deck of the Loup-Garou holds the cockpit, and where the avatar
/// stands when they get up from the pilot seat (S09d "leave the helm" — the
/// reverse of the pilot-seat `TakeHelm` interaction). Mirrors the seat
/// placement in `spawn_props` (`RoomKind::Cockpit`), one tile astern of it.
pub fn cockpit_seat_spawn() -> Option<(usize, Vec2)> {
    let ship = reachlock_core::generator::ship::loup_garou_interior();
    for (deck_index, deck) in ship.decks.iter().enumerate() {
        let layout = scale_layout(&deck.layout, LAYOUT_SCALE);
        if let Some(room) = layout.rooms.iter().find(|r| r.kind == RoomKind::Cockpit) {
            let c = room_center_point(room);
            let seat_y = room.y as f32 + room.height as f32 - WALL - 34.0;
            return Some((deck_index, Vec2::new(c.x, seat_y - 26.0)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{cockpit_seat_spawn, scale_layout, walkable, ysort, DOOR_R, LAYOUT_SCALE, WALL};
    use reachlock_core::generator::{Door, GeneratedLayout, Room, RoomKind};

    /// S09d "leave the helm": standing up from the pilot seat must land the
    /// avatar on the deck that has the cockpit, on walkable floor inside it.
    #[test]
    fn cockpit_seat_spawn_is_walkable_cockpit_floor() {
        let (deck_index, pos) = cockpit_seat_spawn().expect("the Loup-Garou has a cockpit");
        let ship = reachlock_core::generator::ship::loup_garou_interior();
        let deck = &ship.decks[deck_index];
        assert!(deck.zero_g, "the cockpit is Upstairs (docs/SHIPS.md §6)");
        let layout = scale_layout(&deck.layout, LAYOUT_SCALE);
        let cockpit = layout
            .rooms
            .iter()
            .find(|r| r.kind == RoomKind::Cockpit)
            .expect("cockpit room");
        assert!(
            pos.x >= cockpit.x as f32
                && pos.x <= (cockpit.x + cockpit.width) as f32
                && pos.y >= cockpit.y as f32
                && pos.y <= (cockpit.y + cockpit.height) as f32,
            "stand-up point {pos:?} is inside the cockpit rect"
        );
        assert!(
            walkable(pos.x, pos.y, &layout),
            "stand-up point {pos:?} is on walkable floor"
        );
    }

    /// Two rooms sharing the edge y=192 with one door at (96, 192) — the
    /// scaled shape of the generator's hangar + spine corridor.
    fn two_rooms() -> GeneratedLayout {
        GeneratedLayout {
            rooms: vec![
                Room {
                    kind: RoomKind::Hangar,
                    x: 0,
                    y: 0,
                    width: 288,
                    height: 192,
                },
                Room {
                    kind: RoomKind::Corridor,
                    x: 0,
                    y: 192,
                    width: 576,
                    height: 96,
                },
            ],
            doors: vec![Door {
                from: 0,
                to: 1,
                x: 96,
                y: 192,
            }],
        }
    }

    #[test]
    fn scale_layout_multiplies_rooms_and_doors() {
        let grid = GeneratedLayout {
            rooms: vec![Room {
                kind: RoomKind::Hangar,
                x: 0,
                y: 4,
                width: 48,
                height: 32,
            }],
            doors: vec![Door {
                from: 0,
                to: 0,
                x: 16,
                y: 32,
            }],
        };
        let px = scale_layout(&grid, LAYOUT_SCALE);
        assert_eq!(px.rooms[0].width, 48 * LAYOUT_SCALE);
        assert_eq!(px.rooms[0].y, 4 * LAYOUT_SCALE);
        assert_eq!(px.doors[0].x, 16 * LAYOUT_SCALE);
    }

    #[test]
    fn room_centers_are_walkable() {
        let l = two_rooms();
        assert!(walkable(144.0, 96.0, &l));
        assert!(walkable(288.0, 240.0, &l));
    }

    #[test]
    fn walls_block_including_shared_edges_away_from_doors() {
        let l = two_rooms();
        // Outside everything.
        assert!(!walkable(-10.0, 90.0, &l));
        // Inside the wall band on an exterior edge.
        assert!(!walkable(6.0, 96.0, &l));
        // On the shared edge, but far from the door: blocked (no noclip
        // through party walls).
        assert!(!walkable(480.0, 192.0, &l));
    }

    #[test]
    fn door_aperture_is_walkable_but_only_near_the_door() {
        let l = two_rooms();
        // On the shared edge at the door: open.
        assert!(walkable(96.0, 192.0, &l));
        // A step to the side, still within DOOR_R: open.
        assert!(walkable(96.0 + DOOR_R - 1.0, 192.0, &l));
        // Beyond the aperture: wall.
        assert!(!walkable(96.0 + DOOR_R + 2.0 + WALL, 192.0, &l));
    }

    #[test]
    fn ysort_puts_lower_y_in_front() {
        assert!(ysort(100.0) > ysort(900.0));
        // And stays inside the actor band (above floors, below labels).
        assert!(ysort(0.0) < 0.8);
        assert!(ysort(2000.0) > 0.5);
    }
}
