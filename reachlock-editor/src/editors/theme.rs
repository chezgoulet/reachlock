//! Music Theme editor (S48, Phase B): seed note sequence + scale + variation mask.

use reachlock_core::generator::music::Theme;

use super::super::app::{ContentType, Editor};

pub struct ThemeEditor {
    path: Option<std::path::PathBuf>,
    theme: Theme,
    has_changes: bool,
}

impl ThemeEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Theme.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(theme) = crate::io::read_ron::<Theme>(path) {
                return ThemeEditor { path: Some(path.clone()), theme, has_changes: false };
            }
        }
        ThemeEditor {
            path: None,
            theme: Theme {
                id: "new_theme".into(),
                notes: vec![],
                scale: reachlock_core::generator::music::Scale::MinorPentatonic,
                bpm_range: (60, 80),
                allowed_variations: reachlock_core::generator::music::VariationMask(511),
            },
            has_changes: false,
        }
    }
}

impl Editor for ThemeEditor {
    fn title(&self) -> &str { &self.theme.id }
    fn content_type(&self) -> ContentType { ContentType::Theme }
    fn has_unsaved_changes(&self) -> bool { self.has_changes }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let theme: Theme = crate::io::read_ron(path).map_err(|e| format!("reading theme: {e}"))?;
        self.theme = theme;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.theme).map_err(|e| format!("saving theme: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root().join(ContentType::Theme.directory()).join("generated_theme.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
        self.has_changes = false;
        Ok(true)
    }
    fn generate_from_seed(&mut self, seed: u64) {
        let intent = reachlock_core::generator::generate_music_intent(seed, reachlock_core::generator::music::Mood::Calm, 8);
        self.theme.notes = intent.notes.iter().map(|n| reachlock_core::generator::music::NoteEvent {
            degree: n.degree, octave: n.octave, velocity: n.velocity,
            start_tick: n.start_tick, duration_ticks: n.duration_ticks,
        }).collect();
        self.theme.id = format!("theme_{:#x}", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.theme.id.is_empty() { errors.push("id is empty".into()); }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("ID: {}", self.theme.id));
            ui.label(format!("Scale: {:?}", self.theme.scale));
            ui.label(format!("Notes: {}", self.theme.notes.len()));
            ui.label(format!("BPM range: {}–{}", self.theme.bpm_range.0, self.theme.bpm_range.1));
        });
    }
    fn mark_saved(&mut self) { self.has_changes = false; }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ThemeEditor::load_or_new())
}
