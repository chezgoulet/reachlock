use super::super::app::{ContentType, Editor};

struct DialogueEditor {
    has_changes: bool,
    current_seed: u64,
}

impl DialogueEditor {
    fn new() -> Self {
        DialogueEditor {
            has_changes: false,
            current_seed: 42,
        }
    }
}

impl Editor for DialogueEditor {
    fn title(&self) -> &str {
        "Dialogue/Contract Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Contract
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        Err(format!("Dialogue editor: load from {} not yet implemented", path.display()))
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        Err(format!("Dialogue editor: save to {} not yet implemented", path.display()))
    }

    fn validate(&self) -> Vec<String> {
        Vec::new()
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Dialogue / Contract Editor");
            ui.label("Content type: Contract");
            if ui.button("Generate from Seed").clicked() {
                self.generate_from_seed(self.current_seed);
            }
        });
    }

    fn generate_from_seed(&mut self, _seed: u64) {
        self.has_changes = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(DialogueEditor::new())
}
