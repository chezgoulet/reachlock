//! Trope Template editor (S40, Phase B): authored narrative beat templates.

use reachlock_core::generator::trope::TropeTemplate;

use super::super::app::{ContentType, Editor};

pub struct TropeEditor {
    path: Option<std::path::PathBuf>,
    template: TropeTemplate,
    has_changes: bool,
}

impl TropeEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing file and the first save
    /// overwrote it.
    fn new() -> Self {
        TropeEditor {
            path: None,
            template: TropeTemplate {
                id: "new_trope".into(),
                trope_type: reachlock_core::generator::trope::TropeType::DerelictShip,
                title_template: "New Trope".into(),
                narrative_template: "Description here.".into(),
                slots: vec![],
                branches: vec![],
                base_frequency: reachlock_core::util::Fixed::from_int(1),
                location_types: vec![reachlock_core::generator::trope::LocationType::DeepSpace],
                min_threat_level: 1,
                max_threat_level: 5,
                dilemma_chance: reachlock_core::util::Fixed(0),
            },
            has_changes: false,
        }
    }
}

impl Editor for TropeEditor {
    fn title(&self) -> &str {
        &self.template.id
    }
    fn content_type(&self) -> ContentType {
        ContentType::Trope
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let t: TropeTemplate =
            crate::io::read_ron(path).map_err(|e| format!("reading trope: {e}"))?;
        self.template = t;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.template).map_err(|e| format!("saving trope: {e}"))
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
        self.template.id = format!("trope_{:#x}", seed);
        self.template.title_template = "Trope {}".to_string();
        self.template.narrative_template = "You encounter an anomaly.".into();
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.template.id.is_empty() {
            errors.push("id is empty".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(format!("ID: {}", self.template.id));
            ui.label(format!("Type: {:?}", self.template.trope_type));
            ui.label(format!("Slots: {}", self.template.slots.len()));
            ui.label(format!("Branches: {}", self.template.branches.len()));
        });
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.template).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.template = ron::from_str(ron_text).map_err(|e| e.to_string())?;
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
    Box::new(TropeEditor::new())
}
