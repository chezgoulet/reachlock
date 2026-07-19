//! Faction editor (handoff §7): the authored political factions. Edits
//! `FactionCatalog` (`mods/reachlock/factions/canon.ron`) — doctrine, tariff
//! policy, produces, territory, internal divisions, and the diplomatic
//! relationship map.

use std::collections::BTreeMap;

use reachlock_core::economy::GoodCategory;
use reachlock_core::faction::{
    DiplomaticStanding, DivisionAgenda, DivisionId, Doctrine, Faction, FactionCatalog, FactionId,
    InternalDivision, RelationStatus, SystemClaim, TariffPolicy,
};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

const DOCTRINES: [Doctrine; 4] = [
    Doctrine::Military,
    Doctrine::Economic,
    Doctrine::Diplomatic,
    Doctrine::Expansionist,
];

const AGENDAS: [DivisionAgenda; 4] = [
    DivisionAgenda::Hawkish,
    DivisionAgenda::Dovish,
    DivisionAgenda::Mercantile,
    DivisionAgenda::Isolationist,
];

const STATUSES: [RelationStatus; 5] = [
    RelationStatus::Allied,
    RelationStatus::Friendly,
    RelationStatus::Neutral,
    RelationStatus::Hostile,
    RelationStatus::War,
];

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

/// Which tariff variant is selected, for the ComboBox (the enum carries
/// payloads, so variant switching needs a discriminant-only mirror).
#[derive(Clone, Copy, PartialEq)]
enum TariffKind {
    Regulated,
    Flat,
    Dynamic,
    None,
}

impl TariffKind {
    fn of(policy: &TariffPolicy) -> Self {
        match policy {
            TariffPolicy::Regulated { .. } => TariffKind::Regulated,
            TariffPolicy::Flat { .. } => TariffKind::Flat,
            TariffPolicy::Dynamic => TariffKind::Dynamic,
            TariffPolicy::None => TariffKind::None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            TariffKind::Regulated => "Regulated",
            TariffKind::Flat => "Flat",
            TariffKind::Dynamic => "Dynamic",
            TariffKind::None => "None",
        }
    }

    fn default_policy(self) -> TariffPolicy {
        match self {
            TariffKind::Regulated => TariffPolicy::Regulated {
                foreign_mult: 1126, // ~1.10 in fixed 1/1024
                own_mult: 973,      // ~0.95
            },
            TariffKind::Flat => TariffPolicy::Flat { mult: 1075 },
            TariffKind::Dynamic => TariffPolicy::Dynamic,
            TariffKind::None => TariffPolicy::None,
        }
    }
}

pub struct FactionEditor {
    catalog: FactionCatalog,
    path: Option<std::path::PathBuf>,
    selected: usize,
    search: String,
    has_changes: bool,
    new_relationship_target: String,
}

fn blank_faction() -> Faction {
    Faction {
        id: FactionId("new_faction".into()),
        name: "New Faction".into(),
        territory: Vec::new(),
        resources: Default::default(),
        relationships: BTreeMap::new(),
        goals: Vec::new(),
        internal_divisions: Vec::new(),
        doctrine: Doctrine::Economic,
        tariff_policy: TariffPolicy::None,
        produces: Vec::new(),
        color: [0x88, 0x88, 0x88, 0xFF],
    }
}

impl FactionEditor {
    fn new() -> Self {
        let default_path = std::path::Path::new("mods/reachlock/factions/canon.ron");
        let (catalog, path) = match crate::io::read_ron::<FactionCatalog>(default_path) {
            Ok(c) => (c, Some(default_path.to_path_buf())),
            Err(_) => (
                FactionCatalog {
                    version: 1,
                    factions: vec![blank_faction()],
                },
                None,
            ),
        };
        FactionEditor {
            catalog,
            path,
            selected: 0,
            search: String::new(),
            has_changes: false,
            new_relationship_target: String::new(),
        }
    }
}

