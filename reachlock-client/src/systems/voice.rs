//! S29 voice chat: WebRTC peer connection management and audio pipeline.
//! Native uses `webrtc-rs` behind a `Mutex` bridge so Bevy's sync ECS can
//! interact with the async WebRTC event loop. WASM uses browser WebRTC.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use reachlock_core::network::VoiceSignalPayload;

/// Tracked WebRTC peer connection per remote player (stub — full webrtc-rs
/// integration is planned but requires async state management bridging).
#[derive(Resource, Default)]
pub struct VoiceManager {
    pub peers: HashMap<String, bool>, // player_id → connected
}

/// Buffer for incoming voice signals from the network layer.
#[derive(Resource, Default)]
pub struct VoiceSignalBuffer {
    pub queue: Vec<(String, VoiceSignalPayload)>,
}

/// Drain the voice signal buffer. Full webrtc-rs peer connection creation
/// and audio pipeline will be added here.
pub fn process_voice_signals(
    mut buffer: ResMut<VoiceSignalBuffer>,
    mut manager: ResMut<VoiceManager>,
) {
    let signals = std::mem::take(&mut buffer.queue);
    for (from_player, signal) in &signals {
        match signal {
            VoiceSignalPayload::Offer { .. } => {
                manager.peers.insert(from_player.clone(), true);
            }
            VoiceSignalPayload::Hangup => {
                manager.peers.remove(from_player);
            }
            _ => {}
        }
    }
}

