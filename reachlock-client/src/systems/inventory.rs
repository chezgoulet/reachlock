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

use crate::settings::Settings;
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
    /// S21: system id in the gate network (e.g. "aethon") or uncharted hash.
    #[serde(default)]
    pub system_id: String,
    /// S21: system biome as serialized string.
    #[serde(default = "default_biome_str")]
    pub system_biome: String,
    /// S21: generation fidelity ("full" or "sparse").
    #[serde(default = "default_fidelity_str")]
    pub system_fidelity: String,
    /// S21: optional galactic coordinate serialized as [x, y, z].
    #[serde(default)]
    pub galaxy_coord: Option<[i64; 3]>,
    pub station_id: String,
    pub is_docked: bool,
    pub display_name: String,
    pub station_seed: u64,
    pub station_kind: Option<StationKind>,
    pub station_position: [f32; 2],
}

fn default_biome_str() -> String {
    "core".into()
}
fn default_fidelity_str() -> String {
    "full".into()
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
    /// Wall-clock stamp of the save, for universe catch-up on load. `None`
    /// on platforms without a wall clock (wasm) — catch-up just skips.
    #[serde(default)]
    pub saved_at_epoch_secs: Option<u64>,
    /// S13: live soul states (moods, memories, relationships, unlocked
    /// secrets) keyed by soul id. Authored soul files stay immutable.
    #[serde(default)]
    pub souls: BTreeMap<String, reachlock_core::soul::SoulState>,
    /// S17: the applied exterior configuration (spec §19). `None` = the
    /// stock Loup-Garou. The frozen core contract, stored as-is.
    #[serde(default)]
    pub hull_config: Option<reachlock_core::editor::exterior::HullConfiguration>,
    /// S18: the applied interior placement (spec §19). `None` = the
    /// authored Loup-Garou deck plan. The frozen core contract, stored
    /// as-is; On-Board realizes it on boarding.
    #[serde(default)]
    pub interior_layout: Option<reachlock_core::editor::interior::ShipInteriorLayout>,
}

const SAVE_PATH: &str = "save/player.ron";

/// Seconds since the Unix epoch, or `None` where the platform has no wall
/// clock (`SystemTime::now` panics on wasm32-unknown-unknown).
fn epoch_secs() -> Option<u64> {
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    }
}

/// Write the player's state to disk. Best-effort: a failed write is logged,
/// never fatal (offline-first — the game must run with no FS).
pub fn save_player(
    inv: &PlayerInventory,
    loc: &CurrentLocation,
    universe: Option<&UniverseState>,
    souls: &BTreeMap<String, reachlock_core::soul::SoulState>,
    hull_config: Option<&reachlock_core::editor::exterior::HullConfiguration>,
    interior_layout: Option<&reachlock_core::editor::interior::ShipInteriorLayout>,
) {
    let gc = loc.galaxy_coord.map(|c| [c.x, c.y, c.z]);
    fn biome_str(b: reachlock_core::seed::types::Biome) -> &'static str {
        use reachlock_core::seed::types::Biome;
        match b {
            Biome::Core => "core",
            Biome::Frontier => "frontier",
            Biome::Nebula => "nebula",
            Biome::Derelict => "derelict",
            Biome::DeepSpace => "deep_space",
        }
    }
    fn fidelity_str(f: reachlock_core::generator::system::Fidelity) -> &'static str {
        match f {
            reachlock_core::generator::system::Fidelity::Full => "full",
            _ => "sparse",
        }
    }
    let snapshot = LocationSnapshot {
        system_seed: loc.system_seed,
        system_id: loc.system_id.0.clone(),
        system_biome: biome_str(loc.system_biome).to_string(),
        system_fidelity: fidelity_str(loc.system_fidelity).to_string(),
        galaxy_coord: gc,
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
        saved_at_epoch_secs: epoch_secs(),
        souls: souls.clone(),
        hull_config: hull_config.cloned(),
        interior_layout: interior_layout.cloned(),
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
    let loc = file.location.map(|s| {
        use reachlock_core::generator::system::Fidelity;
        use reachlock_core::seed::types::Biome;
        fn parse_biome(s: &str) -> Biome {
            match s {
                "core" => Biome::Core,
                "frontier" => Biome::Frontier,
                "nebula" => Biome::Nebula,
                "derelict" => Biome::Derelict,
                "deep_space" => Biome::DeepSpace,
                _ => Biome::Frontier,
            }
        }
        fn parse_fidelity(s: &str) -> Fidelity {
            match s {
                "sparse" => Fidelity::Sparse,
                _ => Fidelity::Full,
            }
        }
        CurrentLocation {
            system_seed: s.system_seed,
            system_id: reachlock_core::seed::types::SystemId(s.system_id),
            system_biome: parse_biome(&s.system_biome),
            system_fidelity: parse_fidelity(&s.system_fidelity),
            galaxy_coord: s.galaxy_coord.map(|[x, y, z]| reachlock_core::galaxy::GalaxyCoord { x, y, z }),
            // Hostile-location routing is transient (set on POI approach), not
            // persisted — a reload never drops you mid-fight.
            hostile_location_id: None,
            station_id: s.station_id,
            is_docked: s.is_docked,
            display_name: s.display_name,
            station_position: Vec2::new(s.station_position[0], s.station_position[1]),
            station_seed: s.station_seed,
            station_kind: s.station_kind,
        }
    })?;
    Some((file.inventory, loc))
}

