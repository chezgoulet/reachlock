//! Ecosystem editor (S39, Phase B): an authored ecosystem override.

use reachlock_core::generator::Ecosystem;

use super::super::app::{ContentType, Editor};

pub struct EcosystemEditor {
    path: Option<std::path::PathBuf>,
    ecosystem: Ecosystem,
    has_changes: bool,
}

impl EcosystemEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Ecosystem.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(eco) = crate::io::read_ron::<Ecosystem>(path) {
                return EcosystemEditor {
                    path: Some(path.clone()),
                    ecosystem: eco,
                    has_changes: false,
                };
            }
        }
        EcosystemEditor {
            path: None,
            ecosystem: Ecosystem {
                planet_seed: 0,
                biomes: vec![],
                global_species_count: 0,
                endemic_species_count: 0,
                ecological_complexity: reachlock_core::generator::ecosystem::EcosystemComplexity::Barren,
                baseline_recorded: false,
            },
            has_changes: false,
        }
    }
}

impl Editor for EcosystemEditor {
    fn title(&self) -> &str {
        "Ecosystem"
    }
    fn content_type(&self) -> ContentType {
        ContentType::Ecosystem
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let eco: Ecosystem =
            crate::io::read_ron(path).map_err(|e| format!("reading ecosystem: {e}"))?;
        self.ecosystem = eco;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.ecosystem).map_err(|e| format!("saving ecosystem: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root()
                .join(ContentType::Ecosystem.directory())
                .join("generated_ecosystem.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
        self.has_changes = false;
        Ok(true)
    }
    fn generate_from_seed(&mut self, seed: u64) {
        use reachlock_core::seed::types::Biome;
        let params = reachlock_core::generator::ecosystem::PlanetParams {
            habitability: 180,
            age_ticks: 5000,
            biome_diversity: 2,
        };
        self.ecosystem =
            reachlock_core::generator::generate_ecosystem(seed, vec![Biome::Frontier], params);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.ecosystem.biomes.is_empty() {
            errors.push("no biomes defined".into());
        }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!(
                "Complexity: {:?} — {} species across {} biome(s)",
                self.ecosystem.ecological_complexity,
                self.ecosystem.global_species_count,
                self.ecosystem.biomes.len(),
            ));
        });
    }
    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(EcosystemEditor::load_or_new())
}
