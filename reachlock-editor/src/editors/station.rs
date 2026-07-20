//! Station editor (handoff §8): a station's exterior mesh, interior room
//! layout, and NPC spawns. Edits the `ContentFile` envelope wrapping
//! `ContentPayload::Station { exterior, layout, npc_spawns }`.

use reachlock_core::content::{AssetType, ContentFile, ContentPayload, NpcSpawn, Priority};
use reachlock_core::generator::station::{generate_station, StationKind};
use reachlock_core::generator::{Door, Room, RoomKind};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};
use super::room_templates::ROOM_KINDS;

const PRIORITIES: [Priority; 4] = [
    Priority::Procedural,
    Priority::Curated,
    Priority::Event,
    Priority::Authoritative,
];

struct Entry {
    file: ContentFile,
    path: Option<std::path::PathBuf>,
    dirty: bool,
}

pub struct StationEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_file() -> ContentFile {
    let station = generate_station(42, StationKind::Trade, 1);
    ContentFile {
        id: "new_station".into(),
        display_name: "New Station".into(),
        asset_type: AssetType::Station,
        seed: 42,
        universe: "all".into(),
        priority: Priority::Curated,
        expires_at: None,
        payload: ContentPayload::Station {
            exterior: station.exterior,
            layout: station.layout,
            npc_spawns: Vec::new(),
        },
    }
}

impl StationEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) =
            std::fs::read_dir(crate::app::content_root().join(ContentType::Station.directory()))
        {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(file) = crate::io::read_ron::<ContentFile>(&path) {
                    if matches!(file.payload, ContentPayload::Station { .. }) {
                        entries.push(Entry {
                            file,
                            path: Some(path),
                            dirty: false,
                        });
                    }
                }
            }
        }
        if entries.is_empty() {
            entries.push(Entry {
                file: blank_file(),
                path: None,
                dirty: true,
            });
        }
        StationEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

impl Editor for StationEditor {
    fn title(&self) -> &str {
        "Station Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Station
    }

