//! Item Browser (handoff §15): a read-only previewer over the 18 item
//! families — pick a family and tier, see a grid of 8 generated items, and
//! inspect any of them. Nothing persists unless a seed is pinned to
//! `mods/reachlock/items/`.

use reachlock_core::item::types::Rarity;
use reachlock_core::item::{generate_item, GeneratedItem, ItemFamily, ItemSeed};

use super::super::app::{ContentType, Editor};

fn family_label(f: ItemFamily) -> String {
    let subtypes = match f {
        ItemFamily::EnergyWeapon
        | ItemFamily::KineticWeapon
        | ItemFamily::MissileWeapon
        | ItemFamily::MeleeWeapon => 3,
        ItemFamily::BoardingWeapon => 2,
        ItemFamily::Consumable => 10,
        ItemFamily::Component => 8,
        ItemFamily::Implant => 4,
        ItemFamily::Cosmetic => 7,
        _ => 1,
    };
    format!("{} ({subtypes})", f.token())
}

fn rarity_color(r: Rarity) -> egui::Color32 {
    match r {
        Rarity::Common => egui::Color32::GRAY,
        Rarity::Uncommon => egui::Color32::from_rgb(0x3C, 0xB3, 0x71),
        Rarity::Rare => egui::Color32::from_rgb(0x41, 0x69, 0xE1),
        Rarity::Epic => egui::Color32::from_rgb(0x93, 0x70, 0xDB),
        Rarity::Legendary => egui::Color32::from_rgb(0xDA, 0xA5, 0x20),
    }
}

struct Card {
    item_seed: ItemSeed,
    item: GeneratedItem,
    icon: Option<egui::TextureHandle>,
}

pub struct ItemBrowser {
    family: ItemFamily,
    tier: u8,
    seed_base: u64,
    cards: Vec<Card>,
    selected: Option<usize>,
    dirty: bool,
    status: String,
}

impl ItemBrowser {
    fn new() -> Self {
        ItemBrowser {
            family: ItemFamily::EnergyWeapon,
            tier: 1,
            seed_base: 0,
            cards: Vec::new(),
            selected: None,
            dirty: true,
            status: String::new(),
        }
    }

    fn regenerate(&mut self, ctx: &egui::Context) {
        self.cards = (0..8u64)
            .map(|i| {
                let item_seed = ItemSeed {
                    seed: self.seed_base + i,
                    item_type: self.family.representative_item_type(),
                    tier: self.tier,
                    faction: "compact".into(),
                    biome: "frontier".into(),
                };
                let item = generate_item(&item_seed);
                let icon = if item.icon.pixels.len()
                    == (item.icon.width * item.icon.height * 4) as usize
                {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [item.icon.width as usize, item.icon.height as usize],
                        &item.icon.pixels,
                    );
                    Some(ctx.load_texture(
                        format!("item_icon_{}", item.id),
                        image,
                        egui::TextureOptions::NEAREST,
                    ))
                } else {
                    None
                };
                Card {
                    item_seed,
                    item,
                    icon,
                }
            })
            .collect();
        self.selected = None;
        self.dirty = false;
    }
}

impl Editor for ItemBrowser {
    fn title(&self) -> &str {
        "Item Browser"
    }

    fn content_type(&self) -> ContentType {
        ContentType::ItemBrowser
    }

    fn has_unsaved_changes(&self) -> bool {
        false
    }

    fn load(&mut self, _path: &std::path::Path) -> Result<(), String> {
        Err("the item browser is a live previewer — nothing to load".into())
    }

    fn save(&self, _path: &std::path::Path) -> Result<(), String> {
        Err("the item browser is a live previewer — pin a seed instead".into())
    }

    fn validate(&self) -> Vec<String> {
        Vec::new()
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.regenerate(ctx);
        }

