//! Ship exterior editor (spec §19; S17). Keyboard-driven: Tab cycles tabs,
//! W/S selects a row, A/D cycles the choice, Enter applies, Esc cancels.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use reachlock_core::content::AssetType;
use reachlock_core::editor::exterior::{
    compose_hull, handling, ArmorSegment, ComposedHull, Decal, Hardpoint, HullConfiguration,
    HullFrame, ItemRef, PaintScheme, PaintSlot,
};
use reachlock_core::item::{ItemSeed, ItemType};

use crate::settings::{InputAction, Settings};
use crate::states::{GameMode, ModeScope};
use crate::systems::content_index::ContentIndex;
use crate::systems::interaction::{ActivePanel, InteractionPrompt};
use crate::systems::inventory::{save_player, PlayerInventory};
use crate::systems::ship::PlayerShip;
use crate::systems::ticker::UniverseTicker;

use super::{frame_for, ShipConfig, FRAME_IDS};

// ---------------------------------------------------------------------------
// Editor state
// ---------------------------------------------------------------------------

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

#[derive(Resource)]
pub struct ShipEditorState {
    pub draft: Option<HullConfiguration>,
    pub tab: EditorTab,
    pub sel: usize,
    pub dirty: bool,
    pub angle: f32,
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

#[derive(Component)]
pub struct ExteriorPreview;

// ---------------------------------------------------------------------------
// Debug stock + defaults
// ---------------------------------------------------------------------------

fn stock_item(seed: u64, token: &str, tier: u8) -> ItemRef {
    ItemRef(ItemSeed {
        seed,
        item_type: ItemType::from_token(token).expect("stock token is valid"),
        tier,
        faction: "compact".into(),
        biome: "frontier".into(),
    })
}

fn debug_weapons() -> Vec<ItemRef> {
    vec![
        stock_item(0xD0C0_0001, "kinetic_cannon", 2),
        stock_item(0xD0C0_0002, "energy_laser", 3),
        stock_item(0xD0C0_0003, "missile_torpedo", 4),
        stock_item(0xD0C0_0004, "kinetic_railgun", 6),
        stock_item(0xD0C0_0005, "energy_plasma", 7),
    ]
}

fn debug_engines() -> Vec<ItemRef> {
    vec![
        stock_item(0xE9E9_0001, "engine", 2),
        stock_item(0xE9E9_0002, "engine", 4),
        stock_item(0xE9E9_0003, "engine", 6),
        stock_item(0xE9E9_0004, "engine", 8),
        stock_item(0xE9E9_0005, "engine", 10),
    ]
}

const PLATING_STEPS: [(i64, &str); 4] = [
    (0, "bare"),
    (8 * 1024, "light"),
    (24 * 1024, "medium"),
    (48 * 1024, "heavy"),
];

pub fn default_config() -> HullConfiguration {
    HullConfiguration {
        hull_id: "frame_corvette".into(),
        seed: crate::systems::ship::PLAYER_HULL_SEED,
        hardpoints: vec![],
        engine: debug_engines()[1].clone(),
        plating: vec![],
        paint: PaintScheme::default(),
        decals: vec![],
    }
}

// ---------------------------------------------------------------------------
// Refit cost
// ---------------------------------------------------------------------------

const COST_FRAME: i64 = 500;
const COST_ENGINE: i64 = 300;
const COST_HARDPOINT: i64 = 150;
const COST_PLATING: i64 = 50;
const COST_PAINT: i64 = 40;
const COST_DECAL: i64 = 25;

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

// ---------------------------------------------------------------------------
// Editor system
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn editor_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    panel: Res<ActivePanel>,
    mut state: ResMut<ShipEditorState>,
    mut shipcfg: ResMut<ShipConfig>,
    mut inv: ResMut<PlayerInventory>,
    loc: Res<crate::states::CurrentLocation>,
    content: Res<ContentIndex>,
    ticker: Res<UniverseTicker>,
    souls: Res<crate::systems::soul::SoulRegistry>,
    interior_cfg: Res<super::interior::InteriorConfig>,
    ship: Query<Entity, With<PlayerShip>>,
    mut commands: Commands,
) {
    if *panel != ActivePanel::ShipExterior {
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
            draft.hardpoints.clear();
            draft.plating.clear();
            draft.decals.clear();
        }
        EditorTab::Hardpoints => {
            let Some(slot) = frame.slots.get(sel) else {
                return;
            };
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

// ---------------------------------------------------------------------------
// Orbit preview
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Panel text
// ---------------------------------------------------------------------------

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
