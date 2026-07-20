use reachlock_core::editor::exterior::{
    compose_hull, ArmorSegment, Decal, Hardpoint, HullConfiguration, HullFrame, ItemRef,
    PaintScheme, SizeClass,
};
use reachlock_core::generator::hull::{HullClass, HullHandling};
use reachlock_core::item::types::{EnergyWeapon, KineticWeapon, MissileWeapon, WeaponKind};
use reachlock_core::item::{EquipmentKind, ItemSeed, ItemType};

use super::super::app::{ContentType, Editor};

struct HullEditor {
    config: HullConfiguration,
    frame: HullFrame,
    has_changes: bool,
    file_path: Option<std::path::PathBuf>,
}

impl HullEditor {
    fn new() -> Self {
        let class = HullClass::Corvette;
        let frame = HullFrame::reference(class);
        let config = HullConfiguration {
            hull_id: "new_hull".into(),
            seed: 42,
            hardpoints: Vec::new(),
            engine: ItemRef(ItemSeed {
                seed: 1,
                item_type: ItemType::Equipment(EquipmentKind::Engine),
                tier: 1,
                faction: "compact".into(),
                biome: "frontier".into(),
            }),
            plating: Vec::new(),
            paint: PaintScheme::default(),
            decals: Vec::new(),
        };
        HullEditor {
            config,
            frame,
            has_changes: false,
            file_path: None,
        }
    }

    fn generate_config_from_seed(seed: u64, class: HullClass) -> HullConfiguration {
        use reachlock_core::util::rng::SeededRng;
        let mut rng = SeededRng::new(seed ^ 0x5EED_1111);
        let frame = HullFrame::reference(class);
        let hardpoints = frame
            .slots
            .iter()
            .map(|slot| {
                let item_type = match slot.size_class {
                    SizeClass::Small => ItemType::Equipment(EquipmentKind::Weapon(
                        WeaponKind::Energy(EnergyWeapon::Laser),
                    )),
                    SizeClass::Medium => ItemType::Equipment(EquipmentKind::Weapon(
                        WeaponKind::Kinetic(KineticWeapon::Cannon),
                    )),
                    SizeClass::Large => ItemType::Equipment(EquipmentKind::Weapon(
                        WeaponKind::Missile(MissileWeapon::Torpedo),
                    )),
                };
                Hardpoint {
                    slot_id: slot.id.clone(),
                    item: ItemRef(ItemSeed {
                        seed: rng.next_u64() & 0x001F_FFFF_FFFF_FFFF,
                        item_type,
                        tier: 1 + rng.next_below(5) as u8,
                        faction: "faction".into(),
                        biome: "frontier".into(),
                    }),
                    size_class: slot.size_class,
                }
            })
            .collect();

        let plating = frame
            .zones
            .iter()
            .map(|zone| ArmorSegment {
                zone_id: zone.id.clone(),
                mass: (1024 * (1 + rng.next_below(8))) as i64,
            })
            .collect();

        let decals = frame
            .decal_slots
            .iter()
            .map(|slot| Decal {
                slot_id: slot.clone(),
                decal_id: "default".into(),
            })
            .collect();

        let engine = ItemRef(ItemSeed {
            seed: rng.next_u64() & 0x001F_FFFF_FFFF_FFFF,
            item_type: ItemType::Equipment(EquipmentKind::Engine),
            tier: 1 + rng.next_below(5) as u8,
            faction: "compact".into(),
            biome: "frontier".into(),
        });

        HullConfiguration {
            hull_id: format!("hull_{seed:x}"),
            seed,
            hardpoints,
            engine,
            plating,
            paint: PaintScheme::default(),
            decals,
        }
    }
}

