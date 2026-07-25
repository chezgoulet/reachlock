//! Client-side presence (S23): periodical player position updates to the
//! server, remote player ship rendering with eased interpolation, and chat
//! feed management.

use std::collections::VecDeque;

use bevy::prelude::*;

use reachlock_core::network::ClientMessage;

use crate::net::{NetMode, NetOutbox};
use crate::states::{CurrentLocation, GameMode, ModeScope};
use crate::systems::ship::PlayerShip;

/// Marker for a remote player's ship entity (other players in the same
/// system).
#[derive(Component)]
pub struct RemoteShip {
    pub player_id: String,
}

/// Cumulative time since last position broadcast (10 Hz = 100ms interval).
#[derive(Resource, Default)]
pub struct PositionSendTimer {
    pub accum: f32,
}

/// Send the player's position every 100ms when in SpaceFlight and online.
pub fn send_player_position(
    time: Res<Time>,
    mode: Res<NetMode>,
    location: Res<CurrentLocation>,
    ship: Query<&Transform, With<PlayerShip>>,
    mut timer: ResMut<PositionSendTimer>,
    mut outbox: ResMut<NetOutbox>,
) {
    if !matches!(&*mode, NetMode::Online { .. }) {
        timer.accum = 0.0;
        return;
    }
    let Ok(ship_tx) = ship.single() else {
        return;
    };
    timer.accum += time.delta_secs();
    if timer.accum < 0.1 {
        return;
    }
    timer.accum = 0.0;
    outbox.push(ClientMessage::PlayerPosition {
        system_id: location.system_id.clone(),
        position: [
            ship_tx.translation.x as i64,
            ship_tx.translation.y as i64,
            ship_tx.translation.z as i64,
        ],
    });
}

/// A lerp target for easing a remote ship toward its latest reported position.
#[derive(Component)]
pub struct RemoteShipEasing {
    pub target: Vec3,
}

/// Drain PresenceEvents and apply them to the game world (spawn/despawn
/// ships, push chat messages). Runs every frame to keep latency low.
pub fn handle_presence_events(
    mut events: ResMut<PresenceEvents>,
    mut chat: ResMut<ChatFeed>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    ships: Query<(Entity, &RemoteShip)>,
) {
    for player_id in events.joined.drain(..) {
        let mesh = meshes.add(Sphere::new(2.0));
        let mat = materials.add(Color::srgb(0.5, 0.6, 0.8));
        commands.spawn((
            RemoteShip {
                player_id: player_id.clone(),
            },
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_xyz(0.0, 0.0, 0.0),
            RemoteShipEasing { target: Vec3::ZERO },
            Visibility::default(),
            ModeScope(GameMode::SpaceFlight),
        ));
    }
    for player_id in events.left.drain(..) {
        for (entity, ship) in &ships {
            if ship.player_id == player_id {
                commands.entity(entity).despawn();
            }
        }
    }
    for (from_player, text) in events.chat_messages.drain(..) {
        push_chat_message(&mut chat, from_player, text);
    }
}

/// Ease remote ships toward their latest reported position.
pub fn ease_remote_ships(
    time: Res<Time>,
    mut ships: Query<(&mut Transform, &RemoteShipEasing), With<RemoteShip>>,
) {
    let speed = time.delta_secs() * 5.0;
    for (mut tx, easing) in &mut ships {
        tx.translation = tx.translation.lerp(easing.target, speed);
    }
}

// --- events for decoupling network handling from ECS ---

/// Buffer for presence events from network.rs (avoids adding more params
/// to poll_network). Drained by `handle_presence_events`.
#[derive(Resource, Default)]
pub struct PresenceEvents {
    pub joined: Vec<String>,
    pub left: Vec<String>,
    pub chat_messages: Vec<(String, String)>,
}

/// In-memory chat feed: scrollback of up to 50 messages.
#[derive(Resource, Default)]
pub struct ChatFeed {
    pub messages: VecDeque<ChatEntry>,
}

#[allow(dead_code)]
pub struct ChatEntry {
    pub from_player: String,
    pub text: String,
}

/// Push an incoming chat message into the feed.
pub fn push_chat_message(feed: &mut ChatFeed, from_player: String, text: String) {
    feed.messages.push_back(ChatEntry { from_player, text });
    while feed.messages.len() > 50 {
        feed.messages.pop_front();
    }
}
