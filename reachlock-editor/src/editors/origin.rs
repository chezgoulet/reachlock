use reachlock_core::content::origin::{
    CrewAssignment, FactionStandingDelta, ItemStack, LogEntryDraft, Origin,
};
use reachlock_core::seed::Seed;

use super::super::app::{ContentType, Editor};

pub struct OriginEditor {
    path: Option<std::path::PathBuf>,
    origin: Origin,
    has_changes: bool,
}

impl OriginEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing origin and the first
    /// save overwrote it.
    fn new() -> Self {
        OriginEditor {
            path: None,
            origin: Origin {
                id: "new_origin".into(),
                name: "New Origin".into(),
                description: String::new(),
                icon: "default".into(),
                starting_career: "freelance".into(),
                starting_rank: 1,
                faction_deltas: vec![],
                starting_credits: 1000,
                ship_template: None,
                ship_seed: None,
                starting_gear: vec![],
                starting_crew: vec![],
                known_systems: vec![],
                start_system: Seed::new(0),
                start_location: "Starting Station".into(),
                opening_log_entries: vec![],
            },
            has_changes: false,
        }
    }
}

impl Editor for OriginEditor {
    fn title(&self) -> &str {
        &self.origin.id
    }
    fn content_type(&self) -> ContentType {
        ContentType::Origin
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let o: Origin = crate::io::read_ron(path).map_err(|e| format!("reading origin: {e}"))?;
        self.origin = o;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.origin).map_err(|e| format!("saving origin: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        // Only write when dirty, and never invent a filename.
        if !self.has_changes {
            return Ok(self.path.is_some());
        }
        let Some(path) = self.path.clone() else {
            return Ok(false);
        };
        self.save(&path)?;
        self.has_changes = false;
        Ok(true)
    }
    fn generate_from_seed(&mut self, seed: u64) {
        self.origin.id = format!("origin_{:#x}", seed);
        self.origin.start_system = Seed::new(seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.origin.id.is_empty() {
            errors.push("id is empty".into());
        }
        if self.origin.name.is_empty() {
            errors.push("name is empty".into());
        }
        if self.origin.start_location.is_empty() {
            errors.push("start_location is empty".into());
        }
        if self.origin.starting_career.is_empty() {
            errors.push("starting_career is empty".into());
        }
        if self.origin.starting_rank == 0 {
            errors.push("starting_rank must be at least 1".into());
        }
        for (i, entry) in self.origin.opening_log_entries.iter().enumerate() {
            if entry.title.is_empty() {
                errors.push(format!("log entry {i}: title is empty"));
            }
            if entry.body.is_empty() {
                errors.push(format!("log entry {i}: body is empty"));
            }
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Origin Editor");

                // --- Identity ---
                ui.collapsing("Identity", |ui| {
                    let mut changed = false;
                    changed |= ui
                        .text_edit_singleline(&mut self.origin.id)
                        .labelled_by(ui.label("ID").id)
                        .changed();
                    ui.label("id");
                    changed |= ui
                        .text_edit_singleline(&mut self.origin.name)
                        .labelled_by(ui.label("Name").id)
                        .changed();
                    ui.label("name");
                    changed |= ui
                        .text_edit_multiline(&mut self.origin.description)
                        .labelled_by(ui.label("Description").id)
                        .changed();
                    ui.label("description");
                    changed |= ui
                        .text_edit_singleline(&mut self.origin.icon)
                        .labelled_by(ui.label("Icon").id)
                        .changed();
                    ui.label("icon");
                    if changed {
                        self.touch();
                    }
                });

                // --- Career ---
                ui.collapsing("Career", |ui| {
                    let mut changed = false;
                    changed |= ui
                        .text_edit_singleline(&mut self.origin.starting_career)
                        .labelled_by(ui.label("Career Path ID").id)
                        .changed();
                    ui.label("starting_career");
                    let mut rank = self.origin.starting_rank as u32;
                    changed |= ui.add(egui::Slider::new(&mut rank, 1..=20)).changed();
                    self.origin.starting_rank = rank as u8;
                    if changed {
                        self.touch();
                    }
                });

                // --- Faction Deltas ---
                ui.collapsing("Faction Deltas", |ui| {
                    let mut changed = false;
                    let mut remove: Option<usize> = None;
                    for (i, delta) in self.origin.faction_deltas.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui
                                .text_edit_singleline(&mut delta.faction_id)
                                .labelled_by(ui.label("Faction").id)
                                .changed();
                            ui.label("faction");
                            changed |= ui
                                .add(egui::Slider::new(&mut delta.delta, -100..=100))
                                .changed();
                            ui.label("delta");
                            if ui.button("×").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.origin.faction_deltas.remove(i);
                        changed = true;
                    }
                    if ui.button("Add Faction Delta").clicked() {
                        self.origin.faction_deltas.push(FactionStandingDelta {
                            faction_id: String::new(),
                            delta: 0,
                        });
                        changed = true;
                    }
                    if changed {
                        self.touch();
                    }
                });

                // --- Credits ---
                ui.collapsing("Credits", |ui| {
                    let mut credits = self.origin.starting_credits;
                    if ui
                        .add(egui::Slider::new(&mut credits, 0..=1_000_000))
                        .changed()
                    {
                        self.origin.starting_credits = credits;
                        self.touch();
                    }
                });

                // --- Ship ---
                ui.collapsing("Ship", |ui| {
                    let mut changed = false;
                    let mut has_template = self.origin.ship_template.is_some();
                    changed |= ui
                        .checkbox(&mut has_template, "Has ship template")
                        .changed();
                    if has_template {
                        let template = self.origin.ship_template.get_or_insert_with(String::new);
                        changed |= ui
                            .text_edit_singleline(template)
                            .labelled_by(ui.label("Template ID").id)
                            .changed();
                        ui.label("ship_template");
                    } else {
                        self.origin.ship_template = None;
                    }
                    let mut has_seed = self.origin.ship_seed.is_some();
                    changed |= ui.checkbox(&mut has_seed, "Has ship seed").changed();
                    if has_seed {
                        let mut val = self.origin.ship_seed.map(|s| s.value()).unwrap_or(0);
                        changed |= ui.add(egui::Slider::new(&mut val, 0..=Seed::MAX)).changed();
                        self.origin.ship_seed = Some(Seed::new(val));
                    } else {
                        self.origin.ship_seed = None;
                    }
                    if changed {
                        self.touch();
                    }
                });

                // --- Gear ---
                ui.collapsing("Gear", |ui| {
                    let mut changed = false;
                    let mut remove: Option<usize> = None;
                    for (i, stack) in self.origin.starting_gear.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            changed |= ui
                                .text_edit_singleline(&mut stack.item_id)
                                .labelled_by(ui.label("Item ID").id)
                                .changed();
                            ui.label("item_id");
                            let mut count = stack.count;
                            changed |= ui.add(egui::Slider::new(&mut count, 1..=9999)).changed();
                            stack.count = count;
                            if ui.button("×").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.origin.starting_gear.remove(i);
                        changed = true;
                    }
                    if ui.button("Add Item").clicked() {
                        self.origin.starting_gear.push(ItemStack {
                            item_id: String::new(),
                            count: 1,
                        });
                        changed = true;
                    }
                    if changed {
                        self.touch();
                    }
                });

                // --- Crew ---
                ui.collapsing("Crew", |ui| {
                    let mut changed = false;
                    let mut remove: Option<usize> = None;
                    for (i, crew) in self.origin.starting_crew.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.label(format!("Crew #{i}"));
                            match crew {
                                CrewAssignment::Authored { soul_id, role } => {
                                    let mut is_authored = true;
                                    changed |= ui.checkbox(&mut is_authored, "Authored").changed();
                                    changed |= ui
                                        .text_edit_singleline(soul_id)
                                        .labelled_by(ui.label("Soul ID").id)
                                        .changed();
                                    ui.label("soul_id");
                                    changed |= ui
                                        .text_edit_singleline(role)
                                        .labelled_by(ui.label("Role").id)
                                        .changed();
                                    ui.label("role");
                                    if !is_authored {
                                        let seed = 0;
                                        let species = "Human".to_string();
                                        *crew = CrewAssignment::Procedural {
                                            seed: Seed::new(seed),
                                            species,
                                            role: role.clone(),
                                        };
                                        changed = true;
                                    }
                                }
                                CrewAssignment::Procedural {
                                    seed,
                                    species,
                                    role,
                                } => {
                                    let mut is_authored = false;
                                    changed |= ui.checkbox(&mut is_authored, "Authored").changed();
                                    let mut val = seed.value();
                                    changed |= ui
                                        .add(egui::Slider::new(&mut val, 0..=Seed::MAX))
                                        .changed();
                                    *seed = Seed::new(val);
                                    changed |= ui
                                        .text_edit_singleline(species)
                                        .labelled_by(ui.label("Species").id)
                                        .changed();
                                    ui.label("species");
                                    changed |= ui
                                        .text_edit_singleline(role)
                                        .labelled_by(ui.label("Role").id)
                                        .changed();
                                    ui.label("role");
                                    if is_authored {
                                        let soul_id = String::new();
                                        *crew = CrewAssignment::Authored {
                                            soul_id,
                                            role: role.clone(),
                                        };
                                        changed = true;
                                    }
                                }
                            }
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.origin.starting_crew.remove(i);
                        changed = true;
                    }
                    if ui.button("Add Crew").clicked() {
                        self.origin.starting_crew.push(CrewAssignment::Authored {
                            soul_id: String::new(),
                            role: "crew".into(),
                        });
                        changed = true;
                    }
                    if changed {
                        self.touch();
                    }
                });

                // --- Known Systems ---
                ui.collapsing("Known Systems", |ui| {
                    let mut changed = false;
                    let mut remove: Option<usize> = None;
                    for (i, system) in self.origin.known_systems.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let mut val = system.value();
                            changed |= ui.add(egui::Slider::new(&mut val, 0..=Seed::MAX)).changed();
                            *system = Seed::new(val);
                            if ui.button("×").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.origin.known_systems.remove(i);
                        changed = true;
                    }
                    if ui.button("Add System").clicked() {
                        self.origin.known_systems.push(Seed::new(0));
                        changed = true;
                    }
                    if changed {
                        self.touch();
                    }
                });

