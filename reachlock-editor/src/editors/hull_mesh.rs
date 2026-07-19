//! Hull Mesh editor (handoff §4): hand-crafted hull polygon meshes. Edits
//! the `ContentFile` envelope wrapping `ContentPayload::Hull(GeneratedMesh)`.
//! The mesh itself is regenerated procedurally — vertices are shown
//! read-only; "Regenerate" swaps the mesh via `generate_hull_class`.

use reachlock_core::content::{AssetType, ContentFile, ContentPayload, Priority};
use reachlock_core::editor::exterior::{compose_hull, HullConfiguration, HullFrame, ItemRef, PaintScheme};
use reachlock_core::generator::hull::{generate_hull_class, HullClass};
use reachlock_core::item::{EquipmentKind, ItemSeed, ItemType};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

const CLASSES: [HullClass; 5] = [
    HullClass::Shuttle,
    HullClass::Freighter,
    HullClass::Corvette,
    HullClass::Station,
    HullClass::Rock,
];

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

pub struct HullMeshEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
    /// Class used by the Regenerate button (not stored in the payload).
    regen_class: HullClass,
    compose_summary: Option<String>,
}

fn blank_file() -> ContentFile {
    ContentFile {
        id: "new_hull".into(),
        display_name: "New Hull".into(),
        asset_type: AssetType::Hull,
        seed: 42,
        universe: "all".into(),
        priority: Priority::Curated,
        expires_at: None,
        payload: ContentPayload::Hull(generate_hull_class(42, HullClass::Corvette)),
    }
}

impl HullMeshEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir("mods/reachlock/hulls") {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(file) = crate::io::read_ron::<ContentFile>(&path) {
                    if matches!(file.payload, ContentPayload::Hull(_)) {
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
        HullMeshEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
            regen_class: HullClass::Corvette,
            compose_summary: None,
        }
    }
}

impl Editor for HullMeshEditor {
    fn title(&self) -> &str {
        "Hull Mesh Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::HullMesh
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let file: ContentFile = crate::io::read_ron(path)?;
        if !matches!(file.payload, ContentPayload::Hull(_)) {
            return Err(format!("{} is not a hull mesh file", path.display()));
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
            .ok_or_else(|| "no hull selected".to_string())?;
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
        if let ContentPayload::Hull(mesh) = &entry.file.payload {
            if mesh.vertices.is_empty() {
                errors.push("mesh has no vertices".into());
            }
            if mesh.indices.len() % 3 != 0 {
                errors.push("index count must be a multiple of 3".into());
            }
            let max = mesh.vertices.len() as u32;
            if mesh.indices.iter().any(|&i| i >= max) {
                errors.push("index out of vertex range".into());
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("hull_mesh_toolbar").show(ctx, |ui| {
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

        egui::SidePanel::left("hull_mesh_list")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.search);
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let needle = self.search.to_lowercase();
                    for i in 0..self.entries.len() {
                        let label = self.entries[i].file.id.clone();
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
                ui.label("No hull selected.");
                return;
            };
            let mut changed = false;
            let mut regen = false;
            let mut compose = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Envelope — identity and priority")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("hull_mesh_envelope").show(ui, |ui| {
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
                            changed |=
                                ui.text_edit_singleline(&mut entry.file.universe).changed();
                            ui.end_row();
                            ui.label("Priority:");
                            egui::ComboBox::from_id_salt("hull_mesh_priority")
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

                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("Regenerate as class")
                        .selected_text(format!("{:?}", self.regen_class))
                        .show_ui(ui, |ui| {
                            for class in CLASSES {
                                ui.selectable_value(
                                    &mut self.regen_class,
                                    class,
                                    format!("{class:?}"),
                                );
                            }
                        });
                    if ui.button("Regenerate").clicked() {
                        regen = true;
                    }
                    if ui.button("Compose Preview").clicked() {
                        compose = true;
                    }
                });
                if let Some(summary) = &self.compose_summary {
                    ui.label(summary.clone());
                }

                let ContentPayload::Hull(mesh) = &entry.file.payload else {
                    ui.colored_label(egui::Color32::RED, "payload is not a hull mesh");
                    return;
                };

                egui::CollapsingHeader::new(format!(
                    "Vertices — {} (read-only)",
                    mesh.vertices.len()
                ))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("hull_mesh_vertices")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            egui::Grid::new("hull_mesh_vertex_grid").striped(true).show(
                                ui,
                                |ui| {
                                    ui.label("#");
                                    ui.label("x");
                                    ui.label("y");
                                    ui.end_row();
                                    for (i, v) in mesh.vertices.iter().enumerate() {
                                        ui.label(i.to_string());
                                        ui.label(format!("{:.2}", v.x.to_f32()));
                                        ui.label(format!("{:.2}", v.y.to_f32()));
                                        ui.end_row();
                                    }
                                },
                            );
                        });
                });

                egui::CollapsingHeader::new(format!(
                    "Triangles — {} (read-only)",
                    mesh.indices.len() / 3
                ))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("hull_mesh_triangles")
                        .max_height(220.0)
                        .show(ui, |ui| {
                            egui::Grid::new("hull_mesh_index_grid").striped(true).show(
                                ui,
                                |ui| {
                                    ui.label("#");
                                    ui.label("v0");
                                    ui.label("v1");
                                    ui.label("v2");
                                    ui.end_row();
                                    for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
                                        ui.label(t.to_string());
                                        ui.label(tri[0].to_string());
                                        ui.label(tri[1].to_string());
                                        ui.label(tri[2].to_string());
                                        ui.end_row();
                                    }
                                },
                            );
                        });
                });

                if !validation.is_empty() {
                    ui.separator();
                    for err in &validation {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                }
            });
            if regen {
                let seed = entry.file.seed;
                entry.file.payload = ContentPayload::Hull(generate_hull_class(seed, self.regen_class));
                changed = true;
            }
            if compose {
                // Compose against the reference frame of the selected class
                // with a bare default configuration — a sanity summary, not
                // a persisted artifact.
                let frame = HullFrame::reference(self.regen_class);
                let config = HullConfiguration {
                    hull_id: entry.file.id.clone(),
                    seed: entry.file.seed,
                    hardpoints: Vec::new(),
                    engine: ItemRef(ItemSeed {
                        seed: entry.file.seed,
                        item_type: ItemType::Equipment(EquipmentKind::Engine),
                        tier: 1,
                        faction: "compact".into(),
                        biome: "frontier".into(),
                    }),
                    plating: Vec::new(),
                    paint: PaintScheme::default(),
                    decals: Vec::new(),
                };
                let composed = compose_hull(&config, &frame);
                self.compose_summary = Some(format!(
                    "Composed against {:?} reference frame: {} vertices, {} triangles, paint {:?}/{:?}/{:?}",
                    self.regen_class,
                    composed.mesh.vertices.len(),
                    composed.mesh.indices.len() / 3,
                    composed.paint.primary,
                    composed.paint.secondary,
                    composed.paint.accent,
                ));
            }
            if changed {
                self.has_changes = true;
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0x4E11_4004);
        let class = CLASSES[rng.next_below(CLASSES.len() as u64) as usize];
        self.regen_class = class;
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.file.seed = seed;
            entry.file.payload = ContentPayload::Hull(generate_hull_class(seed, class));
        }
        self.has_changes = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(HullMeshEditor::new())
}
