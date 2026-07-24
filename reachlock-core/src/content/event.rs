use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerCondition {
    PlayerReputation { faction: String, min: i32 },
    TickAfter { ticks: u64 },
    ChapterComplete { chapter: String },
    HasItem { item_id: String },
    PlayerInSystem { system_id: String },
    FactionState { faction: String, state: String },
    FlagSet { flag: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Consequence {
    AddReputation { faction: String, delta: i32 },
    AddItem { item_id: String, quantity: u32 },
    AdvanceChapter { chapter: String },
    EcosystemEvent { event_type: String },
    SpawnEncounter { encounter_id: String },
    SetFlag { flag: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventStage {
    pub narrative_text: String,
    pub trigger_conditions: Vec<TriggerCondition>,
    pub consequences: Vec<Consequence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub stages: Vec<EventStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_after_ticks: Option<u64>,
}
