//! Career Path editor (S42, Phase B): one career definition per file.

use reachlock_core::career::{CareerPath, PathType};

use super::super::app::{ContentType, Editor};

pub struct CareerEditor {
    path: Option<std::path::PathBuf>,
    career: CareerPath,
    has_changes: bool,
}

fn readable_path(pt: PathType) -> &'static str {
    match pt {
        PathType::Military => "Military",
        PathType::Trade => "Trade",
        PathType::Exploration => "Exploration",
        PathType::Science => "Science",
        PathType::Political => "Political",
        PathType::Criminal => "Criminal",
        PathType::Freelance => "Freelance",
    }
}

impl CareerEditor {
    /// A genuinely new document.
    ///
    /// This used to read a hardcoded `compact_navy.ron`, so `File > New`
    /// bound to that file and the first save overwrote canonical content.
    fn new() -> Self {
        CareerEditor {
            path: None,
            career: CareerPath {
                id: "new_career".into(),
                path_type: PathType::Military,
                name: "New Career".into(),
                description: String::new(),
                faction_id: None,
                ranks: vec![],
                progression_criteria: vec![],
                perks: vec![],
                conflicting_paths: vec![],
            },
            has_changes: false,
        }
    }
}

impl Editor for CareerEditor {
    fn title(&self) -> &str {
        &self.career.name
    }
    fn content_type(&self) -> ContentType {
        ContentType::Career
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let career: CareerPath =
            crate::io::read_ron(path).map_err(|e| format!("reading career: {e}"))?;
        self.career = career;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.career).map_err(|e| format!("saving career: {e}"))
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
        self.career.id = format!("gen_career_{:#x}", seed);
        self.career.name = format!("Generated Career ({})", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.career.id.is_empty() {
            errors.push("id is empty".into());
        }
        if self.career.name.is_empty() {
            errors.push("name is empty".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Type: {}", readable_path(self.career.path_type)));
            });
            ui.label("Name:");
            ui.text_edit_singleline(&mut self.career.name);
            ui.label("ID:");
            ui.text_edit_singleline(&mut self.career.id);
            ui.label(format!("Ranks: {}", self.career.ranks.len()));
            ui.label(format!("Perks: {}", self.career.perks.len()));
        });
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.career).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.career = ron::from_str(ron_text).map_err(|e| e.to_string())?;
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
    Box::new(CareerEditor::new())
}