                // --- Start Location ---
                ui.collapsing("Start Location", |ui| {
                    let mut changed = false;
                    let mut val = self.origin.start_system.value();
                    changed |= ui.add(egui::Slider::new(&mut val, 0..=Seed::MAX)).changed();
                    self.origin.start_system = Seed::new(val);
                    changed |= ui
                        .text_edit_singleline(&mut self.origin.start_location)
                        .labelled_by(ui.label("Location").id)
                        .changed();
                    if changed {
                        self.touch();
                    }
                });

                // --- Log Entries ---
                ui.collapsing("Opening Log Entries", |ui| {
                    let mut changed = false;
                    let mut remove: Option<usize> = None;
                    for (i, entry) in self.origin.opening_log_entries.iter_mut().enumerate() {
                        ui.group(|ui| {
                            ui.label(format!("Entry #{i}"));
                            changed |= ui
                                .text_edit_singleline(&mut entry.title)
                                .labelled_by(ui.label("Title").id)
                                .changed();
                            ui.label("title");
                            changed |= ui
                                .text_edit_multiline(&mut entry.body)
                                .labelled_by(ui.label("Body").id)
                                .changed();
                            ui.label("body");
                            let mut tick = entry.tick_offset;
                            changed |= ui
                                .add(egui::Slider::new(&mut tick, 0..=1_000_000))
                                .changed();
                            entry.tick_offset = tick;
                            if ui.button("Remove").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.origin.opening_log_entries.remove(i);
                        changed = true;
                    }
                    if ui.button("Add Log Entry").clicked() {
                        self.origin.opening_log_entries.push(LogEntryDraft {
                            title: String::new(),
                            body: String::new(),
                            tick_offset: 0,
                        });
                        changed = true;
                    }
                    if changed {
                        self.touch();
                    }
                });
            });
        });
    }
    fn preview_ui(&self, ui: &mut egui::Ui) {
        ui.heading(&self.origin.name);
        ui.label(&self.origin.description);
        ui.separator();
        ui.label(format!(
            "Career: {} (Rank {})",
            self.origin.starting_career, self.origin.starting_rank
        ));
        ui.label(format!("Credits: {}", self.origin.starting_credits));
        if let Some(ref tmpl) = self.origin.ship_template {
            ui.label(format!("Ship: {tmpl}"));
        }
        ui.label(format!(
            "Start: {} (seed {})",
            self.origin.start_location,
            self.origin.start_system.value()
        ));
        ui.label(format!(
            "Crew: {} member(s)",
            self.origin.starting_crew.len()
        ));
        ui.label(format!("Gear: {} item(s)", self.origin.starting_gear.len()));
        ui.label(format!(
            "Faction deltas: {}",
            self.origin.faction_deltas.len()
        ));
    }
    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.origin).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.origin = ron::from_str(ron_text).map_err(|e| e.to_string())?;
        self.has_changes = true;
        Ok(())
    }

    fn accept_seed_reroll(&self) -> bool {
        false
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(OriginEditor::new())
}
