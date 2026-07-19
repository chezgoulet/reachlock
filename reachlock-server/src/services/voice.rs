//! S29 voice chat: room state tracking, signaling relay, TURN configuration.

use std::collections::HashMap;
use std::sync::Mutex;

use reachlock_core::network::VoiceSignalPayload;
use reachlock_core::seed::types::SystemId;

/// Per-player voice state within a system voice room.
#[derive(Debug, Clone)]
pub struct VoicePeerState {
    pub player_id: String,
    pub muted: bool,
    pub speaking: bool,
    pub connected: bool,
}

/// A voice room for one system: players in the same system share one room,
/// and WebRTC signaling is relayed within it.
#[derive(Debug)]
pub struct VoiceRoom {
    pub system_id: SystemId,
    pub peers: HashMap<String, VoicePeerState>,
}

/// Registry of all active voice rooms, keyed by system_id.
pub struct VoiceRegistry {
    rooms: Mutex<HashMap<String, VoiceRoom>>,
}

impl VoiceRegistry {
    pub fn new() -> Self {
        VoiceRegistry { rooms: Mutex::new(HashMap::new()) }
    }

    pub fn join(&self, system_id: &SystemId, player_id: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        let room = rooms.entry(system_id.0.clone()).or_insert(VoiceRoom {
            system_id: system_id.clone(),
            peers: HashMap::new(),
        });
        room.peers.insert(player_id.into(), VoicePeerState {
            player_id: player_id.into(),
            muted: false,
            speaking: false,
            connected: false,
        });
    }

    pub fn leave(&self, system_id: &SystemId, player_id: &str) {
        let mut rooms = self.rooms.lock().unwrap();
        if let Some(room) = rooms.get_mut(&system_id.0) {
            room.peers.remove(player_id);
            if room.peers.is_empty() {
                rooms.remove(&system_id.0);
            }
        }
    }

    /// Relay a signaling payload from a player to a target player in the
    /// same system. Returns `None` if the target is not in the room.
    pub fn relay(
        &self,
        system_id: &SystemId,
        _from: &str,
        target: &str,
        signal: &VoiceSignalPayload,
    ) -> Option<(String, VoiceSignalPayload)> {
        let rooms = self.rooms.lock().unwrap();
        let room = rooms.get(&system_id.0)?;
        if !room.peers.contains_key(target) {
            return None;
        }
        Some((target.into(), signal.clone()))
    }

    /// Get TURN server configuration from env vars.
    pub fn turn_config() -> Option<(String, String)> {
        let url = std::env::var("REACHLOCK_TURN_URL").ok()?;
        let secret = std::env::var("REACHLOCK_TURN_SECRET").ok()?;
        Some((url, secret))
    }
}

impl Default for VoiceRegistry {
    fn default() -> Self { Self::new() }
}
