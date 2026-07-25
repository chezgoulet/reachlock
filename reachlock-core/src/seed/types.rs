//! Seed protocol types.

use serde::{Deserialize, Serialize};

/// A seed value. Constrained to 53 bits so it survives every JSON
/// implementation that parses numbers as f64 — a v1 lesson learned the hard
/// way (Godot doubles vs Go int64 decode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Seed(u64);

impl Seed {
    pub const MAX: u64 = (1 << 53) - 1;

    /// Constructs a seed, masking to 53 bits.
    pub fn new(value: u64) -> Self {
        Seed(value & Self::MAX)
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

/// Stable identifier for a star system (e.g. `"duskway-0417"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SystemId(pub String);

/// Stable identifier for a player (server-issued in online mode, locally
/// generated in offline mode — the protocol does not care which).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub String);

/// What kind of object a seed generates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    System,
    Ship,
    Station,
    Planet,
    Music,
    UiPanel,
}

impl ObjectType {
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectType::System => "system",
            ObjectType::Ship => "ship",
            ObjectType::Station => "station",
            ObjectType::Planet => "planet",
            ObjectType::Music => "music",
            ObjectType::UiPanel => "ui_panel",
        }
    }
}

/// Biome flavor fed into generation parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Biome {
    Core,
    Frontier,
    Nebula,
    Derelict,
    DeepSpace,
}

impl Biome {
    pub fn as_str(self) -> &'static str {
        match self {
            Biome::Core => "core",
            Biome::Frontier => "frontier",
            Biome::Nebula => "nebula",
            Biome::Derelict => "derelict",
            Biome::DeepSpace => "deep_space",
        }
    }
}
