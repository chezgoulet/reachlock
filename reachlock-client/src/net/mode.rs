//! S02 freeze: online/offline mode selection and connection-state tracking,
//! written before any socket code exists. Offline is the default and every
//! network system must early-out on it (spec §9, iron rule #3).

use bevy::prelude::*;
use reachlock_core::universe::UniverseTier;

/// Selects whether the client talks to a server at all. Chosen once at
/// startup from the `REACHLOCK_SERVER` env var (or a future menu toggle —
/// not built in this sprint). Online adds behavior; it never replaces the
/// offline path.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum NetMode {
    Offline,
    Online {
        url: String,
        player: String,
        universe: UniverseTier,
    },
}

impl NetMode {
    pub fn is_online(&self) -> bool {
        matches!(self, NetMode::Online { .. })
    }

    /// Reads `REACHLOCK_SERVER` (a `ws://` or `wss://` URL) plus the
    /// optional `REACHLOCK_PLAYER` / `REACHLOCK_UNIVERSE` overrides from the
    /// process environment. Delegates to [`Self::from_lookup`] so the
    /// parsing logic is testable without mutating global env state.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Pure parsing entry point: `lookup` stands in for `std::env::var` so
    /// tests can exercise every branch without racing on process-global
    /// environment variables (cargo test runs unit tests on one process,
    /// many threads).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        match lookup("REACHLOCK_SERVER") {
            Some(url) if !url.trim().is_empty() => {
                let player = lookup("REACHLOCK_PLAYER")
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(default_player_id);
                let universe = lookup("REACHLOCK_UNIVERSE")
                    .and_then(|s| s.parse::<UniverseTier>().ok())
                    .unwrap_or(UniverseTier::Classic);
                NetMode::Online {
                    url,
                    player,
                    universe,
                }
            }
            _ => NetMode::Offline,
        }
    }
}

/// A locally-unique-enough player id when none is supplied (spec §4,
/// cross-mode portability: offline uses a local-unique id too). Good enough
/// for the S02 spike; a real identity system is out of scope here.
fn default_player_id() -> String {
    format!("pilot-{}", std::process::id())
}

/// Lifecycle of the online socket (spec S02 "freeze first"). HUD and
/// network systems both read this; only `systems/network.rs` writes it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    /// Socket dropped or errored after having connected at least once (or
    /// failed to connect). The game keeps playing offline-style; the HUD
    /// shows the OFFLINE badge while `systems/network.rs` retries with
    /// backoff.
    Errored,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn no_server_var_is_offline() {
        assert_eq!(NetMode::from_lookup(lookup(&[])), NetMode::Offline);
    }

    #[test]
    fn empty_server_var_is_offline() {
        assert_eq!(
            NetMode::from_lookup(lookup(&[("REACHLOCK_SERVER", "")])),
            NetMode::Offline
        );
    }

    #[test]
    fn server_var_selects_online_with_defaults() {
        let mode = NetMode::from_lookup(lookup(&[("REACHLOCK_SERVER", "ws://127.0.0.1:40711")]));
        let NetMode::Online {
            url,
            player,
            universe,
        } = mode
        else {
            panic!("expected Online");
        };
        assert_eq!(url, "ws://127.0.0.1:40711");
        assert!(player.starts_with("pilot-"));
        assert_eq!(universe, UniverseTier::Classic);
    }

    #[test]
    fn player_and_universe_overrides_are_honored() {
        let mode = NetMode::from_lookup(lookup(&[
            ("REACHLOCK_SERVER", "ws://host:1"),
            ("REACHLOCK_PLAYER", "boris"),
            ("REACHLOCK_UNIVERSE", "fair_play"),
        ]));
        assert_eq!(
            mode,
            NetMode::Online {
                url: "ws://host:1".into(),
                player: "boris".into(),
                universe: UniverseTier::FairPlay,
            }
        );
    }

    #[test]
    fn bad_universe_override_falls_back_to_classic() {
        let mode = NetMode::from_lookup(lookup(&[
            ("REACHLOCK_SERVER", "ws://host:1"),
            ("REACHLOCK_UNIVERSE", "platinum"),
        ]));
        let NetMode::Online { universe, .. } = mode else {
            panic!("expected Online");
        };
        assert_eq!(universe, UniverseTier::Classic);
    }

    #[test]
    fn is_online_reflects_variant() {
        assert!(!NetMode::Offline.is_online());
        assert!(NetMode::Online {
            url: "ws://x".into(),
            player: "p".into(),
            universe: UniverseTier::Classic,
        }
        .is_online());
    }

    #[test]
    fn connection_state_defaults_disconnected() {
        assert_eq!(ConnectionState::default(), ConnectionState::Disconnected);
    }
}
