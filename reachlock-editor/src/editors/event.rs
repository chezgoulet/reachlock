use reachlock_core::content::event::{Consequence, Event, EventStage, TriggerCondition};

use super::super::app::{ContentType, Editor};

pub struct EventEditor {
    path: Option<std::path::PathBuf>,
    event: Event,
    has_changes: bool,
}

impl EventEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Event.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(e) = crate::io::read_ron::<Event>(path) {
                return EventEditor { path: Some(path.clone()), event: e, has_changes: false };
            }
        }
        EventEditor {
            path: None,
            event: Event {
                id: "new_event".into(),
                stages: vec![EventStage {
                    narrative_text: "Event begins.".into(),
                    trigger_conditions: vec![TriggerCondition::FlagSet { flag: "intro".into() }],
                    consequences: vec![Consequence::SetFlag { flag: "event_started".into() }],
                }],
                expires_after_ticks: None,
            },
            has_changes: false,
        }
    }
}

impl Editor for EventEditor {
    fn title(&self) -> &str { &self.event.id }
    fn content_type(&self) -> ContentType { ContentType::Event }
    fn has_unsaved_changes(&self) -> bool { self.has_changes }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let e: Event = crate::io::read_ron(path).map_err(|e| format!("reading event: {e}"))?;
        self.event = e;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.event).map_err(|e| format!("saving event: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root().join(ContentType::Event.directory()).join("generated_event.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
        self.has_changes = false;
        Ok(true)
    }
    fn generate_from_seed(&mut self, seed: u64) {
        self.event.id = format!("event_{:#x}", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.event.id.is_empty() { errors.push("id is empty".into()); }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("Event: {}", self.event.id));
            ui.label(format!("Stages: {}", self.event.stages.len()));
        });
    }
    fn mark_saved(&mut self) { self.has_changes = false; }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(EventEditor::load_or_new())
}
