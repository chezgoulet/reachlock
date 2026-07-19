//! Ship interior editor (spec §19; S18). Keyboard-driven: Tab cycles tabs,
//! arrows move the grid cursor, A/D selects a palette row, Enter places or
//! toggles furniture, Space applies, Esc cancels.

use bevy::prelude::*;

use reachlock_core::content::{AssetType, ContentPayload};
use reachlock_core::editor::interior::{
    auto_corridors, compute_bonuses, furniture_tile, realize, template, FurnitureKind,
    PlacedFurniture, PlacedRoom, RoomTemplate, ShipInteriorLayout, CELL,
};
use reachlock_core::generator::RoomKind;

use crate::settings::{InputAction, Settings};
use crate::states::{GameMode, ModeScope};
use crate::systems::comms::CommFeed;
use crate::systems::content_index::ContentIndex;
use crate::systems::crew::CrewRoster;
use crate::systems::interaction::{ActivePanel, InteractionPrompt};
use crate::systems::inventory::{save_player, PlayerInventory};
use crate::systems::ticker::UniverseTicker;

use super::{frame_for, ShipConfig};

// ---------------------------------------------------------------------------
// Editor state
// ---------------------------------------------------------------------------

/// The APPLIED interior layout — what the On-Board scene realizes and walks.
#[derive(Resource, Default)]
pub struct InteriorConfig {
    pub layout: Option<ShipInteriorLayout>,
}

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

#[derive(Resource)]
pub struct InteriorEditorState {
    pub draft: Option<ShipInteriorLayout>,
    pub cursor: (u8, u8),
    pub tab: InteriorTab,
    pub sel: usize,
    pub rotation: u8,
    pub dirty: bool,
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

#[derive(Component)]
pub struct InteriorPreview;

// ---------------------------------------------------------------------------
// Templates + defaults
// ---------------------------------------------------------------------------

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
        seed: crate::systems::ship::PLAYER_HULL_SEED,
    }
}

// ---------------------------------------------------------------------------
// Refit cost
// ---------------------------------------------------------------------------

const COST_ROOM: i64 = 120;
const COST_FURNITURE: i64 = 45;
const COST_CORRIDOR: i64 = 30;

fn sym_diff<T: PartialEq>(a: &[T], b: &[T]) -> i64 {
    let only = |x: &[T], y: &[T]| x.iter().filter(|item| !y.contains(item)).count() as i64;
    only(a, b) + only(b, a)
}

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

pub fn remap_duty(duty: RoomKind, kinds: &[RoomKind]) -> Option<RoomKind> {
    if kinds.contains(&duty) {
        return None;
    }
    kinds.iter().copied().find(|k| *k != RoomKind::Corridor)
}

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

// ---------------------------------------------------------------------------
// Editor system
// ---------------------------------------------------------------------------

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
    loc: Res<crate::states::CurrentLocation>,
    content: Res<ContentIndex>,
    ticker: Res<UniverseTicker>,
    souls: Res<crate::systems::soul::SoulRegistry>,
) {
    if *panel != ActivePanel::ShipInterior {
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

    if keys.just_pressed(settings.key(InputAction::EditorRotate))
        && state.tab == InteriorTab::Templates
    {
        state.rotation = (state.rotation + 1) % 4;
        state.dirty = true;
        state.status.clear();
    }

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

// ---------------------------------------------------------------------------
// Grid preview
// ---------------------------------------------------------------------------

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

const PREVIEW_CELL: f32 = 13.0;

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

// ---------------------------------------------------------------------------
// Panel text
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
