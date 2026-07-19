mod app;
mod browser;
pub mod editors;
mod io;
mod preview;
mod seed_workflow;

use app::{build_default_registry, ContentType, Editor, EditorRegistry};
use browser::ContentBrowser;
use preview::PreviewPanel;
use seed_workflow::SeedWorkflow;

struct EditorApp {
    registry: EditorRegistry,
    open_editors: Vec<OpenEditor>,
    browser: ContentBrowser,
    seed_workflow: SeedWorkflow,
    preview: PreviewPanel,
    status_text: String,
    active_tab: Option<usize>,
    show_browser: bool,
    browser_content_trigger: Option<(String, ContentType)>,
}

struct OpenEditor {
    editor: Box<dyn Editor>,
    _name: String,
    path: Option<std::path::PathBuf>,
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            registry: build_default_registry(),
            open_editors: Vec::new(),
            browser: ContentBrowser::new(),
            seed_workflow: SeedWorkflow::new(),
            preview: PreviewPanel::new(),
            status_text: "Ready".into(),
            active_tab: None,
            show_browser: true,
            browser_content_trigger: None,
        }
    }
}

impl EditorApp {
    fn open_new_editor(&mut self, name: &str, ct: ContentType) {
        if let Some(editor) = self.registry.create(ct) {
            let idx = self.open_editors.len();
            self.open_editors.push(OpenEditor {
                editor,
                _name: name.to_string(),
                path: None,
            });
            self.active_tab = Some(idx);
            self.status_text = format!("Opened {name}");
        } else {
            self.status_text = format!("No editor for {:?}", ct);
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some((name, ct)) = self.browser_content_trigger.take() {
            self.open_new_editor(&name, ct);
        }

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Hull").clicked() {
                        self.open_new_editor("new_hull", ContentType::HullFrame);
                        ui.close_menu();
                    }
                    if ui.button("New Soul").clicked() {
                        self.open_new_editor("new_soul", ContentType::Soul);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Save").clicked() {
                        if let Some(idx) = self.active_tab {
                            if let Some(open) = self.open_editors.get(idx) {
                                if let Some(path) = &open.path {
                                    match open.editor.save(path) {
                                        Ok(_) => self.status_text = "Saved".into(),
                                        Err(e) => self.status_text = format!("Save error: {e}"),
                                    }
                                } else {
                                    self.status_text = "No file path set".into();
                                }
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save As...").clicked() {
                        self.status_text = "Save As not yet implemented".into();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo").clicked() {
                        ui.close_menu();
                    }
                    if ui.button("Redo").clicked() {
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Preferences").clicked() {
                        self.status_text = "Preferences not yet implemented".into();
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    let mut browser_visible = self.show_browser;
                    if ui
                        .checkbox(&mut browser_visible, "Content Browser")
                        .changed()
                    {
                        self.show_browser = browser_visible;
                    }
                    ui.close_menu();
                });
            });
        });

        if self.show_browser {
            let on_open = &mut |name: String, ct: ContentType| {
                self.browser_content_trigger = Some((name, ct));
            };
            self.browser.ui(ctx, on_open);
        }

        egui::TopBottomPanel::bottom("status_line").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_text);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let modified_count = self
                        .open_editors
                        .iter()
                        .filter(|o| o.editor.has_unsaved_changes())
                        .count();
                    if modified_count > 0 {
                        ui.label(format!("{modified_count} unsaved"));
                    }
                    ui.label(format!("{} editor(s) open", self.open_editors.len()));
                });
            });
        });

        egui::TopBottomPanel::top("seed_panel").show(ctx, |ui| {
            self.seed_workflow.ui(ui);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.open_editors.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("ReachLock Content Editor");
                    ui.add_space(8.0);
                    ui.label("Open a file from the Content Browser or use File → New.");
                    ui.add_space(4.0);
                    ui.label("Press F1 for help.");
                });
            } else {
                egui::TopBottomPanel::top("editor_tabs")
                    .resizable(false)
                    .show_separator_line(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            for (i, open) in self.open_editors.iter().enumerate() {
                                let title = format!(
                                    "{}{}",
                                    open.editor.title(),
                                    if open.editor.has_unsaved_changes() {
                                        " *"
                                    } else {
                                        ""
                                    }
                                );
                                let selected = self.active_tab == Some(i);
                                if ui
                                    .selectable_label(selected, &title)
                                    .clicked()
                                {
                                    self.active_tab = Some(i);
                                }
                                if ui.button("x").clicked() {
                                    self.open_editors.remove(i);
                                    if self.active_tab == Some(i) {
                                        self.active_tab = if self.open_editors.is_empty() {
                                            None
                                        } else {
                                            Some(if i > 0 { i - 1 } else { 0 })
                                        };
                                    } else if let Some(a) = self.active_tab {
                                        if i < a {
                                            self.active_tab = Some(a - 1);
                                        }
                                    }
                                    break;
                                }
                            }
                        });
                    });

                if let Some(idx) = self.active_tab {
                    if let Some(open) = self.open_editors.get_mut(idx) {
                        open.editor.ui(ctx);
                    }
                }
            }
        });

        egui::SidePanel::right("preview_panel")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                self.preview.ui(ctx, ui);
            });

        ctx.request_repaint();
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ReachLock Content Editor")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ReachLock Content Editor",
        options,
        Box::new(|_cc| Ok(Box::new(EditorApp::default()))),
    )
}
