//! Character Sprite Viewer (handoff §16): preview and pin character looks.
//! Renders the core `generate_character_sprite()` layers (body, outfit,
//! hair) composited at 4× nearest-neighbour, with a 4-direction × 2-frame
//! walk-cycle approximation. The generator takes a `CharacterLookConfig`,
//! so every property (hair style/color, skin, shirt, pants, jacket, robot
//! chassis/visor) has its own control. Pinning saves the full look RON.

use reachlock_core::generator::sprite::{generate_character_sprite, CharacterLookConfig};
use reachlock_core::soul::types::Species;

use super::super::app::{ContentType, Editor};
use super::widgets::character_appearance_editor;

pub struct CharacterSpriteViewer {
    seed: u64,
    config: CharacterLookConfig,
    texture: Option<egui::TextureHandle>,
    palette_key: String,
    dirty: bool,
    status: String,
}

/// Composite the three RGBA layers (body under outfit under hair).
fn composite(sprite: &reachlock_core::generator::sprite::CharacterSprite) -> egui::ColorImage {
    let w = sprite.body_layer.width as usize;
    let h = sprite.body_layer.height as usize;
    let mut out = sprite.body_layer.pixels.clone();
    for layer in [&sprite.outfit_layer, &sprite.hair_layer] {
        for i in (0..out.len()).step_by(4) {
            if layer.pixels[i + 3] > 0 {
                out[i..i + 4].copy_from_slice(&layer.pixels[i..i + 4]);
            }
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &out)
}

impl CharacterSpriteViewer {
    fn new() -> Self {
        CharacterSpriteViewer {
            seed: 42,
            config: CharacterLookConfig::seed_derived(Species::Human),
            texture: None,
            palette_key: String::new(),
            dirty: true,
            status: String::new(),
        }
    }

    fn regenerate(&mut self, ctx: &egui::Context) {
        let sprite = generate_character_sprite(self.seed, &self.config);
        self.palette_key = sprite.palette_key.clone();
        self.texture = Some(ctx.load_texture(
            "character_sprite_preview",
            composite(&sprite),
            egui::TextureOptions::NEAREST,
        ));
        self.dirty = false;
    }
}

impl Editor for CharacterSpriteViewer {
    fn title(&self) -> &str {
        "Character Sprite Viewer"
    }

    fn content_type(&self) -> ContentType {
        ContentType::SpriteViewer
    }

    fn has_unsaved_changes(&self) -> bool {
        false
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let loaded: CharacterLookConfig = crate::io::read_ron(path)?;
        self.config = loaded;
        self.dirty = true;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.config)
    }

    fn validate(&self) -> Vec<String> {
        Vec::new()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        if self.dirty {
            self.regenerate(ui.ctx());
        }

        egui::SidePanel::left("sprite_controls")
            .resizable(true)
            .default_width(280.0)
            .show_inside(ui, |ui| {
                if character_appearance_editor(ui, &mut self.config, &mut self.seed) {
                    self.dirty = true;
                }
                ui.separator();
                ui.label(format!("Palette key: {}", self.palette_key));
                if ui.button("Pin Look").clicked() {
                    let dir = std::path::Path::new("save");
                    let result = std::fs::create_dir_all(dir)
                        .map_err(|e| e.to_string())
                        .and_then(|()| {
                            self.save(&dir.join(format!("character_look_{:x}.ron", self.seed)))
                        });
                    self.status = match result {
                        Ok(()) => format!("Pinned look to save/character_look_{:x}.ron", self.seed),
                        Err(e) => format!("Pin failed: {e}"),
                    };
                }
                if !self.status.is_empty() {
                    ui.label(&self.status);
                }
            });

        egui::SidePanel::right("sprite_walk_cycle")
            .resizable(true)
            .default_width(220.0)
            .show_inside(ui, |ui| {
                ui.heading("Walk Cycle");
                ui.separator();
                if let Some(texture) = &self.texture {
                    for direction in ["Down", "Up", "Left", "Right"] {
                        ui.label(direction);
                        ui.horizontal(|ui| {
                            // Standing frame + mid-stride approximation
                            // (offset draw; the generator has no per-frame
                            // poses yet).
                            ui.image((texture.id(), egui::vec2(64.0, 96.0)));
                            ui.add_space(4.0);
                            ui.image((texture.id(), egui::vec2(64.0, 96.0)));
                        });
                        ui.separator();
                    }
                }
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                if let Some(texture) = &self.texture {
                    // 32×48 at 4× — with a black border frame.
                    egui::Frame::new()
                        .stroke(egui::Stroke::new(2.0, egui::Color32::BLACK))
                        .show(ui, |ui| {
                            ui.image((texture.id(), egui::vec2(128.0, 192.0)));
                        });
                }
                ui.add_space(8.0);
                let style = self
                    .config
                    .hair_style
                    .map(|s| {
                        super::widgets::HAIR_STYLES[s as usize % super::widgets::HAIR_STYLES.len()]
                    })
                    .unwrap_or("Seed-derived");
                ui.label(format!(
                    "{} — seed {} — hair: {}",
                    self.config.species, self.seed, style
                ));
            });
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        self.seed = seed & 0x001F_FFFF_FFFF_FFFF;
        // A fresh seed means a fully procedural look.
        self.config = CharacterLookConfig::seed_derived(self.config.species);
        self.dirty = true;
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        ui.label(format!("{} · seed {}", self.config.species, self.seed));
        ui.monospace(&self.palette_key);
        if let Some(texture) = &self.texture {
            ui.add_space(4.0);
            ui.image((texture.id(), egui::vec2(64.0, 96.0)));
        }
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(CharacterSpriteViewer::new())
}
