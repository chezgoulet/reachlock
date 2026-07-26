//! Player inventory + local save (spec §14 Mode 1; S07). Credits are an
//! integer; cargo is a `GoodId` → qty map bounded by `capacity`. Persisted
//! to a minimal local RON file alongside `CurrentLocation` so a quit/relaunch
//! keeps your stuff (S07 acceptance gate). No `f32`/serde on `Vec2` — the
//! snapshot stores a plain tuple for position.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use reachlock_core::economy::GoodId;
use reachlock_core::generator::station::StationKind;
use reachlock_core::identity::PlayerCharacter;
use reachlock_core::sim::UniverseState;

use reachlock_core::item::types::{ItemStats, MeleeWeapon};

use crate::settings::Settings;
use crate::states::CurrentLocation;
use crate::systems::discovery::DiscoveryLog;
use crate::systems::ticker::UniverseTicker;
use crate::theme;

/// The player's wallet + hold. `capacity` is cargo slots (not weight); S10
/// may reinterpret it. `GoodId` is a string newtype (economy module).
#[derive(Resource, Default, Clone, Debug, Serialize, Deserialize)]
pub struct PlayerInventory {
    pub credits: i64,
    pub capacity: u32,
    pub cargo: BTreeMap<GoodId, u32>,
    /// S20: the melee weapon carried into landed combat. `None` → fists.
    #[serde(default)]
    pub equipped_weapon: Option<(MeleeWeapon, ItemStats)>,
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
    /// on platforms without a wall clock — catch-up just skips.
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
    /// S75: the player character identity. `None` = character not yet created
    /// (pre-S75 save or fresh start) — the game presents the character-creation
    /// flow as a first-time setup path.
    #[serde(default)]
    pub character: Option<PlayerCharacter>,
    /// S85: player's discovery log (systems charted).
    #[serde(default)]
    pub discovery_log: DiscoveryLog,
    /// S80: the crew roster (persisted for save/reload).
    #[serde(default)]
    pub crew_roster: Option<crate::systems::crew::CrewRoster>,
}

/// Seconds since the Unix epoch.
fn epoch_secs() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
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
    character: Option<&PlayerCharacter>,
) {
    save_player_with_log(
        inv,
        loc,
        universe,
        souls,
        hull_config,
        interior_layout,
        character,
        &DiscoveryLog::default(),
        None,
    );
}

/// Full save including discovery log.
#[allow(clippy::too_many_arguments)]
pub fn save_player_with_log(
    inv: &PlayerInventory,
    loc: &CurrentLocation,
    universe: Option<&UniverseState>,
    souls: &BTreeMap<String, reachlock_core::soul::SoulState>,
    hull_config: Option<&reachlock_core::editor::exterior::HullConfiguration>,
    interior_layout: Option<&reachlock_core::editor::interior::ShipInteriorLayout>,
    character: Option<&PlayerCharacter>,
    discovery_log: &DiscoveryLog,
    crew_roster: Option<&crate::systems::crew::CrewRoster>,
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
        character: character.cloned(),
        discovery_log: discovery_log.clone(),
        crew_roster: crew_roster.cloned(),
    };
    match ron::to_string(&file) {
        Ok(text) => crate::save_backend::write_save(&text),
        Err(e) => warn!("save_player: serialize failed: {e}"),
    }
}

/// Load a prior save, if present and parseable. Returns the inventory and the
/// location to restore. `None` means a fresh start (no file / corrupt).
pub fn load_player() -> Option<(PlayerInventory, CurrentLocation)> {
    let text = crate::save_backend::read_save()?;
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
            galaxy_coord: s
                .galaxy_coord
                .map(|[x, y, z]| reachlock_core::galaxy::GalaxyCoord { x, y, z }),
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
    discovery_log: Res<DiscoveryLog>,
    roster: Option<Res<crate::systems::crew::CrewRoster>>,
) {
    let interval = settings.gameplay.auto_save_interval_secs as f32;
    timer.0 += time.delta_secs();
    if timer.0 >= interval {
        timer.0 = 0.0;
        save_player_with_log(
            &inv,
            &loc,
            ticker.as_ref().map(|t| &t.state),
            &souls.states,
            shipcfg.config.as_ref(),
            interior_cfg.layout.as_ref(),
            None,
            &discovery_log,
            roster.as_deref(),
        );
    }
}

