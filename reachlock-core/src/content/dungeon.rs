use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DungeonRoom {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub connectors: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DungeonPuzzle {
    pub room_id: String,
    pub puzzle_type: String,
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dungeon {
    pub id: String,
    pub rooms: Vec<DungeonRoom>,
    pub puzzles: Vec<DungeonPuzzle>,
    pub enemies: Vec<String>,
    pub reward_tables: Vec<String>,
}
