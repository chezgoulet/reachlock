//! Universe tier enum. Matches the Postgres `universe_tier` enum in the
//! seed ledger — the serialized names ARE the database values.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UniverseTier {
    /// Rules only — no inference. Instant, offline-friendly, purist.
    Classic,
    /// Server-side small-model inference (≤8B). Balanced competition.
    FairPlay,
    /// Cloud inference, best available open model.
    Spectrum,
    /// Player-provided API key, any model they pay for.
    Byok,
}

impl UniverseTier {
    pub const ALL: [UniverseTier; 4] = [
        UniverseTier::Classic,
        UniverseTier::FairPlay,
        UniverseTier::Spectrum,
        UniverseTier::Byok,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UniverseTier::Classic => "classic",
            UniverseTier::FairPlay => "fair_play",
            UniverseTier::Spectrum => "spectrum",
            UniverseTier::Byok => "byok",
        }
    }
}

impl std::str::FromStr for UniverseTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "classic" => Ok(UniverseTier::Classic),
            "fair_play" => Ok(UniverseTier::FairPlay),
            "spectrum" => Ok(UniverseTier::Spectrum),
            "byok" => Ok(UniverseTier::Byok),
            other => Err(format!("unknown universe tier: {other}")),
        }
    }
}