#[derive(Resource, Default)]
pub struct InventoryPanelVisible(pub bool);

#[derive(Component)]
pub struct InventoryPanel;

/// Spawn the inventory panel entity (hidden by default).
pub fn spawn_inventory_panel(mut commands: Commands) {
    commands.spawn((
        InventoryPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        theme::fg("text.ok"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Toggle inventory panel on the assigned key.
pub fn inventory_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<crate::settings::Settings>,
    mut visible: ResMut<InventoryPanelVisible>,
) {
    if keys.just_pressed(settings.key(crate::settings::InputAction::OpenInventory)) {
        visible.0 = !visible.0;
    }
}

/// Render the inventory panel when visible.
pub fn render_inventory_panel(
    visible: Res<InventoryPanelVisible>,
    inventory: Res<PlayerInventory>,
    mut query: Query<(&mut Text, &mut Visibility), With<InventoryPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── INVENTORY ──".to_string()];
            lines.push(format!("  Credits: {}", inventory.credits));
            lines.push(format!(
                "  Cargo: {}/{} units",
                inventory.cargo_units(),
                inventory.capacity
            ));
            for (good, qty) in &inventory.cargo {
                lines.push(format!("  {}: {}", good.0, qty));
            }
            **text = lines.join("\n");
        } else {
            *vis = Visibility::Hidden;
        }
    }
}

/// Startup: restore inventory + location from a prior local save, if any.
/// Also restores the universe state and fast-forwards the ticks that elapsed
/// while the game was closed (capped inside `catch_up`). Wired in `main.rs`
/// `Startup`; offline-safe (a missing/corrupt save is a fresh start, never a
/// crash). `UniverseTicker` is an `init_resource` so it already exists here.
#[allow(clippy::too_many_arguments)]
pub fn load_save(
    mut inv: ResMut<PlayerInventory>,
    mut loc: ResMut<CurrentLocation>,
    mut ticker: ResMut<UniverseTicker>,
    mut souls: ResMut<crate::systems::soul::SoulRegistry>,
    mut shipcfg: ResMut<crate::systems::shipeditor::ShipConfig>,
    mut interior_cfg: ResMut<crate::systems::shipeditor::InteriorConfig>,
    mut discovery_log: ResMut<DiscoveryLog>,
    mut roster: ResMut<crate::systems::crew::CrewRoster>,
    content: Res<crate::systems::content_index::ContentIndex>,
) {
    if let Some((i, l)) = load_player() {
        *inv = i;
        *loc = l;
    }
    if let Some(text) = crate::save_backend::read_save() {
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
            // Register the player's own soul so the avatar renders as the
            // character that was created, and record its id — the avatar used
            // to be drawn from a hardcoded canonical crew member's soul.
            if let Some(character) = &file.character {
                let soul = character.soul.clone();
                souls
                    .states
                    .entry(soul.id.clone())
                    .or_insert_with(|| reachlock_core::soul::SoulState::from_file(&soul));
                souls.player_soul_id = Some(soul.id.clone());
                souls.files.insert(soul.id.clone(), soul);
                // Put the character aboard the ship their origin grants them.
                //
                // Nothing called `set_active_ship_template`, so `ACTIVE_SHIP`
                // was never written and every character flew the neutral
                // starter hull no matter which origin they picked — while the
                // character-creation summary told them otherwise.
                apply_origin_ship(&character.origin_id, &content);
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
            // S85: restore discovery log.
            *discovery_log = file.discovery_log;
            // S80: restore crew roster from save.
            if let Some(saved_roster) = file.crew_roster {
                *roster = saved_roster;
            }
        }
    }
}

/// Select the active ship from the character's origin.
///
/// Falls back to the neutral starter when the origin is unknown or grants no
/// ship — never to another character's ship, which is what the old hardcoded
/// fallback did.
fn apply_origin_ship(
    origin_id: &str,
    content: &crate::systems::content_index::ContentIndex,
) -> bool {
    use reachlock_core::content::ContentPayload;

    let ship_id = content.files.iter().find_map(|f| match &f.payload {
        ContentPayload::Origin(o) if o.id == origin_id => o.ship_template.as_deref(),
        _ => None,
    });
    let Some(ship_id) = ship_id else {
        // No origin, or an origin that grants no ship: the starter is correct.
        crate::systems::crew::clear_active_ship();
        return false;
    };
    if crate::systems::crew::set_active_ship_template(ship_id) {
        info!("ship: {origin_id} flies '{ship_id}'");
        true
    } else {
        // `set_active_ship_template` already warned about the unknown id.
        crate::systems::crew::clear_active_ship();
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every authored origin that names a ship must resolve it. A typo in an
    /// origin's `ship_template` silently downgraded the player to the starter
    /// hull, and the only symptom was a ship that looked wrong.
    #[test]
    fn every_authored_origin_ship_resolves_to_a_template() {
        let catalog = crate::systems::crew::ship_template_catalog();
        assert!(
            !catalog.is_empty(),
            "no ship templates found — the catalog reads mods/reachlock/hulls \
             relative to the working directory"
        );
        let known: std::collections::HashSet<&str> =
            catalog.iter().map(|t| t.id.as_str()).collect();

        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("mods/reachlock/origins");
        let mut checked = 0usize;
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("origins dir").flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "ron") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read origin");
            let file: reachlock_core::content::ContentFile =
                ron::from_str(&text).expect("origin is an envelope");
            if let reachlock_core::content::ContentPayload::Origin(o) = file.payload {
                if let Some(ship) = &o.ship_template {
                    checked += 1;
                    if !known.contains(ship.as_str()) {
                        missing.push(format!("{} → {ship}", o.id));
                    }
                }
            }
        }
        assert!(checked > 0, "no origin names a ship template");
        assert!(
            missing.is_empty(),
            "origins naming a ship template nothing authors: {missing:?}"
        );
    }

    #[test]
    fn save_file_default_character_is_none() {
        let sf = SaveFile::default();
        assert!(sf.character.is_none());
    }

    #[test]
    fn save_file_round_trips_with_character() {
        let soul = reachlock_core::soul::SoulFile {
            id: "player_soul".into(),
            name: "Rook".into(),
            species: reachlock_core::soul::types::Species::Human,
            portrait_id: String::new(),
            identity: reachlock_core::soul::types::Identity {
                origin: String::new(),
                faction_affiliation: String::new(),
                role: "Captain".into(),
                public_bio: String::new(),
            },
            personality: reachlock_core::soul::types::Personality {
                traits: vec![],
                values: vec![],
                speaking_style: reachlock_core::soul::types::SpeakingStyle::Terse,
                quirks: vec![],
            },
            emotional_state: reachlock_core::soul::types::EmotionalState {
                dominant_mood: reachlock_core::soul::types::Mood::Stable,
                intensity: 512,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![],
            dialogue: None,
            deflections: vec![],
            look: None,
        };
        let pc = PlayerCharacter {
            id: reachlock_core::identity::EntityId(42),
            name: "Rook".into(),
            pronouns: "they/them".into(),
            species: "Human".into(),
            look: reachlock_core::generator::sprite::CharacterLookConfig {
                species: reachlock_core::soul::types::Species::Human,
                hair_style: Some(3),
                ..Default::default()
            },
            origin_id: "orphaned_colony".into(),
            background_id: "spacer".into(),
            soul,
        };
        let sf = SaveFile {
            character: Some(pc.clone()),
            ..Default::default()
        };
        let text = ron::to_string(&sf).unwrap();
        let back: SaveFile = ron::from_str(&text).unwrap();
        assert_eq!(back.character, Some(pc));
    }

    #[test]
    fn pre_s75_save_deserializes_with_character_none() {
        // Simulates a save from before S75 (no `character` field).
        let old_save = r#"
            SaveFile(
                inventory: (credits: 0, capacity: 100, cargo: {}, equipped_weapon: None),
                location: None,
                universe: None,
                saved_at_epoch_secs: None,
                souls: {},
                hull_config: None,
                interior_layout: None,
            )
        "#;
        let sf: SaveFile = ron::from_str(old_save).unwrap();
        assert!(sf.character.is_none());
        assert_eq!(sf.inventory.credits, 0);
    }
}
