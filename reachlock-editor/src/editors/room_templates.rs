//! Room Templates editor (handoff §5): the authored room template set for
//! ship interiors. Edits the `ContentFile` envelope wrapping
//! `ContentPayload::RoomTemplates(Vec<RoomTemplate>)` — one file carries the
//! whole set.

use reachlock_core::content::{AssetType, ContentFile, ContentPayload, Priority};
use reachlock_core::editor::interior::RoomTemplate;
use reachlock_core::generator::RoomKind;

use super::super::app::{ContentType, Editor};

pub const ROOM_KINDS: [RoomKind; 16] = [
    RoomKind::Hangar,
    RoomKind::Corridor,
    RoomKind::Quarters,
    RoomKind::Bar,
    RoomKind::Market,
    RoomKind::Shipyard,
    RoomKind::Reactor,
    RoomKind::Bridge,
    RoomKind::Cockpit,
    RoomKind::TechBay,
    RoomKind::Scanner,
    RoomKind::MedBay,
    RoomKind::Cryo,
    RoomKind::Hydroponics,
    RoomKind::Armory,
    RoomKind::Brig,
];

pub struct RoomTemplatesEditor {
    file: ContentFile,
    path: Option<std::path::PathBuf>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_template() -> RoomTemplate {
    RoomTemplate {
        id: "new_room".into(),
        kind: RoomKind::Quarters,
        label: "New Room".into(),
        width: 4,
        height: 3,
        required_systems: Vec::new(),
        furniture_slots: Vec::new(),
        adjacent_pairs: Vec::new(),
    }
}

fn blank_file() -> ContentFile {
    ContentFile {
        id: "room_templates".into(),
        display_name: "Room Templates".into(),
        asset_type: AssetType::RoomTemplates,
        seed: 0,
        universe: "all".into(),
        priority: Priority::Curated,
        expires_at: None,
        payload: ContentPayload::RoomTemplates(RoomTemplate::reference_set()),
    }
}

impl RoomTemplatesEditor {
    fn new() -> Self {
        let default_path = std::path::Path::new("mods/reachlock/hulls/room_templates.ron");
        let (file, path) = match crate::io::read_ron::<ContentFile>(default_path) {
            Ok(f) if matches!(f.payload, ContentPayload::RoomTemplates(_)) => {
                (f, Some(default_path.to_path_buf()))
            }
            _ => (blank_file(), None),
        };
        RoomTemplatesEditor {
            file,
            path,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }

    fn templates(&self) -> &[RoomTemplate] {
        match &self.file.payload {
            ContentPayload::RoomTemplates(t) => t,
            _ => &[],
        }
    }
}

impl Editor for RoomTemplatesEditor {
    fn title(&self) -> &str {
        "Room Templates Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::RoomTemplates
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let file: ContentFile = crate::io::read_ron(path)?;
        if !matches!(file.payload, ContentPayload::RoomTemplates(_)) {
            return Err(format!("{} is not a room templates file", path.display()));
        }
        self.file = file;
        self.path = Some(path.to_path_buf());
        self.selected = 0;
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.file)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.file.id.is_empty() {
            errors.push("id must not be empty".into());
        }
        let templates = self.templates();
        for (i, t) in templates.iter().enumerate() {
            if t.id.is_empty() {
                errors.push(format!("template {i}: id must not be empty"));
            }
            if t.label.is_empty() {
                errors.push(format!("template {i}: label must not be empty"));
            }
            if !(1..=16).contains(&t.width) || !(1..=16).contains(&t.height) {
                errors.push(format!("template {i}: dimensions must be within 1..=16"));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for t in templates {
            if !seen.insert(&t.id) {
                errors.push(format!("duplicate template id: {}", t.id));
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("room_templates_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    self.generate_from_seed(self.file.seed);
                }
                if ui.button("Add Template").clicked() {
                    if let ContentPayload::RoomTemplates(t) = &mut self.file.payload {
                        t.push(blank_template());
                        self.selected = t.len() - 1;
                        self.has_changes = true;
                    }
                }
                if ui.button("Remove Template").clicked() {
                    if let ContentPayload::RoomTemplates(t) = &mut self.file.payload {
                        if t.len() > 1 && self.selected < t.len() {
                            t.remove(self.selected);
                            if self.selected >= t.len() {
                                self.selected = t.len() - 1;
                            }
                            self.has_changes = true;
                        }
                    }
                }
                let name = self
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(unsaved)".into());
                ui.label(name);
                if self.has_changes {
                    ui.label("*");
                }
            });
        });

