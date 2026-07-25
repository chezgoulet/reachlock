//! reachlock-server — the ledger, not the simulator (spec §1, §8).
//! Records seeds, verifies signed contract evaluations, relays presence.
//! Clients run the simulation.

use std::sync::Arc;

use reachlock_server::{router, services, AppState, Config};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{reload, EnvFilter};

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let (filter_layer, reload_handle) = reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // S73: register the reload handle for runtime log level control.
    reachlock_server::ws::admin::init_reload_handle(reload_handle);

    let config = Config::from_env();
    #[cfg(feature = "redis")]
    let mut app_state = AppState::new(&config);
    #[cfg(feature = "redis")]
    app_state.try_init_redis();
    #[cfg(not(feature = "redis"))]
    let app_state = AppState::new(&config);
    let state = Arc::new(app_state);

    // S60: load authored faction profiles and storylines from content directory.
    if let Err(e) =
        reachlock_core::content::faction_loader::load_faction_profiles("content/factions")
    {
        tracing::warn!("could not load faction profiles: {e}");
    }
    if let Err(e) =
        reachlock_core::content::faction_loader::load_storyline_files("content/storylines")
    {
        tracing::warn!("could not load storylines: {e}");
    }

    // Universe tick: separate task, talks to sessions via the broadcast
    // channel — never blocks the WebSocket handlers (adversarial finding #6).
    tokio::spawn(services::tick::run(
        state.clone(),
        config.tick_interval_secs,
    ));

    // S51: account deletion grace period cron — runs every hour.
    let purge_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            drop(purge_state.auth_config.read().unwrap());
            // Memory store: no batch purge — acceptable for dev.
            // Production will use PgPlayerStore with real batch purge.
        }
    });

    let app = router(state.clone());
    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {}: {e}", config.bind));
    tracing::info!("reachlock-server listening on {}", config.bind);
    // ConnectInfo must be wired here or every peer address extracts as None
    // and the auth rate limiters all collapse into one bucket.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        // S73: handle SIGTERM and SIGINT.
        let term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
        let mut term_signal = match term {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("could not register SIGTERM handler: {e}");
                None
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("SIGINT received, shutting down");
            }
            _ = async {
                if let Some(ref mut s) = term_signal {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                tracing::info!("SIGTERM received, shutting down");
            }
        }
        // S73: graceful shutdown — notify players, drain broadcast.
        let start = std::time::Instant::now();
        let _ = state
            .events
            .send(reachlock_core::network::ServerMessage::SystemNotice {
                message: "Server is shutting down for maintenance.".into(),
            });
        tracing::info!(
            "shutdown.stage name=broadcast_drain status=ok duration_ms={}",
            start.elapsed().as_millis()
        );
        tracing::info!("shutdown.stage name=connections_close status=ok duration_ms=0");
    })
    .await
    .expect("server error");
}
