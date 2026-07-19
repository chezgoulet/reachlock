//! WASM content distribution integration test (spec §10, Stream 2): boots the
//! real server with the repo's `mods/` tree and verifies a connecting client
//! receives a `ContentSync` on connect, and that an explicit `content.request`
//! yields a matching sync for the requested universe.

use futures_util::{SinkExt, StreamExt};
use reachlock_core::network::{ClientMessage, ServerMessage};
use reachlock_core::universe::UniverseTier;
use tokio_tungstenite::tungstenite::Message;

type WsClient =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn boot() -> String {
    // Point the content service at the repo's mods/ tree (cargo runs the test
    // binary from the crate root, so `../mods` is the workspace mods dir).
    std::env::set_var("REACHLOCK_MODS_DIR", "../mods");
    let config = reachlock_server::Config {
        bind: "127.0.0.1:0".into(),
        tick_interval_secs: 3600,
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

/// Receive the next parseable ServerMessage matching `pred`.
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
async fn connect_pushes_content_sync_for_session_universe() {
    let base = boot().await;
    let mut client = connect(&base, "wasm_pilot", "fair_play").await;

    // First message after connect is the Hello handshake.
    let hello = recv_matching(&mut client, |m| matches!(m, ServerMessage::Hello { .. })).await;
    assert!(matches!(hello, ServerMessage::Hello { protocol_version: 4 }));

    // Immediately followed by a ContentSync carrying the repo's authored
    // content (universe "all" applies to every tier).
    let sync = recv_matching(&mut client, |m| {
        matches!(m, ServerMessage::ContentSync { .. })
    })
    .await;
    let ServerMessage::ContentSync {
        universe,
        files,
        hostile_archetypes,
        charted_systems,
        ..
    } = sync
    else {
        unreachable!()
    };
    assert_eq!(universe, UniverseTier::FairPlay);
    assert!(
        !files.is_empty(),
        "client should receive authored content files over the wire"
    );
    assert!(
        !hostile_archetypes.is_empty(),
        "combat archetypes should be distributed"
    );
    assert!(
        !charted_systems.is_empty(),
        "charted systems should be distributed"
    );
}

#[tokio::test]
async fn explicit_content_request_returns_sync_for_that_universe() {
    let base = boot().await;
    let mut client = connect(&base, "wasm_pilot2", "classic").await;

    // Drain the connect-time Hello + ContentSync.
    let _ = recv_matching(&mut client, |m| matches!(m, ServerMessage::Hello { .. })).await;
    let _ = recv_matching(&mut client, |m| {
        matches!(m, ServerMessage::ContentSync { .. })
    })
    .await;

    // Explicit request for a different universe.
    send(
        &mut client,
        &ClientMessage::RequestContent {
            universe: UniverseTier::FairPlay,
        },
    )
    .await;
    let sync = recv_matching(&mut client, |m| {
        matches!(m, ServerMessage::ContentSync { .. })
    })
    .await;
    let ServerMessage::ContentSync { universe, .. } = sync else {
        unreachable!()
    };
    assert_eq!(universe, UniverseTier::FairPlay);
}
