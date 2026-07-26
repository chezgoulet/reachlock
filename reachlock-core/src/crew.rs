//! Crew content schemas (S77). Authored crew packages and ship templates
//! that replace the hardcoded Loup-Garou defaults. All types are serde
//! enabled for RON round-trip through the content index.

use serde::{Deserialize, Serialize};

use crate::generator::ship::ShipInterior;

/// A crew member entry in an authored crew package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrewMemberEntry {
    pub soul_id: String,
    pub role: String,
    #[serde(default)]
    pub duty_room: Option<String>,
    #[serde(default)]
    pub starting: bool,
    /// Salary demand per pay period (credits). Default 0.
    #[serde(default)]
    pub salary: u64,
}

/// A set of crew members that travel together. Authored by background
/// packages ("Loup-Garou veteran") or custom encounters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrewPackage {
    pub id: String,
    pub name: String,
    pub description: String,
    pub members: Vec<CrewMemberEntry>,
}

/// A ship template for starting packages and NPC encounters.
/// The Loup-Garou is one entry among many.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub hull_id: String,
    pub interior: ShipInterior,
    #[serde(default = "default_system_seed")]
    pub default_system_seed: u64,
}

/// Default starting system seed (Aethon, backward compatible).
fn default_system_seed() -> u64 {
    16843009
}

/// Starting location derived from an origin package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartingLocation {
    pub system_seed: u64,
    #[serde(default)]
    pub station_id: Option<String>,
    #[serde(default)]
    pub landing_pad: Option<String>,
}

impl Default for StartingLocation {
    fn default() -> Self {
        StartingLocation {
            system_seed: 16843009,
            station_id: None,
            landing_pad: None,
        }
    }
}
