//! End-to-end WebSocket test: boot the real server on an ephemeral port,
//! connect two clients, and exercise the spec §4 discovery race, signed
//! evaluations, and the LLM deliberation flow.

use futures_util::{SinkExt, StreamExt};
use reachlock_core::contract::signature::SignatureChain;
use reachlock_core::contract::types::Action;
use reachlock_core::network::{ClientMessage, ServerMessage};
use reachlock_core::seed::types::{Seed, SystemId};
use reachlock_core::universe::UniverseTier;
use tokio_tungstenite::tungstenite::Message;

type WsClient =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn boot() -> String {
    let config = reachlock_server::Config {
        bind: "127.0.0.1:0".into(),
        tick_interval_secs: 3600, // effectively silent during tests
        ..Default::default()
    };
    let state = std::sync::Arc::new(reachlock_server::AppState::new(&config));
    let app = reachlock_server::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("ws://{addr}/ws")
}

async fn connect(base: &str, player: &str, universe: &str) -> WsClient {
    let (socket, _) =
        tokio_tungstenite::connect_async(format!("{base}?player={player}&universe={universe}"))
            .await
            .expect("connect failed");
    socket
}

async fn send(socket: &mut WsClient, msg: &ClientMessage) {
    socket
        .send(Message::Text(serde_json::to_string(msg).unwrap().into()))
        .await
        .unwrap();
}

/// Receive the next parseable ServerMessage matching `pred`, skipping
/// broadcast noise (presence, ticks).
async fn recv_matching(
    socket: &mut WsClient,
    pred: impl Fn(&ServerMessage) -> bool,
) -> ServerMessage {
    for _ in 0..50 {
        let raw = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .expect("timed out waiting for server message")
            .expect("socket closed")
            .expect("socket error");
        if let Message::Text(text) = raw {
            if let Ok(msg) = serde_json::from_str::<ServerMessage>(&text) {
                if pred(&msg) {
                    return msg;
                }
            }
        }
    }
    panic!("no matching message in 50 frames");
}

#[tokio::test]
async fn discovery_race_has_one_winner_and_converges() {
    let base = boot().await;
    let mut alice = connect(&base, "alice", "fair_play").await;
    let mut bob = connect(&base, "bob", "fair_play").await;

    let system = SystemId("duskway-0417".into());
    let discover = |seed: u64| ClientMessage::SeedDiscover {
        universe: UniverseTier::FairPlay,
        system_id: system.clone(),
        seed: Seed::new(seed),
    };

    send(&mut alice, &discover(1111)).await;
    let a = recv_matching(&mut alice, |m| {
        matches!(m, ServerMessage::SeedCanonical { .. })
    })
    .await;

    send(&mut bob, &discover(2222)).await;
    let b = recv_matching(&mut bob, |m| {
        matches!(m, ServerMessage::SeedCanonical { .. })
    })
    .await;

    let (
        ServerMessage::SeedCanonical {
            seed: seed_a,
            you_discovered: won_a,
            ..
        },
        ServerMessage::SeedCanonical {
            seed: seed_b,
            you_discovered: won_b,
            ..
        },
    ) = (a, b)
    else {
        unreachable!()
    };

    assert!(won_a, "first discoverer wins");
    assert!(!won_b, "second discoverer loses");
    assert_eq!(
        seed_a, seed_b,
        "both clients converge on one canonical seed"
    );
    assert_eq!(seed_a, Seed::new(1111));
}

#[tokio::test]
async fn signed_evaluations_verify_and_forgeries_bounce() {
    let base = boot().await;
    let mut client = connect(&base, "boris", "classic").await;

    let mut chain = SignatureChain::default();
    let honest = chain.sign_next("cryo-pilot", 1, &Action::verb("maintain_course"));
    send(&mut client, &ClientMessage::EvalSubmit { eval: honest }).await;
    let verdict = recv_matching(&mut client, |m| {
        matches!(
            m,
            ServerMessage::EvalVerified { .. } | ServerMessage::EvalRejected { .. }
        )
    })
    .await;
    assert!(matches!(
        verdict,
        ServerMessage::EvalVerified { accepted: true, .. }
    ));

    // Forge: sign one thing, send another.
    let mut forged = chain.sign_next("cryo-pilot", 2, &Action::verb("maintain_course"));
    forged.action = Action::verb("fire_weapons");
    send(&mut client, &ClientMessage::EvalSubmit { eval: forged }).await;
    let verdict = recv_matching(&mut client, |m| {
        matches!(
            m,
            ServerMessage::EvalVerified { .. } | ServerMessage::EvalRejected { .. }
        )
    })
    .await;
    assert!(matches!(verdict, ServerMessage::EvalRejected { .. }));
}

#[tokio::test]
async fn llm_call_deliberates_then_responds_by_tier() {
    let base = boot().await;

    // FairPlay: deliberation notice, then a response.
    let mut fair = connect(&base, "tove", "fair_play").await;
    send(
        &mut fair,
        &ClientMessage::LlmCall {
            call_id: "call-1".into(),
            contract_id: "cryo-pilot".into(),
            context: serde_json::json!({"unknown_signal": 1}),
        },
    )
    .await;
    let first = recv_matching(&mut fair, |m| {
        matches!(
            m,
            ServerMessage::LlmDeliberating { .. } | ServerMessage::LlmResponse { .. }
        )
    })
    .await;
    assert!(
        matches!(first, ServerMessage::LlmDeliberating { .. }),
        "deliberating notice must precede the response"
    );
    let response = recv_matching(&mut fair, |m| {
        matches!(m, ServerMessage::LlmResponse { .. })
    })
    .await;
    let ServerMessage::LlmResponse { action, .. } = response else {
        unreachable!()
    };
    assert_eq!(action, "maintain_course");

    // Classic: no inference tier, by design.
    let mut classic = connect(&base, "purist", "classic").await;
    send(
        &mut classic,
        &ClientMessage::LlmCall {
            call_id: "call-2".into(),
            contract_id: "cryo-pilot".into(),
            context: serde_json::json!({}),
        },
    )
    .await;
    let failed = recv_matching(&mut classic, |m| {
        matches!(m, ServerMessage::LlmFailed { .. })
    })
    .await;
    let ServerMessage::LlmFailed { reason, .. } = failed else {
        unreachable!()
    };
    assert_eq!(reason, "no_inference_tier");
}
