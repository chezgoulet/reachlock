//! Preferences (handoff completion §Priority 6), persisted to
//! `save/editor-preferences.ron` next to the AI settings. Visual settings
//! apply immediately on change; the file is written whenever something
//! changes so preferences survive a crash too.

use std::path::PathBuf;

const PREFS_PATH: &str = "save/editor-preferences.ron";
/// The preview panel's "Open Recent" list keeps this many entries.
const RECENT_CAP: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub theme: Theme,
    /// UI zoom factor, 0.75..=1.5.
    pub font_scale: f32,
    /// Informational for editors with code-like text areas.
    pub show_line_numbers: bool,
    /// Auto-save every N seconds; 0 disables. Only file-backed editors save.
    pub auto_save_secs: u32,
    /// Root of the content tree the browser scans.
    pub content_root: String,
    /// Most-recently-opened files, newest first (drives "Open Recent").
    pub recent_files: Vec<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            theme: Theme::Dark,
            font_scale: 1.0,
            show_line_numbers: true,
            auto_save_secs: 0,
            content_root: "mods/reachlock".into(),
            recent_files: Vec::new(),
        }
    }
}

impl Preferences {
    /// Apply the visual settings to the running context.
    pub fn apply_visuals(&self, ctx: &egui::Context) {
        ctx.set_visuals(match self.theme {
            Theme::Dark => egui::Visuals::dark(),
            Theme::Light => egui::Visuals::light(),
        });
        ctx.set_zoom_factor(self.font_scale);
    }

    /// Record a file in the recent list (deduped, newest first).
    pub fn push_recent(&mut self, path: &std::path::Path) {
        let entry = path.display().to_string();
        self.recent_files.retain(|p| p != &entry);
        self.recent_files.insert(0, entry);
        self.recent_files.truncate(RECENT_CAP);
    }
}

pub struct PreferencesWindow {
    pub open: bool,
    pub prefs: Preferences,
}

impl PreferencesWindow {
    pub fn load() -> Self {
        let prefs = std::fs::read_to_string(PathBuf::from(PREFS_PATH))
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default();
        PreferencesWindow { open: false, prefs }
    }

    pub fn save(&self) {
        let path = PathBuf::from(PREFS_PATH);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = ron::ser::to_string_pretty(&self.prefs, ron::ser::PrettyConfig::default())
        {
            let _ = std::fs::write(path, text);
        }
    }

    /// Render the window. Returns true when a preference changed this frame
    /// (visuals are already applied; the shell reacts to root changes).
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        if !self.open {
            return false;
        }
        let mut open = self.open;
        let mut changed = false;
        egui::Window::new("Preferences")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("prefs_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Theme:");
                        ui.horizontal(|ui| {
                            changed |= ui
                                .selectable_value(&mut self.prefs.theme, Theme::Dark, "Dark")
                                .changed();
                            changed |= ui
                                .selectable_value(&mut self.prefs.theme, Theme::Light, "Light")
                                .changed();
                        });
                        ui.end_row();

                        ui.label("Font scale:");
                        changed |= ui
                            .add(
                                egui::Slider::new(&mut self.prefs.font_scale, 0.75..=1.5)
                                    .step_by(0.05),
                            )
                            .changed();
                        ui.end_row();

                        ui.label("Line numbers:");
                        changed |= ui
                            .checkbox(
                                &mut self.prefs.show_line_numbers,
                                "Show in code-like editors",
                            )
                            .changed();
                        ui.end_row();

                        ui.label("Auto-save:");
                        egui::ComboBox::from_id_salt("prefs_autosave")
                            .selected_text(if self.prefs.auto_save_secs == 0 {
                                "Disabled".to_string()
                            } else {
                                format!("Every {}s", self.prefs.auto_save_secs)
                            })
                            .show_ui(ui, |ui| {
                                for secs in [0u32, 30, 60, 120, 300] {
                                    let label = if secs == 0 {
                                        "Disabled".to_string()
                                    } else {
                                        format!("Every {secs}s")
                                    };
                                    changed |= ui
                                        .selectable_value(
                                            &mut self.prefs.auto_save_secs,
                                            secs,
                                            label,
                                        )
                                        .changed();
                                }
                            });
                        ui.end_row();

                        ui.label("Content root:");
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.prefs.content_root)
                                        .desired_width(180.0),
                                )
                                .changed();
                            if ui.button("Browse…").clicked() {
                                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                    self.prefs.content_root = dir.display().to_string();
                                    changed = true;
                                }
                            }
                        });
                        ui.end_row();
                    });
                ui.weak(
                    "Auto-save writes file-backed editors only; unsaved-file tabs are skipped.",
                );
                ui.separator();
                if ui.button("Restore Defaults").clicked() {
                    let recent = std::mem::take(&mut self.prefs.recent_files);
                    self.prefs = Preferences {
                        recent_files: recent,
                        ..Preferences::default()
                    };
                    changed = true;
                }
            });
        self.open = open;
        if changed {
            self.prefs.apply_visuals(ctx);
            self.save();
        }
        changed
    }
}
