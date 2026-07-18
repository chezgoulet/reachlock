//! Authored hostile locations (S20, spec §14 Mode 1 step 8 "dungeon-lite").
//!
//! A [`HostileLocation`] is a small authored interior — a handful of rooms
//! wired by door connections, some seeded with enemy spawns and props, one
//! optionally gated by a keycard. It is plain authored data (loaded straight
//! from `content/locations/*.ron`), pure and serde-stable, so the client can
//! realize the same derelict on every machine. The room *geometry* stays
//! deliberately simple — cell width/height and a kind tag — because S20
//! delivers the combat verbs, not a full Predecessor dungeon (that's Phase 2).

use serde::{Deserialize, Serialize};

/// One enemy placement inside a room: which archetype, where it starts, and
/// the patrol loop it walks until it notices the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostileSpawn {
    /// Archetype id, resolved against the loaded `content/combat/*.ron` set.
    pub archetype: String,
    /// Spawn cell within the room.
    pub pos: (i64, i64),
    /// Patrol waypoints (room cells); empty means "hold position".
    #[serde(default)]
    pub patrol: Vec<(i64, i64)>,
}

/// A destructible or interactive fixture — an explosive barrel, a breakable
/// crate. `kind` is a free tag the client maps to behavior/loot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostileProp {
    pub kind: String,
    pub pos: (i64, i64),
}

/// One room in the location: a cell-sized box with a kind tag and any spawns
/// and props authored into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostileRoom {
    pub id: String,
    pub width: u32,
    pub height: u32,
    /// Free-form room role: "empty", "corridor", "arena", "boss", "reward".
    pub kind: String,
    #[serde(default)]
    pub spawns: Vec<HostileSpawn>,
    #[serde(default)]
    pub props: Vec<HostileProp>,
}

/// A locked door plus the key that opens it — the one gating verb S20 ships.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keycard {
    /// The connection (room id pair) this key unlocks.
    pub door: (String, String),
    /// Item/flag name the player must hold to pass.
    pub key_name: String,
}

/// A whole authored hostile interior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostileLocation {
    pub id: String,
    pub display_name: String,
    pub rooms: Vec<HostileRoom>,
    /// Room-id pairs that are walkable neighbours (undirected).
    #[serde(default)]
    pub connections: Vec<(String, String)>,
    /// Optional keycard gate on one of the connections.
    #[serde(default)]
    pub keycard: Option<Keycard>,
}

impl HostileLocation {
    /// Find a room by id.
    pub fn room(&self, id: &str) -> Option<&HostileRoom> {
        self.rooms.iter().find(|r| r.id == id)
    }

    /// Every distinct archetype id referenced by a spawn — what the client
    /// must have loaded before it can realize this location.
    pub fn referenced_archetypes(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self
            .rooms
            .iter()
            .flat_map(|r| r.spawns.iter().map(|s| s.archetype.as_str()))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The authored derelict shape must round-trip through RON exactly as the
    /// content file writes it (this is the wire shape the loader depends on).
    #[test]
    fn parses_authored_shape() {
        let ron = r#"#![enable(implicit_some)]
        (
            id: "derelict_hold",
            display_name: "Derelict Cargo Hold",
            rooms: [
                (id: "airlock", width: 10, height: 6, kind: "empty"),
                (id: "storage", width: 8, height: 8, kind: "arena",
                    spawns: [
                        (archetype: "raider_melee", pos: (2, 3), patrol: [(2,3),(6,3)]),
                    ],
                    props: [
                        (kind: "barrel", pos: (7, 6)),
                    ],
                ),
            ],
            connections: [
                ("airlock", "storage"),
            ],
            keycard: (door: ("storage", "hold"), key_name: "keycard_hold"),
        )"#;
        let loc: HostileLocation = ron::from_str(ron).expect("authored location parses");
        assert_eq!(loc.id, "derelict_hold");
        assert_eq!(loc.rooms.len(), 2);
        // Empty rooms default their spawns/props to nothing.
        assert!(loc.room("airlock").unwrap().spawns.is_empty());
        let storage = loc.room("storage").unwrap();
        assert_eq!(storage.spawns[0].archetype, "raider_melee");
        assert_eq!(storage.spawns[0].patrol.len(), 2);
        assert_eq!(loc.referenced_archetypes(), vec!["raider_melee"]);
        assert_eq!(loc.keycard.unwrap().key_name, "keycard_hold");
    }
}
