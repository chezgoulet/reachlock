//! Ship exterior editor (spec §19; S17). Opened from a SHIPYARD terminal
//! while docked (`InteractKind::Shipyard` → `ActivePanel::ShipExterior`).
//! Keyboard-driven like the market: Tab cycles tabs, W/S selects a row,
//! A/D cycles the choice, Enter applies (charging flat per-change refit
//! costs), Esc cancels. A live orbit preview re-renders on every change —
//! through `reachlock_core::editor::exterior::compose_hull`, the SAME
//! composition function flight mode uses (S17 gotcha: two renderers drift).
//!
//! Items come from a debug stock of S05 items until inventory-of-items
//! exists (S17 non-goal, noted in the PR).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use reachlock_core::content::{AssetType, ContentPayload};
use reachlock_core::editor::exterior::{
    compose_hull, handling, ArmorSegment, ComposedHull, Decal, Hardpoint, HullConfiguration,
    HullFrame, ItemRef, PaintScheme, PaintSlot,
};
use reachlock_core::editor::interior::{
    auto_corridors, compute_bonuses, furniture_tile, realize, template, FurnitureKind,
    PlacedFurniture, PlacedRoom, RoomTemplate, ShipInteriorLayout, CELL,
};
use reachlock_core::generator::hull::{HullClass, HullHandling};
use reachlock_core::generator::RoomKind;
use reachlock_core::item::{ItemSeed, ItemType};

use crate::settings::{InputAction, Settings};
use crate::states::{CurrentLocation, GameMode, ModeScope};
use crate::systems::comms::CommFeed;
use crate::systems::content_index::ContentIndex;
use crate::systems::crew::CrewRoster;
use crate::systems::interaction::{ActivePanel, InteractionPrompt};
use crate::systems::inventory::{save_player, PlayerInventory};
use crate::systems::ship::{PlayerShip, PLAYER_HULL_SEED};
use crate::systems::ticker::UniverseTicker;

/// The APPLIED exterior configuration — what the flight ship spawns from —
/// plus its derived handling (cached so `ship::control` doesn't recompose
/// every frame). `None` = the pre-S17 default Loup-Garou. Persisted in the
/// save (`SaveFile::hull_config`); restored by `inventory::load_save`.
#[derive(Resource, Default)]
pub struct ShipConfig {
    pub config: Option<HullConfiguration>,
    pub handling: Option<HullHandling>,
}

impl ShipConfig {
    /// Set a new applied config and re-derive its handling.
    pub fn set(&mut self, config: HullConfiguration, content: &ContentIndex) {
        let frame = frame_for(content, &config.hull_id);
        self.handling = Some(handling(&config, &frame));
        self.config = Some(config);
    }
}

/// Editor tabs, in Tab-cycle order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorTab {
    Frame,
    Hardpoints,
    Engine,
    Paint,
    Plating,
    Decals,
}

const TABS: [EditorTab; 6] = [
    EditorTab::Frame,
    EditorTab::Hardpoints,
    EditorTab::Engine,
    EditorTab::Paint,
    EditorTab::Plating,
    EditorTab::Decals,
];

/// Live editor state: the draft config being edited (`Some` while the panel
/// is open), cursor, and the preview's rebuild flag + orbit angle.
#[derive(Resource)]
pub struct ShipEditorState {
    pub draft: Option<HullConfiguration>,
    pub tab: EditorTab,
    pub sel: usize,
    /// Preview needs a rebuild (set on every draft change).
    pub dirty: bool,
    /// Orbit-camera angle (render-layer float).
    pub angle: f32,
    /// One-line status from the last apply attempt.
    pub status: String,
}

impl Default for ShipEditorState {
    fn default() -> Self {
        ShipEditorState {
            draft: None,
            tab: EditorTab::Frame,
            sel: 0,
            dirty: false,
            angle: 0.0,
            status: String::new(),
        }
    }
}

/// Marks the orbit-preview entity so rebuilds/closes can despawn it.
#[derive(Component)]
pub struct ExteriorPreview;

// ---------------------------------------------------------------------
// Frames + debug stock.
// ---------------------------------------------------------------------

/// The authored frame ids the picker cycles through, in display order.
/// Missing content falls back to `HullFrame::reference` per class, so the
/// editor works offline with an empty content directory.
const FRAME_IDS: [(&str, HullClass); 3] = [
    ("frame_shuttle", HullClass::Shuttle),
    ("frame_corvette", HullClass::Corvette),
    ("frame_freighter", HullClass::Freighter),
];

/// Resolve a frame by id: authored content first, reference fallback.
pub fn frame_for(content: &ContentIndex, hull_id: &str) -> HullFrame {
    for file in &content.files {
        if file.asset_type == AssetType::HullFrame && file.id == hull_id {
            if let ContentPayload::HullFrame(frame) = &file.payload {
                return frame.clone();
            }
        }
    }
    let class = FRAME_IDS
        .iter()
        .find(|(id, _)| *id == hull_id)
        .map(|(_, class)| *class)
        .unwrap_or(HullClass::Corvette);
    HullFrame::reference(class)
}

fn stock_item(seed: u64, token: &str, tier: u8) -> ItemRef {
    ItemRef(ItemSeed {
        seed,
        item_type: ItemType::from_token(token).expect("stock token is valid"),
        tier,
        faction: "compact".into(),
        biome: "frontier".into(),
    })
}

/// Debug weapon stock (S05 items; inventory ownership is an S17 non-goal).
fn debug_weapons() -> Vec<ItemRef> {
    vec![
        stock_item(0xD0C0_0001, "kinetic_cannon", 2),
        stock_item(0xD0C0_0002, "energy_laser", 3),
        stock_item(0xD0C0_0003, "missile_torpedo", 4),
        stock_item(0xD0C0_0004, "kinetic_railgun", 6),
        stock_item(0xD0C0_0005, "energy_plasma", 7),
    ]
}

/// Debug engine stock: one per tier band so the handling tradeoff is
/// visible in the picker.
fn debug_engines() -> Vec<ItemRef> {
    vec![
        stock_item(0xE9E9_0001, "engine", 2),
        stock_item(0xE9E9_0002, "engine", 4),
        stock_item(0xE9E9_0003, "engine", 6),
        stock_item(0xE9E9_0004, "engine", 8),
        stock_item(0xE9E9_0005, "engine", 10),
    ]
}

/// Plating mass steps a zone cycles through (whole units × 1024).
const PLATING_STEPS: [(i64, &str); 4] = [
    (0, "bare"),
    (8 * 1024, "light"),
    (24 * 1024, "medium"),
    (48 * 1024, "heavy"),
];

