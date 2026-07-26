//! Provider settings modal + persistence (S101 P1).
//!
//! Settings live in `save/editor-settings.ron`. Pre-S101 the file held a
//! single `AiConfig`; it now holds a list of named provider profiles plus the
//! active index, so an author can keep a local model and a cloud model side by
//! side and switch per task. [`crate::agent::load_config`] migrates the old
//! shape rather than silently resetting it.

use std::sync::{Arc, Mutex};

use crate::agent::{AgentConfig, ProviderKind, ProviderProfile};

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
    config: AgentConfig,
    test_status: Arc<Mutex<TestStatus>>,
    /// Transient status message shown after a save.
    saved_msg: Option<String>,
}

impl AiSettingsWindow {
    pub fn load() -> Self {
        AiSettingsWindow {
            open: false,
            config: crate::agent::load_config(),
            test_status: Arc::new(Mutex::new(TestStatus::Idle)),
            saved_msg: None,
        }
    }

    /// The profile requests should be built from.
    pub fn active_profile(&self) -> &ProviderProfile {
        self.config.active()
    }

    /// Persist the current config to disk.
    pub fn save(&mut self) {
        self.saved_msg = Some(match crate::agent::save_config(&self.config) {
            Ok(()) => "Saved.".into(),
            Err(e) => format!("Save failed: {e}"),
        });
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
                // Profile picker. Switching here changes which endpoint
                // every request goes to, so it sits above the fields it edits.
                ui.horizontal(|ui| {
                    ui.label("Profile:");
                    let active = self.config.active.min(self.config.profiles.len() - 1);
                    egui::ComboBox::from_id_salt("provider_profile")
                        .selected_text(self.config.profiles[active].name.clone())
                        .show_ui(ui, |ui| {
                            for (i, p) in self.config.profiles.iter().enumerate() {
                                ui.selectable_value(
                                    &mut self.config.active,
                                    i,
                                    format!("{} ({})", p.name, p.kind.label()),
                                );
                            }
                        });
                    if ui.button("+ Add").clicked() {
                        self.config.profiles.push(ProviderProfile::local_default());
                        self.config.active = self.config.profiles.len() - 1;
                    }
                    // Never remove the last profile: `active()` indexes the
                    // list and an empty list would leave nothing to send with.
                    let can_remove = self.config.profiles.len() > 1;
                    if ui
                        .add_enabled(can_remove, egui::Button::new("Remove"))
                        .clicked()
                    {
                        let i = self.config.active;
                        self.config.profiles.remove(i);
                        self.config.active = 0;
                    }
                });

                ui.separator();

                let active = self.config.active.min(self.config.profiles.len() - 1);
                let profile = &mut self.config.profiles[active];

                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut profile.name);
                });

                ui.horizontal(|ui| {
                    ui.label("API:");
                    egui::ComboBox::from_id_salt("provider_kind")
                        .selected_text(profile.kind.label())
                        .show_ui(ui, |ui| {
                            for kind in [ProviderKind::OpenAiCompatible, ProviderKind::Anthropic] {
                                if ui
                                    .selectable_value(&mut profile.kind, kind, kind.label())
                                    .clicked()
                                {
                                    // The two APIs live at different paths, so
                                    // a kind switch that kept the old base URL
                                    // would 404 with no hint why.
                                    profile.base_url = match kind {
                                        ProviderKind::OpenAiCompatible => {
                                            crate::ai::DEFAULT_API_BASE_URL.into()
                                        }
                                        ProviderKind::Anthropic => {
                                            crate::agent::provider::anthropic::DEFAULT_BASE_URL
                                                .into()
                                        }
                                    };
                                }
                            }
                        });
                });

                ui.label("Endpoint:");
                ui.text_edit_singleline(&mut profile.base_url);

                ui.horizontal(|ui| {
                    ui.label("Model:");
                    ui.text_edit_singleline(&mut profile.model);
                });

                ui.horizontal(|ui| {
                    ui.label("API key:");
                    ui.text_edit_singleline(&mut profile.api_key);
                });

                ui.horizontal(|ui| {
                    ui.label("Max tokens:");
                    ui.add(
                        egui::DragValue::new(&mut profile.max_tokens)
                            .range(256..=32768)
                            .speed(64),
                    );
                });

                // Declared, not probed: there is no reliable capability check
                // across these endpoints, and several local servers 400 on an
                // unrecognised field rather than ignoring it — so a wrong
                // guess breaks every request instead of degrading one feature.
                ui.horizontal(|ui| {
                    ui.checkbox(&mut profile.tools, "Supports tool calling")
                        .on_hover_text(
                            "Required for the agent loop. Most small local models do not.",
                        );
                    ui.checkbox(&mut profile.vision, "Supports images")
                        .on_hover_text("Required to show the model a rendered sprite.");
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
                        let profile = self.config.active().clone();
                        let status = self.test_status.clone();
                        // The probe goes through the same `Provider` the real
                        // requests use, so a green Test means the adapter that
                        // will actually run is reachable — not just that some
                        // URL answered.
                        std::thread::spawn(move || {
                            let next = match profile.build() {
                                Ok(p) => match p.test_connection() {
                                    Ok(Some(m)) => TestStatus::Ok(m),
                                    Ok(None) => TestStatus::Ok("(no model reported)".into()),
                                    Err(e) => TestStatus::Err(e),
                                },
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