/// Autosave throttle: writes the save every interval of *real* time so a
/// quit mid-session preserves progress without hammering the disk each frame.
/// The interval is read from `settings.gameplay.auto_save_interval_secs`.
#[derive(Resource, Default)]
pub struct SaveTimer(pub f32);

/// Accumulate real time and autosave on the interval. Runs in all `InGame`
/// modes (wired in `main.rs`). Offline-safe: `save_player` never panics.
#[allow(clippy::too_many_arguments)]
pub fn autosave_system(
    time: Res<Time<Real>>,
    settings: Res<Settings>,
    inv: Res<PlayerInventory>,
    loc: Res<CurrentLocation>,
    mut timer: ResMut<SaveTimer>,
    ticker: Option<Res<UniverseTicker>>,
    souls: Res<crate::systems::soul::SoulRegistry>,
    shipcfg: Res<crate::systems::shipeditor::ShipConfig>,
    interior_cfg: Res<crate::systems::shipeditor::InteriorConfig>,
) {
    let interval = settings.gameplay.auto_save_interval_secs as f32;
    timer.0 += time.delta_secs();
    if timer.0 >= interval {
        timer.0 = 0.0;
        save_player(
            &inv,
            &loc,
            ticker.as_ref().map(|t| &t.state),
            &souls.states,
            shipcfg.config.as_ref(),
            interior_cfg.layout.as_ref(),
        );
    }
}

/// Startup: restore inventory + location from a prior local save, if any.
/// Also restores the universe state and fast-forwards the ticks that elapsed
/// while the game was closed (capped inside `catch_up`). Wired in `main.rs`
/// `Startup`; offline-safe (a missing/corrupt save is a fresh start, never a
/// crash). `UniverseTicker` is an `init_resource` so it already exists here.
pub fn load_save(
    mut inv: ResMut<PlayerInventory>,
    mut loc: ResMut<CurrentLocation>,
    mut ticker: ResMut<UniverseTicker>,
    mut souls: ResMut<crate::systems::soul::SoulRegistry>,
    mut shipcfg: ResMut<crate::systems::shipeditor::ShipConfig>,
    mut interior_cfg: ResMut<crate::systems::shipeditor::InteriorConfig>,
    content: Res<crate::systems::content_index::ContentIndex>,
) {
    if let Some((i, l)) = load_player() {
        *inv = i;
        *loc = l;
    }
    if let Ok(text) = std::fs::read_to_string(SAVE_PATH) {
        if let Ok(file) = ron::from_str::<SaveFile>(&text) {
            // Restore universe from save (if present), catch up elapsed ticks.
            if let Some(saved) = file.universe {
                ticker.state = saved;
                if let (Some(then), Some(now)) = (file.saved_at_epoch_secs, epoch_secs()) {
                    let elapsed_ticks =
                        now.saturating_sub(then) / crate::systems::ticker::TICK_SECS;
                    let _events = ticker.catch_up(elapsed_ticks);
                }
            }
            // Restore live soul states over the fresh ones init_souls built
            // (runs chained before this system). Authored files stay put.
            for (id, state) in file.souls {
                souls.states.insert(id, state);
            }
            // S17: restore the applied exterior config; handling re-derives
            // from the config + frame (never stored — it's derived data).
            if let Some(config) = file.hull_config {
                shipcfg.set(config, &content);
            }
            // S18: restore the applied interior layout; the realized
            // walkable layout re-derives on boarding (never stored).
            if let Some(layout) = file.interior_layout {
                interior_cfg.layout = Some(layout);
            }
        }
    }
}
