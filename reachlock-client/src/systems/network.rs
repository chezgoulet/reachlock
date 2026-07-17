//! Online-mode network systems (S02): connect on entering Playing, poll the
//! transport every frame (never block — iron rule #5), and route protocol
//! messages into the contract/HUD layers. Every system here early-outs on
//! `NetMode::Offline` (iron rule #3) — offline play is completely untouched.
//!
//! // S02 TODO(integrator): `systems::setup::spawn_world` hardcodes
//! // `SYSTEM_SEED` and has no `SystemId`/multi-system registry yet. This
//! // module stands in `spike_system_id()` below for the one system the
//! // spike renders, and — on a canonical seed that differs from ours —
//! // only *logs* the "Synchronizing system data…" beat and the adoption;
//! // it does not yet despawn/regenerate the scene. When setup.rs grows a
//! // real per-system seed, wire `SeedState::adopted` (set in
//! // `poll_network` below) to a "regenerate this system from this seed"
//! // entry point there, gated on `seed != SYSTEM_SEED`.

use std::time::Duration;

use bevy::prelude::*;
use reachlock_core::network::{ClientMessage, ServerMessage};
use reachlock_core::seed::types::{Seed, SystemId};

use crate::net::{handshake_url, ConnectionState, NetMode, NetOutbox, TransportEvent, WsTransport};
use crate::systems::contract::{self, ContractRuntime, DeliberationState, ShipLog};
use crate::systems::setup::SYSTEM_SEED;
use crate::systems::ship::ShipSystems;
use crate::systems::ticker::UniverseTicker;

/// Owns the live socket, if any. `None` whenever offline, still connecting
/// via backoff, or between "dropped" and "reconnected".
///
/// Not a `Resource`: `ewebsock::WsReceiver` wraps a `std::sync::mpsc::Receiver`,
/// which is `Send` but not `Sync`, so it can't satisfy Bevy's `Resource`
/// bound. It's registered as a `NonSend` resource instead (see `main.rs`)
/// and read via `NonSend`/`NonSendMut` — fine for a single client socket
/// that only ever needs main-thread access.
#[derive(Default)]
pub struct NetworkClient {
    transport: Option<WsTransport>,
}

/// Exponential-ish reconnect backoff (1s, 2s, 4s, 8s, 16s, capped at 30s).
#[derive(Resource)]
pub struct ReconnectBackoff {
    timer: Timer,
    attempt: u32,
}

impl Default for ReconnectBackoff {
    fn default() -> Self {
        ReconnectBackoff {
            timer: Timer::new(Duration::from_secs(1), TimerMode::Once),
            attempt: 0,
        }
    }
}

impl ReconnectBackoff {
    const MAX_SECS: u64 = 30;

    fn schedule_next(&mut self) {
        self.attempt = self.attempt.saturating_add(1);
        let secs = (1u64 << self.attempt.min(5)).min(Self::MAX_SECS);
        self.timer = Timer::new(Duration::from_secs(secs), TimerMode::Once);
    }

    fn reset(&mut self) {
        self.attempt = 0;
        self.timer = Timer::new(Duration::from_secs(1), TimerMode::Once);
    }
}

/// Discovery/adoption state for the system currently rendered (spec §4).
#[derive(Resource, Default)]
pub struct SeedState {
    pub current_system: Option<SystemId>,
    pub adopted: Option<Seed>,
}

/// Placeholder system id for the spike's single hardcoded scene — see the
/// `S02 TODO(integrator)` note at the top of this file.
fn spike_system_id() -> SystemId {
    SystemId(format!("spike-{SYSTEM_SEED:x}"))
}

/// OnEnter(Playing), online mode only: opens the socket. The handshake
/// itself completes asynchronously (background thread on native, the
/// browser's event loop on wasm) — `poll_network` picks up `Opened` once it
/// lands.
pub fn connect_on_enter_playing(
    mode: Res<NetMode>,
    mut client: NonSendMut<NetworkClient>,
    mut conn: ResMut<ConnectionState>,
    mut log: ResMut<ShipLog>,
) {
    let NetMode::Online {
        url,
        player,
        universe,
    } = &*mode
    else {
        return; // offline: never opens a socket
    };
    let target = handshake_url(url, player, *universe);
    match WsTransport::connect(&target) {
        Ok(t) => {
            client.transport = Some(t);
            *conn = ConnectionState::Connecting;
            log.log(format!("Connecting to {url}…"));
        }
        Err(e) => {
            *conn = ConnectionState::Errored;
            log.log(format!("Could not reach {url}: {e}. Flying offline."));
        }
    }
}

