//! S29 voice: WebRTC + Opus + spatial audio + PTT + mic capture.

use std::collections::HashMap;
use std::sync::Mutex;

use bevy::audio::{AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use reachlock_core::network::{ClientMessage, VoiceSignalPayload};

use crate::net::NetOutbox;
use crate::settings::{InputAction, Settings};
use crate::systems::presence::RemoteShip;

#[cfg(not(target_arch = "wasm32"))]
mod voice_native;
#[cfg(target_arch = "wasm32")]
mod voice_wasm;

// ---------------------------------------------------------------------------
// Global push buffer (avoids 17th param on poll_network)
// ---------------------------------------------------------------------------

static VOICE_GLOBAL_BUF: Mutex<Vec<(String, VoiceSignalPayload)>> = Mutex::new(Vec::new());
static TURN_GLOBAL_BUF: Mutex<Option<(String, String, String, u32)>> = Mutex::new(None);

pub fn push_signal(from_player: String, signal: VoiceSignalPayload) {
    if let Ok(mut buf) = VOICE_GLOBAL_BUF.lock() {
        buf.push((from_player, signal));
    }
}

pub fn push_turn_config(url: String, username: String, password: String, ttl_secs: u32) {
    if let Ok(mut buf) = TURN_GLOBAL_BUF.lock() {
        *buf = Some((url, username, password, ttl_secs));
    }
}

fn drain_global_signals(buffer: &mut VoiceSignalBuffer) {
    if let Ok(mut global) = VOICE_GLOBAL_BUF.lock() {
        buffer.queue.append(&mut *global);
    }
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct VoiceSignalBuffer {
    pub queue: Vec<(String, VoiceSignalPayload)>,
}

#[derive(Default)]
pub(crate) struct PcmBuffer {
    frames: Vec<f32>,
}

/// Available microphone input devices (enumerated at startup).
#[derive(Resource, Default)]
pub struct MicDevices {
    pub devices: Vec<String>,
    pub current_index: usize,
}

#[derive(Resource)]
pub struct VoiceManager {
    #[cfg(not(target_arch = "wasm32"))]
    pub cmd_tx: Option<crossbeam_channel::Sender<voice_native::VoiceCommand>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub evt_rx: crossbeam_channel::Receiver<voice_native::VoiceEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    pub pcm_buffers: Mutex<HashMap<String, PcmBuffer>>,
    #[cfg(not(target_arch = "wasm32"))]
    thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    pub mic_tx: Option<voice_native::MicSender>,
}

#[cfg(not(target_arch = "wasm32"))]
impl VoiceManager {
    pub fn new_native(
        cmd_tx: Option<crossbeam_channel::Sender<voice_native::VoiceCommand>>,
        evt_rx: crossbeam_channel::Receiver<voice_native::VoiceEvent>,
        handle: std::thread::JoinHandle<()>,
        mic_tx: Option<voice_native::MicSender>,
    ) -> Self {
        VoiceManager {
            cmd_tx,
            evt_rx,
            pcm_buffers: Mutex::new(HashMap::new()),
            thread_handle: Mutex::new(Some(handle)),
            mic_tx,
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

    // Drain TURN config from the global buffer (set by poll_network).
    if let Ok(mut turn_buf) = TURN_GLOBAL_BUF.lock() {
        if let Some((url, username, password, _ttl)) = turn_buf.take() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(tx) = &manager.cmd_tx {
                let _ = tx.send(voice_native::VoiceCommand::SetTurnConfig(
                    voice_native::TurnCreds {
                        url,
                        username,
                        password,
                    },
                ));
            }
            #[cfg(target_arch = "wasm32")]
            voice_wasm::set_turn_config(&url, &username, &password);
        }
    }

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
                VoiceSignalPayload::IceCandidate {
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                } => {
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
                voice_native::VoiceEvent::LocalIceCandidate {
                    player_id,
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                } => {
                    outbox.push(ClientMessage::VoiceSignal {
                        target_player: player_id,
                        signal: VoiceSignalPayload::IceCandidate {
                            candidate,
                            sdp_mid,
                            sdp_mline_index,
                        },
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
                VoiceSignalPayload::IceCandidate {
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                } => {
                    voice_wasm::handle_ice_candidate(
                        &from_player,
                        &candidate,
                        &sdp_mid,
                        sdp_mline_index,
                    );
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
pub fn audio_feed_voice(
    _manager: Res<VoiceManager>,
    _commands: Commands,
    _audio_sources: ResMut<Assets<AudioSource>>,
    remote_ships: Query<(Entity, &RemoteShip, &GlobalTransform)>,
    _settings: Res<Settings>,
) {
    // Update PannerNode positions from RemoteShip transforms (spatial audio).
    for (_, ship, tx) in &remote_ships {
        let p = tx.translation();
        voice_wasm::update_spatial_position(&ship.player_id, p.x as f64, p.y as f64, p.z as f64);
    }
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
    let key = settings.key(InputAction::VoicePushToTalk);
    let held = input.pressed(key);
    if let Some(tx) = &manager.cmd_tx {
        let _ = tx.send(voice_native::VoiceCommand::SetMicActive(held));
    }
}

#[cfg(target_arch = "wasm32")]
pub fn ptt_system(
    input: Res<ButtonInput<KeyCode>>,
    _manager: Res<VoiceManager>,
    settings: Res<Settings>,
) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let key = settings.key(InputAction::VoicePushToTalk);
    if input.just_pressed(key) && !STARTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        voice_wasm::start_mic();
        log::info!("wasm-voice: mic started on first PTT press");
    }
}

// ---------------------------------------------------------------------------
// Mic device cycle
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub fn mic_cycle_system(
    input: Res<ButtonInput<KeyCode>>,
    mut mic_devices: ResMut<MicDevices>,
    mut settings: ResMut<Settings>,
    _manager: Res<VoiceManager>,
) {
    let key = settings.key(InputAction::MicCycleDevice);
    if !input.just_pressed(key) {
        return;
    }

    if mic_devices.devices.is_empty() {
        return;
    }

    mic_devices.current_index = (mic_devices.current_index + 1) % mic_devices.devices.len();
    let name = mic_devices.devices[mic_devices.current_index].clone();
    settings.audio.voice_input_device = Some(name);

    // Restart mic capture — handled by start_voice_thread on next entry,
    // or we can send a command to the voice thread.
}

#[cfg(target_arch = "wasm32")]
pub fn mic_cycle_system(
    _input: Res<ButtonInput<KeyCode>>,
    _mic_devices: ResMut<MicDevices>,
    _settings: ResMut<Settings>,
    _manager: Res<VoiceManager>,
) {
    // WASM: browser manages mic device selection.
}

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
pub fn start_voice_thread(
    mut commands: Commands,
    _mic_devices: Option<Res<MicDevices>>,
    settings: Res<Settings>,
) {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let (evt_tx, evt_rx) = crossbeam_channel::unbounded();
    let (mic_tx, mic_rx) = crossbeam_channel::unbounded();

    let preferred = settings.audio.voice_input_device.as_deref();
    start_mic_capture(preferred, mic_tx.clone());

    match voice_native::spawn_voice_thread(cmd_rx, evt_tx, mic_rx) {
        Ok(handle) => {
            commands.insert_resource(VoiceManager::new_native(
                Some(cmd_tx),
                evt_rx,
                handle,
                Some(mic_tx),
            ));
        }
        Err(e) => {
            log::warn!("voice: voice thread failed to start — voice disabled: {e}");
            commands.insert_resource(VoiceManager::new_native(
                None,
                evt_rx,
                voice_native_placeholder_handle(),
                None,
            ));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn voice_native_placeholder_handle() -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {})
}

#[cfg(not(target_arch = "wasm32"))]
fn start_mic_capture(preferred: Option<&str>, tx: voice_native::MicSender) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = preferred
        .and_then(|name| {
            host.input_devices()
                .ok()?
                .find(|d| d.name().ok().as_deref() == Some(name))
        })
        .or_else(|| host.default_input_device());

    let device = match device {
        Some(d) => d,
        None => {
            log::warn!("voice: no mic device found");
            return;
        }
    };

    let config = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("voice: failed to get mic config: {e}");
            return;
        }
    };

    let err_log = |e: cpal::StreamError| log::error!("voice: mic stream error: {e}");

    let stream = if config.sample_format() == cpal::SampleFormat::F32 {
        device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(data.to_vec());
            },
            err_log,
            None,
        )
    } else {
        return;
    };

    match stream {
        Ok(s) => {
            if let Err(e) = s.play() {
                log::error!("voice: mic play failed: {e}");
            }
            std::mem::forget(s);
        }
        Err(e) => log::error!("voice: mic stream build failed: {e}"),
    }
}

/// Enumerate available input devices and populate MicDevices resource.
#[cfg(not(target_arch = "wasm32"))]
pub fn enumerate_mic_devices(mut mic_devices: ResMut<MicDevices>) {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    mic_devices.devices = host
        .input_devices()
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|d| d.name().ok())
        .collect();
}

#[cfg(target_arch = "wasm32")]
pub fn enumerate_mic_devices(_mic_devices: ResMut<MicDevices>) {
    // WASM: no cpal; browser manages mic enumeration.
}
