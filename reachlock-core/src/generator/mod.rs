//! Procedural generation primitives (spec §5).
//!
//! Every generator is a pure function: `Seed + Parameters → plain data`.
//! No randomness outside the seed, no floats in gameplay-critical values,
//! no target-dependent behavior. The client's bridge layer converts these
//! structs to Bevy assets; authored content deserializes into the same
//! structs (spec §10 — the bridge doesn't know the difference).

pub mod hull;
pub mod music;
pub mod planet;
pub mod station;
pub mod system;
pub mod ui;

pub use hull::generate_hull;
pub use music::{generate_music, generate_tone, Mood};
pub use planet::generate_planet;
pub use station::generate_station;
pub use system::{generate_starfield, generate_system};
pub use ui::generate_ui_panel;

use serde::{Deserialize, Serialize};

use crate::util::rng::Fixed;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedVec2 {
    pub x: Fixed,
    pub y: Fixed,
}

/// Plain-data mesh, target-independent.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Room {
    pub kind: RoomKind,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomKind {
    Hangar,
    Corridor,
    Quarters,
    Bar,
    Market,
    Shipyard,
    Reactor,
    Bridge,
}

/// A door connecting two rooms (indices into the room list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Door {
    pub from: u32,
    pub to: u32,
    /// Door position in grid units.
    pub x: i32,
    pub y: i32,
}

/// Interior layout: rooms plus the doors that connect them.
#[derive(Debug, Clone, PartialEq, Eq)]
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
