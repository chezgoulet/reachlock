pub mod coord;
pub mod gate;

pub use coord::{deep_space_seed, nearest_charted_distance_sq, GalaxyCoord};
pub use gate::{Gate, GateNetwork, GateStatus};

use serde::{Deserialize, Serialize};

use crate::seed::types::Biome;

/// An authored charted system — the canonical data for every named system in
/// the gate network. Loaded from `content/systems/*.ron` at startup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartedSystem {
    pub id: String,
    pub display_name: String,
    pub position: GalaxyCoord,
    pub biome: Biome,
    pub seed: u64,
    pub description: String,
}
