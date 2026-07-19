pub struct PreviewPanel;

impl PreviewPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn ui(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.heading("Preview");
            ui.add_space(8.0);
            ui.label("Select an editor to see a preview.");
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::GRAY, "Preview panel placeholder");
        });
    }
}