        egui::TopBottomPanel::top("item_browser_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Tier:");
                if ui
                    .add(egui::DragValue::new(&mut self.tier).range(1..=10))
                    .changed()
                {
                    self.dirty = true;
                }
                if ui.button("Re-roll Seeds").clicked() {
                    self.seed_base = self.seed_base.wrapping_add(8) & 0x001F_FFFF_FFFF_FFFF;
                    self.dirty = true;
                }
                if !self.status.is_empty() {
                    ui.label(&self.status);
                }
            });
        });

        egui::SidePanel::left("item_browser_families")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Families");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for family in ItemFamily::ALL {
                        if ui
                            .selectable_label(self.family == family, family_label(family))
                            .clicked()
                            && self.family != family
                        {
                            self.family = family;
                            self.dirty = true;
                        }
                    }
                });
            });

        if let Some(sel) = self.selected {
            egui::SidePanel::right("item_browser_detail")
                .resizable(true)
                .default_width(300.0)
                .show(ctx, |ui| {
                    let Some(card) = self.cards.get(sel) else {
                        return;
                    };
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading(&card.item.display_name);
                        ui.colored_label(
                            rarity_color(card.item.rarity),
                            format!("{:?}", card.item.rarity),
                        );
                        if let Some(icon) = &card.icon {
                            ui.image((icon.id(), egui::vec2(64.0, 64.0)));
                        }
                        ui.label(&card.item.description);
                        ui.separator();
                        // Breadcrumb trail of the type hierarchy.
                        ui.label(format!(
                            "Type: {}",
                            card.item_seed.item_type.token().replace('_', " → ")
                        ));
                        ui.label(format!(
                            "Seed: {}  Tier: {}  Faction: {}  Biome: {}",
                            card.item_seed.seed,
                            card.item_seed.tier,
                            card.item_seed.faction,
                            card.item_seed.biome
                        ));
                        ui.separator();
                        // Stats sorted by value descending.
                        let mut stats: Vec<_> = card.item.stats.0.iter().collect();
                        stats.sort_by(|a, b| b.1.cmp(a.1));
                        egui::Grid::new("item_browser_stats")
                            .striped(true)
                            .show(ui, |ui| {
                                for (key, value) in stats {
                                    ui.label(format!("{key:?}"));
                                    ui.label(format!("{:.2}", *value as f32 / 1024.0));
                                    ui.end_row();
                                }
                            });
                        ui.separator();
                        if ui.button("Pin Seed").clicked() {
                            let dir = std::path::Path::new("mods/reachlock/items");
                            let result = std::fs::create_dir_all(dir)
                                .map_err(|e| e.to_string())
                                .and_then(|()| {
                                    crate::io::write_ron(
                                        &dir.join(format!("{}.ron", card.item.id)),
                                        &card.item_seed,
                                    )
                                });
                            self.status = match result {
                                Ok(()) => {
                                    format!("Pinned {} to mods/reachlock/items/", card.item.id)
                                }
                                Err(e) => format!("Pin failed: {e}"),
                            };
                        }
                    });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("item_browser_grid")
                    .num_columns(4)
                    .spacing(egui::vec2(8.0, 8.0))
                    .show(ui, |ui| {
                        for (i, card) in self.cards.iter().enumerate() {
                            let selected = self.selected == Some(i);
                            let frame = egui::Frame::group(ui.style()).stroke(egui::Stroke::new(
                                if selected { 2.5 } else { 1.0 },
                                rarity_color(card.item.rarity),
                            ));
                            let response = frame
                                .show(ui, |ui| {
                                    ui.set_width(150.0);
                                    ui.strong(&card.item.display_name);
                                    ui.colored_label(
                                        rarity_color(card.item.rarity),
                                        format!("{:?}", card.item.rarity),
                                    );
                                    let mut stats: Vec<_> = card.item.stats.0.iter().collect();
                                    stats.sort_by(|a, b| b.1.cmp(a.1));
                                    for (key, value) in stats.iter().take(4) {
                                        ui.small(format!(
                                            "{key:?}: {:.1}",
                                            **value as f32 / 1024.0
                                        ));
                                    }
                                })
                                .response;
                            if response.interact(egui::Sense::click()).clicked() {
                                self.selected = Some(i);
                            }
                            if i % 4 == 3 {
                                ui.end_row();
                            }
                        }
                    });
            });
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        self.seed_base = seed & 0x001F_FFFF_FFFF_FFFF;
        self.dirty = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ItemBrowser::new())
}