        egui::SidePanel::left("room_templates_list")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.search);
                });
                ui.separator();
                let needle = self.search.to_lowercase();
                let labels: Vec<String> = self
                    .templates()
                    .iter()
                    .map(|t| format!("{} ({:?})", t.label, t.kind))
                    .collect();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, label) in labels.iter().enumerate() {
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        if ui.selectable_label(self.selected == i, label).clicked() {
                            self.selected = i;
                        }
                    }
                });
            });

        let validation = self.validate();
        egui::CentralPanel::default().show(ctx, |ui| {
            let selected = self.selected;
            let ContentPayload::RoomTemplates(templates) = &mut self.file.payload else {
                ui.colored_label(egui::Color32::RED, "payload is not a template set");
                return;
            };
            let Some(t) = templates.get_mut(selected) else {
                ui.label("No template selected.");
                return;
            };
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("room_template_form").show(ui, |ui| {
                    ui.label("ID:");
                    changed |= ui.text_edit_singleline(&mut t.id).changed();
                    ui.end_row();
                    ui.label("Label:");
                    changed |= ui.text_edit_singleline(&mut t.label).changed();
                    ui.end_row();
                    ui.label("Kind:");
                    egui::ComboBox::from_id_salt("room_template_kind")
                        .selected_text(format!("{:?}", t.kind))
                        .show_ui(ui, |ui| {
                            for kind in ROOM_KINDS {
                                changed |= ui
                                    .selectable_value(&mut t.kind, kind, format!("{kind:?}"))
                                    .changed();
                            }
                        });
                    ui.end_row();
                    ui.label("Width (cells):");
                    changed |= ui
                        .add(egui::DragValue::new(&mut t.width).range(1..=16))
                        .changed();
                    ui.end_row();
                    ui.label("Height (cells):");
                    changed |= ui
                        .add(egui::DragValue::new(&mut t.height).range(1..=16))
                        .changed();
                    ui.end_row();
                });

                egui::CollapsingHeader::new("Required systems — must exist on the ship")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, sys) in t.required_systems.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.text_edit_singleline(sys).changed();
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            t.required_systems.remove(i);
                            changed = true;
                        }
                        if ui.button("Add System").clicked() {
                            t.required_systems.push(String::new());
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Furniture slots — placeable fixture anchors")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, slot) in t.furniture_slots.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.text_edit_singleline(slot).changed();
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            t.furniture_slots.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Slot").clicked() {
                            t.furniture_slots.push(String::new());
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Adjacency bonus kinds — likes to sit next to")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, kind) in t.adjacent_pairs.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("adjacent_pair", i))
                                    .selected_text(format!("{kind:?}"))
                                    .show_ui(ui, |ui| {
                                        for k in ROOM_KINDS {
                                            changed |= ui
                                                .selectable_value(kind, k, format!("{k:?}"))
                                                .changed();
                                        }
                                    });
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            t.adjacent_pairs.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Adjacency").clicked() {
                            t.adjacent_pairs.push(RoomKind::Corridor);
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

    fn generate_from_seed(&mut self, _seed: u64) {
        // No procedural generation for templates — the canon reference set
        // is the authored default (handoff §5).
        self.file.payload = ContentPayload::RoomTemplates(RoomTemplate::reference_set());
        self.selected = 0;
        self.has_changes = true;
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let file: ContentFile = serde_json::from_value(value.clone())
            .map_err(|e| format!("room templates content file: {e}"))?;
        if !matches!(file.payload, ContentPayload::RoomTemplates(_)) {
            return Err("response payload is not a room template set".into());
        }
        self.file = file;
        self.selected = 0;
        self.has_changes = true;
        Ok(())
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(RoomTemplatesEditor::new())
}
