//! Trope Template editor (S40, Phase B): authored narrative beat templates.

use reachlock_core::generator::trope::TropeTemplate;

use super::super::app::{ContentType, Editor};

pub struct TropeEditor {
    path: Option<std::path::PathBuf>,
    template: TropeTemplate,
    has_changes: bool,
}

impl TropeEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Trope.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(t) = crate::io::read_ron::<TropeTemplate>(path) {
                return TropeEditor { path: Some(path.clone()), template: t, has_changes: false };
            }
        }
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
    fn title(&self) -> &str { &self.template.id }
    fn content_type(&self) -> ContentType { ContentType::Trope }
    fn has_unsaved_changes(&self) -> bool { self.has_changes }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let t: TropeTemplate = crate::io::read_ron(path).map_err(|e| format!("reading trope: {e}"))?;
        self.template = t;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.template).map_err(|e| format!("saving trope: {e}"))
    }
    fn save_all(&mut self) -> Result<(), String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root().join(ContentType::Trope.directory()).join("generated_trope.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
        self.has_changes = false;
        Ok(())
    }
    fn generate_from_seed(&mut self, seed: u64) {
        self.template.id = format!("trope_{:#x}", seed);
        self.template.title_template = format!("Trope {{}}");
        self.template.narrative_template = "You encounter an anomaly.".into();
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.template.id.is_empty() { errors.push("id is empty".into()); }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("ID: {}", self.template.id));
            ui.label(format!("Type: {:?}", self.template.trope_type));
            ui.label(format!("Slots: {}", self.template.slots.len()));
            ui.label(format!("Branches: {}", self.template.branches.len()));
        });
    }
    fn mark_saved(&mut self) { self.has_changes = false; }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(TropeEditor::load_or_new())
}
