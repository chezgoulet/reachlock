use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{RawQuery, State};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use reachlock_core::network::{ClientMessage, ServerMessage, PROTOCOL_VERSION};
use tokio::time::Instant;

use super::session::Session;
use super::AppState;
use crate::services::verify::Verdict;

pub async fn upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
) -> Response {
    ws.on_upgrade(move |socket| async move {
        let session = match Session::authenticate(
            query.as_deref().unwrap_or(""),
            &*state.sessions,
            state
                .auth_required
                .load(std::sync::atomic::Ordering::Relaxed),
        ) {
            Ok(s) => s,
            Err(reason) => {
                let mut socket = socket;
                let _ = socket
                    .send(Message::Text(
                        serde_json::to_string(&ServerMessage::Error { message: reason })
                            .expect("ServerMessage serializes")
                            .into(),
                    ))
                    .await;
                return;
            }
        };
        handle(socket, state, session).await;
    })
}

async fn handle(socket: WebSocket, state: Arc<AppState>, session: Session) {
    state.session_started();
    tracing::info!(player = %session.player_id, universe = ?session.universe, "session open");

    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMessage>(64);

    // S23: send Hello handshake immediately.
    let hello = ServerMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
    };
    let _ = sink
        .send(Message::Text(
            serde_json::to_string(&hello)
                .expect("ServerMessage serializes")
                .into(),
        ))
        .await;

    // Writer task: single owner of the sink.
    let mut writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let text = serde_json::to_string(&msg).expect("ServerMessage serializes");
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Broadcast forwarder: universe events → this socket (existing flow).
    let mut events = state.events.subscribe();
    let forward_tx = out_tx.clone();
    let _forwarder = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if forward_tx.send(event).await.is_err() {
                break;
            }
        }
    });

    // S23 presence: track current system for interest scoping.
    let mut current_system: Option<reachlock_core::seed::types::SystemId> = None;

    // S57: heartbeat tracking.
    let mut last_heartbeat = Instant::now();
    let heartbeat_timeout = Duration::from_secs(30);

    // S54: register this session's sender for targeted delivery (voice signaling).
    state
        .player_senders
        .write()
        .await
        .insert(session.player_id.clone(), out_tx.clone());

    // Read loop.
    loop {
        let remaining = heartbeat_timeout.saturating_sub(last_heartbeat.elapsed());
        let sleep = tokio::time::sleep(remaining);
        tokio::pin!(sleep);
        tokio::select! {
            msg = stream.next() => {
                let Some(Ok(Message::Text(text))) = msg else {
                    break;
                };
                let cm: ClientMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = out_tx.send(ServerMessage::Error {
                            message: format!("unparseable message: {e}"),
                        }).await;
                        continue;
                    }
                };
                // S57: handle ping before routing.
                if matches!(cm, ClientMessage::Ping) {
                    last_heartbeat = Instant::now();
                    let _ = out_tx.send(ServerMessage::Pong).await;
                    continue;
                }
                let reply = route(
                    &state, &session, cm, &out_tx, &mut current_system,
                ).await;
                if let Some(msg) = reply {
                    let _ = out_tx.send(msg).await;
                }
            }
            _ = &mut writer => break,
            _ = &mut sleep => {
                let _ = out_tx.send(ServerMessage::PlayerDisconnected {
                    player_id: session.player_id.clone(),
                    reason: "timeout".into(),
                }).await;
                break;
            }
        }
    }

    // S57: emit player.disconnected on session close.
    let _ = state.events.send(ServerMessage::PlayerDisconnected {
        player_id: session.player_id.clone(),
        reason: "quit".into(),
    });

    // S54: unregister sender for targeted delivery.
    state
        .player_senders
        .write()
        .await
        .remove(&session.player_id);

    // S23/S29: clean up presence and voice on disconnect.
    if let Some(sys_id) = &current_system {
        state.voice.leave(sys_id, &session.player_id);
        state
            .presence
            .leave(session.universe, sys_id, &out_tx)
            .await;
        state
            .presence
            .broadcast(
                session.universe,
                sys_id,
                &ServerMessage::PlayerLeft {
                    player_id: session.player_id.clone(),
                    system_id: sys_id.clone(),
                },
            )
            .await;
    }

    state.session_ended();
    tracing::info!(player = %session.player_id, "session closed");
}

