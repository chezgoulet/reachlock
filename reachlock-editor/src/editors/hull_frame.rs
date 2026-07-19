//! Hull Frame editor (handoff §1): authored structural constants per hull
//! class — hardpoint slots, engine mount, armor zones, decal slots, grid
//! bounds. Edits the `ContentFile` envelope wrapping
//! `ContentPayload::HullFrame`.

use reachlock_core::content::{AssetType, ContentFile, ContentPayload, Priority};
use reachlock_core::editor::exterior::{ArmorZone, HardpointSlot, HullFrame, SizeClass};
use reachlock_core::generator::hull::HullClass;
use reachlock_core::generator::FixedVec2;
use reachlock_core::util::rng::{Fixed, SeededRng};

use super::super::app::{ContentType, Editor};

const CLASSES: [HullClass; 5] = [
    HullClass::Shuttle,
    HullClass::Freighter,
    HullClass::Corvette,
    HullClass::Station,
    HullClass::Rock,
];

const SIZE_CLASSES: [SizeClass; 3] = [SizeClass::Small, SizeClass::Medium, SizeClass::Large];

const PRIORITIES: [Priority; 4] = [
    Priority::Procedural,
    Priority::Curated,
    Priority::Event,
    Priority::Authoritative,
];

struct Entry {
    file: ContentFile,
    path: Option<std::path::PathBuf>,
}

pub struct HullFrameEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_file() -> ContentFile {
    ContentFile {
        id: "new_frame".into(),
        display_name: "New Frame".into(),
        asset_type: AssetType::HullFrame,
        seed: 0,
        universe: "all".into(),
        priority: Priority::Curated,
        expires_at: None,
        payload: ContentPayload::HullFrame(HullFrame::reference(HullClass::Corvette)),
    }
}

/// Grid bounds per class (handoff §1).
fn class_grid_bounds(class: HullClass) -> (u8, u8) {
    match class {
        HullClass::Shuttle => (8, 6),
        HullClass::Corvette => (16, 12),
        HullClass::Freighter => (20, 16),
        HullClass::Station => (32, 24),
        HullClass::Rock => (12, 8),
    }
}

fn class_name(class: HullClass) -> &'static str {
    match class {
        HullClass::Shuttle => "Shuttle",
        HullClass::Freighter => "Freighter",
        HullClass::Corvette => "Corvette",
        HullClass::Station => "Station",
        HullClass::Rock => "Rock",
    }
}

impl HullFrameEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        // Best-effort scan of the authored frames so the left panel starts
        // populated; missing directory just means an empty list.
        if let Ok(dir) = std::fs::read_dir("mods/reachlock/hulls") {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with("_frame.ron"))
                })
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(file) = crate::io::read_ron::<ContentFile>(&path) {
                    if matches!(file.payload, ContentPayload::HullFrame(_)) {
                        entries.push(Entry {
                            file,
                            path: Some(path),
                        });
                    }
                }
            }
        }
        if entries.is_empty() {
            entries.push(Entry {
                file: blank_file(),
                path: None,
            });
        }
        HullFrameEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

/// Fixed-point position edited as two f32 drag values (×1024 conversion —
/// display only; storage stays integer).
fn fixed_vec2_ui(ui: &mut egui::Ui, value: &mut FixedVec2) -> bool {
    let mut changed = false;
    let mut x = value.x.to_f32();
    let mut y = value.y.to_f32();
    if ui
        .add(egui::DragValue::new(&mut x).speed(0.5).prefix("x: "))
        .changed()
    {
        value.x = Fixed((x * 1024.0).round() as i64);
        changed = true;
    }
    if ui
        .add(egui::DragValue::new(&mut y).speed(0.5).prefix("y: "))
        .changed()
    {
        value.y = Fixed((y * 1024.0).round() as i64);
        changed = true;
    }
    changed
}

