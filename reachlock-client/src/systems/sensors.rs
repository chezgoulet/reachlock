//! Sensor visibility, blip rendering, and the system map (S09, 3D in S09b).
//! Runs in `SpaceFlight`: contacts beyond `ShipSystems.sensor_range` are hidden
//! and replaced by a small 3D blip; the scanner console (or the `T` pulse)
//! resolves identities (see `ship::scanner_pulse`). The system map (`M`)
//! overlays all known contacts as HUD text.

use bevy::prelude::*;
use std::collections::HashMap;

use crate::states::{GameMode, ModeScope};
use crate::systems::docking::Dockable;
use crate::systems::setup::Gate;
use crate::systems::ship::{PlayerShip, ShipSystems};

/// Marker for entities the player's sensors can detect (stations, planets,
/// gates, asteroids). Spawned by `setup.rs` alongside the entity's visual.
#[derive(Component)]
pub struct Contact;

/// Tracks which contacts have been resolved by sensors or scanning.
#[derive(Resource, Default)]
pub struct KnownContacts {
    pub known: std::collections::HashSet<Entity>,
}

/// Marker for a blip entity — a placeholder rendered at a contact's world
/// position while it has not yet been resolved.
#[derive(Component)]
pub struct Blip {
    pub for_contact: Entity,
}

/// The blip mesh and material, built once and shared across all blips.
#[derive(Resource)]
pub struct BlipAssets {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

/// Spawn the blip assets once (on app init).
pub fn init_blip_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(BlipAssets {
        mesh: meshes.add(Sphere::new(4.0)),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.5, 0.7),
            emissive: LinearRgba::rgb(0.2, 0.5, 1.0),
            unlit: true,
            ..default()
        }),
    });
}

/// Each frame: for every `Contact`, check distance from the player ship. Within
/// sensor range → mark known and show the real entity; beyond and unknown →
/// hide the real entity (the blip stands in for it).
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
        let dist = transform.translation.distance(ship_pos.translation);
        let is_known = known.known.contains(&entity);
        if dist <= sensor_range_f32 && !is_known {
            known.known.insert(entity);
        }
        if let Ok(mut v) = vis.get_mut(entity) {
            *v = if dist <= sensor_range_f32 || known.known.contains(&entity) {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

/// Manage blip entities: spawn a small 3D dot at each unknown contact, despawn
/// when the contact becomes known.
pub fn sensor_blips(
    mut commands: Commands,
    contacts: Query<(Entity, &Transform), With<Contact>>,
    known: Res<KnownContacts>,
    blips: Query<(Entity, &Blip)>,
    assets: Option<Res<BlipAssets>>,
) {
    let Some(assets) = assets else { return };

    for (blip_entity, blip) in &blips {
        if known.known.contains(&blip.for_contact) {
            commands.entity(blip_entity).despawn();
        }
    }
    for (contact_entity, transform) in &contacts {
        if known.known.contains(&contact_entity) {
            continue;
        }
        let already = blips.iter().any(|(_, b)| b.for_contact == contact_entity);
        if !already {
            commands.spawn((
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                Transform::from_translation(transform.translation),
                Blip {
                    for_contact: contact_entity,
                },
                ModeScope(GameMode::SpaceFlight),
            ));
        }
    }
}

/// Reposition blips to follow their contact entity.
#[allow(clippy::type_complexity)]
pub fn sensor_blip_follow(
    mut params: ParamSet<(
        Query<(Entity, &Transform), With<Contact>>,
        Query<(&mut Transform, &Blip)>,
    )>,
) {
    let positions: HashMap<Entity, Vec3> = params
        .p0()
        .iter()
        .map(|(e, t)| (e, t.translation))
        .collect();
    for (mut tx, blip) in &mut params.p1() {
        if let Some(p) = positions.get(&blip.for_contact) {
            tx.translation = *p;
        }
    }
}

/// Tracks whether the system-map overlay (`M`) is open.
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

/// Toggle the system-map overlay on `M`.
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

/// Update the map overlay text each frame while open. Bearing is measured on
/// the XZ flight plane.
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
        let dist = transform.translation.distance(ship_pos.translation) as i64;
        let known_mark = if known.known.contains(&entity) {
            " "
        } else {
            "?"
        };
        let bearing = ((transform.translation.x - ship_pos.translation.x)
            .atan2(-(transform.translation.z - ship_pos.translation.z))
            .to_degrees())
        .rem_euclid(360.0);
        s.push_str(&format!(
            "  {known_mark} {kind:12} @ {dist:>6}m  {bearing:>3.0}°\n",
        ));
    }
    **t = s;
}
