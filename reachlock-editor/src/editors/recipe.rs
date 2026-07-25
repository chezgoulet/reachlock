use reachlock_core::content::recipe::{Ingredient, OutputConfig, Recipe, SkillRequirement};

use super::super::app::{ContentType, Editor};

pub struct RecipeEditor {
    path: Option<std::path::PathBuf>,
    recipe: Recipe,
    has_changes: bool,
}

impl RecipeEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing file and the first save
    /// overwrote it.
    fn new() -> Self {
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
    fn title(&self) -> &str {
        &self.recipe.id
    }
    fn content_type(&self) -> ContentType {
        ContentType::Recipe
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
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
        self.recipe.id = format!("recipe_{:#x}", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.recipe.id.is_empty() {
            errors.push("id is empty".into());
        }
        if self.recipe.ingredients.is_empty() {
            errors.push("no ingredients".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(format!("Recipe: {}", self.recipe.id));
            ui.label(format!("Ingredients: {}", self.recipe.ingredients.len()));
            ui.label(format!(
                "Output: {} x{}",
                self.recipe.output.item_id, self.recipe.output.quantity
            ));
            ui.label(format!("Workbench: {}", self.recipe.workbench_type));
            ui.label(format!("Duration: {} ticks", self.recipe.duration_ticks));
        });
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.recipe).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.recipe = ron::from_str(ron_text).map_err(|e| e.to_string())?;
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
    Box::new(RecipeEditor::new())
}
