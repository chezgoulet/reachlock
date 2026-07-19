//! Native voice: dedicated thread + webrtc-rs audio tracks + Opus decode
//! + cpal mic capture + TURN + PTT sender.

use std::collections::HashMap;
use std::pin::Pin;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use crossbeam_channel::{Receiver, Sender};

pub struct TurnCreds {
    pub url: String,
    pub username: String,
    pub password: String,
}

pub enum VoiceCommand {
    CreatePeer { player_id: String, sdp: String },
    SetRemoteAnswer { player_id: String, sdp: String },
    AddIceCandidate { player_id: String, candidate: String, sdp_mid: String, sdp_mline_index: u16 },
    ClosePeer { player_id: String },
    SetMicActive(bool),
    SetTurnConfig(TurnCreds),
    Shutdown,
}

pub enum VoiceEvent {
    LocalAnswer { player_id: String, sdp: String },
    LocalIceCandidate { player_id: String, candidate: String, sdp_mid: String, sdp_mline_index: u16 },
    PeerConnected { player_id: String },
    PeerClosed { player_id: String },
    AudioFrame { player_id: String, pcm: Vec<f32> },
}

/// Crossbeam channel for mic PCM samples (f32 mono, 48 kHz).
pub type MicSender = crossbeam_channel::Sender<Vec<f32>>;
pub type MicReceiver = crossbeam_channel::Receiver<Vec<f32>>;

pub fn spawn_voice_thread(
    cmd_rx: Receiver<VoiceCommand>,
    evt_rx: Sender<VoiceEvent>,
    mic_rx: MicReceiver,
) -> Result<std::thread::JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name("reachlock-voice".into())
        .spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .expect("failed to create tokio runtime for voice thread");
            rt.block_on(voice_loop(cmd_rx, evt_rx, mic_rx));
        })
        .map_err(|e| format!("voice thread spawn failed: {e}"))
}

