//! Procedural generation primitives (spec §5).
//!
//! Every generator is a pure function: `Seed + Parameters → plain data`.
//! No randomness outside the seed, no floats in gameplay-critical values,
//! no target-dependent behavior. The client's bridge layer converts these
//! structs to Bevy assets; authored content deserializes into the same
//! structs (spec §10 — the bridge doesn't know the difference).

pub mod contract;
pub mod dilemma;
pub mod economy;
pub mod ecosystem;
pub mod ecosystem_events;
pub mod enemy;
pub mod faction;
pub mod hull;
pub mod location;
pub mod music;
pub mod planet;
pub mod ship;
pub mod soul;
pub mod sprite;
pub mod station;
pub mod storyline;
pub mod system;
pub mod transit;
pub mod ui;

pub use contract::generate_contract;
pub use economy::generate_economy_catalog;
pub use ecosystem::{generate_ecosystem, generate_species_visual, Ecosystem, PlanetParams};
pub use ecosystem_events::{apply_ecosystem_event, EcosystemEvent, EcosystemEventType};
pub use enemy::generate_enemy;
pub use faction::generate_faction;
pub use hull::{generate_hull, generate_hull_class};
pub use location::generate_location;
pub use music::{generate_music, generate_tone, Mood};
pub use planet::generate_planet;
pub use soul::generate_soul;
pub use sprite::generate_character_sprite;
pub use station::generate_station;
pub use storyline::generate_storyline;
pub use system::{generate_starfield, generate_system, HostileLocationKind, HostileLocationSlot};
pub use transit::{anomaly_rolls, malfunction_roll, transit_destination};
pub use ui::generate_ui_panel;

use serde::{Deserialize, Serialize};

use crate::util::rng::Fixed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedVec2 {
    pub x: Fixed,
    pub y: Fixed,
}

/// Plain-data mesh, target-independent. Serde derives make this the shared
/// wire/authoring shape for both generated output (spec §5) and authored
/// content (spec §10) — field names are pinned by
/// `generator::tests::mesh_round_trip` because authored `.ron` files depend
/// on them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedMesh {
    pub vertices: Vec<FixedVec2>,
    pub indices: Vec<u32>,
}

/// Plain-data RGBA image. Serde-enabled (unlike its mesh/audio/layout
/// siblings above) because S05's `GeneratedItem` embeds an icon texture and
/// needs the whole struct to round-trip over the wire/into storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedTexture {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

/// Plain-data audio: mono PCM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedAudio {
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

/// A room in a generated (or authored) interior layout. Grid units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Room {
    pub kind: RoomKind,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
    Hangar,
    Corridor,
    Quarters,
    Bar,
    Market,
    Shipyard,
    Reactor,
    Bridge,
    // Ship-interior kinds (docs/SHIPS.md §6). Stations don't generate these;
    // authored ship layouts place them.
    /// The pilot's seat and canopy — take the helm here.
    Cockpit,
    /// Processing floor + shuttle pad (the Loup-Garou's zero-g workspace).
    TechBay,
    /// The scanner array, a console in its own room.
    Scanner,
    /// Trauma and surgery.
    MedBay,
    /// Cryo pods — the only way living crew survive a self-generated jump.
    Cryo,
    // S18 interior-editor kinds (spec §19 template list). Appended so the
    // existing kinds keep their discriminants (the determinism manifest
    // hashes `kind as u8`). Stations don't generate these; placed room
    // templates realize into them.
    /// Grow beds and aeroponics — the ship feeds itself.
    Hydroponics,
    /// Weapon racks and armor lockers.
    Armory,
    /// A holding cell with an isolation chamber.
    Brig,
}

/// A door connecting two rooms (indices into the room list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Door {
    pub from: u32,
    pub to: u32,
    /// Door position in grid units.
    pub x: i32,
    pub y: i32,
}

/// Interior layout: rooms plus the doors that connect them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedLayout {
    pub rooms: Vec<Room>,
    pub doors: Vec<Door>,
}

/// Where an asset comes from (spec §5, Override System). The generators
/// produce `Procedural` output; the content pipeline produces the same
/// data structures from authored files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetSource {
    Procedural { seed: u64 },
    HandCrafted { asset_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the JSON field names of the structs authored `.ron` content
    /// files depend on (spec §10 gotcha: "authored files ARE the
    /// compatibility surface"). If this test's expected JSON changes, an
    /// authoring format change is happening — that's a compatibility break,
    /// not a refactor.
    #[test]
    fn mesh_round_trip_and_field_names_are_pinned() {
        let mesh = GeneratedMesh {
            vertices: vec![
                FixedVec2 {
                    x: Fixed(1024),
                    y: Fixed(-2048),
                },
                FixedVec2 {
                    x: Fixed(0),
                    y: Fixed(0),
                },
            ],
            indices: vec![0, 1, 2],
        };
        let json = serde_json::to_string(&mesh).unwrap();
        assert_eq!(
            json,
            r#"{"vertices":[{"x":1024,"y":-2048},{"x":0,"y":0}],"indices":[0,1,2]}"#
        );
        let back: GeneratedMesh = serde_json::from_str(&json).unwrap();
        assert_eq!(mesh, back);
    }

    #[test]
    fn layout_round_trip_and_field_names_are_pinned() {
        let layout = GeneratedLayout {
            rooms: vec![Room {
                kind: RoomKind::Hangar,
                x: 0,
                y: 0,
                width: 48,
                height: 32,
            }],
            doors: vec![Door {
                from: 0,
                to: 1,
                x: 16,
                y: 32,
            }],
        };
        let json = serde_json::to_string(&layout).unwrap();
        assert_eq!(
            json,
            r#"{"rooms":[{"kind":"hangar","x":0,"y":0,"width":48,"height":32}],"doors":[{"from":0,"to":1,"x":16,"y":32}]}"#
        );
        let back: GeneratedLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(layout, back);
    }

    /// A float where a `Fixed` (i64, transparent) is expected must fail to
    /// parse, not silently truncate (spec §10 gotcha).
    #[test]
    fn float_in_place_of_fixed_is_a_parse_error() {
        let err = serde_json::from_str::<FixedVec2>(r#"{"x":1.5,"y":0}"#).unwrap_err();
        assert!(err.to_string().contains("invalid type") || err.is_data());
    }
}
