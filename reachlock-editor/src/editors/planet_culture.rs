//! Planet Culture editor (S47, Phase B): an authored culture override.

use reachlock_core::generator::culture::PlanetCulture;

use super::super::app::{ContentType, Editor};

pub struct PlanetCultureEditor {
    path: Option<std::path::PathBuf>,
    culture: PlanetCulture,
    has_changes: bool,
}

impl PlanetCultureEditor {
    /// A genuinely new document.
    ///
    /// This used to adopt the first `.ron` in the content directory, so
    /// `File > New` silently bound to an existing file and the first save
    /// overwrote it.
    fn new() -> Self {
        PlanetCultureEditor {
            path: None,
            culture: PlanetCulture {
                cultural_id: "new_culture".into(),
                language: reachlock_core::generator::culture::LanguageProfile {
                    base_language: "Standard".into(),
                    drift_intensity: 10,
                    accent_name: "neutral".into(),
                    unique_terms: vec![],
                    greeting: "Hello".into(),
                    farewell: "Goodbye".into(),
                },
                customs: vec![],
                social_structure: reachlock_core::generator::culture::SocialStructure::Egalitarian,
                architecture: reachlock_core::generator::culture::ArchitecturalStyle {
                    style_name: "default".into(),
                    materials: vec![],
                    dominant_shape: "sprawling".into(),
                    color_palette: reachlock_core::generator::culture::ColorScheme {
                        primary: reachlock_core::util::color::ColorRgba8 {
                            r: 100,
                            g: 150,
                            b: 200,
                            a: 255,
                        },
                        secondary: reachlock_core::util::color::ColorRgba8 {
                            r: 60,
                            g: 100,
                            b: 150,
                            a: 255,
                        },
                        accent: reachlock_core::util::color::ColorRgba8 {
                            r: 200,
                            g: 100,
                            b: 50,
                            a: 255,
                        },
                        preference: reachlock_core::generator::culture::ColorPreference::Cool,
                    },
                    adapted_to: vec![],
                },
                clothing: reachlock_core::generator::culture::ClothingStyle {
                    style_name: "default".into(),
                    primary_material: "synth".into(),
                    dominant_colors: reachlock_core::generator::culture::ColorScheme {
                        primary: reachlock_core::util::color::ColorRgba8 {
                            r: 80,
                            g: 120,
                            b: 160,
                            a: 255,
                        },
                        secondary: reachlock_core::util::color::ColorRgba8 {
                            r: 40,
                            g: 80,
                            b: 120,
                            a: 255,
                        },
                        accent: reachlock_core::util::color::ColorRgba8 {
                            r: 160,
                            g: 80,
                            b: 40,
                            a: 255,
                        },
                        preference: reachlock_core::generator::culture::ColorPreference::Earth,
                    },
                    practicality_level: 50,
                    adapted_to: vec![],
                },
                attitude_toward_outsiders:
                    reachlock_core::generator::culture::OutsiderAttitude::Curious,
                faction_allegiance:
                    reachlock_core::generator::culture::FactionAllegiance::Independent,
                dominant_values: vec![],
                cultural_quirk: String::new(),
            },
            has_changes: false,
        }
    }
}

impl Editor for PlanetCultureEditor {
    fn title(&self) -> &str {
        &self.culture.cultural_id
    }
    fn content_type(&self) -> ContentType {
        ContentType::PlanetCulture
    }
    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let culture: PlanetCulture =
            crate::io::read_ron(path).map_err(|e| format!("reading culture: {e}"))?;
        self.culture = culture;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.culture).map_err(|e| format!("saving culture: {e}"))
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
        use std::collections::HashMap;
        let fid = reachlock_core::faction::FactionId("compact".into());
        let mut fmap = HashMap::new();
        fmap.insert(fid.clone(), 120u8);
        self.culture = reachlock_core::generator::generate_culture(
            seed ^ 0x5151,
            60,
            &[],
            &fid,
            reachlock_core::generator::planet_extended::SettlementWave::FirstWave,
            &fmap,
            20,
        );
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.culture.cultural_id.is_empty() {
            errors.push("cultural_id is empty".into());
        }
        errors
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(format!("Language: {}", self.culture.language.base_language));
            ui.label(format!("Customs: {}", self.culture.customs.len()));
        });
    }
    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.culture).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        self.culture = ron::from_str(ron_text).map_err(|e| e.to_string())?;
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
    Box::new(PlanetCultureEditor::new())
}
