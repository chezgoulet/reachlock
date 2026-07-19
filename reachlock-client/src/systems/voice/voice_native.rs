//! Native voice backend: dedicated thread + webrtc-rs data channels.

use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;

use crossbeam_channel::{Receiver, Sender};
use webrtc::data_channel::RTCDataChannel;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

pub enum VoiceCommand {
    CreatePeer { player_id: String, sdp: String },
    SetRemoteAnswer { player_id: String, sdp: String },
    AddIceCandidate { player_id: String, candidate: String, sdp_mid: String, sdp_mline_index: u16 },
    ClosePeer { player_id: String },
    SetMicActive(bool),
    Shutdown,
}

pub enum VoiceEvent {
    LocalAnswer { player_id: String, sdp: String },
    LocalIceCandidate { player_id: String, candidate: String, sdp_mid: String, sdp_mline_index: u16 },
    #[allow(dead_code)]
    PeerConnected { player_id: String },
    PeerClosed { player_id: String },
    AudioFrame { player_id: String, pcm: Vec<f32> },
}

// ---------------------------------------------------------------------------
// Voice thread
// ---------------------------------------------------------------------------

pub fn spawn_voice_thread(
    cmd_rx: Receiver<VoiceCommand>,
    evt_tx: Sender<VoiceEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("reachlock-voice".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .expect("failed to create tokio runtime for voice thread");
            rt.block_on(voice_loop(cmd_rx, evt_tx));
        })
        .expect("failed to spawn voice thread")
}

// ---------------------------------------------------------------------------
// Inner async loop
// ---------------------------------------------------------------------------

async fn voice_loop(cmd_rx: Receiver<VoiceCommand>, evt_tx: Sender<VoiceEvent>) {
    use webrtc::api::APIBuilder;
    use webrtc::peer_connection::configuration::RTCConfiguration;
    use webrtc::peer_connection::RTCPeerConnection;

    let api = {
        let m = webrtc::api::media_engine::MediaEngine::default();
        APIBuilder::new().with_media_engine(m).build()
    };

    let mut peers: HashMap<String, RTCPeerConnection> = HashMap::new();
    loop {
        let cmd = match cmd_rx.recv() {
            Ok(c) => c,
            Err(_) => break,
        };

        match cmd {
            VoiceCommand::CreatePeer { player_id, sdp } => {
                let config = RTCConfiguration {
                    ice_servers: vec![webrtc::ice_transport::ice_server::RTCIceServer {
                        urls: vec!["stun:stun.l.google.com:19302".into()],
                        ..Default::default()
                    }],
                    ..Default::default()
                };
                let pc: RTCPeerConnection = match api.new_peer_connection(config).await {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("voice: new peer connection failed: {e}");
                        continue;
                    }
                };

                let evt = evt_tx.clone();
                let pid = player_id.clone();
                pc.on_ice_candidate(Box::new({
                    let evt = evt.clone();
                    let pid = pid.clone();
                    move |candidate: Option<webrtc::ice_transport::ice_candidate::RTCIceCandidate>| {
                        if let Some(c) = candidate {
                            if let Ok(init) = c.to_json() {
                                let _ = evt.send(VoiceEvent::LocalIceCandidate {
                                    player_id: pid.clone(),
                                    candidate: init.candidate,
                                    sdp_mid: init.sdp_mid.unwrap_or_default(),
                                    sdp_mline_index: init.sdp_mline_index.unwrap_or(0),
                                });
                            }
                        }
                        Box::pin(async {})
                    }
                }));

                let evt = evt_tx.clone();
                let pid = player_id.clone();
                pc.on_data_channel(Box::new({
                    let evt = evt.clone();
                    let pid = pid.clone();
                    move |dc: std::sync::Arc<RTCDataChannel>| {
                        let evt = evt.clone();
                        let pid = pid.clone();
                        Box::pin(async move {
                            tokio::spawn(async move {
                                read_audio_dc(dc, pid, evt).await;
                            });
                        }) as Pin<Box<dyn Future<Output = ()> + Send>>
                    }
                }));

                let desc = RTCSessionDescription::offer(sdp).unwrap();
                if let Err(e) = pc.set_remote_description(desc).await {
                    log::error!("voice: set_remote_description failed: {e}");
                    continue;
                }
                let answer = match pc.create_answer(None).await {
                    Ok(a) => a,
                    Err(e) => {
                        log::error!("voice: create_answer failed: {e}");
                        continue;
                    }
                };
                if let Err(e) = pc.set_local_description(answer).await {
                    log::error!("voice: set_local_description failed: {e}");
                    continue;
                }
                if let Some(local) = pc.local_description().await {
                    let _ = evt_tx.send(VoiceEvent::LocalAnswer {
                        player_id: player_id.clone(),
                        sdp: local.sdp.clone(),
                    });
                }

                let _ = evt_tx.send(VoiceEvent::PeerConnected { player_id: player_id.clone() });
                peers.insert(player_id.clone(), pc);
            }

            VoiceCommand::SetRemoteAnswer { player_id, sdp } => {
                if let Some(pc) = peers.get(&player_id) {
                    let desc = RTCSessionDescription::answer(sdp).unwrap();
                    if let Err(e) = pc.set_remote_description(desc).await {
                        log::error!("voice: set_remote_description(answer) failed: {e}");
                    }
                }
            }

            VoiceCommand::AddIceCandidate { player_id, candidate, sdp_mid, sdp_mline_index } => {
                if let Some(pc) = peers.get(&player_id) {
                    let init = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
                        candidate,
                        sdp_mid: Some(sdp_mid),
                        sdp_mline_index: Some(sdp_mline_index),
                        username_fragment: None,
                    };
                    if let Err(e) = pc.add_ice_candidate(init).await {
                        log::error!("voice: add_ice_candidate failed: {e}");
                    }
                }
            }

            VoiceCommand::ClosePeer { player_id } => {
                if let Some(pc) = peers.remove(&player_id) {
                    let _ = pc.close().await;
                    let _ = evt_tx.send(VoiceEvent::PeerClosed { player_id });
                }
            }

            VoiceCommand::SetMicActive(_active) => {}

            VoiceCommand::Shutdown => {
                for (_, pc) in &peers {
                    let _ = pc.close().await;
                }
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Data channel reader
// ---------------------------------------------------------------------------

async fn read_audio_dc(
    dc: std::sync::Arc<RTCDataChannel>,
    player_id: String,
    evt_tx: Sender<VoiceEvent>,
) {
    use webrtc::data_channel::data_channel_message::DataChannelMessage;

    dc.on_message(Box::new({
        let evt = evt_tx.clone();
        let pid = player_id.clone();
        move |msg: DataChannelMessage| {
            let payload = msg.data.to_vec();
            if payload.len() < 2 {
                return Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>;
            }

            let num_samples = payload.len() / 2;
            let mut pcm = Vec::with_capacity(num_samples);
            for chunk in payload.chunks_exact(2) {
                let val = i16::from_le_bytes([chunk[0], chunk[1]]);
                pcm.push(val as f32 / i16::MAX as f32);
            }

            let _ = evt.send(VoiceEvent::AudioFrame {
                player_id: pid.clone(),
                pcm,
            });
            Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
        }
    }));
}
