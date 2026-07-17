//! WebSocket connection handling (spec §8). One Tokio task per connection;
//! all outbound traffic funnels through an mpsc channel so slow LLM calls
//! and universe broadcasts never block the read loop.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{RawQuery, State};
use axum::response::Response;
use futures::{SinkExt, StreamExt};
use reachlock_core::network::{ClientMessage, ServerMessage};

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
            state.auth_required,
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

    // Writer task: single owner of the sink.
    let writer = tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let text = serde_json::to_string(&msg).expect("ServerMessage serializes");
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    // Broadcast forwarder: universe events → this socket.
    let mut events = state.events.subscribe();
    let forward_tx = out_tx.clone();
    let forwarder = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if forward_tx.send(event).await.is_err() {
                break;
            }
        }
    });

    // Presence: tell the universe someone arrived.
    let _ = state.events.send(ServerMessage::PlayerEntered {
        player_id: session.player_id.clone(),
        system_id: reachlock_core::seed::types::SystemId(String::new()),
        universe: session.universe,
    });

    // Read loop.
    while let Some(Ok(message)) = stream.next().await {
        let Message::Text(text) = message else {
            continue; // binary/ping/pong: nothing in the protocol yet
        };
        let reply = match serde_json::from_str::<ClientMessage>(&text) {
            Ok(msg) => route(&state, &session, msg, &out_tx).await,
            Err(e) => Some(ServerMessage::Error {
                message: format!("unparseable message: {e}"),
            }),
        };
        if let Some(reply) = reply {
            if out_tx.send(reply).await.is_err() {
                break;
            }
        }
    }

    forwarder.abort();
    drop(out_tx);
    let _ = writer.await;
    state.session_ended();
    tracing::info!(player = %session.player_id, "session closed");
}

/// Route one client message. Returns the direct reply, if any; side-channel
/// sends (deliberation notices) go through `out_tx`.
async fn route(
    state: &Arc<AppState>,
    session: &Session,
    msg: ClientMessage,
    out_tx: &tokio::sync::mpsc::Sender<ServerMessage>,
) -> Option<ServerMessage> {
    match msg {
        ClientMessage::SeedDiscover {
            universe,
            system_id,
            seed,
        } => {
            if universe != session.universe {
                return Some(ServerMessage::Error {
                    message: "universe mismatch: message vs session".into(),
                });
            }
            let d = state.seeds.discover(universe, &system_id, seed);
            Some(ServerMessage::SeedCanonical {
                system_id,
                seed: d.canonical_seed,
                diffs: d.diffs,
                you_discovered: d.you_discovered,
            })
        }
        ClientMessage::SeedModify {
            universe,
            system_id,
            diffs,
        } => {
            if universe != session.universe {
                return Some(ServerMessage::Error {
                    message: "universe mismatch: message vs session".into(),
                });
            }
            if state.seeds.modify(universe, &system_id, diffs) {
                None // success is silent; the canonical state flows on entry
            } else {
                Some(ServerMessage::Error {
                    message: format!("cannot modify undiscovered system {}", system_id.0),
                })
            }
        }
        ClientMessage::EvalSubmit { eval } => {
            let eval_id = eval.signature.chars().take(16).collect::<String>();
            match state
                .verify
                .submit(&session.player_id, session.universe, &eval)
            {
                Verdict::Accepted => Some(ServerMessage::EvalVerified {
                    eval_id,
                    accepted: true,
                }),
                Verdict::Rejected(reason) => Some(ServerMessage::EvalRejected { eval_id, reason }),
            }
        }
        ClientMessage::LlmCall {
            call_id,
            contract_id,
            context,
            system_prompt,
            timeout_ms,
            max_tokens,
        } => {
            // Immediately: "the crew is thinking" (spec §6 deliberation UX).
            let _ = out_tx
                .send(ServerMessage::LlmDeliberating {
                    call_id: call_id.clone(),
                })
                .await;
            // S14 gotcha: a real provider call can take seconds; awaiting it
            // inline would block this player's whole message loop. Spawn it
            // and reply through the session's out_tx.
            let state = Arc::clone(state);
            let out_tx = out_tx.clone();
            let universe = session.universe;
            let player_id = session.player_id.clone();
            tokio::spawn(async move {
                let overrides = crate::services::llm_proxy::CallOverrides {
                    system_prompt,
                    timeout_ms,
                    max_tokens,
                };
                let reply = match state
                    .llm
                    .route(universe, &player_id, &contract_id, &context, overrides)
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
        ClientMessage::PlayerPosition {
            system_id,
            position,
        } => {
            let _ = state.events.send(ServerMessage::UniverseEvent {
                event: serde_json::json!({
                    "kind": "player_position",
                    "player": session.player_id,
                    "system": system_id.0,
                    "position": position,
                }),
            });
            None
        }
        ClientMessage::ContractSync { contracts } => {
            // S16B: the sync persists through the store seam (memory by
            // default; the pg store documents its blocking contract, so it
            // runs off the async worker).
            let state = Arc::clone(state);
            let player_id = session.player_id.clone();
            let count = contracts.len();
            tokio::task::spawn_blocking(move || {
                state.contracts.sync(&player_id, &contracts);
            });
            tracing::debug!(player = %session.player_id, count, "contract sync persisted");
            None
        }
    }
}
