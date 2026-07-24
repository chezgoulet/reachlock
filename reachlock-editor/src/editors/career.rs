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
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Career.directory());
        let default_path = dir.join("compact_navy.ron");
        match crate::io::read_ron::<CareerPath>(&default_path) {
            Ok(career) => CareerEditor {
                path: Some(default_path),
                career,
                has_changes: false,
            },
            Err(_) => CareerEditor {
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
            },
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
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root()
                .join(ContentType::Career.directory())
                .join(format!("{}.ron", self.career.id))
        });
        self.save(&path)?;
        self.path = Some(path);
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
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
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
    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(CareerEditor::load_or_new())
}
