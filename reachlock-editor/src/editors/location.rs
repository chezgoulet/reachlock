//! Location editor (handoff §9): authored hostile interiors — rooms, enemy
//! spawns, props, connections, and the optional keycard gate. Edits
//! `HostileLocation` — bare RON structs under `mods/reachlock/locations/`.

use reachlock_core::combat::location::{
    HostileLocation, HostileProp, HostileRoom, HostileSpawn, Keycard,
};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

const ROOM_KINDS: [&str; 5] = ["empty", "corridor", "arena", "boss", "reward"];

/// Enemy ids the seed generator can pull from — the shipped canon set.
const KNOWN_ARCHETYPES: [&str; 4] = [
    "raider_melee",
    "raider_gunner",
    "raider_boss",
    "security_bot",
];

struct Entry {
    location: HostileLocation,
    path: Option<std::path::PathBuf>,
}

pub struct LocationEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_location() -> HostileLocation {
    HostileLocation {
        id: "new_location".into(),
        display_name: "New Location".into(),
        rooms: vec![HostileRoom {
            id: "entry".into(),
            width: 8,
            height: 6,
            kind: "empty".into(),
            spawns: Vec::new(),
            props: Vec::new(),
        }],
        connections: Vec::new(),
        keycard: None,
    }
}

impl LocationEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir("mods/reachlock/locations") {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(location) = crate::io::read_ron::<HostileLocation>(&path) {
                    entries.push(Entry {
                        location,
                        path: Some(path),
                    });
                }
            }
        }
        if entries.is_empty() {
            entries.push(Entry {
                location: blank_location(),
                path: None,
            });
        }
        LocationEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

