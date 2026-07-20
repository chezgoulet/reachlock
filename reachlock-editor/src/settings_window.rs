//! AI settings modal + persistence (handoff §Phase 2.5).
//!
//! Settings live in `save/editor-settings.ron` and are loaded at startup.
//! The modal lets the user edit the endpoint, model, API key, and max tokens,
//! and probe connectivity via `ai::test_connection`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::ai::{self, AiConfig};

const SETTINGS_PATH: &str = "save/editor-settings.ron";

#[derive(Default, Clone)]
enum TestStatus {
    #[default]
    Idle,
    Testing,
    Ok(String),
    Err(String),
}

pub struct AiSettingsWindow {
    pub open: bool,
    config: AiConfig,
    test_status: Arc<Mutex<TestStatus>>,
    /// Transient status message shown after a save.
    saved_msg: Option<String>,
}

impl AiSettingsWindow {
    pub fn load() -> Self {
        let config = load_config().unwrap_or_default();
        AiSettingsWindow {
            open: false,
            config,
            test_status: Arc::new(Mutex::new(TestStatus::Idle)),
            saved_msg: None,
        }
    }

    pub fn config(&self) -> &AiConfig {
        &self.config
    }

    /// Persist the current config to disk.
    pub fn save(&mut self) {
        if save_config(&self.config).is_ok() {
            self.saved_msg = Some("Saved.".into());
        } else {
            self.saved_msg = Some("Save failed.".into());
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let mut is_open = self.open;
        if !is_open {
            return;
        }
        let mut do_close = false;
        egui::Window::new("AI Settings")
            .open(&mut is_open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("Endpoint (OpenAI-compatible). Ollama default is local:");
                ui.text_edit_singleline(&mut self.config.api_base_url);

                ui.horizontal(|ui| {
                    ui.label("Model:");
                    ui.text_edit_singleline(&mut self.config.model);
                });

                ui.horizontal(|ui| {
                    ui.label("API key:");
                    ui.text_edit_singleline(&mut self.config.api_key);
                });

                ui.horizontal(|ui| {
                    ui.label("Max tokens:");
                    ui.add(
                        egui::DragValue::new(&mut self.config.max_tokens)
                            .range(256..=8192)
                            .speed(64),
                    );
                });

                ui.separator();

                {
                    let status = self.test_status.lock().unwrap().clone();
                    match status {
                        TestStatus::Idle => {}
                        TestStatus::Testing => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Testing connection…");
                            });
                        }
                        TestStatus::Ok(m) => {
                            ui.colored_label(
                                egui::Color32::GREEN,
                                format!("Connected. First model: {m}"),
                            );
                        }
                        TestStatus::Err(e) => {
                            ui.colored_label(egui::Color32::RED, format!("Error: {e}"));
                        }
                    }
                }

                ui.horizontal(|ui| {
                    if ui.button("Test Connection").clicked() {
                        *self.test_status.lock().unwrap() = TestStatus::Testing;
                        let cfg = self.config.clone();
                        let status = self.test_status.clone();
                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Builder::new_multi_thread()
                                .enable_all()
                                .build()
                                .unwrap();
                            let res = rt.block_on(ai::test_connection(&cfg));
                            let next = match res {
                                Ok(Some(m)) => TestStatus::Ok(m),
                                Ok(None) => TestStatus::Ok("(no models listed)".into()),
                                Err(e) => TestStatus::Err(e),
                            };
                            *status.lock().unwrap() = next;
                        });
                    }
                    if ui.button("Save").clicked() {
                        self.save();
                    }
                    if ui.button("Close").clicked() {
                        do_close = true;
                    }
                });

                if let Some(msg) = &self.saved_msg {
                    ui.label(msg);
                }

                ui.separator();
                ui.label(
                    "Note: Test Connection result is displayed only for live servers. \
                     For local Ollama, ensure the model is pulled.",
                );
            });
        if do_close {
            is_open = false;
        }
        self.open = is_open;
    }
}

fn settings_path() -> PathBuf {
    PathBuf::from(SETTINGS_PATH)
}

fn load_config() -> Option<AiConfig> {
    let text = std::fs::read_to_string(settings_path()).ok()?;
    ron::from_str(&text).ok()
}

fn save_config(config: &AiConfig) -> std::io::Result<()> {
    if let Some(parent) = settings_path().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = ron::ser::to_string_pretty(config, ron::ser::PrettyConfig::default())
        .map_err(std::io::Error::other)?;
    std::fs::write(settings_path(), text)
}
