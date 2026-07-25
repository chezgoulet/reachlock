//! Scripted Encounter editor (S41, Phase B): authored multi-scene encounters.

use reachlock_core::generator::scripted_encounter::ScriptedEncounter;

use super::super::app::{ContentType, Editor};

pub struct ScriptedEncounterEditor {
    path: Option<std::path::PathBuf>,
    encounter: ScriptedEncounter,
    has_changes: bool,
}

impl ScriptedEncounterEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing file and the first save
    /// overwrote it.
    fn new() -> Self {
        ScriptedEncounterEditor {
            path: None,
            encounter: ScriptedEncounter {
                id: "new_encounter".into(),
                title: "New Encounter".into(),
                encounter_type:
                    reachlock_core::generator::scripted_encounter::ScriptedEncounterType::StoryBeat,
                trigger: reachlock_core::generator::scripted_encounter::EncounterTrigger::Manual,
                prerequisites: vec![],
                scenes: vec![],
                on_complete: vec![],
                repeatable: false,
                cooldown_ticks: None,
            },
            has_changes: false,
        }
    }
}

impl Editor for ScriptedEncounterEditor {
    #[allow(clippy::misnamed_getters)]
    fn title(&self) -> &str {
        &self.encounter.id
    }
    fn content_type(&self) -> ContentType {
        ContentType::ScriptedEncounter
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let e: ScriptedEncounter =
            crate::io::read_ron(path).map_err(|e| format!("reading encounter: {e}"))?;
        self.encounter = e;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.encounter).map_err(|e| format!("saving encounter: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        // Only write when dirty, and never invent a filename: the old
        // fallback name meant two new documents overwrote each other.
        // Returning Ok(false) with no path lets the shell run Save As.
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
        self.encounter.id = format!("encounter_{:#x}", seed);
        self.encounter.title = format!("Generated Encounter ({})", seed);
        self.encounter.scenes = vec![
            reachlock_core::generator::scripted_encounter::EncounterScene {
                scene_id: "opening".into(),
                narrative: format!("Scene generated from seed {}", seed),
                speaker: None,
                choices: vec![],
                time_pressure: None,
            },
        ];
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.encounter.id.is_empty() {
            errors.push("id is empty".into());
        }
        if self.encounter.scenes.is_empty() {
            errors.push("no scenes defined".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(format!("ID: {}", self.encounter.id));
            ui.label(format!("Type: {:?}", self.encounter.encounter_type));
            ui.label(format!("Scenes: {}", self.encounter.scenes.len()));
            ui.label(format!("Triggers: {:?}", self.encounter.trigger));
        });
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.encounter).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.encounter = ron::from_str(ron_text).map_err(|e| e.to_string())?;
        self.has_changes = true;
        Ok(())
    }

    /// Reroll only renames the id, which would break every cross-reference
    /// pointing at it. Opt out rather than corrupt the content graph.
    fn accept_seed_reroll(&self) -> bool {
        false
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        ui.strong(self.content_type().name());
        let issues = self.validate();
        if issues.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(0x4C, 0xAF, 0x50), "✔ clean");
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(0xE5, 0x73, 0x73),
                format!("✘ {} issue(s)", issues.len()),
            );
            for issue in issues.iter().take(5) {
                ui.weak(issue);
            }
        }
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ScriptedEncounterEditor::new())
}