/// The default config a fresh editor session starts from when no exterior
/// was ever applied: the corvette frame with a mid engine, unplated,
/// default paint — seeded by the player hull seed so the silhouette matches
/// the ship you've been flying.
pub fn default_config() -> HullConfiguration {
    HullConfiguration {
        hull_id: "frame_corvette".into(),
        seed: PLAYER_HULL_SEED,
        hardpoints: vec![],
        engine: debug_engines()[1].clone(),
        plating: vec![],
        paint: PaintScheme::default(),
        decals: vec![],
    }
}

// ---------------------------------------------------------------------
// Refit cost (flat per-change credit costs — S07 credits are merged).
// ---------------------------------------------------------------------

const COST_FRAME: i64 = 500;
const COST_ENGINE: i64 = 300;
const COST_HARDPOINT: i64 = 150;
const COST_PLATING: i64 = 50;
const COST_PAINT: i64 = 40;
const COST_DECAL: i64 = 25;

/// Flat per-change refit cost between the applied config and the draft.
pub fn refit_cost(old: &HullConfiguration, new: &HullConfiguration) -> i64 {
    let mut cost = 0;
    if old.hull_id != new.hull_id {
        cost += COST_FRAME;
    }
    if old.engine != new.engine {
        cost += COST_ENGINE;
    }
    let hp = |cfg: &HullConfiguration, slot: &str| {
        cfg.hardpoints
            .iter()
            .find(|h| h.slot_id == slot)
            .map(|h| h.item.clone())
    };
    let mut slots: Vec<&String> = old
        .hardpoints
        .iter()
        .chain(&new.hardpoints)
        .map(|h| &h.slot_id)
        .collect();
    slots.sort();
    slots.dedup();
    for slot in slots {
        if hp(old, slot) != hp(new, slot) {
            cost += COST_HARDPOINT;
        }
    }
    let plate = |cfg: &HullConfiguration, zone: &str| {
        cfg.plating
            .iter()
            .find(|s| s.zone_id == zone)
            .map(|s| s.mass)
            .unwrap_or(0)
    };
    let mut zones: Vec<&String> = old
        .plating
        .iter()
        .chain(&new.plating)
        .map(|s| &s.zone_id)
        .collect();
    zones.sort();
    zones.dedup();
    for zone in zones {
        if plate(old, zone) != plate(new, zone) {
            cost += COST_PLATING;
        }
    }
    if old.paint != new.paint {
        cost += COST_PAINT;
    }
    let decal = |cfg: &HullConfiguration, slot: &str| {
        cfg.decals
            .iter()
            .find(|d| d.slot_id == slot)
            .map(|d| d.decal_id.clone())
    };
    let mut dslots: Vec<&String> = old
        .decals
        .iter()
        .chain(&new.decals)
        .map(|d| &d.slot_id)
        .collect();
    dslots.sort();
    dslots.dedup();
    for slot in dslots {
        if decal(old, slot) != decal(new, slot) {
            cost += COST_DECAL;
        }
    }
    cost
}

/// Decal choices: faction insignia, gated by reputation (S11 is merged —
/// only factions you're not in the red with will paint their mark on you).
fn decal_choices(ticker: &UniverseTicker) -> Vec<(String, String)> {
    ticker
        .state
        .factions
        .catalog
        .factions
        .iter()
        .filter(|f| ticker.state.factions.rep(&f.id).trust >= 0)
        .map(|f| (f.id.0.clone(), f.name.clone()))
        .collect()
}

// ---------------------------------------------------------------------
// Input system.
// ---------------------------------------------------------------------

/// Drive the editor from the keyboard while the panel is open. Closing the
/// panel (Esc / walking away) without Enter discards the draft — that's
/// cancel. Enter applies: charges the refit cost, persists the config to
/// the save, and despawns the flight ship so the next launch rebuilds it
/// from the new config.
#[allow(clippy::too_many_arguments)]
pub fn editor_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    panel: Res<ActivePanel>,
    mut state: ResMut<ShipEditorState>,
    mut shipcfg: ResMut<ShipConfig>,
    mut inv: ResMut<PlayerInventory>,
    loc: Res<CurrentLocation>,
    content: Res<ContentIndex>,
    ticker: Res<UniverseTicker>,
    souls: Res<crate::systems::soul::SoulRegistry>,
    interior_cfg: Res<InteriorConfig>,
    ship: Query<Entity, With<PlayerShip>>,
    mut commands: Commands,
) {
    if *panel != ActivePanel::ShipExterior {
        // Panel closed without Enter: cancel — drop the draft.
        if state.draft.is_some() {
            state.draft = None;
            state.status.clear();
        }
        return;
    }

    if state.draft.is_none() {
        state.draft = Some(shipcfg.config.clone().unwrap_or_else(default_config));
        state.tab = EditorTab::Frame;
        state.sel = 0;
        state.dirty = true;
        state.status.clear();
    }

    let baseline = shipcfg.config.clone().unwrap_or_else(default_config);
    let frame = frame_for(&content, &state.draft.as_ref().unwrap().hull_id);

    if keys.just_pressed(settings.key(InputAction::EditorTabNext)) {
        let i = TABS.iter().position(|t| *t == state.tab).unwrap_or(0);
        state.tab = TABS[(i + 1) % TABS.len()];
        state.sel = 0;
    }

    let rows = match state.tab {
        EditorTab::Frame => 1,
        EditorTab::Hardpoints => frame.slots.len().max(1),
        EditorTab::Engine => 1,
        EditorTab::Paint => 3,
        EditorTab::Plating => frame.zones.len().max(1),
        EditorTab::Decals => frame.decal_slots.len().max(1),
    };
    if keys.just_pressed(settings.key(InputAction::EditorCursorUp)) {
        state.sel = (state.sel + rows - 1) % rows;
    }
    if keys.just_pressed(settings.key(InputAction::EditorCursorDown)) {
        state.sel = (state.sel + 1) % rows;
    }

    let step: i64 = if keys.just_pressed(settings.key(InputAction::EditorCursorRight)) {
        1
    } else if keys.just_pressed(settings.key(InputAction::EditorCursorLeft)) {
        -1
    } else {
        0
    };

    if step != 0 {
        let sel = state.sel;
        let tab = state.tab;
        let draft = state.draft.as_mut().unwrap();
        cycle_choice(draft, &frame, tab, sel, step, &ticker);
        state.dirty = true;
        state.status.clear();
    }

    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        let draft = state.draft.as_ref().unwrap().clone();
        let cost = refit_cost(&baseline, &draft);
        if cost == 0 {
            state.status = "no changes to apply".into();
        } else if inv.credits < cost {
            state.status = format!("need {cost} cr — you have {}", inv.credits);
        } else {
            inv.credits -= cost;
            shipcfg.set(draft, &content);
            // The flight ship respawns from the new config on the next
            // SpaceFlight entry (setup::enter_spaceflight spawns only when
            // no PlayerShip exists).
            for entity in &ship {
                commands.entity(entity).despawn();
            }
            save_player(
                &inv,
                &loc,
                Some(&ticker.state),
                &souls.states,
                shipcfg.config.as_ref(),
                interior_cfg.layout.as_ref(),
            );
            state.status = format!("applied — {cost} cr");
        }
    }
}