async fn voice_loop(
    cmd_rx: Receiver<VoiceCommand>,
    evt_tx: Sender<VoiceEvent>,
    mic_rx: MicReceiver,
) {
    use webrtc::api::interceptor_registry::register_default_interceptors;
    use webrtc::api::media_engine::MediaEngine;
    use webrtc::api::APIBuilder;
    use webrtc::interceptor::registry::Registry;
    use webrtc::peer_connection::configuration::RTCConfiguration;
    use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
    use webrtc::peer_connection::RTCPeerConnection;
    use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
    use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
    let api = {
        let mut m = MediaEngine::default();
        m.register_default_codecs().unwrap();
        let mut registry = Registry::default();
        registry = register_default_interceptors(registry, &mut m).unwrap();
        APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .build()
    };

    let mut peers: HashMap<String, RTCPeerConnection> = HashMap::new();
    let mut local_tracks: HashMap<String, Arc<TrackLocalStaticSample>> = HashMap::new();
    let mut turn_creds: Option<TurnCreds> = None;
    let mut mic_active = false;
    let mut pcm_accum: Vec<f32> = Vec::new();

    loop {
        match cmd_rx.try_recv() {
            Ok(cmd) => match cmd {
                VoiceCommand::CreatePeer { player_id, sdp } => {
                    let config = RTCConfiguration {
                        ice_servers: ice_servers(&turn_creds),
                        ..Default::default()
                    };
                    let pc = match api.new_peer_connection(config).await {
                        Ok(p) => p,
                        Err(e) => { log::error!("voice: new pc: {e}"); continue; }
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
                    pc.on_track(Box::new({
                        let evt = evt.clone();
                        let pid = pid.clone();
                        move |track, _, _| {
                            let evt = evt.clone();
                            let pid = pid.clone();
                            Box::pin(async move {
                                tokio::spawn(async move {
                                    let mut dec = opus::Decoder::new(48000, opus::Channels::Mono)
                                        .expect("opus decoder");
                                    let mut pcm = vec![0.0f32; 960];
                                    loop {
                                        match track.read_rtp().await {
                                            Ok((packet, _)) => {
                                                if packet.payload.is_empty() { continue; }
                                                match dec.decode_float(
                                                    &packet.payload, &mut pcm, false,
                                                ) {
                                                    Ok(n) => {
                                                        let _ = evt.send(VoiceEvent::AudioFrame {
                                                            player_id: pid.clone(),
                                                            pcm: pcm[..n].to_vec(),
                                                        });
                                                    }
                                                    Err(e) => log::debug!("opus decode: {e}"),
                                                }
                                            }
                                            Err(_) => break,
                                        }
                                    }
                                });
                            }) as Pin<Box<dyn Future<Output = ()> + Send>>
                        }
                    }));

                    let codec = RTCRtpCodecCapability {
                        mime_type: webrtc::api::media_engine::MIME_TYPE_OPUS.to_owned(),
                        ..Default::default()
                    };
                    let local_track = Arc::new(TrackLocalStaticSample::new(
                        codec, "audio".into(), "stream1".into(),
                    ));
                    let _ = pc.add_track(local_track.clone()).await;
                    local_tracks.insert(player_id.clone(), local_track);

                    let desc = RTCSessionDescription::offer(sdp).unwrap();
                    if let Err(e) = pc.set_remote_description(desc).await {
                        log::error!("voice: set_remote_desc: {e}"); continue;
                    }
                    let answer = match pc.create_answer(None).await {
                        Ok(a) => a,
                        Err(e) => { log::error!("voice: create_answer: {e}"); continue; }
                    };
                    if let Err(e) = pc.set_local_description(answer).await {
                        log::error!("voice: set_local_desc: {e}"); continue;
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
                            log::error!("voice: set_remote_answer: {e}");
                        }
                    }
                }

                VoiceCommand::AddIceCandidate { player_id, candidate, sdp_mid, sdp_mline_index } => {
                    if let Some(pc) = peers.get(&player_id) {
                        let init = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
                            candidate, sdp_mid: Some(sdp_mid),
                            sdp_mline_index: Some(sdp_mline_index),
                            username_fragment: None,
                        };
                        if let Err(e) = pc.add_ice_candidate(init).await {
                            log::error!("voice: add_ice_candidate: {e}");
                        }
                    }
                }

                VoiceCommand::ClosePeer { player_id } => {
                    if let Some(pc) = peers.remove(&player_id) {
                        let _ = pc.close().await;
                    }
                    local_tracks.remove(&player_id);
                    let _ = evt_tx.send(VoiceEvent::PeerClosed { player_id });
                }

                VoiceCommand::SetMicActive(active) => { mic_active = active; }

                VoiceCommand::SetTurnConfig(creds) => { turn_creds = Some(creds); }

                VoiceCommand::Shutdown => {
                    for (_, pc) in &peers { let _ = pc.close().await; }
                    break;
                }
            },
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => break,
        }

        // Mic encode: read PCM from channel, accumulate, write 960-sample frames.
        while let Ok(pcm) = mic_rx.try_recv() {
            pcm_accum.extend(pcm);
        }

        while pcm_accum.len() >= 960 && mic_active && !local_tracks.is_empty() {
            let chunk: Vec<f32> = pcm_accum.drain(..960).collect();
            let mut i16_buf = Vec::with_capacity(960 * 2);
            for &s in &chunk {
                i16_buf.extend_from_slice(
                    &((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes(),
                );
            }
            let sample = webrtc::media::Sample {
                data: Bytes::from(i16_buf),
                timestamp: std::time::SystemTime::now(),
                duration: Duration::from_millis(20),
                packet_timestamp: 0,
                prev_dropped_packets: 0,
                prev_padding_packets: 0,
            };
            for (_, track) in &local_tracks {
                let _ = track.write_sample(&sample).await;
            }
        }
    }
}

fn ice_servers(turn: &Option<TurnCreds>) -> Vec<webrtc::ice_transport::ice_server::RTCIceServer> {
    let mut servers = vec![webrtc::ice_transport::ice_server::RTCIceServer {
        urls: vec!["stun:stun.l.google.com:19302".into()],
        ..Default::default()
    }];
    if let Some(tc) = turn {
        servers.push(webrtc::ice_transport::ice_server::RTCIceServer {
            urls: vec![tc.url.clone()],
            username: tc.username.clone(),
            credential: tc.password.clone(),
            credential_type: webrtc::ice_transport::ice_credential_type::RTCIceCredentialType::Password,
        });
    }
    servers
}
