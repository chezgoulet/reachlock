//! Per-connection identity (spec §8). Two handshakes:
//!
//! - **Token** (`?token=…`): resolved through the `SessionStore`. This is the
//!   path when auth is enabled — the token was minted by `POST /auth/dev`.
//! - **Legacy** (`?player=…&universe=…`): the pre-auth query-string identity.
//!   Accepted only when auth is NOT required, so S02 clients and local play
//!   keep working. When `REACHLOCK_AUTH=1`, a token is mandatory.

use reachlock_core::universe::UniverseTier;

use crate::services::auth::SessionStore;

pub struct Session {
    pub player_id: String,
    pub universe: UniverseTier,
}

/// Fields we understand in the WS query string.
struct Query {
    token: Option<String>,
    player: Option<String>,
    universe: UniverseTier,
}

impl Query {
    fn parse(query: &str) -> Result<Self, String> {
        let mut token = None;
        let mut player = None;
        let mut universe = UniverseTier::Classic;
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("token", v)) if !v.is_empty() => token = Some(v.to_string()),
                Some(("player", v)) if !v.is_empty() => player = Some(v.to_string()),
                Some(("universe", v)) => universe = v.parse().map_err(|e: String| e)?,
                _ => {}
            }
        }
        Ok(Query {
            token,
            player,
            universe,
        })
    }
}

impl Session {
    /// Resolve a connection's identity. A token always wins (and its stored
    /// universe is authoritative). Absent a token, fall back to the legacy
    /// `?player=` handshake only when `auth_required` is false.
    pub fn authenticate(
        query: &str,
        sessions: &dyn SessionStore,
        auth_required: bool,
    ) -> Result<Self, String> {
        let q = Query::parse(query)?;

        if let Some(token) = q.token {
            return match sessions.resolve(&token) {
                Some(info) => Ok(Session {
                    player_id: info.player_id,
                    universe: info.universe,
                }),
                None => Err("invalid or expired token".into()),
            };
        }

        if auth_required {
            return Err("authentication required: connect with ?token=… (POST /auth/dev)".into());
        }

        Ok(Session {
            player_id: q.player.ok_or("missing player=<id> in query string")?,
            universe: q.universe,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::auth::{MemorySessionStore, SessionInfo};

    #[test]
    fn legacy_query_parses_when_auth_off() {
        let store = MemorySessionStore::default();
        let s = Session::authenticate("player=boris&universe=spectrum", &store, false).unwrap();
        assert_eq!(s.player_id, "boris");
        assert_eq!(s.universe, UniverseTier::Spectrum);
    }

    #[test]
    fn universe_defaults_to_classic() {
        let store = MemorySessionStore::default();
        let s = Session::authenticate("player=tib", &store, false).unwrap();
        assert_eq!(s.universe, UniverseTier::Classic);
    }

    #[test]
    fn missing_player_is_an_error() {
        let store = MemorySessionStore::default();
        assert!(Session::authenticate("universe=byok", &store, false).is_err());
        assert!(Session::authenticate("", &store, false).is_err());
    }

    #[test]
    fn bad_universe_is_an_error() {
        let store = MemorySessionStore::default();
        assert!(Session::authenticate("player=x&universe=platinum", &store, false).is_err());
    }

    #[test]
    fn token_resolves_to_stored_identity() {
        let store = MemorySessionStore::default();
        let token = store.issue(SessionInfo {
            player_id: "boris".into(),
            universe: UniverseTier::FairPlay,
        });
        // Even with auth on, the token carries identity; the stored universe
        // wins over anything in the query.
        let s = Session::authenticate(&format!("token={token}&universe=classic"), &store, true)
            .unwrap();
        assert_eq!(s.player_id, "boris");
        assert_eq!(s.universe, UniverseTier::FairPlay);
    }

    #[test]
    fn auth_required_rejects_missing_token() {
        let store = MemorySessionStore::default();
        assert!(Session::authenticate("player=boris", &store, true).is_err());
    }

    #[test]
    fn bad_token_is_rejected() {
        let store = MemorySessionStore::default();
        assert!(Session::authenticate("token=dev_nope_0", &store, true).is_err());
    }
}
