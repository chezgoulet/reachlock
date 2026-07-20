//! Item editor (handoff §10): authored `ItemSeed`s — the save format is the
//! seed, not the generated item (items are seed-deterministic). The type
//! picker is a cascading ComboBox chain over the 5-level ItemType hierarchy;
//! "Generate Preview" runs the real generator and shows the result.

use reachlock_core::item::types::{
    BoardingWeapon, ComponentKind, ConsumableKind, CosmeticKind, EnergyWeapon, ImplantKind,
    KineticWeapon, MeleeWeapon, MissileWeapon, Rarity, WeaponKind,
};
use reachlock_core::item::{generate_item, EquipmentKind, GeneratedItem, ItemSeed, ItemType};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

const TOP_LEVELS: [&str; 5] = ["Equipment", "Consumable", "Component", "Implant", "Cosmetic"];

const EQUIPMENT_KINDS: [(&str, EquipmentKind); 10] = [
    (
        "Weapon",
        EquipmentKind::Weapon(WeaponKind::Energy(EnergyWeapon::Laser)),
    ),
    ("Armor", EquipmentKind::Armor),
    ("Shield", EquipmentKind::Shield),
    ("Engine", EquipmentKind::Engine),
    ("Sensor", EquipmentKind::Sensor),
    ("MiningTool", EquipmentKind::MiningTool),
    ("RepairTool", EquipmentKind::RepairTool),
    ("Cybernetic", EquipmentKind::Cybernetic),
    ("Augmentation", EquipmentKind::Augmentation),
    ("Spacesuit", EquipmentKind::Spacesuit),
];

const WEAPON_KINDS: [(&str, WeaponKind); 5] = [
    ("Energy", WeaponKind::Energy(EnergyWeapon::Laser)),
    ("Kinetic", WeaponKind::Kinetic(KineticWeapon::Cannon)),
    ("Missile", WeaponKind::Missile(MissileWeapon::Torpedo)),
    ("Melee", WeaponKind::Melee(MeleeWeapon::Blade)),
    (
        "Boarding",
        WeaponKind::Boarding(BoardingWeapon::BreachingCharge),
    ),
];

const CONSUMABLES: [ConsumableKind; 10] = [
    ConsumableKind::Medkit,
    ConsumableKind::RepairPack,
    ConsumableKind::Ammunition,
    ConsumableKind::FuelCell,
    ConsumableKind::BatteryPack,
    ConsumableKind::Booster,
    ConsumableKind::Grenade,
    ConsumableKind::Mine,
    ConsumableKind::DeployableCover,
    ConsumableKind::DataShard,
];

const COMPONENTS: [ComponentKind; 8] = [
    ComponentKind::Hardpoint,
    ComponentKind::HullPlating,
    ComponentKind::ArmorSegment,
    ComponentKind::PowerPlant,
    ComponentKind::Capacitor,
    ComponentKind::JumpDriveComponent,
    ComponentKind::CraftingMaterial,
    ComponentKind::RefinedOre,
];

const IMPLANTS: [ImplantKind; 4] = [
    ImplantKind::NeuralLace,
    ImplantKind::DroidInterface,
    ImplantKind::MemoryUpgrade,
    ImplantKind::FactionSpecific,
];

const COSMETICS: [CosmeticKind; 7] = [
    CosmeticKind::Costume,
    CosmeticKind::Hat,
    CosmeticKind::ShipPaint,
    CosmeticKind::Decal,
    CosmeticKind::CrewOutfit,
    CosmeticKind::PortraitFrame,
    CosmeticKind::InteriorDecoration,
];

const ENERGY: [EnergyWeapon; 3] = [EnergyWeapon::Laser, EnergyWeapon::Plasma, EnergyWeapon::Tachyon];
const KINETIC: [KineticWeapon; 3] = [
    KineticWeapon::Cannon,
    KineticWeapon::Railgun,
    KineticWeapon::Autocannon,
];
const MISSILE: [MissileWeapon; 3] = [
    MissileWeapon::Torpedo,
    MissileWeapon::Standard,
    MissileWeapon::Decoy,
];
const MELEE: [MeleeWeapon; 3] = [MeleeWeapon::Blade, MeleeWeapon::Baton, MeleeWeapon::ArcWelder];
const BOARDING: [BoardingWeapon; 2] = [
    BoardingWeapon::BreachingCharge,
    BoardingWeapon::SuppressionTool,
];

