//! Economy Goods editor (handoff §6): the trade goods catalogue. Edits
//! `GoodsCatalog` (`mods/reachlock/economy/goods.ron`). Goods are held as a
//! list in the editor and re-keyed into the catalog's BTreeMap on save, so
//! renaming an id is a plain field edit.

use std::collections::BTreeMap;

use reachlock_core::economy::{load_goods_catalog, Good, GoodCategory, GoodId, GoodsCatalog};

use super::super::app::{ContentType, Editor};

pub struct EconomyEditor {
    version: u32,
    goods: Vec<Good>,
    path: Option<std::path::PathBuf>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_good() -> Good {
    Good {
        id: GoodId("new_good".into()),
        name: "New Good".into(),
        base_price: 10,
        mass: 1,
        category: GoodCategory::Consumable,
        contraband: false,
    }
}

fn category_name(c: GoodCategory) -> &'static str {
    match c {
        GoodCategory::Consumable => "Consumable",
        GoodCategory::Fuel => "Fuel",
        GoodCategory::Material => "Material",
        GoodCategory::Manufactured => "Manufactured",
        GoodCategory::Medical => "Medical",
        GoodCategory::Luxury => "Luxury",
        GoodCategory::Contraband => "Contraband",
    }
}

impl EconomyEditor {
    fn new() -> Self {
        let default_path = std::path::Path::new("mods/reachlock/economy/goods.ron");
        let (catalog, path) = match crate::io::read_ron::<GoodsCatalog>(default_path) {
            Ok(c) => (c, Some(default_path.to_path_buf())),
            Err(_) => (load_goods_catalog(), None),
        };
        EconomyEditor {
            version: catalog.version,
            goods: catalog.goods.into_values().collect(),
            path,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }

    fn to_catalog(&self) -> GoodsCatalog {
        let mut goods = BTreeMap::new();
        for good in &self.goods {
            goods.insert(good.id.clone(), good.clone());
        }
        GoodsCatalog {
            version: self.version,
            goods,
        }
    }
}

impl Editor for EconomyEditor {
    fn title(&self) -> &str {
        "Economy Goods Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::EconomyGoods
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let catalog: GoodsCatalog = crate::io::read_ron(path)?;
        self.version = catalog.version;
        self.goods = catalog.goods.into_values().collect();
        self.path = Some(path.to_path_buf());
        self.selected = 0;
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.to_catalog())
    }

    fn validate(&self) -> Vec<String> {
        // The catalog owns its consistency rules — surface them directly,
        // plus the re-keying hazard (duplicate ids collapse map entries).
        let mut errors = self.to_catalog().validate();
        let mut seen = std::collections::HashSet::new();
        for good in &self.goods {
            if !seen.insert(&good.id.0) {
                errors.push(format!("duplicate good id: {}", good.id.0));
            }
        }
        errors
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("economy_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate (canon goods)").clicked() {
                    self.generate_from_seed(0);
                }
                if ui.button("Add Good").clicked() {
                    self.goods.push(blank_good());
                    self.selected = self.goods.len() - 1;
                    self.has_changes = true;
                }
                if ui.button("Remove Good").clicked()
                    && self.goods.len() > 1
                    && self.selected < self.goods.len()
                {
                    self.goods.remove(self.selected);
                    if self.selected >= self.goods.len() {
                        self.selected = self.goods.len() - 1;
                    }
                    self.has_changes = true;
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

        egui::SidePanel::left("economy_list")
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
                    for i in 0..self.goods.len() {
                        let label = self.goods[i].name.clone();
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
            let Some(good) = self.goods.get_mut(self.selected) else {
                ui.label("No good selected.");
                return;
            };
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("economy_form").show(ui, |ui| {
                    ui.label("ID:");
                    changed |= ui.text_edit_singleline(&mut good.id.0).changed();
                    ui.end_row();
                    ui.label("Name:");
                    changed |= ui.text_edit_singleline(&mut good.name).changed();
                    ui.end_row();
                    ui.label("Base Price (credits):");
                    changed |= ui
                        .add(egui::DragValue::new(&mut good.base_price).range(0..=1_000_000))
                        .changed();
                    ui.end_row();
                    ui.label("Mass (cargo units):");
                    changed |= ui
                        .add(egui::DragValue::new(&mut good.mass).range(0..=10_000))
                        .changed();
                    ui.end_row();
                    ui.label("Category:");
                    egui::ComboBox::from_id_salt("economy_category")
                        .selected_text(category_name(good.category))
                        .show_ui(ui, |ui| {
                            for c in GoodCategory::ALL {
                                changed |= ui
                                    .selectable_value(&mut good.category, c, category_name(c))
                                    .changed();
                            }
                        });
                    ui.end_row();
                    ui.label("Contraband:");
                    changed |= ui.checkbox(&mut good.contraband, "").changed();
                    ui.end_row();
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
        // Goods are hand-authored economics — "generate" seeds the canon
        // catalog as a starting point (handoff §6).
        let catalog = load_goods_catalog();
        self.version = catalog.version;
        self.goods = catalog.goods.into_values().collect();
        self.selected = 0;
        self.has_changes = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(EconomyEditor::new())
}
