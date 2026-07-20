//! Charted System editor (handoff §3): the hand-charted star systems the
//! gate network connects. Edits `ChartedSystem` — bare RON structs under
//! `mods/reachlock/systems/`.

use reachlock_core::galaxy::{ChartedSystem, GalaxyCoord};
use reachlock_core::seed::types::Biome;
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

pub const BIOMES: [Biome; 5] = [
    Biome::Core,
    Biome::Frontier,
    Biome::Nebula,
    Biome::Derelict,
    Biome::DeepSpace,
];

pub fn biome_name(biome: Biome) -> &'static str {
    match biome {
        Biome::Core => "Core",
        Biome::Frontier => "Frontier",
        Biome::Nebula => "Nebula",
        Biome::Derelict => "Derelict",
        Biome::DeepSpace => "Deep Space",
    }
}

fn biome_description(biome: Biome) -> &'static str {
    match biome {
        Biome::Core => {
            "A bustling core-world system: orbital rings, dense traffic lanes, \
             and the steady hum of Compact administration."
        }
        Biome::Frontier => {
            "A frontier system on the edge of charted space. Sparse stations, \
             independent traders, and more rumors than patrols."
        }
        Biome::Nebula => {
            "A system wrapped in luminous nebula gas. Sensors struggle here, \
             and the gate beacons glow like lighthouses in fog."
        }
        Biome::Derelict => {
            "A dead system littered with wreckage from before the Collapse. \
             Salvagers pick the bones; something older watches."
        }
        Biome::DeepSpace => {
            "A deep-space waypoint far from any star. Cold, silent, and \
             visited only by those with a reason to be unseen."
        }
    }
}

struct Entry {
    system: ChartedSystem,
    path: Option<std::path::PathBuf>,
    dirty: bool,
}

pub struct ChartedSystemEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_system() -> ChartedSystem {
    ChartedSystem {
        id: "new_system".into(),
        display_name: "New System".into(),
        position: GalaxyCoord { x: 0, y: 0, z: 0 },
        biome: Biome::Frontier,
        seed: 0,
        description: String::new(),
    }
}

impl ChartedSystemEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir(
            crate::app::content_root().join(ContentType::ChartedSystem.directory()),
        ) {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(system) = crate::io::read_ron::<ChartedSystem>(&path) {
                    entries.push(Entry {
                        system,
                        path: Some(path),
                        dirty: false,
                    });
                }
            }
        }
        entries.sort_by(|a, b| a.system.display_name.cmp(&b.system.display_name));
        if entries.is_empty() {
            entries.push(Entry {
                system: blank_system(),
                path: None,
                dirty: true,
            });
        }
        ChartedSystemEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

