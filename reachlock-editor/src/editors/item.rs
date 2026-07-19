use super::super::app::{ContentType, Editor};

struct ItemEditor {
    has_changes: bool,
    current_seed: u64,
}

impl ItemEditor {
    fn new() -> Self {
        ItemEditor {
            has_changes: false,
            current_seed: 42,
        }
    }
}

impl Editor for ItemEditor {
    fn title(&self) -> &str {
        "Item Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Item
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        Err(format!("Item editor: load from {} not yet implemented", path.display()))
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        Err(format!("Item editor: save to {} not yet implemented", path.display()))
    }

    fn validate(&self) -> Vec<String> {
        Vec::new()
    }

    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Item Editor");
            ui.label("Content type: Item");
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
    Box::new(ItemEditor::new())
}