impl Editor for LocationEditor {
    fn title(&self) -> &str {
        "Location Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Location
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let location: HostileLocation = crate::io::read_ron(path)?;
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.path.as_deref() == Some(path))
        {
            self.entries[i].location = location;
            self.selected = i;
        } else {
            self.entries.push(Entry {
                location,
                path: Some(path.to_path_buf()),
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
            .ok_or_else(|| "no location selected".to_string())?;
        crate::io::write_ron(path, &entry.location)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(entry) = self.entries.get(self.selected) else {
            return errors;
        };
        let loc = &entry.location;
        if loc.id.is_empty() {
            errors.push("id must not be empty".into());
        }
        if loc.display_name.is_empty() {
            errors.push("display_name must not be empty".into());
        }
        if loc.rooms.is_empty() {
            errors.push("at least one room is required".into());
        }
        for (i, room) in loc.rooms.iter().enumerate() {
            if room.id.is_empty() {
                errors.push(format!("room {i}: id must not be empty"));
            }
            if !(4..=64).contains(&room.width) || !(4..=64).contains(&room.height) {
                errors.push(format!("room {i}: dimensions must be within 4..=64"));
            }
        }
        for (i, (a, b)) in loc.connections.iter().enumerate() {
            if loc.room(a).is_none() || loc.room(b).is_none() {
                errors.push(format!("connection {i}: references unknown room"));
            }
        }
        if let Some(keycard) = &loc.keycard {
            if loc.room(&keycard.door.0).is_none() || loc.room(&keycard.door.1).is_none() {
                errors.push("keycard door references unknown room".into());
            }
            if keycard.key_name.is_empty() {
                errors.push("keycard key_name must not be empty".into());
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("location_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self.selected as u64 + 42;
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.entries.push(Entry {
                        location: blank_location(),
                        path: None,
                    });
                    self.selected = self.entries.len() - 1;
                    self.has_changes = true;
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

        egui::SidePanel::left("location_list")
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
                        let label = self.entries[i].location.display_name.clone();
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
                ui.label("No location selected.");
                return;
            };
            let loc = &mut entry.location;
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("location_identity").show(ui, |ui| {
                    ui.label("ID:");
                    changed |= ui.text_edit_singleline(&mut loc.id).changed();
                    ui.end_row();
                    ui.label("Display Name:");
                    changed |= ui.text_edit_singleline(&mut loc.display_name).changed();
                    ui.end_row();
                });

                egui::CollapsingHeader::new("Rooms — spawns, props, geometry")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove_room: Option<usize> = None;
                        for (i, room) in loc.rooms.iter_mut().enumerate() {
                            egui::CollapsingHeader::new(format!(
                                "Room {i}: {} ({})",
                                room.id, room.kind
                            ))
                            .id_salt(("location_room", i))
                            .show(ui, |ui| {
                                egui::Grid::new(("location_room_grid", i)).show(ui, |ui| {
                                    ui.label("ID:");
                                    changed |= ui.text_edit_singleline(&mut room.id).changed();
                                    ui.end_row();
                                    ui.label("Kind:");
                                    ui.horizontal(|ui| {
                                        changed |=
                                            ui.text_edit_singleline(&mut room.kind).changed();
                                        egui::ComboBox::from_id_salt(("location_kind", i))
                                            .selected_text("presets")
                                            .show_ui(ui, |ui| {
                                                for k in ROOM_KINDS {
                                                    if ui.button(k).clicked() {
                                                        room.kind = k.into();
                                                        changed = true;
                                                    }
                                                }
                                            });
                                    });
                                    ui.end_row();
                                    ui.label("Width:");
                                    changed |= ui
                                        .add(egui::DragValue::new(&mut room.width).range(4..=64))
                                        .changed();
                                    ui.end_row();
                                    ui.label("Height:");
                                    changed |= ui
                                        .add(egui::DragValue::new(&mut room.height).range(4..=64))
                                        .changed();
                                    ui.end_row();
                                });

                                ui.label("Spawns:");
                                let mut remove_spawn: Option<usize> = None;
                                for (j, spawn) in room.spawns.iter_mut().enumerate() {
                                    ui.group(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label("Archetype:");
                                            changed |= ui
                                                .text_edit_singleline(&mut spawn.archetype)
                                                .changed();
                                            ui.label("Pos:");
                                            changed |= ui
                                                .add(egui::DragValue::new(&mut spawn.pos.0))
                                                .changed();
                                            changed |= ui
                                                .add(egui::DragValue::new(&mut spawn.pos.1))
                                                .changed();
                                            if ui.button("Remove Spawn").clicked() {
                                                remove_spawn = Some(j);
                                            }
                                        });
                                        ui.horizontal(|ui| {
                                            ui.label("Patrol:");
                                            let mut remove_wp: Option<usize> = None;
                                            for (k, wp) in spawn.patrol.iter_mut().enumerate() {
                                                changed |= ui
                                                    .add(egui::DragValue::new(&mut wp.0))
                                                    .changed();
                                                changed |= ui
                                                    .add(egui::DragValue::new(&mut wp.1))
                                                    .changed();
                                                if ui.button("×").clicked() {
                                                    remove_wp = Some(k);
                                                }
                                            }
                                            if let Some(k) = remove_wp {
                                                spawn.patrol.remove(k);
                                                changed = true;
                                            }
                                            if ui.button("+").clicked() {
                                                spawn.patrol.push((0, 0));
                                                changed = true;
                                            }
                                        });
                                    });
                                }
                                if let Some(j) = remove_spawn {
                                    room.spawns.remove(j);
                                    changed = true;
                                }
                                if ui.button("Add Spawn").clicked() {
                                    room.spawns.push(HostileSpawn {
                                        archetype: "raider_melee".into(),
                                        pos: (1, 1),
                                        patrol: Vec::new(),
                                    });
                                    changed = true;
                                }

                                ui.label("Props:");
                                let mut remove_prop: Option<usize> = None;
                                for (j, prop) in room.props.iter_mut().enumerate() {
                                    ui.horizontal(|ui| {
                                        changed |=
                                            ui.text_edit_singleline(&mut prop.kind).changed();
                                        changed |=
                                            ui.add(egui::DragValue::new(&mut prop.pos.0)).changed();
                                        changed |=
                                            ui.add(egui::DragValue::new(&mut prop.pos.1)).changed();
                                        if ui.button("×").clicked() {
                                            remove_prop = Some(j);
                                        }
                                    });
                                }
                                if let Some(j) = remove_prop {
                                    room.props.remove(j);
                                    changed = true;
                                }
                                if ui.button("Add Prop").clicked() {
                                    room.props.push(HostileProp {
                                        kind: "barrel".into(),
                                        pos: (0, 0),
                                    });
                                    changed = true;
                                }

                                if ui.button("Remove Room").clicked() {
                                    remove_room = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_room {
                            loc.rooms.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Room").clicked() {
                            loc.rooms.push(HostileRoom {
                                id: format!("room_{}", loc.rooms.len()),
                                width: 8,
                                height: 6,
                                kind: "empty".into(),
                                spawns: Vec::new(),
                                props: Vec::new(),
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Connections — walkable neighbours")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, (a, b)) in loc.connections.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.text_edit_singleline(a).changed();
                                ui.label("↔");
                                changed |= ui.text_edit_singleline(b).changed();
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            loc.connections.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Connection").clicked() {
                            loc.connections.push((String::new(), String::new()));
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Keycard — optional door gate")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut enabled = loc.keycard.is_some();
                        if ui.checkbox(&mut enabled, "Keycard gate").changed() {
                            loc.keycard = enabled.then(|| Keycard {
                                door: (String::new(), String::new()),
                                key_name: "keycard".into(),
                            });
                            changed = true;
                        }
                        if let Some(keycard) = &mut loc.keycard {
                            ui.horizontal(|ui| {
                                ui.label("Door:");
                                changed |= ui.text_edit_singleline(&mut keycard.door.0).changed();
                                ui.label("↔");
                                changed |= ui.text_edit_singleline(&mut keycard.door.1).changed();
                            });
                            ui.horizontal(|ui| {
                                ui.label("Key Name:");
                                changed |= ui.text_edit_singleline(&mut keycard.key_name).changed();
                            });
                        }
                    });

                if !validation.is_empty() {
                    ui.separator();
                    for err in &validation {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                }
            });
            if changed {
                self.has_changes = true;
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0x10CA_9009);
        let room_count = 3 + rng.next_below(4);
        let kinds = ["empty", "corridor", "arena", "boss", "reward"];
        let rooms: Vec<HostileRoom> = (0..room_count)
            .map(|i| {
                let kind = kinds[rng.next_below(kinds.len() as u64) as usize];
                let width = 6 + rng.next_below(12) as u32;
                let height = 5 + rng.next_below(10) as u32;
                let spawns = if kind == "arena" || kind == "boss" {
                    (0..1 + rng.next_below(3))
                        .map(|_| HostileSpawn {
                            archetype: KNOWN_ARCHETYPES
                                [rng.next_below(KNOWN_ARCHETYPES.len() as u64) as usize]
                                .into(),
                            pos: (
                                1 + rng.next_below(width as u64 - 2) as i64,
                                1 + rng.next_below(height as u64 - 2) as i64,
                            ),
                            patrol: Vec::new(),
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let props = (0..rng.next_below(3))
                    .map(|_| HostileProp {
                        kind: ["barrel", "crate"][rng.next_below(2) as usize].into(),
                        pos: (
                            1 + rng.next_below(width as u64 - 2) as i64,
                            1 + rng.next_below(height as u64 - 2) as i64,
                        ),
                    })
                    .collect();
                HostileRoom {
                    id: format!("room_{i}"),
                    width,
                    height,
                    kind: kind.into(),
                    spawns,
                    props,
                }
            })
            .collect();
        // Chain rooms so the layout is connected, then add 0-2 extra links.
        let mut connections: Vec<(String, String)> = (1..room_count)
            .map(|i| (format!("room_{}", i - 1), format!("room_{i}")))
            .collect();
        for _ in 0..rng.next_below(3) {
            let a = rng.next_below(room_count);
            let b = rng.next_below(room_count);
            if a != b {
                connections.push((format!("room_{a}"), format!("room_{b}")));
            }
        }
        let keycard = (rng.next_below(2) == 1 && room_count > 1).then(|| Keycard {
            door: (
                format!("room_{}", room_count - 2),
                format!("room_{}", room_count - 1),
            ),
            key_name: "keycard_hold".into(),
        });
        let location = HostileLocation {
            id: format!("location_{seed:x}"),
            display_name: format!("Derelict {seed:04}"),
            rooms,
            connections,
            keycard,
        };
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.location = location;
        }
        self.has_changes = true;
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let location: HostileLocation =
            serde_json::from_value(value.clone()).map_err(|e| format!("location: {e}"))?;
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.location = location;
        } else {
            self.entries.push(Entry {
                location,
                path: None,
            });
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = true;
        Ok(())
    }

    fn snapshot(&self) -> Option<String> {
        let state: Vec<(&HostileLocation, &Option<std::path::PathBuf>)> = self
            .entries
            .iter()
            .map(|e| (&e.location, &e.path))
            .collect();
        ron::to_string(&(state, self.selected)).ok()
    }

    fn restore_snapshot(&mut self, ron: &str) -> Result<(), String> {
        let (state, selected): (Vec<(HostileLocation, Option<std::path::PathBuf>)>, usize) =
            ron::from_str(ron).map_err(|e| e.to_string())?;
        self.entries = state
            .into_iter()
            .map(|(location, path)| Entry { location, path })
            .collect();
        self.selected = selected.min(self.entries.len().saturating_sub(1));
        self.has_changes = true;
        Ok(())
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }

    fn selected_entry_name(&self) -> Option<String> {
        if self.entries.len() <= 1 {
            return None;
        }
        self.entries
            .get(self.selected)
            .map(|e| e.location.display_name.clone())
    }

    fn delete_selected(&mut self) -> bool {
        if self.entries.len() <= 1 || self.selected >= self.entries.len() {
            return false;
        }
        self.entries.remove(self.selected);
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = true;
        true
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        let l = &entry.location;
        ui.strong(&l.display_name);
        let spawns: usize = l.rooms.iter().map(|r| r.spawns.len()).sum();
        ui.label(format!(
            "{} room(s) · {} connection(s)",
            l.rooms.len(),
            l.connections.len()
        ));
        ui.label(format!("{spawns} enemy spawn(s)"));
        match &l.keycard {
            Some(_) => ui.label("Keycard gate: yes"),
            None => ui.weak("No keycard gate"),
        };
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(LocationEditor::new())
}
