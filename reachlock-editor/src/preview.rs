//! Preview panel (handoff completion §Priority 7): a context-sensitive
//! summary card for the active editor tab, rendered by each editor's
//! `Editor::preview_ui`. With no tab active it shows a welcome card and an
//! "Open Recent" list drawn from preferences.

use std::path::PathBuf;

use crate::app::Editor;

pub struct PreviewPanel;

impl PreviewPanel {
    pub fn new() -> Self {
        Self
    }

    /// Render the panel. Returns a path when the user clicks an entry in
    /// the Open Recent list.
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        active: Option<(&str, &dyn Editor)>,
        recent_files: &[String],
    ) -> Option<PathBuf> {
        let mut open_recent = None;
        match active {
            Some((name, editor)) => {
                ui.heading("Preview");
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong(name);
                    if editor.has_unsaved_changes() {
                        ui.colored_label(egui::Color32::from_rgb(0xFF, 0xB3, 0x00), "●");
                    }
                });
                ui.weak(editor.content_type().name());
                ui.add_space(6.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    editor.preview_ui(ui);
                    let issues = editor.validate();
                    if !issues.is_empty() {
                        ui.add_space(6.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(0xF4, 0x43, 0x36),
                            format!("{} validation issue(s)", issues.len()),
                        );
                    }
                });
            }
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space(24.0);
                    ui.label(
                        egui::RichText::new("REACHLOCK")
                            .heading()
                            .strong()
                            .color(egui::Color32::from_rgb(0xE8, 0x63, 0x2B)),
                    );
                    ui.weak("content editor");
                    ui.add_space(12.0);
                    ui.label("Open an editor to see a preview.");
                });
                if !recent_files.is_empty() {
                    ui.add_space(16.0);
                    ui.strong("Open Recent");
                    ui.separator();
                    for entry in recent_files {
                        let path = PathBuf::from(entry);
                        let label = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(entry)
                            .to_string();
                        if ui
                            .link(egui::RichText::new(label).monospace())
                            .on_hover_text(entry)
                            .clicked()
                        {
                            open_recent = Some(path);
                        }
                    }
                }
            }
        }
        open_recent
    }
}