impl Editor for FactionEditor {
    fn title(&self) -> &str {
        "Faction Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Faction
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.catalog = crate::io::read_ron(path)?;
        self.path = Some(path.to_path_buf());
        self.selected = 0;
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.catalog)
    }

    fn validate(&self) -> Vec<String> {
        // The catalog owns its invariants (unique ids, relationship
        // symmetry); surface them plus per-field basics.
        let mut errors = self.catalog.validate();
        if let Some(f) = self.catalog.factions.get(self.selected) {
            if f.id.0.is_empty() {
                errors.push("id must not be empty".into());
            }
            if f.name.is_empty() {
                errors.push("name must not be empty".into());
            }
            for (i, d) in f.internal_divisions.iter().enumerate() {
                if !(0.0..=1.0).contains(&d.influence) {
                    errors.push(format!("division {i}: influence must be 0.0..=1.0"));
                }
            }
            for (i, c) in f.territory.iter().enumerate() {
                if c.control > 100 {
                    errors.push(format!("claim {i}: control must be 0..=100"));
                }
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("faction_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self.selected as u64 + 42;
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.catalog.factions.push(blank_faction());
                    self.selected = self.catalog.factions.len() - 1;
                    self.has_changes = true;
                }
                if ui.button("Remove").clicked()
                    && self.catalog.factions.len() > 1
                    && self.selected < self.catalog.factions.len()
                {
                    self.catalog.factions.remove(self.selected);
                    if self.selected >= self.catalog.factions.len() {
                        self.selected = self.catalog.factions.len() - 1;
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

        egui::SidePanel::left("faction_list")
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
                    for i in 0..self.catalog.factions.len() {
                        let f = &self.catalog.factions[i];
                        if !needle.is_empty() && !f.name.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let color = egui::Color32::from_rgba_unmultiplied(
                            f.color[0], f.color[1], f.color[2], f.color[3],
                        );
                        let name = f.name.clone();
                        ui.horizontal(|ui| {
                            ui.colored_label(color, "●");
                            if ui.selectable_label(self.selected == i, &name).clicked() {
                                self.selected = i;
                            }
                        });
                    }
                });
            });

        let validation = self.validate();
        let all_ids: Vec<String> = self
            .catalog
            .factions
            .iter()
            .map(|f| f.id.0.clone())
            .collect();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(f) = self.catalog.factions.get_mut(self.selected) else {
                ui.label("No faction selected.");
                return;
            };
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Identity — name, color, doctrine")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("faction_identity").show(ui, |ui| {
                            ui.label("ID:");
                            changed |= ui.text_edit_singleline(&mut f.id.0).changed();
                            ui.end_row();
                            ui.label("Name:");
                            changed |= ui.text_edit_singleline(&mut f.name).changed();
                            ui.end_row();
                            ui.label("Color (RGBA):");
                            ui.horizontal(|ui| {
                                for c in &mut f.color {
                                    changed |= ui
                                        .add(egui::DragValue::new(c).range(0..=255))
                                        .changed();
                                }
                                let swatch = egui::Color32::from_rgba_unmultiplied(
                                    f.color[0], f.color[1], f.color[2], f.color[3],
                                );
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(32.0, 32.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(rect, 4.0, swatch);
                            });
                            ui.end_row();
                            ui.label("Doctrine:");
                            egui::ComboBox::from_id_salt("faction_doctrine")
                                .selected_text(format!("{:?}", f.doctrine))
                                .show_ui(ui, |ui| {
                                    for d in DOCTRINES {
                                        changed |= ui
                                            .selectable_value(
                                                &mut f.doctrine,
                                                d,
                                                format!("{d:?}"),
                                            )
                                            .changed();
                                    }
                                });
                            ui.end_row();
                        });
                    });

                egui::CollapsingHeader::new("Tariff policy — port economics")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut kind = TariffKind::of(&f.tariff_policy);
                        egui::ComboBox::from_id_salt("faction_tariff_kind")
                            .selected_text(kind.name())
                            .show_ui(ui, |ui| {
                                for k in [
                                    TariffKind::Regulated,
                                    TariffKind::Flat,
                                    TariffKind::Dynamic,
                                    TariffKind::None,
                                ] {
                                    if ui.selectable_value(&mut kind, k, k.name()).changed() {
                                        f.tariff_policy = k.default_policy();
                                        changed = true;
                                    }
                                }
                            });
                        match &mut f.tariff_policy {
                            TariffPolicy::Regulated {
                                foreign_mult,
                                own_mult,
                            } => {
                                egui::Grid::new("faction_tariff_regulated").show(ui, |ui| {
                                    ui.label("Foreign Mult (fixed 1/1024):");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(foreign_mult)
                                                .range(0..=10_240),
                                        )
                                        .changed();
                                    ui.end_row();
                                    ui.label("Own Mult (fixed 1/1024):");
                                    changed |= ui
                                        .add(egui::DragValue::new(own_mult).range(0..=10_240))
                                        .changed();
                                    ui.end_row();
                                });
                            }
                            TariffPolicy::Flat { mult } => {
                                ui.horizontal(|ui| {
                                    ui.label("Mult (fixed 1/1024):");
                                    changed |= ui
                                        .add(egui::DragValue::new(mult).range(0..=10_240))
                                        .changed();
                                });
                            }
                            TariffPolicy::Dynamic => {
                                ui.label("Adjusts with demand.");
                            }
                            TariffPolicy::None => {
                                ui.label("No tariffs.");
                            }
                        }
                    });

                egui::CollapsingHeader::new("Produces — subsidized good categories")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, cat) in f.produces.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt(("faction_produces", i))
                                    .selected_text(category_name(*cat))
                                    .show_ui(ui, |ui| {
                                        for c in GoodCategory::ALL {
                                            changed |= ui
                                                .selectable_value(cat, c, category_name(c))
                                                .changed();
                                        }
                                    });
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            f.produces.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Category").clicked() {
                            f.produces.push(GoodCategory::Material);
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Territory — system claims")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, claim) in f.territory.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |=
                                    ui.text_edit_singleline(&mut claim.system_id).changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut claim.control)
                                            .range(0..=100)
                                            .suffix("%"),
                                    )
                                    .changed();
                                if ui.button("×").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            f.territory.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Claim").clicked() {
                            f.territory.push(SystemClaim {
                                system_id: String::new(),
                                control: 50,
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Internal divisions — wings and agendas")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        for (i, d) in f.internal_divisions.iter_mut().enumerate() {
                            ui.group(|ui| {
                                egui::Grid::new(("faction_division", i)).show(ui, |ui| {
                                    ui.label("ID:");
                                    changed |= ui.text_edit_singleline(&mut d.id.0).changed();
                                    ui.end_row();
                                    ui.label("Name:");
                                    changed |= ui.text_edit_singleline(&mut d.name).changed();
                                    ui.end_row();
                                    ui.label("Influence (0..=1):");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut d.influence)
                                                .speed(0.01)
                                                .range(0.0..=1.0),
                                        )
                                        .changed();
                                    ui.end_row();
                                    ui.label("Agenda:");
                                    egui::ComboBox::from_id_salt(("faction_agenda", i))
                                        .selected_text(format!("{:?}", d.agenda))
                                        .show_ui(ui, |ui| {
                                            for a in AGENDAS {
                                                changed |= ui
                                                    .selectable_value(
                                                        &mut d.agenda,
                                                        a,
                                                        format!("{a:?}"),
                                                    )
                                                    .changed();
                                            }
                                        });
                                    ui.end_row();
                                    ui.label("Player Standing (−100..=100):");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut d.player_standing)
                                                .range(-100..=100),
                                        )
                                        .changed();
                                    ui.end_row();
                                });
                                if ui.button("Remove Division").clicked() {
                                    remove = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove {
                            f.internal_divisions.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Division").clicked() {
                            f.internal_divisions.push(InternalDivision {
                                id: DivisionId(format!(
                                    "division_{}",
                                    f.internal_divisions.len()
                                )),
                                name: String::new(),
                                influence: 0.5,
                                agenda: DivisionAgenda::Mercantile,
                                player_standing: 0,
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Relationships — diplomatic standing")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove: Option<FactionId> = None;
                        for (target, standing) in f.relationships.iter_mut() {
                            ui.horizontal(|ui| {
                                ui.label(&target.0);
                                let mut status = standing.status_snapshot;
                                egui::ComboBox::from_id_salt(("faction_rel", &target.0))
                                    .selected_text(format!("{status:?}"))
                                    .show_ui(ui, |ui| {
                                        for s in STATUSES {
                                            if ui
                                                .selectable_value(
                                                    &mut status,
                                                    s,
                                                    format!("{s:?}"),
                                                )
                                                .changed()
                                            {
                                                standing.status_snapshot = s;
                                                standing.affinity = s.affinity();
                                                changed = true;
                                            }
                                        }
                                    });
                                let mut treaty =
                                    standing.treaty.clone().unwrap_or_default();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut treaty)
                                            .hint_text("treaty")
                                            .desired_width(120.0),
                                    )
                                    .changed()
                                {
                                    standing.treaty =
                                        (!treaty.is_empty()).then_some(treaty);
                                    changed = true;
                                }
                                let mut war_goal =
                                    standing.war_goal.clone().unwrap_or_default();
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut war_goal)
                                            .hint_text("war goal")
                                            .desired_width(120.0),
                                    )
                                    .changed()
                                {
                                    standing.war_goal =
                                        (!war_goal.is_empty()).then_some(war_goal);
                                    changed = true;
                                }
                                if ui.button("×").clicked() {
                                    remove = Some(target.clone());
                                }
                            });
                        }
                        if let Some(target) = remove {
                            f.relationships.remove(&target);
                            changed = true;
                        }
                        ui.horizontal(|ui| {
                            // Offer factions from the catalog that aren't in
                            // the map yet.
                            egui::ComboBox::from_id_salt("faction_rel_new")
                                .selected_text(if self.new_relationship_target.is_empty() {
                                    "target faction…".to_string()
                                } else {
                                    self.new_relationship_target.clone()
                                })
                                .show_ui(ui, |ui| {
                                    for id in &all_ids {
                                        if *id == f.id.0
                                            || f.relationships
                                                .contains_key(&FactionId(id.clone()))
                                        {
                                            continue;
                                        }
                                        ui.selectable_value(
                                            &mut self.new_relationship_target,
                                            id.clone(),
                                            id,
                                        );
                                    }
                                });
                            if ui.button("Add Relationship").clicked()
                                && !self.new_relationship_target.is_empty()
                            {
                                f.relationships.insert(
                                    FactionId(self.new_relationship_target.clone()),
                                    DiplomaticStanding {
                                        affinity: 0,
                                        status_snapshot: RelationStatus::Neutral,
                                        treaty: None,
                                        war_goal: None,
                                    },
                                );
                                self.new_relationship_target.clear();
                                changed = true;
                            }
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
                self.has_changes = true;
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0xFAC7_7007);
        let doctrine = DOCTRINES[rng.next_below(4) as usize];
        let tariff_policy = match rng.next_below(4) {
            0 => TariffKind::Regulated.default_policy(),
            1 => TariffKind::Flat.default_policy(),
            2 => TariffPolicy::Dynamic,
            _ => TariffPolicy::None,
        };
        let produces = (0..1 + rng.next_below(2))
            .map(|_| GoodCategory::ALL[rng.next_below(7) as usize])
            .collect();
        let territory = (0..rng.next_below(3))
            .map(|i| SystemClaim {
                system_id: format!("system_{i}"),
                control: 20 + rng.next_below(80) as u8,
            })
            .collect();
        let color = [
            64 + rng.next_below(192) as u8,
            64 + rng.next_below(192) as u8,
            64 + rng.next_below(192) as u8,
            0xFF,
        ];
        let faction = Faction {
            id: FactionId(format!("faction_{seed:x}")),
            name: format!("Faction {seed:04}"),
            territory,
            resources: Default::default(),
            relationships: BTreeMap::new(),
            goals: Vec::new(),
            internal_divisions: Vec::new(),
            doctrine,
            tariff_policy,
            produces,
            color,
        };
        if let Some(f) = self.catalog.factions.get_mut(self.selected) {
            *f = faction;
        }
        self.has_changes = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(FactionEditor::new())
}