/// Update, online mode only: drains every buffered transport event (never
/// blocks) and routes it. Also flushes `NetOutbox` once actually connected.
#[allow(clippy::too_many_arguments)]
pub fn poll_network(
    mode: Res<NetMode>,
    mut client: NonSendMut<NetworkClient>,
    mut conn: ResMut<ConnectionState>,
    mut backoff: ResMut<ReconnectBackoff>,
    mut seed_state: ResMut<SeedState>,
    mut outbox: ResMut<NetOutbox>,
    mut log: ResMut<ShipLog>,
    mut deliberation: ResMut<DeliberationState>,
    mut ship: ResMut<ShipSystems>,
    mut runtime: ResMut<ContractRuntime>,
    mut ticker: Option<ResMut<UniverseTicker>>,
    mut souls: ResMut<crate::systems::soul::SoulRegistry>,
    mut dialogue: ResMut<crate::systems::dialogue::DialogueSession>,
    mut feed: ResMut<crate::systems::comms::CommFeed>,
) {
    let NetMode::Online { universe, .. } = &*mode else {
        return;
    };
    // Own the transport locally for this call so we never hold a `&mut`
    // into `client.transport` while also wanting to clear it on
    // disconnect — see the borrow note below.
    let Some(mut transport) = client.transport.take() else {
        return; // nothing to poll — `reconnect_backoff` owns re-opening it
    };

    let mut lost_connection = false;

    for event in transport.poll() {
        match event {
            TransportEvent::Opened => {
                *conn = ConnectionState::Connected;
                backoff.reset();
                log.log("Link established.");
                let system_id = spike_system_id();
                seed_state.current_system = Some(system_id.clone());
                transport.send(&ClientMessage::SeedDiscover {
                    universe: *universe,
                    system_id,
                    seed: Seed::new(SYSTEM_SEED),
                });
                // Pause the local universe ticker — server is authoritative.
                if let Some(ref mut ticker) = ticker {
                    ticker.online_mode = true;
                }
            }
            TransportEvent::Message(ServerMessage::SeedCanonical {
                system_id,
                seed,
                you_discovered,
                ..
            }) => {
                if seed_state.adopted != Some(seed) {
                    log.log("Synchronizing system data…");
                    if you_discovered {
                        log.log(format!(
                            "{} — canonical seed adopted (ours, {:#x}).",
                            system_id.0,
                            seed.value()
                        ));
                    } else {
                        log.log(format!(
                            "{} — canonical seed adopted ({:#x}); diverges from local.",
                            system_id.0,
                            seed.value()
                        ));
                    }
                    seed_state.adopted = Some(seed);
                }
            }
            TransportEvent::Message(ServerMessage::EvalVerified { .. }) => {
                // Routine confirmations are silent; only rejections are
                // worth a ship's-log line.
            }
            TransportEvent::Message(ServerMessage::EvalRejected { eval_id, reason }) => {
                log.log(format!("Eval {eval_id} rejected: {reason}"));
            }
            TransportEvent::Message(ServerMessage::LlmDeliberating { call_id }) => {
                if let Some(active) = deliberation.active.as_mut() {
                    if active.call_id.as_deref() == Some(call_id.as_str()) {
                        active.overlay_visible = true;
                    }
                }
            }
            TransportEvent::Message(ServerMessage::LlmResponse {
                call_id,
                action,
                reasoning,
            }) => {
                // S16: dialogue calls resolve into the open conversation
                // (shaped in the soul's voice); superseded calls are
                // ignored quietly.
                if let Some(t) = ticker.as_deref() {
                    if crate::systems::dialogue::resolve_dialogue_response(
                        &mut dialogue,
                        &mut souls,
                        t,
                        &call_id,
                        &reasoning,
                    ) {
                        continue;
                    }
                }
                let matches_active = deliberation
                    .active
                    .as_ref()
                    .is_some_and(|d| d.call_id.as_deref() == Some(call_id.as_str()));
                if matches_active {
                    // S15: the deliberating crew member's trust shifts the
                    // outcome odds (S13 bridge); unknown souls read as 0.
                    let trust = deliberation
                        .active
                        .as_ref()
                        .and_then(|d| souls.states.get(&d.crew_member.to_lowercase()))
                        .and_then(|s| s.relationship("player").map(|r| r.trust))
                        .unwrap_or(0);
                    contract::resolve_response(
                        &mut deliberation,
                        &mut ship,
                        &mut log,
                        &mut runtime,
                        &mut outbox,
                        &mut feed,
                        &action,
                        &reasoning,
                        *universe,
                        trust,
                    );
                }
            }
            TransportEvent::Message(ServerMessage::LlmFailed { call_id, reason }) => {
                if let Some(t) = ticker.as_deref() {
                    if crate::systems::dialogue::resolve_dialogue_failure(
                        &mut dialogue,
                        &souls,
                        t,
                        &call_id,
                        &mut log,
                        &reason,
                    ) {
                        continue;
                    }
                }
                let matches_active = deliberation
                    .active
                    .as_ref()
                    .is_some_and(|d| d.call_id.as_deref() == Some(call_id.as_str()));
                if matches_active {
                    // S15: failure categories read distinctly in the log
                    // (a timeout and a collapse are different stories).
                    contract::resolve_failed(
                        &mut deliberation,
                        &mut ship,
                        &mut log,
                        &runtime,
                        &mut feed,
                        &reason,
                    );
                }
            }
            TransportEvent::Message(ServerMessage::PlayerEntered { .. }) => {
                // S23 (presence/chat) territory — nothing to show yet.
            }
            TransportEvent::Message(ServerMessage::UniverseEvent { event }) => {
                // Online mode: the server is the tick authority. An
                // `EconomyTick` marks one authoritative tick — replaying it
                // locally with the shared canonical seed reproduces the
                // server's step exactly (prices, standings, news; see the
                // `parity_offline_vs_server` test). The other event kinds
                // are regenerated by that replay, so appending them here too
                // would double-log them — they're ignored.
                if let Ok(sim) = serde_json::from_value::<reachlock_core::sim::SimEvent>(event) {
                    if let (reachlock_core::sim::SimEvent::EconomyTick { .. }, Some(ticker)) =
                        (&sim, ticker.as_mut())
                    {
                        ticker.replay_server_tick();
                    }
                }
            }
            TransportEvent::Message(ServerMessage::Error { message }) => {
                log.log(format!("Server error: {message}"));
            }
            TransportEvent::Unparseable(reason) => {
                warn!("{reason}");
            }
            TransportEvent::Error(e) => {
                *conn = ConnectionState::Errored;
                backoff.schedule_next();
                log.log(format!("Connection error: {e}. Flying offline; retrying…"));
                lost_connection = true;
                if let Some(ref mut ticker) = ticker {
                    ticker.online_mode = false;
                }
            }
            TransportEvent::Closed => {
                *conn = ConnectionState::Errored;
                backoff.schedule_next();
                log.log("Connection closed. Flying offline; retrying…");
                lost_connection = true;
                if let Some(ref mut ticker) = ticker {
                    ticker.online_mode = false;
                }
            }
        }
    }

    if lost_connection {
        return; // transport dropped; client.transport stays None
    }

    if matches!(*conn, ConnectionState::Connected) {
        for msg in outbox.drain() {
            transport.send(&msg);
        }
    }

    client.transport = Some(transport);
}

/// Update, online mode only: when the socket is down, waits out the backoff
/// timer and tries again. The game keeps playing locally the whole time
/// (iron rule #3) — this system only ever touches connection bookkeeping.
pub fn reconnect_backoff(
    time: Res<Time>,
    mode: Res<NetMode>,
    mut client: NonSendMut<NetworkClient>,
    mut conn: ResMut<ConnectionState>,
    mut backoff: ResMut<ReconnectBackoff>,
    mut log: ResMut<ShipLog>,
) {
    let NetMode::Online {
        url,
        player,
        universe,
    } = &*mode
    else {
        return;
    };
    if client.transport.is_some() || !matches!(*conn, ConnectionState::Errored) {
        return;
    }
    if !backoff.timer.tick(time.delta()).is_finished() {
        return;
    }

    let target = handshake_url(url, player, *universe);
    let attempt = backoff.attempt + 1;
    match WsTransport::connect(&target) {
        Ok(t) => {
            client.transport = Some(t);
            *conn = ConnectionState::Connecting;
            log.log(format!("Reconnect attempt {attempt}…"));
        }
        Err(e) => {
            log.log(format!("Reconnect attempt {attempt} failed: {e}"));
            backoff.schedule_next();
        }
    }
}
