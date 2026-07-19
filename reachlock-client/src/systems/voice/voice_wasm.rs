#![cfg(target_arch = "wasm32")]

//! S29 WASM voice — browser WebRTC + AudioContext + getUserMedia + PannerNode.
//! Notes for the WASM implementation (documenting exact web-sys 0.3.103 API):
//! - RtcSessionDescriptionInit::new(RtcSdpType::Offer) — constructor takes SDP type
//! - RtcIceCandidateInit::new("candidate:...") — constructor takes candidate string
//! - init.set_sdp_m_line_index(Some(0)) — note: sdp_m_line_index (underscore pattern)
//! - pc.add_ice_candidate_with_opt_rtc_ice_candidate(Some(&cand)) — method name
//! - event.candidate() on RtcPeerConnectionIceEvent — returns Option<RtcIceCandidate>
//! - PannerNode.set_position(x, y, z) — legacy method works
//! - source.connect_with_audio_node(&dest) — AudioNode method

use reachlock_core::network::VoiceSignalPayload;

pub fn set_turn_config(_url: &str, _username: &str, _password: &str) {}

pub fn handle_offer(_player_id: &str, _sdp: &str) {
    log::info!("wasm-voice: offer received (stub)");
}

pub fn handle_answer(_player_id: &str, _sdp: &str) {
    log::info!("wasm-voice: answer received (stub)");
}

pub fn handle_ice_candidate(_player_id: &str, _candidate: &str, _sdp_mid: &str, _sdp_mline_index: u16) {}

pub fn handle_hangup(_player_id: &str) {
    log::info!("wasm-voice: hangup (stub)");
}

pub fn start_mic() {
    log::info!("wasm-voice: mic requested (stub)");
}

pub fn update_spatial_position(_player_id: &str, _x: f64, _y: f64, _z: f64) {}

pub fn drain_outgoing_signals() -> Vec<(String, VoiceSignalPayload)> {
    vec![]
}