impl Editor for HullEditor {
    fn title(&self) -> &str {
        "Hull Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::HullMesh
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let config: HullConfiguration =
            crate::io::read_ron(path).map_err(|e| format!("load hull: {e}"))?;
        self.config = config;
        self.has_changes = false;
        self.file_path = Some(path.to_path_buf());
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.config).map_err(|e| format!("save hull: {e}"))
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.config.hull_id.is_empty() {
            errors.push("hull_id must not be empty".into());
        }
        errors
    }

    fn ui(&mut self, _ctx: &egui::Context) {
        egui::TopBottomPanel::top("hull_toolbar").show(_ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    self.generate_from_seed(self.config.seed);
                }
                let label = if self.has_changes { " (modified)" } else { "" };
                ui.label(format!("{}: {}", self.title(), self.config.hull_id));
                ui.label(label);
            });
        });

        egui::CentralPanel::default().show(_ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Hull Configuration");

                ui.separator();
                ui.label("Hull ID:");
                let mut hull_id = self.config.hull_id.clone();
                if ui.text_edit_singleline(&mut hull_id).changed() {
                    self.config.hull_id = hull_id;
                    self.has_changes = true;
                }

                ui.label("Seed:");
                let mut seed_str = self.config.seed.to_string();
                if ui.text_edit_singleline(&mut seed_str).changed() {
                    if let Ok(s) = seed_str.parse() {
                        self.config.seed = s;
                        self.has_changes = true;
                    }
                }

                ui.separator();

                let class_list = [
                    HullClass::Shuttle,
                    HullClass::Freighter,
                    HullClass::Corvette,
                    HullClass::Station,
                ];
                let current = self.frame.class;
                let mut current_idx = class_list.iter().position(|c| *c == current).unwrap_or(0);
                egui::ComboBox::from_label("Class")
                    .selected_text(format!("{:?}", current))
                    .show_ui(ui, |ui| {
                        for (i, class) in class_list.iter().enumerate() {
                            if ui
                                .selectable_value(&mut current_idx, i, format!("{:?}", class))
                                .clicked()
                            {
                                self.frame = HullFrame::reference(*class);
                                self.has_changes = true;
                            }
                        }
                    });

                ui.separator();
                ui.heading("Hardpoints");
                let mut remove_idx: Option<usize> = None;
                for (i, hp) in self.config.hardpoints.iter().enumerate() {
                    ui.group(|ui| {
                        ui.label(format!("Slot: {}", hp.slot_id));
                        ui.label(format!("Item seed: {}", hp.item.0.seed));
                        if ui.button("Remove").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                }
                if let Some(idx) = remove_idx {
                    self.config.hardpoints.remove(idx);
                    self.has_changes = true;
                }

                ui.separator();
                ui.heading("Engine");
                ui.label(format!("Engine seed: {}", self.config.engine.0.seed));

                ui.separator();
                ui.heading("Plating");
                for segment in &self.config.plating {
                    ui.label(format!("Zone: {}, mass: {}", segment.zone_id, segment.mass));
                }

                ui.separator();
                if ui.button("Compose Preview").clicked() {
                    let _composed = compose_hull(&self.config, &self.frame);
                    ui.label("Hull composed (check console)");
                }

                let handling = HullHandling::for_class(self.config.seed, self.frame.class);
                ui.separator();
                ui.heading("Flight Handling");
                ui.label(format!("Mass: {}", handling.mass));
                ui.label(format!("Thrust: {}", handling.thrust));
                ui.label(format!("Turn Rate: {}", handling.turn_rate));
                ui.label(format!("Fuel Burn: {}", handling.fuel_burn));

                let validation = self.validate();
                if !validation.is_empty() {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, "Validation Errors:");
                    for err in &validation {
                        ui.label(err);
                    }
                }
            });
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        self.config = Self::generate_config_from_seed(seed, self.frame.class);
        self.has_changes = true;
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let config: HullConfiguration = serde_json::from_value(value.clone())
            .map_err(|e| format!("hull configuration: {e}"))?;
        self.config = config;
        self.has_changes = true;
        Ok(())
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(HullEditor::new())
}
