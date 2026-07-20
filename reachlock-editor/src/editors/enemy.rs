//! Enemy Archetype editor (handoff §2): the data-driven landed-combat enemy
//! class. Edits `HostileArchetype` — bare RON structs under
//! `mods/reachlock/combat/`.

use reachlock_core::combat::humanoid::{AttackWindow, BlockWindow, DodgeWindow, HostileArchetype};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

struct Entry {
    archetype: HostileArchetype,
    path: Option<std::path::PathBuf>,
    dirty: bool,
}

pub struct EnemyEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_archetype() -> HostileArchetype {
    HostileArchetype {
        id: "new_enemy".into(),
        display_name: "New Enemy".into(),
        hp: 8192,
        speed: 128,
        light_attack: AttackWindow {
            startup_ticks: 8,
            active_ticks: 4,
            recovery_ticks: 12,
            damage: 1024,
            range: 2048,
        },
        heavy_attack: AttackWindow {
            startup_ticks: 16,
            active_ticks: 6,
            recovery_ticks: 20,
            damage: 2048,
            range: 2560,
        },
        block: BlockWindow {
            active_ticks: 20,
            cooldown_ticks: 30,
            parry_ticks: 4,
        },
        dodge: DodgeWindow {
            i_frame_ticks: 8,
            recovery_ticks: 12,
            distance: 3072,
        },
        chase_radius: 8192,
        disengage_radius: 16000,
        flee_hp_frac: 256,
    }
}

impl EnemyEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(
            crate::app::content_root().join(ContentType::EnemyArchetype.directory()),
        ) {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(archetype) = crate::io::read_ron::<HostileArchetype>(&path) {
                    entries.push(Entry {
                        archetype,
                        path: Some(path),
                        dirty: false,
                    });
                }
            }
        }
        if entries.is_empty() {
            entries.push(Entry {
                archetype: blank_archetype(),
                path: None,
                dirty: true,
            });
        }
        EnemyEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

fn attack_window_ui(ui: &mut egui::Ui, id: &str, attack: &mut AttackWindow, changed: &mut bool) {
    egui::Grid::new(id).show(ui, |ui| {
        ui.label("Startup Ticks:");
        *changed |= ui
            .add(egui::DragValue::new(&mut attack.startup_ticks).range(0..=200))
            .changed();
        ui.end_row();
        ui.label("Active Ticks:");
        *changed |= ui
            .add(egui::DragValue::new(&mut attack.active_ticks).range(0..=200))
            .changed();
        ui.end_row();
        ui.label("Recovery Ticks:");
        *changed |= ui
            .add(egui::DragValue::new(&mut attack.recovery_ticks).range(0..=200))
            .changed();
        ui.end_row();
        ui.label("Damage (fixed 1/1024):");
        *changed |= ui
            .add(egui::DragValue::new(&mut attack.damage).range(0..=1_048_576))
            .changed();
        ui.end_row();
        ui.label("Range (fixed 1/1024):");
        *changed |= ui
            .add(egui::DragValue::new(&mut attack.range).range(0..=1_048_576))
            .changed();
        ui.end_row();
    });
}

