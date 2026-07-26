//! Music Theme editor (S48, Phase B): seed note sequence + scale + variation mask.

use reachlock_core::generator::music::Theme;

use super::super::app::{ContentType, Editor};
use crate::io::EnvelopeMeta;

pub struct ThemeEditor {
    path: Option<std::path::PathBuf>,
    theme: Theme,
    /// Envelope fields the UI doesn't edit but the file must keep. Themes live
    /// on disk as `ContentFile` envelopes; this tab used to read and write the
    /// bare `Theme`, so it could not open a single authored theme.
    meta: EnvelopeMeta,
    has_changes: bool,
}

impl ThemeEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing file and the first save
    /// overwrote it.
    fn new() -> Self {
        ThemeEditor {
            path: None,
            theme: Theme {
                id: "new_theme".into(),
                notes: vec![],
                scale: reachlock_core::generator::music::Scale::MinorPentatonic,
                bpm_range: (60, 80),
                allowed_variations: reachlock_core::generator::music::VariationMask(511),
            },
            meta: EnvelopeMeta::new_for("new_theme"),
            has_changes: false,
        }
    }
}

impl Editor for ThemeEditor {
    fn title(&self) -> &str {
        &self.theme.id
    }
    fn content_type(&self) -> ContentType {
        ContentType::Theme
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let (meta, theme) =
            crate::io::read_enveloped::<Theme>(path).map_err(|e| format!("reading theme: {e}"))?;
        self.theme = theme;
        self.meta = meta;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_enveloped(path, &self.meta, self.theme.clone())
            .map_err(|e| format!("saving theme: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        // Only write when dirty, and never invent a filename: the old
        // fallback name meant two new documents overwrote each other.
        // Returning Ok(false) with no path lets the shell run Save As.
        if !self.has_changes {
            return Ok(self.path.is_some());
        }
        let Some(path) = self.path.clone() else {
            return Ok(false);
        };
        self.save(&path)?;
        self.has_changes = false;
        Ok(true)
    }
    fn generate_from_seed(&mut self, seed: u64) {
        let intent = reachlock_core::generator::generate_music_intent(
            seed,
            reachlock_core::generator::music::Mood::Calm,
            8,
        );
        self.theme.notes = intent
            .notes
            .iter()
            .map(|n| reachlock_core::generator::music::NoteEvent {
                degree: n.degree,
                octave: n.octave,
                velocity: n.velocity,
                start_tick: n.start_tick,
                duration_ticks: n.duration_ticks,
            })
            .collect();
        self.theme.id = format!("theme_{:#x}", seed);
        // The envelope's id is what the content tree indexes, so it has to
        // follow the payload's rename or the file defines an id nothing
        // references.
        self.meta.id = self.theme.id.clone();
        self.meta.display_name = self.theme.id.clone();
        self.meta.seed = seed;
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.theme.id.is_empty() {
            errors.push("id is empty".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(format!("ID: {}", self.theme.id));
            ui.label(format!("Scale: {:?}", self.theme.scale));
            ui.label(format!("Notes: {}", self.theme.notes.len()));
            ui.label(format!(
                "BPM range: {}–{}",
                self.theme.bpm_range.0, self.theme.bpm_range.1
            ));
        });
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.theme).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.theme = ron::from_str(ron_text).map_err(|e| e.to_string())?;
        self.has_changes = true;
        Ok(())
    }

    /// Reroll only renames the id, which would break every cross-reference
    /// pointing at it. Opt out rather than corrupt the content graph.
    fn accept_seed_reroll(&self) -> bool {
        false
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        ui.strong(self.content_type().name());
        let issues = self.validate();
        if issues.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(0x4C, 0xAF, 0x50), "✔ clean");
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(0xE5, 0x73, 0x73),
                format!("✘ {} issue(s)", issues.len()),
            );
            for issue in issues.iter().take(5) {
                ui.weak(issue);
            }
        }
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ThemeEditor::new())
}
