//! Per-connection state (spec §8). Identity is declared in the connection
//! query string for now (`/ws?player=boris&universe=fair_play`) — session
//! tokens come with the auth service.

use reachlock_core::universe::UniverseTier;

pub struct Session {
    pub player_id: String,
    pub universe: UniverseTier,
}

impl Session {
    pub fn from_query(query: &str) -> Result<Self, String> {
        let mut player_id = None;
        let mut universe = UniverseTier::Classic;
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("player", v)) if !v.is_empty() => player_id = Some(v.to_string()),
                Some(("universe", v)) => {
                    universe = v.parse().map_err(|e: String| e)?;
                }
                _ => {}
            }
        }
        Ok(Session {
            player_id: player_id.ok_or("missing player=<id> in query string")?,
            universe,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_query() {
        let s = Session::from_query("player=boris&universe=spectrum").unwrap();
        assert_eq!(s.player_id, "boris");
        assert_eq!(s.universe, UniverseTier::Spectrum);
    }

    #[test]
    fn universe_defaults_to_classic() {
        let s = Session::from_query("player=tib").unwrap();
        assert_eq!(s.universe, UniverseTier::Classic);
    }

    #[test]
    fn missing_player_is_an_error() {
        assert!(Session::from_query("universe=byok").is_err());
        assert!(Session::from_query("").is_err());
    }

    #[test]
    fn bad_universe_is_an_error() {
        assert!(Session::from_query("player=x&universe=platinum").is_err());
    }
}