impl Editor for EnemyEditor {
    fn title(&self) -> &str {
        "Enemy Archetype Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::EnemyArchetype
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
        let archetype: HostileArchetype = crate::io::read_ron(path)?;
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.path.as_deref() == Some(path))
        {
            self.entries[i].archetype = archetype;
            self.selected = i;
        } else {
            self.entries.push(Entry {
                archetype,
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
            .ok_or_else(|| "no archetype selected".to_string())?;
        crate::io::write_ron(path, &entry.archetype)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(entry) = self.entries.get(self.selected) else {
            return errors;
        };
        let a = &entry.archetype;
        if a.id.is_empty() {
            errors.push("id must not be empty".into());
        }
        if a.display_name.is_empty() {
            errors.push("display_name must not be empty".into());
        }
        if a.hp <= 0 {
            errors.push("hp must be positive".into());
        }
        if a.disengage_radius < a.chase_radius {
            errors.push("disengage_radius must be >= chase_radius".into());
        }
        if !(0..=1024).contains(&a.flee_hp_frac) {
            errors.push("flee_hp_frac must be within 0..=1024".into());
        }
        errors
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("enemy_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self.selected as u64 + 42;
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.entries.push(Entry {
                        archetype: blank_archetype(),
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

        egui::SidePanel::left("enemy_list")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.search);
                });
                ui.separator();
                let mut duplicate_idx: Option<usize> = None;
                let mut delete_idx: Option<usize> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let needle = self.search.to_lowercase();
                    for i in 0..self.entries.len() {
                        let label = self.entries[i].archetype.display_name.clone();
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let response = ui.selectable_label(self.selected == i, &label);
                        if response.clicked() {
                            self.selected = i;
                        }
                        response.context_menu(|ui| {
                            if ui.button("Duplicate").clicked() {
                                duplicate_idx = Some(i);
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                delete_idx = Some(i);
                                ui.close_menu();
                            }
                        });
                    }
                });
                if let Some(i) = duplicate_idx {
                    let mut archetype = self.entries[i].archetype.clone();
                    archetype.id = format!("{}_copy", archetype.id);
                    archetype.display_name = format!("{} Copy", archetype.display_name);
                    self.entries.push(Entry {
                        archetype,
                        path: None,
                        dirty: true,
                    });
                    self.selected = self.entries.len() - 1;
                    self.touch();
                }
                if let Some(i) = delete_idx {
                    if self.entries.len() > 1 {
                        self.entries.remove(i);
                        if self.selected >= self.entries.len() {
                            self.selected = self.entries.len() - 1;
                        }
                        self.touch();
                    }
                }
            });

        let validation = self.validate();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(entry) = self.entries.get_mut(self.selected) else {
                ui.label("No archetype selected.");
                return;
            };
            let a = &mut entry.archetype;
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Identity and vitals")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("enemy_identity").show(ui, |ui| {
                            ui.label("ID:");
                            changed |= ui.text_edit_singleline(&mut a.id).changed();
                            ui.end_row();
                            ui.label("Display Name:");
                            changed |= ui.text_edit_singleline(&mut a.display_name).changed();
                            ui.end_row();
                            ui.label("HP (fixed 1/1024):");
                            changed |= ui
                                .add(egui::DragValue::new(&mut a.hp).range(1..=1_048_576))
                                .changed();
                            ui.end_row();
                            ui.label("Speed (fixed/tick):");
                            changed |= ui
                                .add(egui::DragValue::new(&mut a.speed).range(0..=4096))
                                .changed();
                            ui.end_row();
                            ui.label("Chase Radius:");
                            changed |= ui
                                .add(egui::DragValue::new(&mut a.chase_radius).range(0..=262_144))
                                .changed();
                            ui.end_row();
                            ui.label("Disengage Radius:");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut a.disengage_radius)
                                        .range(0..=262_144),
                                )
                                .changed();
                            ui.end_row();
                            ui.label("Flee HP Fraction (0..=1024):");
                            changed |= ui
                                .add(egui::DragValue::new(&mut a.flee_hp_frac).range(0..=1024))
                                .changed();
                            ui.end_row();
                        });
                    });

                egui::CollapsingHeader::new("Light attack — fast poke")
                    .default_open(true)
                    .show(ui, |ui| {
                        attack_window_ui(ui, "enemy_light", &mut a.light_attack, &mut changed);
                    });

                egui::CollapsingHeader::new("Heavy attack — committed swing")
                    .default_open(true)
                    .show(ui, |ui| {
                        attack_window_ui(ui, "enemy_heavy", &mut a.heavy_attack, &mut changed);
                    });

                egui::CollapsingHeader::new("Block — guard and parry windows")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("enemy_block").show(ui, |ui| {
                            ui.label("Active Ticks:");
                            changed |= ui
                                .add(egui::DragValue::new(&mut a.block.active_ticks).range(0..=200))
                                .changed();
                            ui.end_row();
                            ui.label("Cooldown Ticks:");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut a.block.cooldown_ticks)
                                        .range(0..=200),
                                )
                                .changed();
                            ui.end_row();
                            ui.label("Parry Ticks:");
                            changed |= ui
                                .add(egui::DragValue::new(&mut a.block.parry_ticks).range(0..=200))
                                .changed();
                            ui.end_row();
                        });
                    });

                egui::CollapsingHeader::new("Dodge — i-frames and roll distance")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("enemy_dodge").show(ui, |ui| {
                            ui.label("I-Frame Ticks:");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut a.dodge.i_frame_ticks).range(0..=200),
                                )
                                .changed();
                            ui.end_row();
                            ui.label("Recovery Ticks:");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut a.dodge.recovery_ticks)
                                        .range(0..=200),
                                )
                                .changed();
                            ui.end_row();
                            ui.label("Distance (fixed 1/1024):");
                            changed |= ui
                                .add(egui::DragValue::new(&mut a.dodge.distance).range(0..=65_536))
                                .changed();
                            ui.end_row();
                        });
                    });

                if !validation.is_empty() {
                    ui.separator();
                    for err in &validation {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                }
            });
            if changed {
                self.touch();
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0xE4E4_2002);
        // Seed parity: even = light/fast archetype, odd = heavy/slow.
        let light_build = seed.is_multiple_of(2);
        // Raider vs bot band split (handoff §2 numbers).
        let is_bot = rng.next_below(2) == 1;
        let (hp_lo, hp_span, sp_lo, sp_span) = if is_bot {
            (8000, 24_000, 32, 96)
        } else {
            (4000, 12_000, 64, 192)
        };
        let hp = hp_lo + rng.next_below(hp_span) as i64;
        let speed = sp_lo + rng.next_below(sp_span) as i64;
        let speed = if light_build {
            speed + speed / 2
        } else {
            speed
        };
        let atk = |rng: &mut SeededRng, heavy: bool| {
            let scale = if heavy { 2 } else { 1 };
            AttackWindow {
                startup_ticks: (4 + rng.next_below(8)) as u32 * scale as u32,
                active_ticks: (3 + rng.next_below(4)) as u32,
                recovery_ticks: (8 + rng.next_below(10)) as u32 * scale as u32,
                damage: (512 + rng.next_below(1536) as i64) * scale,
                range: 1536 + rng.next_below(1536) as i64,
            }
        };
        let archetype = HostileArchetype {
            id: format!("enemy_{seed:x}"),
            display_name: format!(
                "{} {}",
                if light_build { "Swift" } else { "Heavy" },
                if is_bot { "Bot" } else { "Raider" }
            ),
            hp,
            speed,
            light_attack: atk(&mut rng, false),
            heavy_attack: atk(&mut rng, true),
            block: BlockWindow {
                active_ticks: (12 + rng.next_below(16)) as u32,
                cooldown_ticks: (20 + rng.next_below(20)) as u32,
                parry_ticks: (2 + rng.next_below(5)) as u32,
            },
            dodge: DodgeWindow {
                i_frame_ticks: (6 + rng.next_below(6)) as u32,
                recovery_ticks: (8 + rng.next_below(10)) as u32,
                distance: 2048 + rng.next_below(2048) as i64,
            },
            chase_radius: 6144 + rng.next_below(4096) as i64,
            disengage_radius: 12_288 + rng.next_below(8192) as i64,
            flee_hp_frac: if is_bot {
                0
            } else {
                rng.next_below(512) as i64
            },
        };
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.archetype = archetype;
        }
        self.touch();
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let archetype: HostileArchetype =
            serde_json::from_value(value.clone()).map_err(|e| format!("enemy archetype: {e}"))?;
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.archetype = archetype;
        } else {
            self.entries.push(Entry {
                archetype,
                path: None,
                dirty: true,
            });
            self.selected = self.entries.len() - 1;
        }
        self.touch();
        Ok(())
    }

    fn snapshot(&self) -> Option<String> {
        let state: Vec<(&HostileArchetype, &Option<std::path::PathBuf>, bool)> = self
            .entries
            .iter()
            .map(|e| (&e.archetype, &e.path, e.dirty))
            .collect();
        ron::to_string(&(state, self.selected)).ok()
    }

    fn restore_snapshot(&mut self, ron: &str) -> Result<(), String> {
        let (state, selected): (
            Vec<(HostileArchetype, Option<std::path::PathBuf>, bool)>,
            usize,
        ) = ron::from_str(ron).map_err(|e| e.to_string())?;
        self.entries = state
            .into_iter()
            .map(|(archetype, path, dirty)| Entry {
                archetype,
                path,
                dirty,
            })
            .collect();
        self.selected = selected.min(self.entries.len().saturating_sub(1));
        self.has_changes = self.entries.iter().any(|e| e.dirty);
        self.touch();
        Ok(())
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
        for e in &mut self.entries {
            e.dirty = false;
        }
    }

    fn save_all(&mut self) -> Result<(), String> {
        use crate::app::content_root;
        let mut wrote = 0usize;
        for entry in &mut self.entries {
            if !entry.dirty {
                continue;
            }
            let Some(path) = &entry.path else {
                let dir = content_root().join(ContentType::EnemyArchetype.directory());
                let _ = std::fs::create_dir_all(&dir);
                let stem = if entry.archetype.display_name.is_empty() {
                    format!("enemy_{}", wrote)
                } else {
                    entry.archetype.display_name.clone()
                };
                let p = dir.join(format!("{stem}.ron"));
                crate::io::write_ron(&p, &entry.archetype)?;
                entry.path = Some(p);
                wrote += 1;
                continue;
            };
            crate::io::write_ron(path, &entry.archetype)?;
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
            .map(|e| e.archetype.display_name.clone())
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
        let a = &entry.archetype;
        ui.strong(&a.display_name);
        // HP visualized against the archetype band ceiling (32000).
        let frac = (a.hp as f32 / 32000.0).clamp(0.0, 1.0);
        ui.add(egui::ProgressBar::new(frac).text(format!("HP {}", a.hp)));
        ui.label(format!(
            "Speed {} · chase {} · disengage {}",
            a.speed, a.chase_radius, a.disengage_radius
        ));
        ui.label(format!(
            "Light dmg {} · heavy dmg {}",
            a.light_attack.damage, a.heavy_attack.damage
        ));
        if a.flee_hp_frac == 0 {
            ui.weak("Fearless (never flees)");
        }
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(EnemyEditor::new())
}
