#![cfg(target_arch = "wasm32")]

//! S29 WASM voice: browser RTCPeerConnection + AudioContext + getUserMedia + PannerNode.

use std::cell::RefCell;
use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use reachlock_core::network::VoiceSignalPayload;

// ---------------------------------------------------------------------------
// Peer state per remote player
// ---------------------------------------------------------------------------

struct PeerState {
    pc: web_sys::RtcPeerConnection,
    panner: web_sys::PannerNode,
    pending_candidates: Vec<(String, String, u16)>,
    _on_ice: Closure<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>,
    _on_track: Closure<dyn FnMut(web_sys::RtcTrackEvent)>,
}

// ---------------------------------------------------------------------------
// Thread-local state
// ---------------------------------------------------------------------------

thread_local! {
    static PEERS: RefCell<HashMap<String, PeerState>> = RefCell::new(HashMap::new());
    static AUDIO_CTX: RefCell<Option<web_sys::AudioContext>> = RefCell::new(None);
    static TURN_URL: RefCell<Option<String>> = RefCell::new(None);
    static TURN_USER: RefCell<Option<String>> = RefCell::new(None);
    static TURN_PASS: RefCell<Option<String>> = RefCell::new(None);
    static PENDING_OUT: RefCell<Vec<(String, VoiceSignalPayload)>> = RefCell::new(Vec::new());
    static MIC_STREAM: RefCell<Option<web_sys::MediaStream>> = RefCell::new(None);
}

// ---------------------------------------------------------------------------
// TURN
// ---------------------------------------------------------------------------

pub fn set_turn_config(url: &str, username: &str, password: &str) {
    TURN_URL.with(|c| *c.borrow_mut() = Some(url.to_owned()));
    TURN_USER.with(|c| *c.borrow_mut() = Some(username.to_owned()));
    TURN_PASS.with(|c| *c.borrow_mut() = Some(password.to_owned()));
}

// ---------------------------------------------------------------------------
// AudioContext (lazy, first use)
// ---------------------------------------------------------------------------

fn audio_ctx() -> Option<web_sys::AudioContext> {
    AUDIO_CTX.with(|cell| {
        let mut ctx = cell.borrow_mut();
        if ctx.is_none() {
            if let Ok(ac) = web_sys::AudioContext::new() {
                if ac.state() == web_sys::AudioContextState::Suspended {
                    let _ = ac.resume();
                }
                *ctx = Some(ac);
            }
        }
        ctx.clone()
    })
}

// ---------------------------------------------------------------------------
// Build peer config with STUN + optional TURN
// ---------------------------------------------------------------------------

fn build_peer_config() -> web_sys::RtcConfiguration {
    let config = web_sys::RtcConfiguration::new();
    let servers = js_sys::Array::new();

    let stun = web_sys::RtcIceServer::new();
    stun.set_urls_str("stun:stun.l.google.com:19302");
    servers.push(&stun);

    if let Some(url) = TURN_URL.with(|c| c.borrow().clone()) {
        let user = TURN_USER.with(|c| c.borrow().clone());
        let pass = TURN_PASS.with(|c| c.borrow().clone());
        if let (Some(u), Some(p)) = (user, pass) {
            let turn = web_sys::RtcIceServer::new();
            turn.set_urls_str(&url);
            turn.set_username(&u);
            turn.set_credential(&p);
            servers.push(&turn);
        }
    }

    config.set_ice_servers(&JsValue::from(&servers));
    config
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn handle_offer(player_id: &str, sdp: &str) {
    let ctx = match audio_ctx() {
        Some(c) => c,
        None => return,
    };

    let pc = match web_sys::RtcPeerConnection::new_with_configuration(&build_peer_config()) {
        Ok(p) => p,
        Err(e) => {
            log::error!("wasm-voice: new pc: {:?}", e);
            return;
        }
    };

    let panner = match ctx.create_panner() {
        Ok(p) => p,
        Err(e) => {
            log::error!("wasm-voice: create_panner: {:?}", e);
            return;
        }
    };
    panner.set_panning_model(web_sys::PanningModelType::Hrtf);
    panner.set_ref_distance(100.0);
    panner.set_max_distance(2000.0);
    let _ = panner.connect_with_audio_node(&ctx.destination());

    // ICE candidate callback
    let pid_ice = player_id.to_owned();
    let on_ice = Closure::wrap(Box::new(move |ev: web_sys::RtcPeerConnectionIceEvent| {
        if let Some(cand) = ev.candidate() {
            PENDING_OUT.with(|cell| {
                cell.borrow_mut().push((
                    pid_ice.clone(),
                    VoiceSignalPayload::IceCandidate {
                        candidate: cand.candidate(),
                        sdp_mid: cand.sdp_mid().unwrap_or_default(),
                        sdp_mline_index: cand.sdp_m_line_index().unwrap_or(0),
                    },
                ));
            });
        }
    }) as Box<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>);
    pc.set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));

    // Track callback — connect incoming audio to PannerNode
    let panner_track = panner.clone();
    let ctx_track = ctx.clone();
    let on_track = Closure::wrap(Box::new(move |ev: web_sys::RtcTrackEvent| {
        let streams: js_sys::Array = ev.streams();
        if streams.length() > 0 {
            let stream_val = streams.get(0);
            if !stream_val.is_undefined() {
                if let Ok(stream) = stream_val.dyn_into::<web_sys::MediaStream>() {
                    if let Ok(source) = ctx_track.create_media_stream_source(&stream) {
                        let _ = source.connect_with_audio_node(&panner_track);
                    }
                }
            }
        }
    }) as Box<dyn FnMut(web_sys::RtcTrackEvent)>);
    pc.set_ontrack(Some(on_track.as_ref().unchecked_ref()));

    // Also add our mic stream to the new peer if we already have one
    MIC_STREAM.with(|cell| {
        if let Some(stream) = cell.borrow().as_ref() {
            for track in stream.get_audio_tracks().iter() {
                let track: web_sys::MediaStreamTrack = track.into();
                let _ = pc.add_track_0(&track, stream);
            }
        }
    });

    // Store before async answer creation so ICE candidates arriving early can be queued
    PEERS.with(|cell| {
        cell.borrow_mut().insert(
            player_id.to_owned(),
            PeerState {
                pc: pc.clone(),
                panner: panner.clone(),
                pending_candidates: vec![],
                _on_ice: on_ice,
                _on_track: on_track,
            },
        );
    });

    // Async: set remote offer → create answer → set local description → relay answer
    let pid = player_id.to_owned();
    let sdp_owned = sdp.to_owned();
    wasm_bindgen_futures::spawn_local(async move {
        let mut offer_init = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
        offer_init.set_sdp(&sdp_owned);

        let _ = wasm_bindgen_futures::JsFuture::from(pc.set_remote_description(&offer_init))
            .await
            .map(|_| ())
            .map_err(|e| {
                log::error!("wasm-voice: set_remote_desc: {:?}", e);
            });

        let answer_val = match wasm_bindgen_futures::JsFuture::from(pc.create_answer()).await {
            Ok(v) => v,
            Err(e) => {
                log::error!("wasm-voice: create_answer: {:?}", e);
                return;
            }
        };
        let answer_desc: web_sys::RtcSessionDescriptionInit = answer_val.unchecked_into();
        let answer_sdp = answer_desc.get_sdp().unwrap_or_default();

        let _ = wasm_bindgen_futures::JsFuture::from(pc.set_local_description(&answer_desc))
            .await
            .map_err(|e| {
                log::error!("wasm-voice: set_local_desc: {:?}", e);
            });

        PENDING_OUT.with(|cell| {
            cell.borrow_mut()
                .push((pid, VoiceSignalPayload::Answer { sdp: answer_sdp }));
        });
    });
}

