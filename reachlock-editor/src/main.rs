mod ai;
mod app;
mod browser;
pub mod editors;
mod io;
mod preview;
mod schema;
mod seed_workflow;
mod settings_window;

use std::sync::mpsc::channel;
use std::sync::Arc;

use app::{build_default_registry, ContentType, Editor, EditorRegistry};
use browser::ContentBrowser;
use preview::PreviewPanel;
use schema::SchemaCache;
use seed_workflow::SeedWorkflow;
use settings_window::AiSettingsWindow;

struct EditorApp {
    registry: EditorRegistry,
    open_editors: Vec<OpenEditor>,
    browser: ContentBrowser,
    seed_workflow: SeedWorkflow,
    preview: PreviewPanel,
    ai_settings: AiSettingsWindow,
    ai_prompt: String,
    ai_running: bool,
    ai_status: Arc<std::sync::Mutex<String>>,
    ai_result_rx: Option<std::sync::mpsc::Receiver<ai::AiGenOutcome>>,
    schemas: SchemaCache,
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
            ai_settings: AiSettingsWindow::load(),
            ai_prompt: String::new(),
            ai_running: false,
            ai_status: Arc::new(std::sync::Mutex::new(String::new())),
            ai_result_rx: None,
            schemas: SchemaCache::load_all(),
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

        // Poll the background AI generation thread.
        if let Some(rx) = &self.ai_result_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.ai_running = false;
                self.ai_result_rx = None;
                match outcome {
                    ai::AiGenOutcome::Ok { ct, result } => {
                        let mut applied = false;
                        if let Some(idx) = self.active_tab {
                            if let Some(open) = self.open_editors.get_mut(idx) {
                                if open.editor.content_type() == ct {
                                    match open.editor.apply_ai_json(&result.json_value) {
                                        Ok(_) => {
                                            applied = true;
                                            if !result.warnings.is_empty() {
                                                *self.ai_status.lock().unwrap() = format!(
                                                    "Applied with {} schema warning(s).",
                                                    result.warnings.len()
                                                );
                                            } else {
                                                *self.ai_status.lock().unwrap() =
                                                    "AI content applied.".into();
                                            }
                                        }
                                        Err(e) => {
                                            *self.ai_status.lock().unwrap() =
                                                format!("Applied parse failed: {e}");
                                        }
                                    }
                                }
                            }
                        }
                        if !applied {
                            *self.ai_status.lock().unwrap() =
                                "Generation returned, but the active editor changed.".into();
                        }
                    }
                    ai::AiGenOutcome::Err(e) => {
                        *self.ai_status.lock().unwrap() = format!("AI error: {e}");
                    }
                }
            }
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

                ui.menu_button("AI", |ui| {
                    if ui.button("AI Settings…").clicked() {
                        self.ai_settings.open = true;
                        ui.close_menu();
                    }
                    ui.label("Generate from the bar below the seed panel.");
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

        // AI generation bar (handoff §Phase 2.5).
        egui::TopBottomPanel::top("ai_bar")
            .resizable(false)
            .show_separator_line(true)
            .show(ctx, |ui| {
                let active_ct = self.active_tab.and_then(|idx| {
                    self.open_editors.get(idx).map(|o| o.editor.content_type())
                });
                ui.horizontal(|ui| {
                    ui.label("AI:");
                    ui.text_edit_multiline(&mut self.ai_prompt);

                    let has_schema = match active_ct {
                        Some(ct) => self.schemas.has(&ct),
                        None => false,
                    };

                    let can_generate = active_ct.is_some()
                        && has_schema
                        && !self.ai_prompt.trim().is_empty()
                        && !self.ai_running;

                    let btn = if self.ai_running {
                        "Generating…"
                    } else {
                        "Generate"
                    };
                    if ui.add_enabled(can_generate, egui::Button::new(btn)).clicked() {
                        let ct = active_ct.expect("guarded by can_generate");
                        self.ai_running = true;
                        *self.ai_status.lock().unwrap() =
                            format!("Generating {ct:?} content…");
                        let cfg = self.ai_settings.config().clone();
                        let prompt = self.ai_prompt.trim().to_string();
                        let (tx, rx) = channel();
                        self.ai_result_rx = Some(rx);
                        std::thread::spawn(move || {
                            let schemas = SchemaCache::load_all();
                            let rt = tokio::runtime::Builder::new_multi_thread()
                                .enable_all()
                                .build()
                                .unwrap();
                            let outcome = rt.block_on(async {
                                match ai::generate_content(&cfg, ct, &schemas, &prompt).await {
                                    Ok(result) => ai::AiGenOutcome::Ok { ct, result },
                                    Err(e) => ai::AiGenOutcome::Err(e),
                                }
                            });
                            let _ = tx.send(outcome);
                        });
                    }

                    if ui.button("Clear").clicked() {
                        self.ai_prompt.clear();
                    }
                });
                ui.horizontal(|ui| {
                    let status = self.ai_status.lock().unwrap().clone();
                    ui.label(&status);
                    if let Some(ct) = active_ct {
                        if !self.schemas.has(&ct) {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "No schema for this type — AI generation unavailable.",
                            );
                        } else if matches!(ct, ContentType::ItemBrowser | ContentType::SpriteViewer)
                        {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Previewers have no AI target.",
                            );
                        }
                    }
                });
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

        self.ai_settings.show(ctx);

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
