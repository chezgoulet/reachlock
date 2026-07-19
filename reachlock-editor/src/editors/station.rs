use super::super::app::{ContentType, Editor};

struct StationEditor {
    has_changes: bool,
    current_seed: u64,
}

impl StationEditor {
    fn new() -> Self {
        StationEditor {
            has_changes: false,
            current_seed: 42,
        }
    }
}

impl Editor for StationEditor {
    fn title(&self) -> &str {
        "Station Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Station
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        Err(format!("Station editor: load from {} not yet implemented", path.display()))
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        Err(format!("Station editor: save to {} not yet implemented", path.display()))
    }

    fn validate(&self) -> Vec<String> {
        Vec::new()
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Station Editor");
            ui.label("Content type: Station");
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
    Box::new(StationEditor::new())
}