fn rarity_color(r: Rarity) -> egui::Color32 {
    match r {
        Rarity::Common => egui::Color32::GRAY,
        Rarity::Uncommon => egui::Color32::from_rgb(0x3C, 0xB3, 0x71),
        Rarity::Rare => egui::Color32::from_rgb(0x41, 0x69, 0xE1),
        Rarity::Epic => egui::Color32::from_rgb(0x93, 0x70, 0xDB),
        Rarity::Legendary => egui::Color32::from_rgb(0xDA, 0xA5, 0x20),
    }
}

struct Entry {
    item_seed: ItemSeed,
    path: Option<std::path::PathBuf>,
}

pub struct ItemEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
    preview: Option<GeneratedItem>,
}

fn blank_seed() -> ItemSeed {
    ItemSeed {
        seed: 42,
        item_type: ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Energy(
            EnergyWeapon::Laser,
        ))),
        tier: 1,
        faction: "compact".into(),
        biome: "frontier".into(),
    }
}

impl ItemEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir("mods/reachlock/items") {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(item_seed) = crate::io::read_ron::<ItemSeed>(&path) {
                    entries.push(Entry {
                        item_seed,
                        path: Some(path),
                    });
                }
            }
        }
        if entries.is_empty() {
            entries.push(Entry {
                item_seed: blank_seed(),
                path: None,
            });
        }
        ItemEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
            preview: None,
        }
    }
}

