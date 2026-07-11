//! reachlock-server — the ledger, not the simulator (spec §1, §8).
//! Records seeds, verifies signed contract evaluations, relays presence.
//! Clients run the simulation.

use std::sync::Arc;

use reachlock_server::{router, services, AppState, Config};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = Config::from_env();
    let state = Arc::new(AppState::new(&config));

    // Universe tick: separate task, talks to sessions via the broadcast
    // channel — never blocks the WebSocket handlers (adversarial finding #6).
    tokio::spawn(services::tick::run(
        state.clone(),
        config.tick_interval_secs,
    ));

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {}: {e}", config.bind));
    tracing::info!("reachlock-server listening on {}", config.bind);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutting down");
        })
        .await
        .expect("server error");
}
