use reachlock_core::content::recipe::{Ingredient, OutputConfig, Recipe, SkillRequirement};

use super::super::app::{ContentType, Editor};

pub struct RecipeEditor {
    path: Option<std::path::PathBuf>,
    recipe: Recipe,
    has_changes: bool,
}

impl RecipeEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Recipe.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(r) = crate::io::read_ron::<Recipe>(path) {
                return RecipeEditor { path: Some(path.clone()), recipe: r, has_changes: false };
            }
        }
        RecipeEditor {
            path: None,
            recipe: Recipe {
                id: "new_recipe".into(),
                ingredients: vec![Ingredient {
                    item_id: "ore".into(),
                    quantity: 5,
                    optional: false,
                }],
                output: OutputConfig {
                    item_id: "ingot".into(),
                    quantity: 1,
                    quality_min: 50,
                    quality_max: 100,
                    durability: 100,
                },
                skill_requirement: Some(SkillRequirement {
                    category: "smithing".into(),
                    minimum_level: 1,
                }),
                workbench_type: "forge".into(),
                duration_ticks: 60,
            },
            has_changes: false,
        }
    }
}

impl Editor for RecipeEditor {
    fn title(&self) -> &str { &self.recipe.id }
    fn content_type(&self) -> ContentType { ContentType::Recipe }
    fn has_unsaved_changes(&self) -> bool { self.has_changes }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let r: Recipe = crate::io::read_ron(path).map_err(|e| format!("reading recipe: {e}"))?;
        self.recipe = r;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.recipe).map_err(|e| format!("saving recipe: {e}"))
    }
    fn save_all(&mut self) -> Result<bool, String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root().join(ContentType::Recipe.directory()).join("generated_recipe.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
        self.has_changes = false;
        Ok(true)
    }
    fn generate_from_seed(&mut self, seed: u64) {
        self.recipe.id = format!("recipe_{:#x}", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.recipe.id.is_empty() { errors.push("id is empty".into()); }
        if self.recipe.ingredients.is_empty() { errors.push("no ingredients".into()); }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("Recipe: {}", self.recipe.id));
            ui.label(format!("Ingredients: {}", self.recipe.ingredients.len()));
            ui.label(format!("Output: {} x{}", self.recipe.output.item_id, self.recipe.output.quantity));
            ui.label(format!("Workbench: {}", self.recipe.workbench_type));
            ui.label(format!("Duration: {} ticks", self.recipe.duration_ticks));
        });
    }
    fn mark_saved(&mut self) { self.has_changes = false; }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(RecipeEditor::load_or_new())
}