/// The cascading type picker. Each row narrows the hierarchy; switching a
/// broader row resets the narrower ones to that branch's first leaf.
fn item_type_picker(ui: &mut egui::Ui, item_type: &mut ItemType) -> bool {
    let mut changed = false;

    // Row 1: top level.
    let top_idx = match item_type {
        ItemType::Equipment(_) => 0,
        ItemType::Consumable(_) => 1,
        ItemType::Component(_) => 2,
        ItemType::Implant(_) => 3,
        ItemType::Cosmetic(_) => 4,
    };
    egui::ComboBox::from_label("Category")
        .selected_text(TOP_LEVELS[top_idx])
        .show_ui(ui, |ui| {
            for (i, name) in TOP_LEVELS.iter().enumerate() {
                if ui.selectable_label(top_idx == i, *name).clicked() && top_idx != i {
                    *item_type = match i {
                        0 => ItemType::Equipment(EQUIPMENT_KINDS[0].1),
                        1 => ItemType::Consumable(CONSUMABLES[0]),
                        2 => ItemType::Component(COMPONENTS[0]),
                        3 => ItemType::Implant(IMPLANTS[0]),
                        _ => ItemType::Cosmetic(COSMETICS[0]),
                    };
                    changed = true;
                }
            }
        });

    // Row 2: kind within the top level.
    match item_type {
        ItemType::Equipment(kind) => {
            let idx = EQUIPMENT_KINDS
                .iter()
                .position(|(_, k)| {
                    std::mem::discriminant(k) == std::mem::discriminant(kind)
                })
                .unwrap_or(0);
            egui::ComboBox::from_label("Equipment Kind")
                .selected_text(EQUIPMENT_KINDS[idx].0)
                .show_ui(ui, |ui| {
                    for (i, (name, k)) in EQUIPMENT_KINDS.iter().enumerate() {
                        if ui.selectable_label(idx == i, *name).clicked() && idx != i {
                            *kind = *k;
                            changed = true;
                        }
                    }
                });
            // Row 3+4: weapon class and leaf.
            if let EquipmentKind::Weapon(weapon) = kind {
                let widx = WEAPON_KINDS
                    .iter()
                    .position(|(_, w)| {
                        std::mem::discriminant(w) == std::mem::discriminant(weapon)
                    })
                    .unwrap_or(0);
                egui::ComboBox::from_label("Weapon Class")
                    .selected_text(WEAPON_KINDS[widx].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, w)) in WEAPON_KINDS.iter().enumerate() {
                            if ui.selectable_label(widx == i, *name).clicked() && widx != i {
                                *weapon = *w;
                                changed = true;
                            }
                        }
                    });
                match weapon {
                    WeaponKind::Energy(leaf) => {
                        egui::ComboBox::from_label("Type")
                            .selected_text(format!("{leaf:?}"))
                            .show_ui(ui, |ui| {
                                for l in ENERGY {
                                    changed |= ui
                                        .selectable_value(leaf, l, format!("{l:?}"))
                                        .changed();
                                }
                            });
                    }
                    WeaponKind::Kinetic(leaf) => {
                        egui::ComboBox::from_label("Type")
                            .selected_text(format!("{leaf:?}"))
                            .show_ui(ui, |ui| {
                                for l in KINETIC {
                                    changed |= ui
                                        .selectable_value(leaf, l, format!("{l:?}"))
                                        .changed();
                                }
                            });
                    }
                    WeaponKind::Missile(leaf) => {
                        egui::ComboBox::from_label("Type")
                            .selected_text(format!("{leaf:?}"))
                            .show_ui(ui, |ui| {
                                for l in MISSILE {
                                    changed |= ui
                                        .selectable_value(leaf, l, format!("{l:?}"))
                                        .changed();
                                }
                            });
                    }
                    WeaponKind::Melee(leaf) => {
                        egui::ComboBox::from_label("Type")
                            .selected_text(format!("{leaf:?}"))
                            .show_ui(ui, |ui| {
                                for l in MELEE {
                                    changed |= ui
                                        .selectable_value(leaf, l, format!("{l:?}"))
                                        .changed();
                                }
                            });
                    }
                    WeaponKind::Boarding(leaf) => {
                        egui::ComboBox::from_label("Type")
                            .selected_text(format!("{leaf:?}"))
                            .show_ui(ui, |ui| {
                                for l in BOARDING {
                                    changed |= ui
                                        .selectable_value(leaf, l, format!("{l:?}"))
                                        .changed();
                                }
                            });
                    }
                }
            }
        }
        ItemType::Consumable(kind) => {
            egui::ComboBox::from_label("Consumable Kind")
                .selected_text(format!("{kind:?}"))
                .show_ui(ui, |ui| {
                    for k in CONSUMABLES {
                        changed |= ui.selectable_value(kind, k, format!("{k:?}")).changed();
                    }
                });
        }
        ItemType::Component(kind) => {
            egui::ComboBox::from_label("Component Kind")
                .selected_text(format!("{kind:?}"))
                .show_ui(ui, |ui| {
                    for k in COMPONENTS {
                        changed |= ui.selectable_value(kind, k, format!("{k:?}")).changed();
                    }
                });
        }
        ItemType::Implant(kind) => {
            egui::ComboBox::from_label("Implant Kind")
                .selected_text(format!("{kind:?}"))
                .show_ui(ui, |ui| {
                    for k in IMPLANTS {
                        changed |= ui.selectable_value(kind, k, format!("{k:?}")).changed();
                    }
                });
        }
        ItemType::Cosmetic(kind) => {
            egui::ComboBox::from_label("Cosmetic Kind")
                .selected_text(format!("{kind:?}"))
                .show_ui(ui, |ui| {
                    for k in COSMETICS {
                        changed |= ui.selectable_value(kind, k, format!("{k:?}")).changed();
                    }
                });
        }
    }
    changed
}