/// Apply one A/D step to the selected row of the given tab.
fn cycle_choice(
    draft: &mut HullConfiguration,
    frame: &HullFrame,
    tab: EditorTab,
    sel: usize,
    step: i64,
    ticker: &UniverseTicker,
) {
    let cycle = |len: usize, current: usize| -> usize {
        (current as i64 + step).rem_euclid(len as i64) as usize
    };
    match tab {
        EditorTab::Frame => {
            let i = FRAME_IDS
                .iter()
                .position(|(id, _)| *id == draft.hull_id)
                .unwrap_or(1);
            draft.hull_id = FRAME_IDS[cycle(FRAME_IDS.len(), i)].0.into();
            // A new frame has different slots/zones — configs reference
            // them by id, so frame-specific choices reset.
            draft.hardpoints.clear();
            draft.plating.clear();
            draft.decals.clear();
        }
        EditorTab::Hardpoints => {
            let Some(slot) = frame.slots.get(sel) else {
                return;
            };
            // Choices: [empty] + the weapon stock.
            let stock = debug_weapons();
            let current = draft
                .hardpoints
                .iter()
                .position(|h| h.slot_id == slot.id)
                .map(|i| {
                    1 + stock
                        .iter()
                        .position(|w| *w == draft.hardpoints[i].item)
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let next = cycle(stock.len() + 1, current);
            draft.hardpoints.retain(|h| h.slot_id != slot.id);
            if next > 0 {
                draft.hardpoints.push(Hardpoint {
                    slot_id: slot.id.clone(),
                    item: stock[next - 1].clone(),
                    size_class: slot.size_class,
                });
            }
        }
        EditorTab::Engine => {
            let stock = debug_engines();
            let i = stock.iter().position(|e| *e == draft.engine).unwrap_or(0);
            draft.engine = stock[cycle(stock.len(), i)].clone();
        }
        EditorTab::Paint => {
            let layer = match sel {
                0 => &mut draft.paint.primary,
                1 => &mut draft.paint.secondary,
                _ => &mut draft.paint.accent,
            };
            let i = PaintSlot::ALL.iter().position(|s| s == layer).unwrap_or(0);
            *layer = PaintSlot::ALL[cycle(PaintSlot::ALL.len(), i)];
        }
        EditorTab::Plating => {
            let Some(zone) = frame.zones.get(sel) else {
                return;
            };
            let current_mass = draft
                .plating
                .iter()
                .find(|s| s.zone_id == zone.id)
                .map(|s| s.mass)
                .unwrap_or(0);
            let i = PLATING_STEPS
                .iter()
                .position(|(m, _)| *m == current_mass)
                .unwrap_or(0);
            let next = PLATING_STEPS[cycle(PLATING_STEPS.len(), i)].0;
            draft.plating.retain(|s| s.zone_id != zone.id);
            if next > 0 {
                draft.plating.push(ArmorSegment {
                    zone_id: zone.id.clone(),
                    mass: next,
                });
            }
        }
        EditorTab::Decals => {
            let Some(slot_id) = frame.decal_slots.get(sel) else {
                return;
            };
            let choices = decal_choices(ticker);
            if choices.is_empty() {
                return;
            }
            let current = draft
                .decals
                .iter()
                .find(|d| d.slot_id == *slot_id)
                .map(|d| {
                    1 + choices
                        .iter()
                        .position(|(id, _)| *id == d.decal_id)
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let next = cycle(choices.len() + 1, current);
            draft.decals.retain(|d| d.slot_id != *slot_id);
            if next > 0 {
                draft.decals.push(Decal {
                    slot_id: slot_id.clone(),
                    decal_id: choices[next - 1].0.clone(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------
// Orbit preview.
// ---------------------------------------------------------------------

/// Composed hull → a flat 2D mesh with per-region vertex colors: base hull
/// in primary, hardpoint attachments in accent, the engine nozzle in
/// secondary. Geometry comes verbatim from `compose_hull` — this only
/// converts fixed-point to render floats (bridge-layer rule).
fn preview_mesh(composed: &ComposedHull, placed: usize) -> Mesh {
    let verts = &composed.mesh.vertices;
    let total = verts.len();
    let nozzle_start = total.saturating_sub(4);
    let base_len = total.saturating_sub(4 * placed + 4);
    let lin = |c: reachlock_core::util::color::ColorRgba8| {
        Color::srgba_u8(c.r, c.g, c.b, c.a)
            .to_linear()
            .to_f32_array()
    };
    let positions: Vec<[f32; 3]> = verts
        .iter()
        .map(|v| [v.x.to_f32(), v.y.to_f32(), 0.0])
        .collect();
    let colors: Vec<[f32; 4]> = (0..total)
        .map(|i| {
            if i < base_len {
                lin(composed.paint.primary)
            } else if i < nozzle_start {
                lin(composed.paint.accent)
            } else {
                lin(composed.paint.secondary)
            }
        })
        .collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(composed.mesh.indices.clone()));
    mesh
}

/// Keep the orbit preview alive while the panel is open: rebuild it on any
/// draft change (dirty flag), spin it, and despawn it on close.
#[allow(clippy::too_many_arguments)]
pub fn editor_preview(
    time: Res<Time>,
    panel: Res<ActivePanel>,
    mut state: ResMut<ShipEditorState>,
    content: Res<ContentIndex>,
    prompt: Res<InteractionPrompt>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut preview: Query<(Entity, &mut Transform), With<ExteriorPreview>>,
    mut commands: Commands,
) {
    let open = *panel == ActivePanel::ShipExterior && state.draft.is_some();
    if !open {
        for (entity, _) in &preview {
            commands.entity(entity).despawn();
        }
        return;
    }

    // The "orbit camera": the composed hull turning under the shipyard
    // lights. Render-layer float, never gameplay state.
    state.angle = (state.angle + time.delta_secs() * 0.8).rem_euclid(std::f32::consts::TAU);
    let rotation = Quat::from_rotation_z(state.angle);

    if state.dirty || preview.is_empty() {
        state.dirty = false;
        for (entity, _) in &preview {
            commands.entity(entity).despawn();
        }
        let draft = state.draft.as_ref().unwrap();
        let frame = frame_for(&content, &draft.hull_id);
        let composed = compose_hull(draft, &frame);
        let placed = draft
            .hardpoints
            .iter()
            .filter(|h| frame.slot(&h.slot_id).is_some())
            .count();
        let anchor = prompt.anchor.unwrap_or_default();
        commands.spawn((
            ExteriorPreview,
            ModeScope(GameMode::Landed),
            Mesh2d(meshes.add(preview_mesh(&composed, placed))),
            MeshMaterial2d(materials.add(ColorMaterial::from(Color::WHITE))),
            Transform::from_xyz(anchor.x, anchor.y + 130.0, 40.0)
                .with_rotation(rotation)
                .with_scale(Vec3::splat(0.9)),
        ));
        return;
    }

    for (_, mut transform) in &mut preview {
        transform.rotation = rotation;
    }
}

// ---------------------------------------------------------------------
// Panel text (rendered by hud::update_hud_panels).
// ---------------------------------------------------------------------

/// Render the editor panel text: tabs, rows with the cursor, the draft's
/// derived handling next to the applied one, and the refit cost preview.
pub fn editor_panel_text(
    state: &ShipEditorState,
    shipcfg: &ShipConfig,
    inv: &PlayerInventory,
    content: &ContentIndex,
    ticker: &UniverseTicker,
) -> String {
    let Some(draft) = &state.draft else {
        return String::new();
    };
    let frame = frame_for(content, &draft.hull_id);
    let baseline = shipcfg.config.clone().unwrap_or_else(default_config);
    let cost = refit_cost(&baseline, draft);

    let mut lines = vec![
        "── SHIPYARD · EXTERIOR ──  Tab tab · W/S row · A/D change · Enter apply · Esc cancel"
            .to_string(),
        {
            let tabs: Vec<String> = TABS
                .iter()
                .map(|t| {
                    let name = match t {
                        EditorTab::Frame => "FRAME",
                        EditorTab::Hardpoints => "HARDPOINTS",
                        EditorTab::Engine => "ENGINE",
                        EditorTab::Paint => "PAINT",
                        EditorTab::Plating => "PLATING",
                        EditorTab::Decals => "DECALS",
                    };
                    if *t == state.tab {
                        format!("[{name}]")
                    } else {
                        name.to_string()
                    }
                })
                .collect();
            tabs.join(" ")
        },
    ];

    let cursor = |i: usize| if i == state.sel { "> " } else { "  " };
    match state.tab {
        EditorTab::Frame => {
            let display = content
                .files
                .iter()
                .find(|f| f.asset_type == AssetType::HullFrame && f.id == draft.hull_id)
                .map(|f| f.display_name.clone())
                .unwrap_or_else(|| draft.hull_id.clone());
            lines.push(format!("{}frame: {display}", cursor(0)));
            lines.push("  (changing frame clears slots/plating/decals)".to_string());
        }
        EditorTab::Hardpoints => {
            let stock = debug_weapons();
            for (i, slot) in frame.slots.iter().enumerate() {
                let fitted = draft
                    .hardpoints
                    .iter()
                    .find(|h| h.slot_id == slot.id)
                    .map(|h| h.item.generate().display_name)
                    .unwrap_or_else(|| "— empty —".into());
                lines.push(format!(
                    "{}{:<16} [{:?}]  {fitted}",
                    cursor(i),
                    slot.id,
                    slot.size_class,
                ));
            }
            lines.push(format!("  debug stock: {} weapon(s)", stock.len()));
        }
        EditorTab::Engine => {
            let engine = draft.engine.generate();
            lines.push(format!(
                "{}engine: {} (tier {})",
                cursor(0),
                engine.display_name,
                draft.engine.0.tier
            ));
        }
        EditorTab::Paint => {
            for (i, (name, slot)) in [
                ("primary", draft.paint.primary),
                ("secondary", draft.paint.secondary),
                ("accent", draft.paint.accent),
            ]
            .iter()
            .enumerate()
            {
                lines.push(format!("{}{name}: palette.{}", cursor(i), slot.label()));
            }
        }
        EditorTab::Plating => {
            for (i, zone) in frame.zones.iter().enumerate() {
                let mass = draft
                    .plating
                    .iter()
                    .find(|s| s.zone_id == zone.id)
                    .map(|s| s.mass)
                    .unwrap_or(0);
                let label = PLATING_STEPS
                    .iter()
                    .find(|(m, _)| *m == mass)
                    .map(|(_, l)| *l)
                    .unwrap_or("custom");
                lines.push(format!("{}{:<16} {label}", cursor(i), zone.id));
            }
        }
        EditorTab::Decals => {
            let choices = decal_choices(ticker);
            for (i, slot_id) in frame.decal_slots.iter().enumerate() {
                let fitted = draft
                    .decals
                    .iter()
                    .find(|d| d.slot_id == *slot_id)
                    .and_then(|d| {
                        choices
                            .iter()
                            .find(|(id, _)| *id == d.decal_id)
                            .map(|(_, name)| name.clone())
                    })
                    .unwrap_or_else(|| "— none —".into());
                lines.push(format!("{}{:<16} {fitted}", cursor(i), slot_id));
            }
            lines.push("  (faction insignia — reputation-gated)".to_string());
        }
    }

    let h = handling(draft, &frame);
    lines.push(format!(
        "handling: mass {} thrust {} turn {} burn {}/s",
        h.mass, h.thrust, h.turn_rate, h.fuel_burn
    ));
    lines.push(format!(
        "refit cost: {cost} cr   credits: {}{}",
        inv.credits,
        if state.status.is_empty() {
            String::new()
        } else {
            format!("   · {}", state.status)
        }
    ));
    lines.join("\n")
}

// =====================================================================
// S18 — Interior editor (spec §19). Same access surface as the exterior
// editor (the shipyard while docked), same resource/draft/dirty pattern,
// but the panel is a 2D grid: place room templates on the hull frame's
// cell grid, let corridors auto-route, fill furniture slots, watch the
// adjacency bonuses light up. Apply realizes the placement through
// `reachlock_core::editor::interior::realize` — the SAME function the
// On-Board scene builds from, so you walk exactly what you edited.
// =====================================================================

/// The APPLIED interior layout — what the On-Board scene realizes and
/// walks. `None` = the authored Loup-Garou deck plan (the pre-S18 ship).
/// Persisted in the save (`SaveFile::interior_layout`); restored by
/// `inventory::load_save`.
#[derive(Resource, Default)]
pub struct InteriorConfig {
    pub layout: Option<ShipInteriorLayout>,
}

/// Interior editor tabs, in Tab-cycle order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InteriorTab {
    Templates,
    Furniture,
    Corridors,
    Adjacency,
}

const INTERIOR_TABS: [InteriorTab; 4] = [
    InteriorTab::Templates,
    InteriorTab::Furniture,
    InteriorTab::Corridors,
    InteriorTab::Adjacency,
];

/// Live interior editor state: the draft placement (`Some` while the panel
/// is open), the grid cursor, and the preview's rebuild flag.
#[derive(Resource)]
pub struct InteriorEditorState {
    pub draft: Option<ShipInteriorLayout>,
    /// Grid cursor in cells. Everything about the cursor except its
    /// screen-space rendering is integer (S18 gotcha).
    pub cursor: (u8, u8),
    pub tab: InteriorTab,
    /// Selected palette row: template index (Templates tab) or furniture
    /// kind index (Furniture tab).
    pub sel: usize,
    /// Rotation for the next placement, in quarter-turns (0..=3).
    pub rotation: u8,
    /// Preview needs a rebuild (set on every draft/cursor change).
    pub dirty: bool,
    /// One-line status from the last action.
    pub status: String,
}

impl Default for InteriorEditorState {
    fn default() -> Self {
        InteriorEditorState {
            draft: None,
            cursor: (0, 0),
            tab: InteriorTab::Templates,
            sel: 0,
            rotation: 0,
            dirty: false,
            status: String::new(),
        }
    }
}

/// Marks the grid-preview entities so rebuilds/closes can despawn them.
#[derive(Component)]
pub struct InteriorPreview;

/// Resolve the room template set: authored content first
/// (`content/hulls/room_templates.ron`), reference fallback so the editor
/// works offline with an empty content directory.
pub fn templates_for(content: &ContentIndex) -> Vec<RoomTemplate> {
    for file in &content.files {
        if file.asset_type == AssetType::RoomTemplates {
            if let ContentPayload::RoomTemplates(templates) = &file.payload {
                return templates.clone();
            }
        }
    }
    RoomTemplate::reference_set()
}

/// The layout a fresh interior session starts from when none was ever
/// applied: the minimum viable ship (airlock + cockpit, adjacent) on the
/// applied exterior's frame, ready to build out.
pub fn default_interior(shipcfg: &ShipConfig) -> ShipInteriorLayout {
    let hull_id = shipcfg
        .config
        .as_ref()
        .map(|c| c.hull_id.clone())
        .unwrap_or_else(|| "frame_corvette".into());
    ShipInteriorLayout {
        hull_id,
        rooms: vec![
            PlacedRoom {
                template_id: "airlock".into(),
                position: (0, 0),
                rotation: 0,
            },
            PlacedRoom {
                template_id: "cockpit".into(),
                position: (0, 2),
                rotation: 0,
            },
        ],
        corridors: vec![],
        furniture: vec![],
        seed: PLAYER_HULL_SEED,
    }
}

// Flat per-change refit costs (same debug latitude as the exterior's).
const COST_ROOM: i64 = 120;
const COST_FURNITURE: i64 = 45;
const COST_CORRIDOR: i64 = 30;

/// Count the items of `a` missing from `b` plus the items of `b` missing
/// from `a` — each is one billable change.
fn sym_diff<T: PartialEq>(a: &[T], b: &[T]) -> i64 {
    let only = |x: &[T], y: &[T]| x.iter().filter(|item| !y.contains(item)).count() as i64;
    only(a, b) + only(b, a)
}

/// Flat per-change refit cost between the applied layout and the draft.
/// `None` applied = the authored ship: every draft room is a change.
pub fn interior_refit_cost(old: Option<&ShipInteriorLayout>, new: &ShipInteriorLayout) -> i64 {
    let empty = ShipInteriorLayout {
        hull_id: new.hull_id.clone(),
        rooms: vec![],
        corridors: vec![],
        furniture: vec![],
        seed: new.seed,
    };
    let old = old.unwrap_or(&empty);
    sym_diff(&old.rooms, &new.rooms) * COST_ROOM
        + sym_diff(&old.furniture, &new.furniture) * COST_FURNITURE
        + sym_diff(&old.corridors, &new.corridors) * COST_CORRIDOR
}

/// The duty room a crew member should hold after a layout change: `None` =
/// their current duty kind still exists, keep it; `Some(kind)` = remap to
/// the first available (non-corridor) room. Pure — unit-tested.
pub fn remap_duty(duty: RoomKind, kinds: &[RoomKind]) -> Option<RoomKind> {
    if kinds.contains(&duty) {
        return None;
    }
    kinds.iter().copied().find(|k| *k != RoomKind::Corridor)
}

/// The index of the draft room whose footprint contains the cursor cell.
fn room_at_cursor(
    draft: &ShipInteriorLayout,
    templates: &[RoomTemplate],
    cursor: (u8, u8),
) -> Option<usize> {
    draft.rooms.iter().position(|p| {
        template(templates, &p.template_id).is_some_and(|t| {
            let (fw, fh) = p.footprint(t);
            cursor.0 >= p.position.0
                && cursor.0 < p.position.0 + fw
                && cursor.1 >= p.position.1
                && cursor.1 < p.position.1 + fh
        })
    })
}

/// Drive the interior editor from the keyboard while the panel is open.
/// Closing the panel without Space discards the draft — that's cancel.
/// Space applies: validates through `realize`, charges the refit cost,
/// persists the layout to the save, remaps crew duty rooms by kind, and
/// lets the next boarding rebuild On-Board from the new layout (the scene
/// registry rebuilds whenever the target mode differs).
#[allow(clippy::too_many_arguments)]
pub fn interior_editor_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    panel: Res<ActivePanel>,
    mut state: ResMut<InteriorEditorState>,
    mut interior_cfg: ResMut<InteriorConfig>,
    shipcfg: Res<ShipConfig>,
    mut inv: ResMut<PlayerInventory>,
    mut roster: ResMut<CrewRoster>,
    mut comms: ResMut<CommFeed>,
    loc: Res<CurrentLocation>,
    content: Res<ContentIndex>,
    ticker: Res<UniverseTicker>,
    souls: Res<crate::systems::soul::SoulRegistry>,
) {
    if *panel != ActivePanel::ShipInterior {
        // Panel closed without Space: cancel — drop the draft.
        if state.draft.is_some() {
            state.draft = None;
            state.status.clear();
        }
        return;
    }

    let templates = templates_for(&content);
    if state.draft.is_none() {
        state.draft = Some(
            interior_cfg
                .layout
                .clone()
                .unwrap_or_else(|| default_interior(&shipcfg)),
        );
        state.cursor = (0, 0);
        state.tab = InteriorTab::Templates;
        state.sel = 0;
        state.rotation = 0;
        state.dirty = true;
        state.status.clear();
    }

    let bounds = frame_for(&content, &state.draft.as_ref().unwrap().hull_id).grid_bounds;

    if keys.just_pressed(settings.key(InputAction::EditorTabNext)) {
        let i = INTERIOR_TABS
            .iter()
            .position(|t| *t == state.tab)
            .unwrap_or(0);
        state.tab = INTERIOR_TABS[(i + 1) % INTERIOR_TABS.len()];
        state.sel = 0;
        state.dirty = true;
    }

    // Arrow keys: the grid cursor (integer cells, clamped to the frame).
    let mut cursor = state.cursor;
    if keys.just_pressed(settings.key(InputAction::EditorCursorUp)) {
        cursor.1 = (cursor.1 + 1).min(bounds.1.saturating_sub(1));
    }
    if keys.just_pressed(settings.key(InputAction::EditorCursorDown)) {
        cursor.1 = cursor.1.saturating_sub(1);
    }
    if keys.just_pressed(settings.key(InputAction::EditorCursorRight)) {
        cursor.0 = (cursor.0 + 1).min(bounds.0.saturating_sub(1));
    }
    if keys.just_pressed(settings.key(InputAction::EditorCursorLeft)) {
        cursor.0 = cursor.0.saturating_sub(1);
    }
    if cursor != state.cursor {
        state.cursor = cursor;
        state.dirty = true;
    }

    // A/D: cycle the palette row for the active tab.
    let step: i64 = if keys.just_pressed(settings.key(InputAction::EditorCursorRight)) {
        1
    } else if keys.just_pressed(settings.key(InputAction::EditorCursorLeft)) {
        -1
    } else {
        0
    };
    if step != 0 {
        let rows = match state.tab {
            InteriorTab::Templates => templates.len(),
            InteriorTab::Furniture => FurnitureKind::ALL.len(),
            _ => 0,
        };
        if rows > 0 {
            state.sel = (state.sel as i64 + step).rem_euclid(rows as i64) as usize;
            state.dirty = true;
        }
    }

    // E: rotate the next placement a quarter-turn.
    if keys.just_pressed(settings.key(InputAction::EditorRotate))
        && state.tab == InteriorTab::Templates
    {
        state.rotation = (state.rotation + 1) % 4;
        state.dirty = true;
        state.status.clear();
    }

    // Enter: place (Templates), toggle furniture (Furniture), re-route
    // (Corridors).
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        let sel = state.sel;
        let tab = state.tab;
        let rotation = state.rotation;
        let cursor = state.cursor;
        let draft = state.draft.as_mut().unwrap();
        match tab {
            InteriorTab::Templates => {
                if let Some(tpl) = templates.get(sel) {
                    draft.rooms.push(PlacedRoom {
                        template_id: tpl.id.clone(),
                        position: cursor,
                        rotation,
                    });
                    draft.corridors = auto_corridors(&draft.rooms, &templates);
                    state.dirty = true;
                    state.status.clear();
                }
            }
            InteriorTab::Furniture => {
                let kind = FurnitureKind::ALL[sel.min(FurnitureKind::ALL.len() - 1)];
                match room_at_cursor(draft, &templates, cursor) {
                    None => state.status = "no room under the cursor".into(),
                    Some(room_idx) => {
                        // Toggle: remove this kind from the room if placed,
                        // else fill the first free slot with it.
                        if let Some(i) = draft
                            .furniture
                            .iter()
                            .position(|f| f.room_idx == room_idx && f.kind == kind)
                        {
                            draft.furniture.remove(i);
                            state.dirty = true;
                            state.status.clear();
                        } else {
                            let tpl = template(&templates, &draft.rooms[room_idx].template_id)
                                .expect("placed rooms reference known templates");
                            let free = tpl.furniture_slots.iter().find(|slot| {
                                !draft
                                    .furniture
                                    .iter()
                                    .any(|f| f.room_idx == room_idx && f.slot_id == **slot)
                            });
                            match free {
                                None => state.status = "no free furniture slot here".into(),
                                Some(slot) => {
                                    draft.furniture.push(PlacedFurniture {
                                        slot_id: slot.clone(),
                                        room_idx,
                                        kind,
                                    });
                                    state.dirty = true;
                                    state.status.clear();
                                }
                            }
                        }
                    }
                }
            }
            InteriorTab::Corridors => {
                draft.corridors = auto_corridors(&draft.rooms, &templates);
                state.dirty = true;
                state.status = "corridors re-routed".into();
            }
            InteriorTab::Adjacency => {}
        }
    }

    // Backspace: remove the room under the cursor (furniture indices
    // shift down past the removed room).
    if keys.just_pressed(settings.key(InputAction::EditorDelete)) {
        let cursor = state.cursor;
        let draft = state.draft.as_mut().unwrap();
        if let Some(index) = room_at_cursor(draft, &templates, cursor) {
            draft.rooms.remove(index);
            draft.furniture.retain(|f| f.room_idx != index);
            for f in &mut draft.furniture {
                if f.room_idx > index {
                    f.room_idx -= 1;
                }
            }
            draft.corridors = auto_corridors(&draft.rooms, &templates);
            state.dirty = true;
            state.status.clear();
        }
    }

    // Space: apply.
    if keys.just_pressed(settings.key(InputAction::Brake)) {
        let draft = state.draft.as_ref().unwrap().clone();
        match realize(&draft, &templates, bounds) {
            Err(e) => state.status = format!("cannot apply: {e}"),
            Ok(realized) => {
                let cost = interior_refit_cost(interior_cfg.layout.as_ref(), &draft);
                if cost == 0 {
                    state.status = "no changes to apply".into();
                } else if inv.credits < cost {
                    state.status = format!("need {cost} cr — you have {}", inv.credits);
                } else {
                    inv.credits -= cost;
                    // Crew duty rooms remap by kind: keep a duty whose room
                    // still exists, else the first available room. Custom
                    // layouts are single-deck — everyone lives on deck 0.
                    let kinds: Vec<RoomKind> = realized.rooms.iter().map(|r| r.kind).collect();
                    for m in &mut roster.members {
                        if let Some(new_duty) = remap_duty(m.duty_room, &kinds) {
                            comms.say(
                                "SHIP",
                                format!(
                                    "Crew notice: {} reassigned to {:?} (refit removed {:?}).",
                                    m.name, new_duty, m.duty_room
                                ),
                            );
                            m.duty_room = new_duty;
                            m.order = None;
                        }
                        if !kinds.contains(&m.current_room) {
                            m.current_room = m.duty_room;
                        }
                        m.deck = 0;
                    }
                    interior_cfg.layout = Some(draft);
                    // The On-Board scene rebuilds from the new layout on the
                    // next boarding (enter_interior rebuilds whenever the
                    // scene registry doesn't already hold OnBoard).
                    save_player(
                        &inv,
                        &loc,
                        Some(&ticker.state),
                        &souls.states,
                        shipcfg.config.as_ref(),
                        interior_cfg.layout.as_ref(),
                    );
                    state.status = format!("applied — {cost} cr");
                }
            }
        }
    }
}

/// Per-room validity for the grid preview: a room is invalid when it runs
/// out of bounds or overlaps another room (corridors aside — those show as
/// missing connections instead).
fn invalid_rooms(
    draft: &ShipInteriorLayout,
    templates: &[RoomTemplate],
    bounds: (u8, u8),
) -> Vec<bool> {
    let rects: Vec<Option<(i32, i32, i32, i32)>> = draft
        .rooms
        .iter()
        .map(|p| {
            template(templates, &p.template_id).map(|t| {
                let (fw, fh) = p.footprint(t);
                (
                    p.position.0 as i32,
                    p.position.1 as i32,
                    fw as i32,
                    fh as i32,
                )
            })
        })
        .collect();
    let overlaps = |a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)| {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    };
    rects
        .iter()
        .enumerate()
        .map(|(i, rect)| match rect {
            None => true,
            Some(r) => {
                r.0 + r.2 > bounds.0 as i32
                    || r.1 + r.3 > bounds.1 as i32
                    || rects
                        .iter()
                        .enumerate()
                        .any(|(j, o)| i != j && o.is_some_and(|o| overlaps(*r, o)))
            }
        })
        .collect()
}

/// Preview cell size in screen px (render-layer float — the one place the
/// editor leaves integer space).
const PREVIEW_CELL: f32 = 13.0;

/// Keep the 2D grid preview alive while the panel is open: rebuild on any
/// draft/cursor change (dirty flag), despawn on close. Rooms draw as
/// border + fill rects (green border = valid, red = invalid), corridors
/// and doors from the realized layout, furniture as accent dots, the
/// cursor as a white cell.
#[allow(clippy::too_many_arguments)]
pub fn interior_editor_preview(
    panel: Res<ActivePanel>,
    mut state: ResMut<InteriorEditorState>,
    content: Res<ContentIndex>,
    prompt: Res<InteractionPrompt>,
    preview: Query<Entity, With<InteriorPreview>>,
    mut commands: Commands,
) {
    let open = *panel == ActivePanel::ShipInterior && state.draft.is_some();
    if !open {
        for entity in &preview {
            commands.entity(entity).despawn();
        }
        return;
    }
    if !state.dirty && !preview.is_empty() {
        return;
    }
    state.dirty = false;
    for entity in &preview {
        commands.entity(entity).despawn();
    }

    let draft = state.draft.as_ref().unwrap();
    let templates = templates_for(&content);
    let bounds = frame_for(&content, &draft.hull_id).grid_bounds;
    let anchor = prompt.anchor.unwrap_or_default();
    let origin = anchor
        + Vec2::new(
            -(bounds.0 as f32) * PREVIEW_CELL * 0.5,
            80.0 - (bounds.1 as f32) * PREVIEW_CELL * 0.5,
        );
    let rect = |commands: &mut Commands, cx: f32, cy: f32, w: f32, h: f32, color: Color, z: f32| {
        commands.spawn((
            InteriorPreview,
            ModeScope(GameMode::Landed),
            Sprite::from_color(color, Vec2::new(w, h)),
            Transform::from_xyz(origin.x + cx, origin.y + cy, z),
        ));
    };
    let cell_center = |x: f32, y: f32, w: f32, h: f32| {
        ((x + w * 0.5) * PREVIEW_CELL, (y + h * 0.5) * PREVIEW_CELL)
    };

    // The hull grid backdrop.
    let (bx, by) = cell_center(0.0, 0.0, bounds.0 as f32, bounds.1 as f32);
    rect(
        &mut commands,
        bx,
        by,
        bounds.0 as f32 * PREVIEW_CELL + 4.0,
        bounds.1 as f32 * PREVIEW_CELL + 4.0,
        Color::srgba(0.08, 0.1, 0.14, 0.92),
        40.0,
    );

    // Placed rooms: validity border + kind fill.
    let invalid = invalid_rooms(draft, &templates, bounds);
    for (i, placed) in draft.rooms.iter().enumerate() {
        let Some(tpl) = template(&templates, &placed.template_id) else {
            continue;
        };
        let (fw, fh) = placed.footprint(tpl);
        let (cx, cy) = cell_center(
            placed.position.0 as f32,
            placed.position.1 as f32,
            fw as f32,
            fh as f32,
        );
        let border = if invalid[i] {
            Color::srgb(0.9, 0.25, 0.2)
        } else {
            Color::srgb(0.3, 0.85, 0.4)
        };
        let (w, h) = (fw as f32 * PREVIEW_CELL, fh as f32 * PREVIEW_CELL);
        rect(&mut commands, cx, cy, w, h, border, 40.1);
        rect(
            &mut commands,
            cx,
            cy,
            w - 3.0,
            h - 3.0,
            crate::systems::interior::editor_room_color(tpl.kind),
            40.2,
        );
    }

    // Corridors + doors, from the realized layout when the draft is valid.
    if let Ok(realized) = realize(draft, &templates, bounds) {
        for room in realized
            .rooms
            .iter()
            .filter(|r| r.kind == RoomKind::Corridor)
        {
            let (cx, cy) = cell_center(
                room.x as f32 / CELL as f32,
                room.y as f32 / CELL as f32,
                room.width as f32 / CELL as f32,
                room.height as f32 / CELL as f32,
            );
            rect(
                &mut commands,
                cx,
                cy,
                room.width as f32 / CELL as f32 * PREVIEW_CELL - 3.0,
                room.height as f32 / CELL as f32 * PREVIEW_CELL - 3.0,
                Color::srgb(0.5, 0.53, 0.58),
                40.2,
            );
        }
        for door in &realized.doors {
            rect(
                &mut commands,
                door.x as f32 / CELL as f32 * PREVIEW_CELL,
                door.y as f32 / CELL as f32 * PREVIEW_CELL,
                5.0,
                5.0,
                Color::srgb(0.95, 0.85, 0.4),
                40.3,
            );
        }
    }

    // Furniture: an accent dot on its slot tile.
    for piece in &draft.furniture {
        let Some(placed) = draft.rooms.get(piece.room_idx) else {
            continue;
        };
        let Some(tpl) = template(&templates, &placed.template_id) else {
            continue;
        };
        if let Some((tx, ty)) = furniture_tile(placed, tpl, &piece.slot_id) {
            let (cx, cy) = cell_center(tx as f32, ty as f32, 1.0, 1.0);
            rect(
                &mut commands,
                cx,
                cy,
                6.0,
                6.0,
                Color::srgb(0.95, 0.6, 0.25),
                40.4,
            );
        }
    }

    // The cursor cell.
    let (cx, cy) = cell_center(state.cursor.0 as f32, state.cursor.1 as f32, 1.0, 1.0);
    rect(
        &mut commands,
        cx,
        cy,
        PREVIEW_CELL - 2.0,
        PREVIEW_CELL - 2.0,
        Color::srgba(1.0, 1.0, 1.0, 0.55),
        40.5,
    );
}

/// Render the interior editor panel text: tabs, the active palette with
/// the selection cursor, grid/cursor state, the bonus summary, and the
/// refit cost preview. Rendered by `hud::update_hud_panels`.
pub fn interior_panel_text(
    state: &InteriorEditorState,
    interior_cfg: &InteriorConfig,
    inv: &PlayerInventory,
    content: &ContentIndex,
) -> String {
    let Some(draft) = &state.draft else {
        return String::new();
    };
    let templates = templates_for(content);
    let bounds = frame_for(content, &draft.hull_id).grid_bounds;
    let cost = interior_refit_cost(interior_cfg.layout.as_ref(), draft);

    let mut lines = vec![
        "── SHIPYARD · INTERIOR ──  Tab tab · arrows cursor · A/D select · E rotate · \
         Enter place/toggle · Backspace remove · Space apply · Esc cancel"
            .to_string(),
        {
            let tabs: Vec<String> = INTERIOR_TABS
                .iter()
                .map(|t| {
                    let name = match t {
                        InteriorTab::Templates => "TEMPLATES",
                        InteriorTab::Furniture => "FURNITURE",
                        InteriorTab::Corridors => "CORRIDORS",
                        InteriorTab::Adjacency => "ADJACENCY",
                    };
                    if *t == state.tab {
                        format!("[{name}]")
                    } else {
                        name.to_string()
                    }
                })
                .collect();
            tabs.join(" ")
        },
        format!(
            "grid {}x{} · cursor ({}, {}) · {} room(s), {} corridor(s), {} furniture",
            bounds.0,
            bounds.1,
            state.cursor.0,
            state.cursor.1,
            draft.rooms.len(),
            draft.corridors.len(),
            draft.furniture.len(),
        ),
    ];

    match state.tab {
        InteriorTab::Templates => {
            let tpl = templates.get(state.sel);
            let name = tpl.map(|t| t.label.as_str()).unwrap_or("—");
            let size = tpl
                .map(|t| format!("{}x{}", t.width, t.height))
                .unwrap_or_default();
            lines.push(format!(
                "> template: {name} ({size} cells, rot {}°)",
                state.rotation as u32 * 90
            ));
            if let Some(t) = tpl {
                if !t.required_systems.is_empty() {
                    lines.push(format!("  requires: {}", t.required_systems.join(", ")));
                }
                if !t.furniture_slots.is_empty() {
                    lines.push(format!("  slots: {}", t.furniture_slots.join(", ")));
                }
            }
        }
        InteriorTab::Furniture => {
            let kind = FurnitureKind::ALL[state.sel.min(FurnitureKind::ALL.len() - 1)];
            lines.push(format!("> furniture: {}", kind.label()));
            let stats: Vec<String> = kind
                .stat_contributions()
                .iter()
                .map(|(k, v)| format!("{k:?} +{}", v / 1024))
                .collect();
            lines.push(format!("  contributes: {}", stats.join(", ")));
            lines.push("  Enter toggles this kind in the room under the cursor".into());
        }
        InteriorTab::Corridors => {
            lines.push(format!(
                "auto-routed corridors: {} (recomputed on every placement; Enter re-routes)",
                draft.corridors.len()
            ));
            for c in &draft.corridors {
                lines.push(format!(
                    "  ({},{}) → ({},{})",
                    c.from.0, c.from.1, c.to.0, c.to.1
                ));
            }
        }
        InteriorTab::Adjacency => {
            let bonuses = compute_bonuses(draft, &templates);
            let mark = |on: bool| if on { "ACTIVE" } else { "—" };
            lines.push(format!(
                "galley + quarters (relationship recovery): {}",
                mark(bonuses.galley_quarters_bonus)
            ));
            lines.push(format!(
                "engineering + cargo (repair transfer): {}",
                mark(bonuses.engineering_cargo_bonus)
            ));
            lines.push("  (bonus numbers are inert until their systems land)".into());
        }
    }

    match realize(draft, &templates, bounds) {
        Ok(_) => lines.push("layout: VALID — walkable from the airlock".into()),
        Err(e) => lines.push(format!("layout: INVALID — {e}")),
    }
    lines.push(format!(
        "refit cost: {cost} cr   credits: {}{}",
        inv.credits,
        if state.status.is_empty() {
            String::new()
        } else {
            format!("   · {}", state.status)
        }
    ));
    lines.join("\n")
}

#[cfg(test)]
mod interior_editor {
    use super::*;

    #[test]
    fn remap_keeps_an_existing_duty() {
        let kinds = [RoomKind::Cockpit, RoomKind::Reactor, RoomKind::Hangar];
        assert_eq!(remap_duty(RoomKind::Reactor, &kinds), None);
    }

    #[test]
    fn remap_moves_a_lost_duty_to_the_first_available_room() {
        let kinds = [RoomKind::Corridor, RoomKind::Cockpit, RoomKind::Hangar];
        assert_eq!(
            remap_duty(RoomKind::Reactor, &kinds),
            Some(RoomKind::Cockpit),
            "corridors don't count as duty rooms"
        );
    }

    #[test]
    fn refit_cost_charges_per_change() {
        let base = default_interior(&ShipConfig::default());
        assert_eq!(interior_refit_cost(Some(&base), &base), 0);
        // From nothing applied: both starter rooms are billed.
        assert_eq!(interior_refit_cost(None, &base), 2 * COST_ROOM);
        let mut grown = base.clone();
        grown.rooms.push(PlacedRoom {
            template_id: "galley".into(),
            position: (4, 0),
            rotation: 0,
        });
        grown.furniture.push(PlacedFurniture {
            slot_id: "galley".into(),
            room_idx: 2,
            kind: FurnitureKind::GalleyUnit,
        });
        assert_eq!(
            interior_refit_cost(Some(&base), &grown),
            COST_ROOM + COST_FURNITURE
        );
    }

    #[test]
    fn default_interior_is_a_valid_minimum_ship() {
        let layout = default_interior(&ShipConfig::default());
        let templates = RoomTemplate::reference_set();
        assert!(realize(&layout, &templates, (16, 12)).is_ok());
    }
}