    fn touch(&mut self) {
        self.has_changes = true;
        if let Some(e) = self.entries.get_mut(self.selected) {
            e.dirty = true;
        }
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let file: ContentFile = crate::io::read_ron(path)?;
        if !matches!(file.payload, ContentPayload::Station { .. }) {
            return Err(format!("{} is not a station file", path.display()));
        }
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.path.as_deref() == Some(path))
        {
            self.entries[i].file = file;
            self.selected = i;
        } else {
            self.entries.push(Entry {
                file,
                path: Some(path.to_path_buf()),
                dirty: false,
            });
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let entry = self
            .entries
            .get(self.selected)
            .ok_or_else(|| "no station selected".to_string())?;
        crate::io::write_ron(path, &entry.file)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(entry) = self.entries.get(self.selected) else {
            return errors;
        };
        if entry.file.id.is_empty() {
            errors.push("id must not be empty".into());
        }
        if entry.file.display_name.is_empty() {
            errors.push("display_name must not be empty".into());
        }
        if let ContentPayload::Station {
            layout, npc_spawns, ..
        } = &entry.file.payload
        {
            if layout.rooms.is_empty() {
                errors.push("layout must have at least one room".into());
            }
            let count = layout.rooms.len();
            for (i, door) in layout.doors.iter().enumerate() {
                if door.from as usize >= count || door.to as usize >= count {
                    errors.push(format!("door {i}: room index out of range"));
                }
            }
            for (i, spawn) in npc_spawns.iter().enumerate() {
                if spawn.room_index >= count {
                    errors.push(format!("npc {i}: room_index out of range"));
                }
                if spawn.name.is_empty() {
                    errors.push(format!("npc {i}: name must not be empty"));
                }
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("station_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self
                        .entries
                        .get(self.selected)
                        .map(|e| e.file.seed)
                        .unwrap_or(42);
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.entries.push(Entry {
                        file: blank_file(),
                        path: None,
                        dirty: true,
                    });
                    self.selected = self.entries.len() - 1;
                    self.touch();
                }
                if let Some(entry) = self.entries.get(self.selected) {
                    let name = entry
                        .path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "(unsaved)".into());
                    ui.label(name);
                    if self.has_changes {
                        ui.label("*");
                    }
                }
            });
        });

        egui::SidePanel::left("station_list")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.search);
                });
                ui.separator();
                let needle = self.search.to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for i in 0..self.entries.len() {
                        let label = self.entries[i].file.display_name.clone();
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        if ui.selectable_label(self.selected == i, &label).clicked() {
                            self.selected = i;
                        }
                    }
                });
            });

        let validation = self.validate();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(entry) = self.entries.get_mut(self.selected) else {
                ui.label("No station selected.");
                return;
            };
            let mut changed = false;
            let mut regen_exterior = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Envelope — identity and priority")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("station_envelope").show(ui, |ui| {
                            ui.label("ID:");
                            changed |= ui.text_edit_singleline(&mut entry.file.id).changed();
                            ui.end_row();
                            ui.label("Display Name:");
                            changed |= ui
                                .text_edit_singleline(&mut entry.file.display_name)
                                .changed();
                            ui.end_row();
                            ui.label("Seed:");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut entry.file.seed)
                                        .range(0..=((1u64 << 53) - 1)),
                                )
                                .changed();
                            ui.end_row();
                            ui.label("Universe:");
                            changed |= ui.text_edit_singleline(&mut entry.file.universe).changed();
                            ui.end_row();
                            ui.label("Priority:");
                            egui::ComboBox::from_id_salt("station_priority")
                                .selected_text(format!("{:?}", entry.file.priority))
                                .show_ui(ui, |ui| {
                                    for p in PRIORITIES {
                                        changed |= ui
                                            .selectable_value(
                                                &mut entry.file.priority,
                                                p,
                                                format!("{p:?}"),
                                            )
                                            .changed();
                                    }
                                });
                            ui.end_row();
                        });
                    });

                let ContentPayload::Station {
                    exterior,
                    layout,
                    npc_spawns,
                } = &mut entry.file.payload
                else {
                    ui.colored_label(egui::Color32::RED, "payload is not a station");
                    return;
                };

                egui::CollapsingHeader::new("Exterior — hull mesh (read-only)")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(format!(
                            "{} vertices, {} indices",
                            exterior.vertices.len(),
                            exterior.indices.len()
                        ));
                        if !exterior.vertices.is_empty() {
                            let (min, max) = exterior.vertices.iter().fold(
                                ((i64::MAX, i64::MAX), (i64::MIN, i64::MIN)),
                                |(min, max), v| {
                                    (
                                        (min.0.min(v.x.0), min.1.min(v.y.0)),
                                        (max.0.max(v.x.0), max.1.max(v.y.0)),
                                    )
                                },
                            );
                            ui.label(format!(
                                "bounding box: ({:.1}, {:.1}) — ({:.1}, {:.1})",
                                min.0 as f32 / 1024.0,
                                min.1 as f32 / 1024.0,
                                max.0 as f32 / 1024.0,
                                max.1 as f32 / 1024.0,
                            ));
                        }
                        if ui.button("Regenerate Exterior").clicked() {
                            regen_exterior = true;
                        }
                    });

                egui::CollapsingHeader::new("Layout — rooms and doors")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove_room: Option<usize> = None;
                        for (i, room) in layout.rooms.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{i}:"));
                                egui::ComboBox::from_id_salt(("station_room_kind", i))
                                    .selected_text(format!("{:?}", room.kind))
                                    .show_ui(ui, |ui| {
                                        for kind in ROOM_KINDS {
                                            changed |= ui
                                                .selectable_value(
                                                    &mut room.kind,
                                                    kind,
                                                    format!("{kind:?}"),
                                                )
                                                .changed();
                                        }
                                    });
                                changed |= ui
                                    .add(egui::DragValue::new(&mut room.x).prefix("x: "))
                                    .changed();
                                changed |= ui
                                    .add(egui::DragValue::new(&mut room.y).prefix("y: "))
                                    .changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut room.width)
                                            .range(1..=64)
                                            .prefix("w: "),
                                    )
                                    .changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut room.height)
                                            .range(1..=64)
                                            .prefix("h: "),
                                    )
                                    .changed();
                                if ui.button("×").clicked() {
                                    remove_room = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_room {
                            layout.rooms.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Room").clicked() {
                            layout.rooms.push(Room {
                                kind: RoomKind::Quarters,
                                x: 0,
                                y: 0,
                                width: 4,
                                height: 3,
                            });
                            changed = true;
                        }

                        ui.separator();
                        ui.label("Doors:");
                        let room_count = layout.rooms.len() as u32;
                        let mut remove_door: Option<usize> = None;
                        for (i, door) in layout.doors.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("station_door_from", i))
                                    .selected_text(format!("from {}", door.from))
                                    .show_ui(ui, |ui| {
                                        for r in 0..room_count {
                                            changed |= ui
                                                .selectable_value(
                                                    &mut door.from,
                                                    r,
                                                    format!("room {r}"),
                                                )
                                                .changed();
                                        }
                                    });
                                egui::ComboBox::from_id_salt(("station_door_to", i))
                                    .selected_text(format!("to {}", door.to))
                                    .show_ui(ui, |ui| {
                                        for r in 0..room_count {
                                            changed |= ui
                                                .selectable_value(
                                                    &mut door.to,
                                                    r,
                                                    format!("room {r}"),
                                                )
                                                .changed();
                                        }
                                    });
                                changed |= ui
                                    .add(egui::DragValue::new(&mut door.x).prefix("x: "))
                                    .changed();
                                changed |= ui
                                    .add(egui::DragValue::new(&mut door.y).prefix("y: "))
                                    .changed();
                                if ui.button("×").clicked() {
                                    remove_door = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_door {
                            layout.doors.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Door").clicked() {
                            layout.doors.push(Door {
                                from: 0,
                                to: if room_count > 1 { 1 } else { 0 },
                                x: 0,
                                y: 0,
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("NPC spawns — who stands where")
                    .default_open(true)
                    .show(ui, |ui| {
                        let room_count = layout.rooms.len().max(1);
                        let mut remove_npc: Option<usize> = None;
                        for (i, spawn) in npc_spawns.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("Room:");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut spawn.room_index)
                                                .range(0..=room_count - 1),
                                        )
                                        .changed();
                                    ui.label("Name:");
                                    changed |= ui.text_edit_singleline(&mut spawn.name).changed();
                                    if ui.button("Remove NPC").clicked() {
                                        remove_npc = Some(i);
                                    }
                                });
                                ui.label("Dialogue lines:");
                                let mut remove_line: Option<usize> = None;
                                for (j, line) in spawn.dialogue.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        changed |= ui.text_edit_singleline(line).changed();
                                        if ui.button("×").clicked() {
                                            remove_line = Some(j);
                                        }
                                    });
                                }
                                if let Some(j) = remove_line {
                                    spawn.dialogue.remove(j);
                                    changed = true;
                                }
                                if ui.button("Add Line").clicked() {
                                    spawn.dialogue.push(String::new());
                                    changed = true;
                                }
                            });
                        }
                        if let Some(i) = remove_npc {
                            npc_spawns.remove(i);
                            changed = true;
                        }
                        if ui.button("Add NPC").clicked() {
                            npc_spawns.push(NpcSpawn {
                                room_index: 0,
                                name: "New NPC".into(),
                                dialogue: vec!["Hello, traveler.".into()],
                            });
                            changed = true;
                        }
                    });

                if !validation.is_empty() {
                    ui.separator();
                    for err in &validation {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                }
            });
            if regen_exterior {
                let seed = entry.file.seed;
                if let ContentPayload::Station { exterior, .. } = &mut entry.file.payload {
                    *exterior = generate_station(seed, StationKind::Trade, 1).exterior;
                    changed = true;
                }
            }
            if changed {
                self.touch();
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0x57A7_8008);
        let kind = [
            StationKind::Trade,
            StationKind::Mining,
            StationKind::Military,
        ][rng.next_below(3) as usize];
        // Size 1-2 lands the handoff's 4-8 room band.
        let station = generate_station(seed, kind, 1 + rng.next_below(2) as u32);
        let room_count = station.layout.rooms.len();
        let npc_names = ["Mara", "Doss", "Yuri", "Sable", "Okonkwo", "Trellis"];
        let npc_spawns = (0..1 + rng.next_below(2))
            .map(|_| NpcSpawn {
                room_index: rng.next_below(room_count as u64) as usize,
                name: npc_names[rng.next_below(npc_names.len() as u64) as usize].into(),
                dialogue: vec![
                    "Welcome aboard.".into(),
                    "Watch the pressure doors — they bite.".into(),
                ],
            })
            .collect();
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.file.seed = seed;
            entry.file.payload = ContentPayload::Station {
                exterior: station.exterior,
                layout: station.layout,
                npc_spawns,
            };
        }
        self.touch();
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let file: ContentFile = serde_json::from_value(value.clone())
            .map_err(|e| format!("station content file: {e}"))?;
        if !matches!(file.payload, ContentPayload::Station { .. }) {
            return Err("response payload is not a station".into());
        }
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.file = file;
        } else {
            self.entries.push(Entry {
                file,
                path: None,
                dirty: true,
            });
            self.selected = self.entries.len() - 1;
        }
        self.touch();
        Ok(())
    }

    fn snapshot(&self) -> Option<String> {
        let state: Vec<(&ContentFile, &Option<std::path::PathBuf>)> =
            self.entries.iter().map(|e| (&e.file, &e.path)).collect();
        ron::to_string(&(state, self.selected)).ok()
    }

    fn restore_snapshot(&mut self, ron: &str) -> Result<(), String> {
        let (state, selected): (Vec<(ContentFile, Option<std::path::PathBuf>)>, usize) =
            ron::from_str(ron).map_err(|e| e.to_string())?;
        self.entries = state
            .into_iter()
            .map(|(file, path)| Entry {
                file,
                path,
                dirty: true,
            })
            .collect();
        self.selected = selected.min(self.entries.len().saturating_sub(1));
        self.touch();
        Ok(())
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
        for e in &mut self.entries {
            e.dirty = false;
        }
    }

    fn save_all(&self) -> Result<(), String> {
        use crate::app::content_root;
        let mut wrote = 0usize;
        for entry in &self.entries {
            if !entry.dirty {
                continue;
            }
            let Some(path) = &entry.path else {
                let dir = content_root().join(ContentType::Station.directory());
                let _ = std::fs::create_dir_all(&dir);
                let stem = if entry.file.display_name.is_empty() {
                    format!("station_{}", wrote)
                } else {
                    entry.file.display_name.clone()
                };
                let p = dir.join(format!("{stem}.ron"));
                crate::io::write_ron(&p, &entry.file)?;
                wrote += 1;
                continue;
            };
            crate::io::write_ron(path, &entry.file)?;
            wrote += 1;
        }
        if wrote == 0 {
            return Err("no dirty entries to save".into());
        }
        Ok(())
    }

    fn selected_entry_name(&self) -> Option<String> {
        if self.entries.len() <= 1 {
            return None;
        }
        self.entries
            .get(self.selected)
            .map(|e| e.file.display_name.clone())
    }

    fn delete_selected(&mut self) -> bool {
        if self.entries.len() <= 1 || self.selected >= self.entries.len() {
            return false;
        }
        self.entries.remove(self.selected);
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }
        self.touch();
        true
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        ui.strong(&entry.file.display_name);
        if let ContentPayload::Station {
            exterior,
            layout,
            npc_spawns,
        } = &entry.file.payload
        {
            ui.label(format!(
                "{} room(s) · {} door(s)",
                layout.rooms.len(),
                layout.doors.len()
            ));
            ui.label(format!("{} NPC spawn(s)", npc_spawns.len()));
            ui.weak(format!(
                "Exterior: {} vertices, {} triangles",
                exterior.vertices.len(),
                exterior.indices.len() / 3
            ));
        }
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(StationEditor::new())
}