pub fn handle_answer(player_id: &str, sdp: &str) {
    PEERS.with(|cell| {
        let mut peers = cell.borrow_mut();
        if let Some(state) = peers.get_mut(player_id) {
            let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
            desc.set_sdp(sdp);
            let _ = wasm_bindgen_futures::JsFuture::from(state.pc.set_remote_description(&desc));
        }
    });
}

pub fn handle_ice_candidate(player_id: &str, candidate: &str, sdp_mid: &str, sdp_mline_index: u16) {
    PEERS.with(|cell| {
        let mut peers = cell.borrow_mut();
        if let Some(state) = peers.get_mut(player_id) {
            if state.pc.current_local_description().is_some() {
                let init = web_sys::RtcIceCandidateInit::new(candidate);
                init.set_sdp_mid(Some(sdp_mid));
                init.set_sdp_m_line_index(Some(sdp_mline_index));
                let promise = state
                    .pc
                    .add_ice_candidate_with_opt_rtc_ice_candidate_init(Some(&init));
                let _ = wasm_bindgen_futures::JsFuture::from(promise);
            } else {
                state.pending_candidates.push((
                    candidate.to_owned(),
                    sdp_mid.to_owned(),
                    sdp_mline_index,
                ));
            }
        }
    });
}

pub fn handle_hangup(player_id: &str) {
    PEERS.with(|cell| {
        if let Some(state) = cell.borrow_mut().remove(player_id) {
            let _ = state.pc.close();
            // closures (on_ice, on_track) dropped with PeerState
        }
    });
}

pub fn start_mic() {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let media_devices = match window.navigator().media_devices() {
        Ok(md) => md,
        Err(_) => return,
    };

    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_audio_bool(true);

    let promise = match media_devices.get_user_media_with_constraints(&constraints) {
        Ok(p) => p,
        Err(e) => {
            log::error!("wasm-voice: getUserMedia: {:?}", e);
            return;
        }
    };

    wasm_bindgen_futures::spawn_local(async move {
        let stream_val = match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(v) => v,
            Err(e) => {
                log::error!("wasm-voice: getUserMedia failed: {:?}", e);
                return;
            }
        };
        let stream: web_sys::MediaStream = stream_val.into();
        MIC_STREAM.with(|cell| *cell.borrow_mut() = Some(stream.clone()));

        PEERS.with(|cell| {
            for (_pid, state) in cell.borrow_mut().iter_mut() {
                for track in stream.get_audio_tracks().iter() {
                    let track: web_sys::MediaStreamTrack = track.into();
                    let _ = state.pc.add_track_0(&track, &stream);
                }
            }
        });
    });
}

pub fn update_spatial_position(player_id: &str, x: f64, y: f64, z: f64) {
    PEERS.with(|cell| {
        if let Some(state) = cell.borrow().get(player_id) {
            state.panner.set_position(x, y, z);
        }
    });
}

pub fn drain_outgoing_signals() -> Vec<(String, VoiceSignalPayload)> {
    PENDING_OUT.with(|cell| cell.borrow_mut().drain(..).collect())
}
