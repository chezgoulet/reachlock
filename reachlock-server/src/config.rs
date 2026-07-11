//! Server configuration from environment variables. v1 convention kept:
//! ports live in the 407xx block (pan 40707, simd 40708, eard 40709,
//! ship-share 40710 — reachlock-server takes 40711).

pub struct Config {
    pub bind: String,
    pub tick_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Self {
        Config {
            bind: std::env::var("REACHLOCK_BIND").unwrap_or_else(|_| "127.0.0.1:40711".into()),
            tick_interval_secs: std::env::var("REACHLOCK_TICK_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
        }
    }
}
