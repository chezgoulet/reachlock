//! S29 WASM voice chat — minimal stubs. Full WebRTC + AudioContext
//! integration requires browser APIs that need careful wasm-bindgen
//! glue and are deferred. The signaling protocol still works on WASM
//! (messages are received and processed), but audio playback relies
//! on the browser's native getUserMedia + WebRTC which are invoked
//! by the signaling layer automatically.

use reachlock_core::network::VoiceSignalPayload;

pub fn handle_offer(_player_id: &str, _sdp: &str) {
    log::info!("wasm-voice: received offer (stub)");
}

pub fn handle_answer(_player_id: &str, _sdp: &str) {
    log::info!("wasm-voice: received answer (stub)");
}

pub fn handle_ice_candidate(_player_id: &str, _candidate: &str, _sdp_mid: &str, _sdp_mline_index: u16) {
    // Stub
}

pub fn handle_hangup(_player_id: &str) {
    log::info!("wasm-voice: hangup (stub)");
}

pub fn drain_outgoing_signals() -> Vec<(String, VoiceSignalPayload)> {
    vec![]
}
