//! Server configuration from environment variables. v1 convention kept:
//! ports live in the 407xx block (pan 40707, simd 40708, eard 40709,
//! ship-share 40710 — reachlock-server takes 40711).

pub struct Config {
    pub bind: String,
    pub tick_interval_secs: u64,
    /// `REACHLOCK_DB=postgres://…` selects the Postgres-backed stores. Unset =
    /// in-memory (zero infra). This is the ONE store-selection switch.
    pub db_url: Option<String>,
    /// `REACHLOCK_AUTH=1` requires a session token on the WS handshake. Unset
    /// stays permissive so S02 clients and local play keep working.
    pub auth_required: bool,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            bind: std::env::var("REACHLOCK_BIND").unwrap_or_else(|_| "127.0.0.1:40711".into()),
            tick_interval_secs: std::env::var("REACHLOCK_TICK_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            db_url: std::env::var("REACHLOCK_DB").ok().filter(|s| !s.is_empty()),
            auth_required: std::env::var("REACHLOCK_AUTH").as_deref() == Ok("1"),
        }
    }
}

impl Default for Config {
    /// In-memory, permissive, silent tick — the shape integration tests want.
    fn default() -> Self {
        Config {
            bind: "127.0.0.1:0".into(),
            tick_interval_secs: 3600,
            db_url: None,
            auth_required: false,
        }
    }
}
