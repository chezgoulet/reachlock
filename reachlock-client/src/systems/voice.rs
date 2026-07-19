//! S29 voice chat: WebRTC peer connection management and audio pipeline.
//! Native builds use `webrtc-rs`; WASM uses the browser's `RTCPeerConnection`.

use std::collections::HashMap;

use bevy::prelude::*;

use reachlock_core::network::{VoiceSignalPayload, ServerMessage};

use crate::net::NetOutbox;
use crate::systems::presence::PresenceEvents;

/// Tracks WebRTC peer connections indexed by remote player id.
#[derive(Resource, Default)]
pub struct VoiceManager {
    pub connections: HashMap<String, VoicePeerConnection>,
}

/// Stub for a WebRTC peer connection. Full webrtc-rs integration is
/// planned — for now, this tracks which peers are in voice range.
pub struct VoicePeerConnection {
    pub player_id: String,
    pub connected: bool,
    pub muted: bool,
}

/// Handles incoming VoiceSignal messages from the network layer by
/// forwarding them to PresenceEvents for processing.
pub fn handle_voice_signal(
    mut presence: ResMut<PresenceEvents>,
    signals: Res<VoiceSignalBuffer>,
) {
    for (_from, _signal) in &signals.queue {
        // S29 follow-up: create/update WebRTC peer connections here.
        // The signal buffer decouples network polling from ECS.
    }
}

/// Temporary buffer for voice signals received from the network.
/// In the full implementation, this is replaced by webrtc-rs peer
/// connection signaling.
#[derive(Resource, Default)]
pub struct VoiceSignalBuffer {
    pub queue: Vec<(String, VoiceSignalPayload)>,
}
