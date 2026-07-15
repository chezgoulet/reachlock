//! Player inventory + local save (spec §14 Mode 1; S07). Credits are an
//! integer; cargo is a `GoodId` → qty map bounded by `capacity`. Persisted
//! to a minimal local RON file alongside `CurrentLocation` so a quit/relaunch
//! keeps your stuff (S07 acceptance gate). No `f32`/serde on `Vec2` — the
//! snapshot stores a plain tuple for position.

use std::collections::BTreeMap;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use reachlock_core::economy::GoodId;
use reachlock_core::generator::station::StationKind;
use reachlock_core::sim::UniverseState;

use crate::states::CurrentLocation;
use crate::systems::ticker::UniverseTicker;

/// The player's wallet + hold. `capacity` is cargo slots (not weight); S10
/// may reinterpret it. `GoodId` is a string newtype (economy module).
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct PlayerInventory {
    pub credits: i64,
    pub capacity: u32,
    pub cargo: BTreeMap<GoodId, u32>,
}

impl PlayerInventory {
    /// Total units of cargo currently held (for capacity checks).
    pub fn cargo_units(&self) -> u32 {
        self.cargo.values().sum()
    }

    pub fn can_hold(&self, extra: u32) -> bool {
        self.cargo_units().saturating_add(extra) <= self.capacity
    }
}

/// Serializable snapshot of where the player is (no `Vec2`, no live scene).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocationSnapshot {
    pub system_seed: u64,
    pub station_id: String,
    pub is_docked: bool,
    pub display_name: String,
    pub station_seed: u64,
    pub station_kind: Option<StationKind>,
    pub station_position: [f32; 2],
}

/// On-disk save shape.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SaveFile {
    #[serde(default)]
    pub inventory: PlayerInventory,
    #[serde(default)]
    pub location: Option<LocationSnapshot>,
    #[serde(default)]
    pub universe: Option<UniverseState>,
}

const SAVE_PATH: &str = "save/player.ron";

/// Write the player's state to disk. Best-effort: a failed write is logged,
/// never fatal (offline-first — the game must run with no FS).
pub fn save_player(inv: &PlayerInventory, loc: &CurrentLocation, universe: Option<&UniverseState>) {
    let snapshot = LocationSnapshot {
        system_seed: loc.system_seed,
        station_id: loc.station_id.clone(),
        is_docked: loc.is_docked,
        display_name: loc.display_name.clone(),
        station_seed: loc.station_seed,
        station_kind: loc.station_kind,
        station_position: [loc.station_position.x, loc.station_position.y],
    };
    let file = SaveFile {
        inventory: inv.clone(),
        location: Some(snapshot),
        universe: universe.cloned(),
    };
    match ron::to_string(&file) {
        Ok(text) => {
            if let Some(parent) = Path::new(SAVE_PATH).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(SAVE_PATH, text) {
                warn!("save_player: could not write {SAVE_PATH}: {e}");
            }
        }
        Err(e) => warn!("save_player: serialize failed: {e}"),
    }
}

/// Load a prior save, if present and parseable. Returns the inventory and the
/// location to restore. `None` means a fresh start (no file / corrupt).
pub fn load_player() -> Option<(PlayerInventory, CurrentLocation)> {
    let text = std::fs::read_to_string(SAVE_PATH).ok()?;
    let file: SaveFile = ron::from_str(&text).ok()?;
    let loc = file.location.map(|s| CurrentLocation {
        system_seed: s.system_seed,
        station_id: s.station_id,
        is_docked: s.is_docked,
        display_name: s.display_name,
        station_position: Vec2::new(s.station_position[0], s.station_position[1]),
        station_seed: s.station_seed,
        station_kind: s.station_kind,
    })?;
    Some((file.inventory, loc))
}

/// Autosave throttle: writes the save every `INTERVAL` of *real* time so a
/// quit mid-session preserves progress without hammering the disk each frame.
#[derive(Resource, Default)]
pub struct SaveTimer(pub f32);

const INTERVAL: f32 = 5.0;

/// Accumulate real time and autosave on the interval. Runs in all `InGame`
/// modes (wired in `main.rs`). Offline-safe: `save_player` never panics.
pub fn autosave_system(
    time: Res<Time<Real>>,
    inv: Res<PlayerInventory>,
    loc: Res<CurrentLocation>,
    mut timer: ResMut<SaveTimer>,
    ticker: Option<Res<UniverseTicker>>,
) {
    timer.0 += time.delta_secs();
    if timer.0 >= INTERVAL {
        timer.0 = 0.0;
        save_player(&inv, &loc, ticker.as_ref().map(|t| &t.state));
    }
}

/// Startup: restore inventory + location from a prior local save, if any.
/// Also restores the universe state and runs catch-up for elapsed ticks.
/// Wired in `main.rs` `Startup`; offine-safe (a missing/corrupt save is a
/// fresh start, never a crash).
pub fn load_save(
    mut inv: ResMut<PlayerInventory>,
    mut loc: ResMut<CurrentLocation>,
    mut ticker: Option<ResMut<UniverseTicker>>,
) {
    if let Some((i, l)) = load_player() {
        *inv = i;
        *loc = l;
    }
    // Restore universe from save (if present) and catch up.
    if let Some(ref mut ticker) = ticker {
        if let Ok(text) = std::fs::read_to_string(SAVE_PATH) {
            if let Ok(file) = ron::from_str::<SaveFile>(&text) {
                if let Some(saved) = file.universe {
                    ticker.state = saved;
                    let seed = 0x5EED_0001u64; // canonical catch-up seed
                    let _events = ticker.catch_up(seed);
                }
            }
        }
    }
}