/// Route a single client message. Returns an optional direct reply.
#[allow(clippy::too_many_arguments)]
async fn route(
    state: &Arc<AppState>,
    session: &Session,
    msg: ClientMessage,
    out_tx: &tokio::sync::mpsc::Sender<ServerMessage>,
    current_system: &mut Option<reachlock_core::seed::types::SystemId>,
) -> Option<ServerMessage> {
    match msg {
        ClientMessage::SeedDiscover {
            universe,
            system_id,
            seed,
        } => {
            // S23/S29: update presence and voice scoping.
            if let Some(old_id) = current_system.take() {
                state.voice.leave(&old_id, &session.player_id);
                state.presence.leave(universe, &old_id, out_tx).await;
                state
                    .presence
                    .broadcast(
                        universe,
                        &old_id,
                        &ServerMessage::PlayerLeft {
                            player_id: session.player_id.clone(),
                            system_id: old_id.clone(),
                        },
                    )
                    .await;
                // S57: broadcast PlayerJumped to old system and new system.
                state
                    .presence
                    .broadcast(
                        universe,
                        &old_id,
                        &ServerMessage::PlayerJumped {
                            player_id: session.player_id.clone(),
                            from_system: old_id.clone(),
                            to_system: system_id.clone(),
                        },
                    )
                    .await;
                state
                    .presence
                    .broadcast(
                        universe,
                        &system_id,
                        &ServerMessage::PlayerJumped {
                            player_id: session.player_id.clone(),
                            from_system: old_id,
                            to_system: system_id.clone(),
                        },
                    )
                    .await;
            }
            state.voice.join(&system_id, &session.player_id);
            state
                .presence
                .join(universe, system_id.clone(), out_tx.clone())
                .await;
            *current_system = Some(system_id.clone());
            state
                .presence
                .broadcast(
                    universe,
                    &system_id,
                    &ServerMessage::PlayerJoined {
                        player_id: session.player_id.clone(),
                        system_id: system_id.clone(),
                        universe,
                        name: None,
                        species: None,
                        look: None,
                    },
                )
                .await;

            let result = state
                .seeds
                .discover(universe, &system_id, seed, Some(&session.player_id));
            Some(ServerMessage::SeedCanonical {
                system_id,
                seed: result.canonical_seed,
                diffs: result.diffs,
                you_discovered: result.you_discovered,
                discoverer_name: result.discoverer_name,
                discovered_at: result.discovered_at,
            })
        }
        ClientMessage::SeedModify {
            universe,
            system_id,
            diffs,
        } => {
            state.seeds.modify(universe, &system_id, diffs);
            None
        }
        ClientMessage::PlayerPosition {
            system_id,
            position,
        } => {
            // S23: scoped presence — only players in the same system.
            state
                .presence
                .broadcast(
                    session.universe,
                    &system_id,
                    &ServerMessage::UniverseEvent {
                        event: serde_json::json!({
                            "kind": "player_position",
                            "player": session.player_id,
                            "system": system_id.0,
                            "position": position,
                        }),
                    },
                )
                .await;
            None
        }
        ClientMessage::ChatSend { text } => {
            // S23: system-scope chat. Rate limit and length check.
            let Some(sys_id) = current_system.as_ref() else {
                return Some(ServerMessage::Error {
                    message: "not in a system".into(),
                });
            };
            if text.len() > 256 {
                return Some(ServerMessage::Error {
                    message: "chat message too long (max 256 bytes)".into(),
                });
            }
            state
                .presence
                .broadcast(
                    session.universe,
                    sys_id,
                    &ServerMessage::ChatMessage {
                        from_player: session.player_id.clone(),
                        text,
                    },
                )
                .await;
            None
        }
        ClientMessage::VoiceSignal {
            target_player,
            signal,
        } => {
            // S29: relay voice signaling to the target peer.
            let Some(sys_id) = current_system.as_ref() else {
                return Some(ServerMessage::Error {
                    message: "not in a system".into(),
                });
            };
            // Update voice room state and relay.
            state.voice.join(sys_id, &session.player_id);
            if let Some((target, sig)) =
                state
                    .voice
                    .relay(sys_id, &session.player_id, &target_player, &signal)
            {
                // S54: send directly to the target via their registered sender.
                let senders = state.player_senders.read().await;
                if let Some(target_tx) = senders.get(&target) {
                    let _ = target_tx
                        .send(ServerMessage::VoiceSignal {
                            from_player: session.player_id.clone(),
                            signal: sig,
                        })
                        .await;
                }
            }
            None
        }
        ClientMessage::EvalSubmit { eval } => {
            let eval_id = eval.signature.chars().take(16).collect::<String>();
            Some(
                match state
                    .verify
                    .submit(&session.player_id, session.universe, &eval)
                {
                    Verdict::Accepted => ServerMessage::EvalVerified {
                        eval_id,
                        accepted: true,
                    },
                    Verdict::Rejected(reason) => ServerMessage::EvalRejected { eval_id, reason },
                },
            )
        }
        ClientMessage::LlmCall {
            call_id,
            contract_id,
            context,
            system_prompt,
            timeout_ms,
            max_tokens,
        } => {
            let _ = out_tx
                .send(ServerMessage::LlmDeliberating {
                    call_id: call_id.clone(),
                })
                .await;
            let out_tx = out_tx.clone();
            let state = Arc::clone(state);
            let player_id = session.player_id.clone();
            let universe = session.universe;
            tokio::spawn(async move {
                use crate::services::llm_proxy::CallOverrides;
                let reply = match state
                    .llm
                    .route(
                        universe,
                        &player_id,
                        &contract_id,
                        &context,
                        CallOverrides {
                            system_prompt,
                            timeout_ms,
                            max_tokens,
                        },
                    )
                    .await
                {
                    Ok(response) => ServerMessage::LlmResponse {
                        call_id,
                        action: response.action,
                        reasoning: response.reasoning,
                    },
                    Err(e) => ServerMessage::LlmFailed {
                        call_id,
                        reason: e.reason().into(),
                    },
                };
                let _ = out_tx.send(reply).await;
            });
            None
        }
        ClientMessage::ContractSync { contracts } => {
            let state = Arc::clone(state);
            let player = session.player_id.clone();
            tokio::spawn(async move {
                state.contracts.sync(&player, &contracts);
            });
            None
        }
        ClientMessage::RequestTurnConfig => {
            if let Some((url, username, password, ttl_secs)) =
                crate::services::voice::VoiceRegistry::generate_turn_credentials(&session.player_id)
            {
                Some(ServerMessage::TurnConfig {
                    url,
                    username,
                    password,
                    ttl_secs,
                })
            } else {
                // No TURN configured — it's OK, peers still work via STUN.
                None
            }
        }
        ClientMessage::LibraryList { role_filter, sort } => {
            let state = Arc::clone(state);
            let rf = role_filter.map(|r| format!("{r:?}"));
            let sf = sort.clone();
            let entries = tokio::task::spawn_blocking(move || {
                state.library.list(rf.as_deref(), sf.as_deref())
            })
            .await
            .unwrap_or_default();
            Some(ServerMessage::LibraryListResponse { entries })
        }
        ClientMessage::LibraryPublish {
            metadata,
            contract_ron,
        } => {
            let state = Arc::clone(state);
            let entry = reachlock_core::contract::metadata::ContractLibraryEntry {
                metadata,
                contract_ron,
            };
            let player = session.player_id.clone();
            let result = tokio::task::spawn_blocking(move || {
                state.library.publish_rate_limited(&player, entry)
            })
            .await
            .unwrap_or(Err(
                crate::services::library::PublishError::DailyLimitExceeded,
            ));
            match result {
                Ok(share_code) => Some(ServerMessage::LibraryPublishResponse {
                    ok: true,
                    share_code: Some(share_code),
                    error: None,
                }),
                Err(e) => Some(ServerMessage::LibraryPublishResponse {
                    ok: false,
                    share_code: None,
                    error: Some(format!("{e:?}")),
                }),
            }
        }
        ClientMessage::LibrarySubmitStory {
            story,
            contract_id,
            event_type,
            outcome_type,
        } => {
            let state = Arc::clone(state);
            let story_obj = reachlock_core::contract::metadata::ContractStory {
                contract_id,
                story,
                event_type,
                outcome_type,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            };
            let story_id =
                tokio::task::spawn_blocking(move || state.library.submit_story(story_obj))
                    .await
                    .unwrap_or(0);
            Some(ServerMessage::LibraryStoryAck {
                success: story_id > 0,
                story_id,
            })
        }
        ClientMessage::LibrarySync {
            role_filter,
            sort,
            search,
            page,
            page_size,
        } => {
            let state = Arc::clone(state);
            let rf = role_filter.clone();
            let sf = sort.clone();
            let q = search.clone();
            let entries = tokio::task::spawn_blocking(move || {
                let mut entries = if let Some(ref query) = q {
                    state.library.search(query)
                } else {
                    state.library.list(rf.as_deref(), sf.as_deref())
                };
                // Apply pagination.
                let page_size = page_size.clamp(1, 100).max(1) as usize;
                let total = entries.len() as u32;
                let offset = (page as usize).saturating_mul(page_size);
                if offset < entries.len() {
                    entries.drain(..offset.min(entries.len()));
                }
                entries.truncate(page_size);
                (entries, total)
            })
            .await
            .unwrap_or_default();
            Some(ServerMessage::LibrarySyncResponse {
                entries: entries.0,
                total: entries.1,
                page,
            })
        }
        ClientMessage::LibraryShareLookup { share_code } => {
            let state = Arc::clone(state);
            let code = share_code.clone();
            let entry = tokio::task::spawn_blocking(move || state.library.lookup_share_code(&code))
                .await
                .unwrap_or(None);
            Some(ServerMessage::LibraryShareResponse { entry })
        }
        // S57: handled before route(), unreachable here.
        ClientMessage::Ping => None,
    }
}