impl Editor for ChartedSystemEditor {
    fn title(&self) -> &str {
        "Charted System Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::ChartedSystem
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
        let system: ChartedSystem = crate::io::read_ron(path)?;
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.path.as_deref() == Some(path))
        {
            self.entries[i].system = system;
            self.selected = i;
        } else {
            self.entries.push(Entry {
                system,
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
            .ok_or_else(|| "no system selected".to_string())?;
        crate::io::write_ron(path, &entry.system)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(entry) = self.entries.get(self.selected) else {
            return errors;
        };
        let s = &entry.system;
        if s.id.is_empty() {
            errors.push("id must not be empty".into());
        }
        if s.display_name.is_empty() {
            errors.push("display_name must not be empty".into());
        }
        if s.description.is_empty() {
            errors.push("description must not be empty".into());
        }
        if s.seed >= (1 << 53) {
            errors.push("seed must be below 2^53".into());
        }
        errors
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("charted_system_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self
                        .entries
                        .get(self.selected)
                        .map(|e| e.system.seed)
                        .unwrap_or(42);
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.entries.push(Entry {
                        system: blank_system(),
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

        egui::SidePanel::left("charted_system_list")
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
                        let label = format!(
                            "{} ({})",
                            self.entries[i].system.display_name,
                            biome_name(self.entries[i].system.biome)
                        );
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
                    let mut system = self.entries[i].system.clone();
                    system.id = format!("{}_copy", system.id);
                    system.display_name = format!("{} Copy", system.display_name);
                    self.entries.push(Entry {
                        system,
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
                ui.label("No system selected.");
                return;
            };
            let s = &mut entry.system;
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("charted_system_form").show(ui, |ui| {
                    ui.label("ID:");
                    changed |= ui.text_edit_singleline(&mut s.id).changed();
                    ui.end_row();
                    ui.label("Display Name:");
                    changed |= ui.text_edit_singleline(&mut s.display_name).changed();
                    ui.end_row();
                    ui.label("Position:");
                    ui.horizontal(|ui| {
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut s.position.x)
                                    .range(-32768..=32767)
                                    .prefix("x: "),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut s.position.y)
                                    .range(-32768..=32767)
                                    .prefix("y: "),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut s.position.z)
                                    .range(-32768..=32767)
                                    .prefix("z: "),
                            )
                            .changed();
                    });
                    ui.end_row();
                    ui.label("Biome:");
                    egui::ComboBox::from_id_salt("charted_system_biome")
                        .selected_text(biome_name(s.biome))
                        .show_ui(ui, |ui| {
                            for b in BIOMES {
                                changed |= ui
                                    .selectable_value(&mut s.biome, b, biome_name(b))
                                    .changed();
                            }
                        });
                    ui.end_row();
                    ui.label("Seed:");
                    changed |= ui
                        .add(egui::DragValue::new(&mut s.seed).range(0..=((1u64 << 53) - 1)))
                        .changed();
                    ui.end_row();
                });
                ui.label("Description:");
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(&mut s.description)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();

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
        let mut rng = SeededRng::new(seed ^ 0xC4A7_3003);
        let biome = BIOMES[rng.next_below(BIOMES.len() as u64) as usize];
        let coord = |rng: &mut SeededRng| rng.next_below(4001) as i64 - 2000;
        let system = ChartedSystem {
            id: format!("system_{seed:x}"),
            display_name: format!("Uncharted {seed:04}"),
            position: GalaxyCoord {
                x: coord(&mut rng),
                y: coord(&mut rng),
                z: coord(&mut rng),
            },
            biome,
            seed,
            description: biome_description(biome).into(),
        };
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.system = system;
        }
        self.touch();
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let system: ChartedSystem =
            serde_json::from_value(value.clone()).map_err(|e| format!("charted system: {e}"))?;
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.system = system;
        } else {
            self.entries.push(Entry {
                system,
                path: None,
                dirty: true,
            });
            self.selected = self.entries.len() - 1;
        }
        self.touch();
        Ok(())
    }

    fn snapshot(&self) -> Option<String> {
        let state: Vec<(&ChartedSystem, &Option<std::path::PathBuf>, bool)> = self
            .entries
            .iter()
            .map(|e| (&e.system, &e.path, e.dirty))
            .collect();
        ron::to_string(&(state, self.selected)).ok()
    }

    fn restore_snapshot(&mut self, ron: &str) -> Result<(), String> {
        let (state, selected): (
            Vec<(ChartedSystem, Option<std::path::PathBuf>, bool)>,
            usize,
        ) = ron::from_str(ron).map_err(|e| e.to_string())?;
        self.entries = state
            .into_iter()
            .map(|(system, path, dirty)| Entry {
                system,
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
                let dir = content_root().join(ContentType::ChartedSystem.directory());
                let _ = std::fs::create_dir_all(&dir);
                let stem = if entry.system.display_name.is_empty() {
                    format!("system_{}", wrote)
                } else {
                    entry.system.display_name.clone()
                };
                let p = dir.join(format!("{stem}.ron"));
                crate::io::write_ron(&p, &entry.system)?;
                entry.path = Some(p);
                wrote += 1;
                continue;
            };
            crate::io::write_ron(path, &entry.system)?;
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
            .map(|e| e.system.display_name.clone())
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
        let s = &entry.system;
        ui.strong(&s.display_name);
        let badge = match s.biome {
            Biome::Core => egui::Color32::from_rgb(0xDA, 0xA5, 0x20),
            Biome::Frontier => egui::Color32::from_rgb(0x3C, 0xB3, 0x71),
            Biome::Nebula => egui::Color32::from_rgb(0x93, 0x70, 0xDB),
            Biome::Derelict => egui::Color32::from_rgb(0x80, 0x80, 0x80),
            Biome::DeepSpace => egui::Color32::from_rgb(0x5F, 0x8F, 0x8F),
        };
        ui.colored_label(badge, biome_name(s.biome));
        ui.monospace(format!(
            "({}, {}, {})",
            s.position.x, s.position.y, s.position.z
        ));
        let desc: String = s.description.chars().take(120).collect();
        if desc.len() < s.description.len() {
            ui.label(format!("{desc}…"));
        } else if !desc.is_empty() {
            ui.label(desc);
        }
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ChartedSystemEditor::new())
}
