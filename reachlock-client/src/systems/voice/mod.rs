//! S29 voice chat: WebRTC peer connection management and audio pipeline.

use std::collections::HashMap;
use std::sync::Mutex;

use bevy::audio::{AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use reachlock_core::network::{ClientMessage, VoiceSignalPayload};

use crate::net::NetOutbox;
use crate::settings::Settings;
use crate::systems::presence::RemoteShip;

/// Global push buffer for voice signals from poll_network. Kept separate
/// from the Bevy ECS Resource so poll_network (which already has 16 params)
/// doesn't need another system parameter. Drained by process_voice_signals
/// into the actual Resource each frame.
static VOICE_GLOBAL_BUF: Mutex<Vec<(String, VoiceSignalPayload)>> = Mutex::new(Vec::new());

/// Push a voice signal from the network layer (poll_network).
pub fn push_signal(from_player: String, signal: VoiceSignalPayload) {
    if let Ok(mut buf) = VOICE_GLOBAL_BUF.lock() {
        buf.push((from_player, signal));
    }
}

/// Drain the global buffer into the ECS Resource. Called at the top of
/// process_voice_signals.
fn drain_global_signals(buffer: &mut VoiceSignalBuffer) {
    if let Ok(mut global) = VOICE_GLOBAL_BUF.lock() {
        buffer.queue.append(&mut *global);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod voice_native;
#[cfg(target_arch = "wasm32")]
mod voice_wasm;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct VoiceSignalBuffer {
    pub queue: Vec<(String, VoiceSignalPayload)>,
}

#[derive(Default)]
struct PcmBuffer {
    frames: Vec<f32>,
}

#[derive(Resource)]
pub struct VoiceManager {
    #[cfg(not(target_arch = "wasm32"))]
    pub cmd_tx: Option<crossbeam_channel::Sender<voice_native::VoiceCommand>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub evt_rx: crossbeam_channel::Receiver<voice_native::VoiceEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    pcm_buffers: Mutex<HashMap<String, PcmBuffer>>,
    #[cfg(not(target_arch = "wasm32"))]
    thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl VoiceManager {
    pub fn new_native(
        cmd_tx: crossbeam_channel::Sender<voice_native::VoiceCommand>,
        evt_rx: crossbeam_channel::Receiver<voice_native::VoiceEvent>,
        handle: std::thread::JoinHandle<()>,
    ) -> Self {
        VoiceManager {
            cmd_tx: Some(cmd_tx),
            evt_rx,
            pcm_buffers: Mutex::new(HashMap::new()),
            thread_handle: Mutex::new(Some(handle)),
        }
    }
}

impl Drop for VoiceManager {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(tx) = self.cmd_tx.take() {
                let _ = tx.send(voice_native::VoiceCommand::Shutdown);
            }
            if let Ok(mut handle) = self.thread_handle.lock() {
                if let Some(h) = handle.take() {
                    let _ = h.join();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// WAV encoding
// ---------------------------------------------------------------------------

fn pcm_to_wav(pcm: &[f32], sample_rate: u32) -> Vec<u8> {
    let num_samples = pcm.len();
    let data_size = (num_samples * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_size as usize);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for &sample in pcm {
        let clamped = sample.clamp(-1.0, 1.0);
        let i = (clamped * i16::MAX as f32) as i16;
        wav.extend_from_slice(&i.to_le_bytes());
    }
    wav
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

pub fn process_voice_signals(
    mut buffer: ResMut<VoiceSignalBuffer>,
    manager: ResMut<VoiceManager>,
    mut outbox: ResMut<NetOutbox>,
) {
    drain_global_signals(&mut buffer);
    let signals = std::mem::take(&mut buffer.queue);

    #[cfg(not(target_arch = "wasm32"))]
    {
        let cmd_tx = match &manager.cmd_tx {
            Some(tx) => tx,
            None => return,
        };
        for (from_player, signal) in signals {
            match signal {
                VoiceSignalPayload::Offer { sdp } => {
                    let _ = cmd_tx.send(voice_native::VoiceCommand::CreatePeer {
                        player_id: from_player,
                        sdp,
                    });
                }
                VoiceSignalPayload::Answer { sdp } => {
                    let _ = cmd_tx.send(voice_native::VoiceCommand::SetRemoteAnswer {
                        player_id: from_player,
                        sdp,
                    });
                }
                VoiceSignalPayload::IceCandidate { candidate, sdp_mid, sdp_mline_index } => {
                    let _ = cmd_tx.send(voice_native::VoiceCommand::AddIceCandidate {
                        player_id: from_player,
                        candidate,
                        sdp_mid,
                        sdp_mline_index,
                    });
                }
                VoiceSignalPayload::Hangup => {
                    let _ = cmd_tx.send(voice_native::VoiceCommand::ClosePeer {
                        player_id: from_player,
                    });
                }
            }
        }

        while let Ok(evt) = manager.evt_rx.try_recv() {
            match evt {
                voice_native::VoiceEvent::LocalAnswer { player_id, sdp } => {
                    outbox.push(ClientMessage::VoiceSignal {
                        target_player: player_id,
                        signal: VoiceSignalPayload::Answer { sdp },
                    });
                }
                voice_native::VoiceEvent::LocalIceCandidate { player_id, candidate, sdp_mid, sdp_mline_index } => {
                    outbox.push(ClientMessage::VoiceSignal {
                        target_player: player_id,
                        signal: VoiceSignalPayload::IceCandidate { candidate, sdp_mid, sdp_mline_index },
                    });
                }
                voice_native::VoiceEvent::PeerConnected { .. } => {}
                voice_native::VoiceEvent::PeerClosed { player_id } => {
                    let mut bufs = manager.pcm_buffers.lock().unwrap();
                    bufs.remove(&player_id);
                }
                voice_native::VoiceEvent::AudioFrame { player_id, pcm } => {
                    let mut bufs = manager.pcm_buffers.lock().unwrap();
                    let buf = bufs.entry(player_id).or_default();
                    buf.frames.extend(pcm);
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        for (from_player, signal) in signals {
            match signal {
                VoiceSignalPayload::Offer { sdp } => {
                    voice_wasm::handle_offer(&from_player, &sdp);
                }
                VoiceSignalPayload::Answer { sdp } => {
                    voice_wasm::handle_answer(&from_player, &sdp);
                }
                VoiceSignalPayload::IceCandidate { candidate, sdp_mid, sdp_mline_index } => {
                    voice_wasm::handle_ice_candidate(&from_player, &candidate, &sdp_mid, sdp_mline_index);
                }
                VoiceSignalPayload::Hangup => {
                    voice_wasm::handle_hangup(&from_player);
                }
            }
        }

        for (target, signal) in voice_wasm::drain_outgoing_signals() {
            outbox.push(ClientMessage::VoiceSignal {
                target_player: target,
                signal,
            });
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn audio_feed_voice(
    manager: Res<VoiceManager>,
    mut commands: Commands,
    mut audio_sources: ResMut<Assets<AudioSource>>,
    remote_ships: Query<(Entity, &RemoteShip, &GlobalTransform)>,
    settings: Res<Settings>,
) {
    let gain = settings.audio.master_volume * settings.audio.voice_volume;
    let mut bufs = manager.pcm_buffers.lock().unwrap();

    let keys: Vec<String> = bufs.keys().cloned().collect();
    for player_id in keys {
        let buf = match bufs.get_mut(&player_id) {
            Some(b) => b,
            None => continue,
        };

        let chunk_size = 960.min(buf.frames.len());
        if chunk_size < 120 {
            continue;
        }

        let chunk: Vec<f32> = buf.frames.drain(..chunk_size).collect();
        let wav = pcm_to_wav(&chunk, 48000);
        let source = audio_sources.add(AudioSource { bytes: wav.into() });

        let transform = remote_ships
            .iter()
            .find(|(_, s, _)| s.player_id == *player_id)
            .map(|(_, _, tx)| *tx)
            .unwrap_or(GlobalTransform::default());

        commands.spawn((
            AudioPlayer(source),
            PlaybackSettings {
                volume: Volume::Linear(gain),
                spatial: true,
                ..default()
            },
            Transform::from_translation(transform.translation()),
            Visibility::default(),
        ));
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(unused_variables)]
pub fn audio_feed_voice(
    manager: Res<VoiceManager>,
    commands: Commands,
    audio_sources: ResMut<Assets<AudioSource>>,
    remote_ships: Query<(Entity, &RemoteShip, &GlobalTransform)>,
    settings: Res<Settings>,
) {
    // WASM: audio handled by browser AudioContext directly.
}

// ---------------------------------------------------------------------------
// Push-to-talk
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub fn ptt_system(
    input: Res<ButtonInput<KeyCode>>,
    manager: Res<VoiceManager>,
    settings: Res<Settings>,
) {
    let key = settings.key(crate::settings::InputAction::VoicePushToTalk);
    let held = input.pressed(key);
    if let Some(tx) = &manager.cmd_tx {
        let _ = tx.send(voice_native::VoiceCommand::SetMicActive(held));
    }
}

#[cfg(target_arch = "wasm32")]
pub fn ptt_system(
    _input: Res<ButtonInput<KeyCode>>,
    _manager: Res<VoiceManager>,
    _settings: Res<Settings>,
) {
    // WASM: getUserMedia needs user gesture. PTT key provides it.
    // Full mic integration deferred — voice_wasm::start_mic is a placeholder.
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub fn start_voice_thread(mut commands: Commands) {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (evt_tx, evt_rx) = crossbeam_channel::unbounded();
    let handle = voice_native::spawn_voice_thread(cmd_rx, evt_tx);
    commands.insert_resource(VoiceManager::new_native(cmd_tx, evt_rx, handle));
}
