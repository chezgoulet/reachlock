use crate::app::ContentType;

pub struct ContentBrowser;

impl ContentBrowser {
    pub fn new() -> Self {
        Self
    }

    pub fn ui(&mut self, ctx: &egui::Context, on_open: &mut dyn FnMut(String, ContentType)) {
        egui::SidePanel::left("browser_panel")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Content Browser");
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let types = ContentType::all();
                    for ct in types {
                        let label = ct.name();
                        if ui.button(label).clicked() {
                            on_open(format!("new_{}", ct.directory()), *ct);
                        }
                    }
                });

                ui.separator();
                ui.label("Right-click items to delete.");
            });
    }
}