impl Editor for HullFrameEditor {
    fn title(&self) -> &str {
        "Hull Frame Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::HullFrame
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let file: ContentFile = crate::io::read_ron(path)?;
        if !matches!(file.payload, ContentPayload::HullFrame(_)) {
            return Err(format!("{} is not a hull frame file", path.display()));
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
            .ok_or_else(|| "no frame selected".to_string())?;
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
        if let ContentPayload::HullFrame(frame) = &entry.file.payload {
            for (i, slot) in frame.slots.iter().enumerate() {
                if slot.id.is_empty() {
                    errors.push(format!("slot {i}: id must not be empty"));
                }
            }
            for (i, zone) in frame.zones.iter().enumerate() {
                if zone.id.is_empty() {
                    errors.push(format!("zone {i}: id must not be empty"));
                }
            }
            let (w, h) = frame.grid_bounds;
            if !(4..=32).contains(&w) || !(4..=32).contains(&h) {
                errors.push("grid_bounds must be within 4..=32".into());
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("hull_frame_toolbar").show(ctx, |ui| {
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

        egui::SidePanel::left("hull_frame_list")
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
                        let label = {
                            let entry = &self.entries[i];
                            let class = match &entry.file.payload {
                                ContentPayload::HullFrame(f) => class_name(f.class),
                                _ => "?",
                            };
                            format!("{} ({class})", entry.file.display_name)
                        };
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
                    let mut file = self.entries[i].file.clone();
                    file.id = format!("{}_copy", file.id);
                    file.display_name = format!("{} Copy", file.display_name);
                    self.entries.push(Entry { file, path: None });
                    self.selected = self.entries.len() - 1;
                    self.has_changes = true;
                }
                if let Some(i) = delete_idx {
                    if self.entries.len() > 1 {
                        self.entries.remove(i);
                        if self.selected >= self.entries.len() {
                            self.selected = self.entries.len() - 1;
                        }
                        self.has_changes = true;
                    }
                }
            });

        let validation = self.validate();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(entry) = self.entries.get_mut(self.selected) else {
                ui.label("No frame selected.");
                return;
            };
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Envelope — identity and priority")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("hull_frame_envelope").show(ui, |ui| {
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
                            ui.text_edit_singleline(&mut entry.file.universe)
                                .changed()
                                .then(|| changed = true);
                            ui.end_row();
                            ui.label("Priority:");
                            egui::ComboBox::from_id_salt("hull_frame_priority")
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

                let ContentPayload::HullFrame(frame) = &mut entry.file.payload else {
                    ui.colored_label(egui::Color32::RED, "payload is not a hull frame");
                    return;
                };

                egui::CollapsingHeader::new("Frame — class and grid")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("hull_frame_class").show(ui, |ui| {
                            ui.label("Class:");
                            egui::ComboBox::from_id_salt("hull_frame_class_combo")
                                .selected_text(class_name(frame.class))
                                .show_ui(ui, |ui| {
                                    for class in CLASSES {
                                        changed |= ui
                                            .selectable_value(
                                                &mut frame.class,
                                                class,
                                                class_name(class),
                                            )
                                            .changed();
                                    }
                                });
                            ui.end_row();
                            ui.label("Grid Bounds:");
                            ui.horizontal(|ui| {
                                changed |= ui
                                    .add(egui::DragValue::new(&mut frame.grid_bounds.0)
                                        .range(4..=32)
                                        .prefix("w: "))
                                    .changed();
                                changed |= ui
                                    .add(egui::DragValue::new(&mut frame.grid_bounds.1)
                                        .range(4..=32)
                                        .prefix("h: "))
                                    .changed();
                            });
                            ui.end_row();
                            ui.label("Engine Mount:");
                            ui.horizontal(|ui| {
                                changed |= fixed_vec2_ui(ui, &mut frame.engine_mount);
                            });
                            ui.end_row();
                        });
                    });

                egui::CollapsingHeader::new("Hardpoint slots — what hangs where")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, slot) in frame.slots.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.text_edit_singleline(&mut slot.id).changed();
                                egui::ComboBox::from_id_salt(("slot_size", i))
                                    .selected_text(format!("{:?}", slot.size_class))
                                    .show_ui(ui, |ui| {
                                        for sc in SIZE_CLASSES {
                                            changed |= ui
                                                .selectable_value(
                                                    &mut slot.size_class,
                                                    sc,
                                                    format!("{sc:?}"),
                                                )
                                                .changed();
                                        }
                                    });
                                changed |= fixed_vec2_ui(ui, &mut slot.position);
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            frame.slots.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Slot").clicked() {
                            frame.slots.push(HardpointSlot {
                                id: format!("slot_{}", frame.slots.len()),
                                position: FixedVec2 {
                                    x: Fixed(0),
                                    y: Fixed(0),
                                },
                                size_class: SizeClass::Small,
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Armor zones — plating attachment points")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, zone) in frame.zones.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.text_edit_singleline(&mut zone.id).changed();
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            frame.zones.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Zone").clicked() {
                            frame.zones.push(ArmorZone {
                                id: format!("zone_{}", frame.zones.len()),
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Decal slots — insignia placement")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, slot) in frame.decal_slots.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.text_edit_singleline(slot).changed();
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            frame.decal_slots.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Decal Slot").clicked() {
                            frame.decal_slots.push(String::new());
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
            if changed {
                self.has_changes = true;
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0xF4A3_E001);
        let class = CLASSES[rng.next_below(CLASSES.len() as u64) as usize];
        let coord = |rng: &mut SeededRng| {
            // ±48 whole units in fixed-point.
            Fixed((rng.next_below(97) as i64 - 48) * 1024)
        };
        let slots = (0..2 + rng.next_below(5))
            .map(|i| HardpointSlot {
                id: format!("slot_{i}"),
                position: FixedVec2 {
                    x: coord(&mut rng),
                    y: coord(&mut rng),
                },
                size_class: SIZE_CLASSES[rng.next_below(3) as usize],
            })
            .collect();
        let zones = (0..2 + rng.next_below(3))
            .map(|i| ArmorZone {
                id: format!("zone_{i}"),
            })
            .collect();
        let decal_slots = (0..1 + rng.next_below(3))
            .map(|i| format!("decal_{i}"))
            .collect();
        let frame = HullFrame {
            class,
            slots,
            engine_mount: FixedVec2 {
                x: coord(&mut rng),
                y: Fixed(0),
            },
            zones,
            decal_slots,
            grid_bounds: class_grid_bounds(class),
        };
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.file.seed = seed;
            entry.file.payload = ContentPayload::HullFrame(frame);
        }
        self.has_changes = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(HullFrameEditor::new())
}
