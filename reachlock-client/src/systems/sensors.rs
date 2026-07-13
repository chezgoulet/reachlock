//! Sensor visibility, blip rendering, and scanning (S09). Runs in
//! `SpaceFlight`: contacts beyond `ShipSystems.sensor_range` are hidden and
//! replaced by a small blip dot; holding `S` near an unknown contact resolves
//! its identity. The system map (`M` key) overlays all known contacts.

use bevy::prelude::*;

use crate::states::{GameMode, ModeScope};
use crate::systems::docking::Dockable;
use crate::systems::setup::Gate;
use crate::systems::ship::{PlayerShip, ShipSystems};

/// Marker for entities the player's sensors can detect (stations, planets,
/// gates, asteroids). Spawned by `setup.rs` alongside the entity's visual.
#[derive(Component)]
pub struct Contact;

/// Tracks which contacts have been resolved by sensors or scanning. Used by
/// the sensor visibility / blip / map systems (S09).
#[derive(Resource, Default)]
pub struct KnownContacts {
    pub known: std::collections::HashSet<Entity>,
}

/// Marker for a blip entity — a placeholder rendered at a contact's world
/// position when it has not yet been resolved by sensors or scanning.
#[derive(Component)]
pub struct Blip {
    pub for_contact: Entity,
}

/// The blip mesh and material, built once and shared across all blips.
#[derive(Resource)]
pub struct BlipAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<ColorMaterial>,
}

/// Spawn the blip assets once (on app init).
pub fn init_blip_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.insert_resource(BlipAssets {
        mesh: meshes.add(Circle::new(3.0)),
        material: materials.add(Color::srgb(0.3, 0.5, 0.7)),
    });
}

/// Each frame: for every `Contact` entity in the world, check distance from
/// the player ship. If within sensor range, mark as known and ensure the
/// real entity is visible. If beyond sensor range and not known, hide the
/// real entity.
pub fn sensor_visibility(
    ship: Query<&Transform, With<PlayerShip>>,
    contacts: Query<(Entity, &Transform), With<Contact>>,
    mut known: ResMut<KnownContacts>,
    systems: Res<ShipSystems>,
    mut vis: Query<&mut Visibility, (With<Contact>, Without<Blip>)>,
) {
    let Ok(ship_pos) = ship.single() else {
        return;
    };
    let sensor_range_f32 = systems.sensor_range.0 as f32 / 1024.0;

    for (entity, transform) in &contacts {
        let dist = transform
            .translation
            .truncate()
            .distance(ship_pos.translation.truncate());
        let is_known = known.known.contains(&entity);

        if dist <= sensor_range_f32 && !is_known {
            known.known.insert(entity);
        }

        // Toggle the contact entity's visibility.
        if let Ok(mut v) = vis.get_mut(entity) {
            *v = if dist <= sensor_range_f32 || known.known.contains(&entity) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Manage blip entities: spawn a small dot at each unknown contact's
/// position, despawn when the contact becomes known.
pub fn sensor_blips(
    mut commands: Commands,
    contacts: Query<(Entity, &Transform), With<Contact>>,
    known: Res<KnownContacts>,
    blips: Query<(Entity, &Blip)>,
    assets: Option<Res<BlipAssets>>,
) {
    let Some(assets) = assets else { return };

    // Despawn blips for now-known contacts.
    for (blip_entity, blip) in &blips {
        if known.known.contains(&blip.for_contact) {
            commands.entity(blip_entity).despawn();
        }
    }

    // Spawn blips for unknown contacts that don't have one yet.
    for (contact_entity, transform) in &contacts {
        if known.known.contains(&contact_entity) {
            continue;
        }
        let already = blips.iter().any(|(_, b)| b.for_contact == contact_entity);
        if !already {
            commands.spawn((
                Mesh2d(assets.mesh.clone()),
                MeshMaterial2d(assets.material.clone()),
                Transform::from_xyz(transform.translation.x, transform.translation.y, 1.0),
                Blip {
                    for_contact: contact_entity,
                },
                ModeScope(GameMode::SpaceFlight),
            ));
        }
    }
}

/// Reposition blip entities to follow their contact entity (contacts don't
/// move in this sprint, but the design should support it).
pub fn sensor_blip_follow(
    contacts: Query<(&Transform,), With<Contact>>,
    mut blips: Query<(&mut Transform, &Blip)>,
) {
    for (mut tx, blip) in &mut blips {
        if let Ok((contact_tx,)) = contacts.get(blip.for_contact) {
            tx.translation.x = contact_tx.translation.x;
            tx.translation.y = contact_tx.translation.y;
        }
    }
}

/// Tracks whether the system-map overlay (`M` key in SpaceFlight) is open.
#[derive(Resource, Default)]
pub struct MapOverlayState {
    pub open: bool,
}

/// Marker for the system-map overlay UI entity.
#[derive(Component)]
pub struct MapOverlay;

/// Marker for the map's text child.
#[derive(Component)]
pub struct MapOverlayText;

fn contact_kind_name(
    entity: Entity,
    dockables: &Query<&Dockable>,
    gates: &Query<&Gate>,
) -> &'static str {
    if dockables.contains(entity) {
        "Station"
    } else if gates.contains(entity) {
        "Gate"
    } else {
        "Celestial"
    }
}

/// Toggle the system-map overlay on `M` press. Spawns / despawns a
/// semi-transparent panel listing all contacts (known vs unknown).
pub fn system_map(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<MapOverlayState>,
    mut commands: Commands,
    overlay: Query<Entity, With<MapOverlay>>,
) {
    if !keys.just_pressed(KeyCode::KeyM) {
        return;
    }
    if let Ok(e) = overlay.single() {
        commands.entity(e).despawn();
        state.open = false;
    } else {
        state.open = true;
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Auto,
                    top: Val::Auto,
                    right: Val::Auto,
                    bottom: Val::Auto,
                    width: Val::Px(500.0),
                    height: Val::Px(400.0),
                    align_self: AlignSelf::Center,
                    justify_self: JustifySelf::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
                MapOverlay,
            ))
            .with_child((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.9, 0.95)),
                MapOverlayText,
            ));
    }
}

