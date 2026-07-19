use reachlock_core::soul::types::{
    EmotionalState, Identity, Mood, Personality, SoulFile, SpeakingStyle, Species,
};

use super::super::app::{ContentType, Editor};

struct SoulEditor {
    soul: SoulFile,
    has_changes: bool,
    file_path: Option<std::path::PathBuf>,
    formality: f32,
    verbosity: f32,
    humor: f32,
    aggression: f32,
}

impl SoulEditor {
    fn new() -> Self {
        SoulEditor {
            soul: SoulFile {
                id: "new_soul".into(),
                name: "New Character".into(),
                species: Species::Human,
                portrait_id: String::new(),
                identity: Identity {
                    origin: "unknown".into(),
                    faction_affiliation: "independent".into(),
                    role: "crew".into(),
                    public_bio: String::new(),
                },
                personality: Personality {
                    traits: vec![],
                    values: vec![],
                    speaking_style: SpeakingStyle::Elaborate,
                    quirks: vec![],
                },
                emotional_state: EmotionalState {
                    dominant_mood: Mood::Stable,
                    intensity: 512,
                    triggers: vec![],
                },
                memory_tree: vec![],
                relationship_graph: vec![],
                goals: vec![],
                breaking_points: vec![],
                contracts: vec![],
                backstory: String::new(),
                secrets: vec![],
                dialogue: None,
                deflections: vec![],
            },
            has_changes: false,
            file_path: None,
            formality: 0.5,
            verbosity: 0.5,
            humor: 0.3,
            aggression: 0.2,
        }
    }

    fn generate_soul_from_seed(seed: u64) -> SoulFile {
        use reachlock_core::util::rng::SeededRng;
        let mut rng = SeededRng::new(seed ^ 0x50_50);

        let species_list = [Species::Human, Species::Android, Species::Robot];
        let species = species_list[rng.next_below(3) as usize];

        let styles = [
            SpeakingStyle::Terse,
            SpeakingStyle::Elaborate,
            SpeakingStyle::Technical,
            SpeakingStyle::Lyrical,
            SpeakingStyle::Sarcastic,
            SpeakingStyle::Blunt,
            SpeakingStyle::Formal,
            SpeakingStyle::Wry,
            SpeakingStyle::Warm,
        ];
        let style = styles[rng.next_below(styles.len() as u64) as usize];

        let moods = [
            Mood::Stable,
            Mood::Happy,
            Mood::Tense,
            Mood::Suspicious,
            Mood::Grateful,
            Mood::Focused,
        ];
        let mood = moods[rng.next_below(moods.len() as u64) as usize];

        let names = [
            "Kaelen",
            "Zeryn",
            "Mira",
            "Torben",
            "Saris",
            "Lyra",
            "Dax",
            "Vella",
        ];
        let name_idx = rng.next_below(names.len() as u64) as usize;

        SoulFile {
            id: format!("soul_{seed:x}"),
            name: names[name_idx].into(),
            species,
            portrait_id: format!("portrait_{seed:x}"),
            identity: Identity {
                origin: "unknown sector".into(),
                faction_affiliation: "independent".into(),
                role: "crew".into(),
                public_bio: format!("A {species:?} crew member."),
            },
            personality: Personality {
                traits: vec!["adaptable".into()],
                values: vec!["survival".into()],
                speaking_style: style,
                quirks: vec![],
            },
            emotional_state: EmotionalState {
                dominant_mood: mood,
                intensity: 256 + (rng.next_below(768)) as i64,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![],
            dialogue: None,
            deflections: vec![],
        }
    }
}

impl Editor for SoulEditor {
    fn title(&self) -> &str {
        "Soul Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Soul
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let soul: SoulFile =
            crate::io::read_ron(path).map_err(|e| format!("load soul: {e}"))?;
        self.soul = soul;
        self.has_changes = false;
        self.file_path = Some(path.to_path_buf());
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.soul).map_err(|e| format!("save soul: {e}"))
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.soul.id.is_empty() {
            errors.push("soul id must not be empty".into());
        }
        if self.soul.name.is_empty() {
            errors.push("name must not be empty".into());
        }
        errors
    }

    fn ui(&mut self, _ctx: &egui::Context) {
        egui::TopBottomPanel::top("soul_toolbar").show(_ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    self.generate_from_seed(
                        self.soul.id.parse::<u64>().unwrap_or(42),
                    );
                }
                let label = if self.has_changes { " (modified)" } else { "" };
                ui.label(format!("{}: {}", self.title(), self.soul.name));
                ui.label(label);
            });
        });

        egui::CentralPanel::default().show(_ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Soul Configuration");

                ui.separator();
                ui.label("ID:");
                let mut id = self.soul.id.clone();
                if ui.text_edit_singleline(&mut id).changed() {
                    self.soul.id = id;
                    self.has_changes = true;
                }

                ui.label("Name:");
                let mut name = self.soul.name.clone();
                if ui.text_edit_singleline(&mut name).changed() {
                    self.soul.name = name;
                    self.has_changes = true;
                }

                ui.separator();
                ui.label("Species:");
                let species_options = [
                    (Species::Human, "Human"),
                    (Species::Android, "Android"),
                    (Species::Robot, "Robot"),
                ];
                for (sp, label) in &species_options {
                    let selected = self.soul.species == *sp;
                    if ui.radio(selected, *label).clicked() {
                        self.soul.species = *sp;
                        self.has_changes = true;
                    }
                }

                ui.separator();
                ui.label("Backstory:");
                let mut backstory = self.soul.backstory.clone();
                if ui
                    .text_edit_multiline(&mut backstory)
                    .changed()
                {
                    self.soul.backstory = backstory;
                    self.has_changes = true;
                }

                ui.separator();
                ui.heading("Speaking Style Sliders");
                ui.label("Formality:");
                if ui
                    .add(egui::Slider::new(&mut self.formality, 0.0..=1.0))
                    .changed()
                {
                    self.has_changes = true;
                }
                ui.label("Verbosity:");
                if ui
                    .add(egui::Slider::new(&mut self.verbosity, 0.0..=1.0))
                    .changed()
                {
                    self.has_changes = true;
                }
                ui.label("Humor:");
                if ui
                    .add(egui::Slider::new(&mut self.humor, 0.0..=1.0))
                    .changed()
                {
                    self.has_changes = true;
                }
                ui.label("Aggression:");
                if ui
                    .add(egui::Slider::new(&mut self.aggression, 0.0..=1.0))
                    .changed()
                {
                    self.has_changes = true;
                }

                ui.separator();
                ui.heading("Portrait Preview");
                ui.add_space(60.0);
                ui.vertical_centered(|ui| {
                    ui.label("[ Portrait Preview Area ]");
                    if !self.soul.portrait_id.is_empty() {
                        ui.label(format!("Portrait: {}", self.soul.portrait_id));
                    }
                });
                ui.add_space(60.0);

                ui.separator();
                ui.heading("Relationships");
                for rel in &self.soul.relationship_graph {
                    ui.label(format!(
                        "→ {} (trust: {}, familiarity: {})",
                        rel.target_id, rel.trust, rel.familiarity
                    ));
                }

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
        self.soul = Self::generate_soul_from_seed(seed);
        self.has_changes = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(SoulEditor::new())
}
