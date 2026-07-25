//! S02 integration test: native, headless, no Bevy. Boots the real
//! `reachlock-server` crate on an ephemeral port and drives the client's
//! own `net::transport::WsTransport` against it — the same transport the
//! Bevy systems in `systems/network.rs` poll every frame — covering the
//! spec §4 discovery race (adopt the canonical seed) and a signed
//! eval.submit round trip (spec §6).
//!
//! `net` isn't reachable as `reachlock_client::net` (the crate has no `lib`
//! target — it's a Bevy binary), so this test pulls the module in directly
//! via `#[path]`, exactly as `main.rs` does with `mod net;`. No Bevy types
//! are involved: `net::mode`/`net::transport`/`net::outbox` only depend on
//! `bevy::prelude::Resource` for the derive, not on running an `App`.

// This test binary is a separate compilation unit from the `reachlock-client`
// bin crate, so it only exercises a slice of `net`'s public surface — the
// rest reads as dead code / unused imports *here* even though the bin crate
// uses all of it (see systems/network.rs, hud.rs). Silencing that at the
// `mod` boundary keeps the warning scoped to this test-only duplicate
// compilation instead of touching the real source.
#[allow(dead_code, unused_imports)]
#[path = "../src/net/mod.rs"]
mod net;

use std::time::{Duration, Instant};

use reachlock_core::contract::signature::SignatureChain;
use reachlock_core::contract::types::Action;
use reachlock_core::network::{ClientMessage, ServerMessage};
use reachlock_core::seed::types::{Seed, SystemId};
use reachlock_core::universe::UniverseTier;

use net::{TransportEvent, WsTransport};

/// Boots the real server in-process on an ephemeral port, exactly as
/// `reachlock-server/tests/ws_roundtrip.rs` does for the server-side tests.
async fn boot() -> String {
    let config = reachlock_server::Config {
        bind: "127.0.0.1:0".into(),
        tick_interval_secs: 3600, // effectively silent during the test
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

/// Polls `transport` (never blocking the socket itself — same contract as
/// the Bevy systems) until an event satisfies `pred`, or panics after
/// `timeout`. Sleeping between polls is test-harness plumbing, not part of
/// the transport's own API.
fn poll_until(
    transport: &mut WsTransport,
    timeout: Duration,
    mut pred: impl FnMut(&TransportEvent) -> bool,
) -> TransportEvent {
    let start = Instant::now();
    loop {
        for event in transport.poll() {
            if pred(&event) {
                return event;
            }
        }
        if start.elapsed() > timeout {
            panic!("timed out waiting for a matching transport event");
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn connect(base: &str, player: &str, universe: UniverseTier) -> WsTransport {
    let url = net::handshake_url(base, player, universe);
    WsTransport::connect(&url).expect("connect failed")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn discovery_race_second_client_adopts_canonical_seed() {
    let base = boot().await;
    let mut alice = connect(&base, "alice", UniverseTier::FairPlay);
    let mut bob = connect(&base, "bob", UniverseTier::FairPlay);

    poll_until(&mut alice, Duration::from_secs(5), |e| {
        matches!(e, TransportEvent::Opened)
    });
    poll_until(&mut bob, Duration::from_secs(5), |e| {
        matches!(e, TransportEvent::Opened)
    });

    let system = SystemId("duskway-0417".into());
    let discover = |seed: u64| ClientMessage::SeedDiscover {
        universe: UniverseTier::FairPlay,
        system_id: system.clone(),
        seed: Seed::new(seed),
    };

    alice.send(&discover(1111));
    let a = poll_until(&mut alice, Duration::from_secs(5), |e| {
        matches!(
            e,
            TransportEvent::Message(ServerMessage::SeedCanonical { .. })
        )
    });

    bob.send(&discover(2222));
    let b = poll_until(&mut bob, Duration::from_secs(5), |e| {
        matches!(
            e,
            TransportEvent::Message(ServerMessage::SeedCanonical { .. })
        )
    });

    let (
        TransportEvent::Message(ServerMessage::SeedCanonical {
            seed: seed_a,
            you_discovered: won_a,
            ..
        }),
        TransportEvent::Message(ServerMessage::SeedCanonical {
            seed: seed_b,
            you_discovered: won_b,
            ..
        }),
    ) = (a, b)
    else {
        unreachable!("poll_until only returns matching events");
    };

    assert!(won_a, "first discoverer's seed becomes canonical");
    assert!(!won_b, "second discoverer adopts, doesn't win");
    assert_eq!(
        seed_a, seed_b,
        "both clients converge on one canonical seed — this is the 'canonical seed adopted' beat"
    );
    assert_eq!(seed_a, Seed::new(1111));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn signed_eval_is_accepted() {
    let base = boot().await;
    let mut client = connect(&base, "boris", UniverseTier::Classic);
    poll_until(&mut client, Duration::from_secs(5), |e| {
        matches!(e, TransportEvent::Opened)
    });

    let mut chain = SignatureChain::default();
    let eval = chain.sign_next("cryo-pilot", 1, &Action::verb("maintain_course"));
    client.send(&ClientMessage::EvalSubmit { eval });

    let verdict = poll_until(&mut client, Duration::from_secs(5), |e| {
        matches!(
            e,
            TransportEvent::Message(ServerMessage::EvalVerified { .. })
                | TransportEvent::Message(ServerMessage::EvalRejected { .. })
        )
    });
    assert!(
        matches!(
            verdict,
            TransportEvent::Message(ServerMessage::EvalVerified { accepted: true, .. })
        ),
        "an honestly-signed first link must be accepted: {verdict:?}"
    );
}