/// Update the map overlay text each frame while open.
pub fn map_overlay_text(
    state: Res<MapOverlayState>,
    known: Res<KnownContacts>,
    contacts: Query<(Entity, &Transform), With<Contact>>,
    ship: Query<&Transform, With<PlayerShip>>,
    mut texts: Query<&mut Text, With<MapOverlayText>>,
    dockables: Query<&Dockable>,
    gates: Query<&Gate>,
) {
    if !state.open {
        return;
    }
    let Ok(mut t) = texts.single_mut() else {
        return;
    };
    let Ok(ship_pos) = ship.single() else {
        return;
    };
    let total = contacts.iter().count();
    let known_count = known.known.len();
    let mut s = format!("SYSTEM MAP  {known_count}/{total} known\n\n");
    for (entity, transform) in &contacts {
        let kind = contact_kind_name(entity, &dockables, &gates);
        let dist = transform
            .translation
            .truncate()
            .distance(ship_pos.translation.truncate()) as i64;
        let known_mark = if known.known.contains(&entity) {
            " "
        } else {
            "?"
        };
        let bearing = ((transform.translation.y - ship_pos.translation.y)
            .atan2(transform.translation.x - ship_pos.translation.x)
            .to_degrees()
            + 90.0)
            .rem_euclid(360.0);
        s.push_str(&format!(
            "  {known_mark} {kind:12} @ {dist:>6}m  {bearing:>3.0}°\n",
        ));
    }
    **t = s;
}

/// Hold `S` inside `SCAN_RANGE` of an unknown contact to resolve its
/// identity. Scan range is half the sensor range (configured per
/// `ShipSystems`).
const SCAN_RANGE_FRACTION: f32 = 0.5;

pub fn scan_contact(
    keys: Res<ButtonInput<KeyCode>>,
    ship: Query<&Transform, With<PlayerShip>>,
    contacts: Query<(Entity, &Transform), With<Contact>>,
    mut known: ResMut<KnownContacts>,
    systems: Res<ShipSystems>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
) {
    if !keys.pressed(KeyCode::KeyS) {
        return;
    }
    let Ok(ship_pos) = ship.single() else {
        return;
    };
    let scan_range = (systems.sensor_range.0 as f32 / 1024.0) * SCAN_RANGE_FRACTION;

    for (entity, transform) in &contacts {
        if known.known.contains(&entity) {
            continue;
        }
        let dist = transform
            .translation
            .truncate()
            .distance(ship_pos.translation.truncate());
        if dist <= scan_range {
            known.known.insert(entity);
            log.log(format!("Scanned contact at {:.0}m", dist));
        }
    }
}