impl Editor for ItemEditor {
    fn title(&self) -> &str {
        "Item Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Item
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let item_seed: ItemSeed = crate::io::read_ron(path)?;
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.path.as_deref() == Some(path))
        {
            self.entries[i].item_seed = item_seed;
            self.selected = i;
        } else {
            self.entries.push(Entry {
                item_seed,
                path: Some(path.to_path_buf()),
            });
            self.selected = self.entries.len() - 1;
        }
        self.preview = None;
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let entry = self
            .entries
            .get(self.selected)
            .ok_or_else(|| "no item selected".to_string())?;
        crate::io::write_ron(path, &entry.item_seed)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(entry) = self.entries.get(self.selected) else {
            return errors;
        };
        let s = &entry.item_seed;
        if !(1..=10).contains(&s.tier) {
            errors.push("tier must be within 1..=10".into());
        }
        if s.faction.is_empty() {
            errors.push("faction must not be empty".into());
        }
        if s.biome.is_empty() {
            errors.push("biome must not be empty".into());
        }
        if s.seed >= (1 << 53) {
            errors.push("seed must be below 2^53".into());
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("item_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self
                        .entries
                        .get(self.selected)
                        .map(|e| e.item_seed.seed)
                        .unwrap_or(42);
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.entries.push(Entry {
                        item_seed: blank_seed(),
                        path: None,
                    });
                    self.selected = self.entries.len() - 1;
                    self.preview = None;
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

        egui::SidePanel::left("item_list")
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
                        // The list shows the generated display name — the
                        // seed is deterministic so this is stable.
                        let label = generate_item(&self.entries[i].item_seed).display_name;
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        if ui.selectable_label(self.selected == i, &label).clicked() {
                            self.selected = i;
                            self.preview = None;
                        }
                    }
                });
            });

        let validation = self.validate();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(entry) = self.entries.get_mut(self.selected) else {
                ui.label("No item selected.");
                return;
            };
            let s = &mut entry.item_seed;
            let mut changed = false;
            let mut run_preview = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Item Type");
                changed |= item_type_picker(ui, &mut s.item_type);

                ui.separator();
                egui::Grid::new("item_form").show(ui, |ui| {
                    ui.label("Seed:");
                    changed |= ui
                        .add(egui::DragValue::new(&mut s.seed).range(0..=((1u64 << 53) - 1)))
                        .changed();
                    ui.end_row();
                    ui.label("Tier:");
                    changed |= ui
                        .add(egui::DragValue::new(&mut s.tier).range(1..=10))
                        .changed();
                    ui.end_row();
                    ui.label("Faction:");
                    changed |= ui.text_edit_singleline(&mut s.faction).changed();
                    ui.end_row();
                    ui.label("Biome:");
                    changed |= ui.text_edit_singleline(&mut s.biome).changed();
                    ui.end_row();
                });

                if ui.button("Generate Preview").clicked() {
                    run_preview = true;
                }

                if let Some(item) = &self.preview {
                    ui.separator();
                    ui.heading(&item.display_name);
                    ui.colored_label(
                        rarity_color(item.rarity),
                        format!("{:?}", item.rarity),
                    );
                    ui.label(&item.description);
                    ui.separator();
                    egui::Grid::new("item_stats").striped(true).show(ui, |ui| {
                        ui.label("Stat");
                        ui.label("Value");
                        ui.end_row();
                        for (key, value) in &item.stats.0 {
                            ui.label(format!("{key:?}"));
                            ui.label(format!("{:.2}", *value as f32 / 1024.0));
                            ui.end_row();
                        }
                    });
                    ui.label(format!(
                        "{}×{} pixel icon",
                        item.icon.width, item.icon.height
                    ));
                }

                if !validation.is_empty() {
                    ui.separator();
                    for err in &validation {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                }
            });
            if run_preview {
                self.preview = Some(generate_item(&entry.item_seed));
            }
            if changed {
                self.preview = None;
                self.has_changes = true;
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0x17E4_A00A);
        let item_type = reachlock_core::item::ItemFamily::ALL
            [rng.next_below(18) as usize]
            .representative_item_type();
        let item_seed = ItemSeed {
            seed: rng.next_u64() & 0x001F_FFFF_FFFF_FFFF,
            item_type,
            tier: 1 + rng.next_below(10) as u8,
            faction: "compact".into(),
            biome: "frontier".into(),
        };
        self.preview = Some(generate_item(&item_seed));
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.item_seed = item_seed;
        }
        self.has_changes = true;
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let item_seed: ItemSeed = serde_json::from_value(value.clone())
            .map_err(|e| format!("item seed: {e}"))?;
        self.preview = Some(generate_item(&item_seed));
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.item_seed = item_seed;
        } else {
            self.entries.push(Entry {
                item_seed,
                path: None,
            });
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = true;
        Ok(())
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ItemEditor::new())
}
