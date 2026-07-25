use serde::{Deserialize, Serialize};

use crate::seed::Seed;

pub type CareerPathId = String;
pub type Rank = u8;
pub type ShipTemplateId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FactionStandingDelta {
    pub faction_id: String,
    pub delta: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ItemStack {
    pub item_id: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Origin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub starting_career: CareerPathId,
    pub starting_rank: Rank,
    pub faction_deltas: Vec<FactionStandingDelta>,
    pub starting_credits: u64,
    pub ship_template: Option<ShipTemplateId>,
    pub ship_seed: Option<Seed>,
    pub starting_gear: Vec<ItemStack>,
    pub starting_crew: Vec<CrewAssignment>,
    pub known_systems: Vec<Seed>,
    pub start_system: Seed,
    pub start_location: String,
    pub opening_log_entries: Vec<LogEntryDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CrewAssignment {
    Authored {
        soul_id: String,
        role: String,
    },
    Procedural {
        seed: Seed,
        species: String,
        role: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogEntryDraft {
    pub title: String,
    pub body: String,
    pub tick_offset: u64,
}
