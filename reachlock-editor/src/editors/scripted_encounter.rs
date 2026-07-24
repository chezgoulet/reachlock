//! Scripted Encounter editor (S41, Phase B): authored multi-scene encounters.

use reachlock_core::generator::scripted_encounter::ScriptedEncounter;

use super::super::app::{ContentType, Editor};

pub struct ScriptedEncounterEditor {
    path: Option<std::path::PathBuf>,
    encounter: ScriptedEncounter,
    has_changes: bool,
}

impl ScriptedEncounterEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::ScriptedEncounter.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok().into_iter().flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(e) = crate::io::read_ron::<ScriptedEncounter>(path) {
                return ScriptedEncounterEditor { path: Some(path.clone()), encounter: e, has_changes: false };
            }
        }
        ScriptedEncounterEditor {
            path: None,
            encounter: ScriptedEncounter {
                id: "new_encounter".into(),
                title: "New Encounter".into(),
                encounter_type: reachlock_core::generator::scripted_encounter::ScriptedEncounterType::StoryBeat,
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
    fn title(&self) -> &str { &self.encounter.id }
    fn content_type(&self) -> ContentType { ContentType::ScriptedEncounter }
    fn has_unsaved_changes(&self) -> bool { self.has_changes }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let e: ScriptedEncounter = crate::io::read_ron(path).map_err(|e| format!("reading encounter: {e}"))?;
        self.encounter = e;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.encounter).map_err(|e| format!("saving encounter: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root().join(ContentType::ScriptedEncounter.directory()).join("generated_encounter.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
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
        if self.encounter.id.is_empty() { errors.push("id is empty".into()); }
        if self.encounter.scenes.is_empty() { errors.push("no scenes defined".into()); }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("ID: {}", self.encounter.id));
            ui.label(format!("Type: {:?}", self.encounter.encounter_type));
            ui.label(format!("Scenes: {}", self.encounter.scenes.len()));
            ui.label(format!("Triggers: {:?}", self.encounter.trigger));
        });
    }
    fn mark_saved(&mut self) { self.has_changes = false; }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ScriptedEncounterEditor::load_or_new())
}
