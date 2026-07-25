use reachlock_core::content::event::{Consequence, Event, EventStage, TriggerCondition};

use super::super::app::{ContentType, Editor};

pub struct EventEditor {
    path: Option<std::path::PathBuf>,
    event: Event,
    has_changes: bool,
}

impl EventEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing file and the first save
    /// overwrote it.
    fn new() -> Self {
        EventEditor {
            path: None,
            event: Event {
                id: "new_event".into(),
                stages: vec![EventStage {
                    narrative_text: "Event begins.".into(),
                    trigger_conditions: vec![TriggerCondition::FlagSet {
                        flag: "intro".into(),
                    }],
                    consequences: vec![Consequence::SetFlag {
                        flag: "event_started".into(),
                    }],
                }],
                expires_after_ticks: None,
            },
            has_changes: false,
        }
    }
}

impl Editor for EventEditor {
    fn title(&self) -> &str {
        &self.event.id
    }
    fn content_type(&self) -> ContentType {
        ContentType::Event
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
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
        self.event.id = format!("event_{:#x}", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.event.id.is_empty() {
            errors.push("id is empty".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(format!("Event: {}", self.event.id));
            ui.label(format!("Stages: {}", self.event.stages.len()));
        });
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.event).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.event = ron::from_str(ron_text).map_err(|e| e.to_string())?;
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
    Box::new(EventEditor::new())
}
