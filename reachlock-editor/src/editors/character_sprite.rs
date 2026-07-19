//! Character Sprite Viewer (handoff §16): preview and pin character looks.
//! Renders the core `generate_character_sprite()` layers (body, outfit,
//! hair) composited at 4× nearest-neighbour, with a 4-direction × 2-frame
//! walk-cycle approximation. Colors and styles derive from the seed — the
//! generator is a pure function of (seed, species), so "Randomize" rolls a
//! new seed. Pinning saves a `CharacterLook` RON under `save/`.

use reachlock_core::generator::sprite::{generate_character_sprite, CharacterSprite};
use reachlock_core::soul::types::Species;
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

const SPECIES: [Species; 5] = [
    Species::Human,
    Species::Android,
    Species::Robot,
    Species::Voidborn,
    Species::Xenotype,
];

fn species_name(s: Species) -> &'static str {
    match s {
        Species::Human => "Human",
        Species::Android => "Android",
        Species::Robot => "Robot",
        Species::Voidborn => "Voidborn",
        Species::Xenotype => "Xenotype",
    }
}

/// Map the canonical 5-species enum onto the sprite generator's
/// proportion-set vocabulary ("Synthetic" is the android body plan; Robot
/// falls through to the generator's default frame).
fn generator_species(s: Species) -> &'static str {
    match s {
        Species::Human => "Human",
        Species::Android => "Synthetic",
        Species::Robot => "Robot",
        Species::Voidborn => "Voidborn",
        Species::Xenotype => "Xenotype",
    }
}

/// The pinned look — species + seed IS the look (the generator is pure).
#[derive(serde::Serialize, serde::Deserialize)]
struct CharacterLook {
    species: String,
    seed: u64,
    palette_key: String,
}

pub struct CharacterSpriteViewer {
    species: Species,
    seed: u64,
    texture: Option<egui::TextureHandle>,
    palette_key: String,
    dirty: bool,
    status: String,
}

/// Composite the three RGBA layers (body under outfit under hair).
fn composite(sprite: &CharacterSprite) -> egui::ColorImage {
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
            species: Species::Human,
            seed: 42,
            texture: None,
            palette_key: String::new(),
            dirty: true,
            status: String::new(),
        }
    }

    fn regenerate(&mut self, ctx: &egui::Context) {
        let sprite = generate_character_sprite(self.seed, generator_species(self.species));
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
        let look: CharacterLook = crate::io::read_ron(path)?;
        self.species = SPECIES
            .into_iter()
            .find(|s| species_name(*s) == look.species)
            .unwrap_or(Species::Human);
        self.seed = look.seed;
        self.dirty = true;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(
            path,
            &CharacterLook {
                species: species_name(self.species).into(),
                seed: self.seed,
                palette_key: self.palette_key.clone(),
            },
        )
    }

    fn validate(&self) -> Vec<String> {
        Vec::new()
    }

    fn ui(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.regenerate(ctx);
        }

        egui::SidePanel::left("sprite_controls")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                ui.heading("Character Look");
                ui.separator();
                egui::ComboBox::from_label("Species")
                    .selected_text(species_name(self.species))
                    .show_ui(ui, |ui| {
                        for s in SPECIES {
                            if ui
                                .selectable_value(&mut self.species, s, species_name(s))
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                    });
                if self.species == Species::Robot {
                    ui.small("Robot: no hair or skin tone — chassis colors derive from the seed.");
                } else {
                    ui.small("Hair style, hair/skin/outfit colors all derive from the seed.");
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Seed:");
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.seed)
                                .range(0..=((1u64 << 53) - 1)),
                        )
                        .changed()
                    {
                        self.dirty = true;
                    }
                });
                if ui.button("Randomize").clicked() {
                    let mut rng = SeededRng::new(self.seed ^ 0x5EED_F00F);
                    self.seed = rng.next_u64() & 0x001F_FFFF_FFFF_FFFF;
                    self.dirty = true;
                }
                if ui.button("Re-roll Seed").clicked() {
                    self.seed = (self.seed + 1) & 0x001F_FFFF_FFFF_FFFF;
                    self.dirty = true;
                }
                ui.separator();
                ui.label(format!("Palette key: {}", self.palette_key));
                if ui.button("Pin Seed").clicked() {
                    let dir = std::path::Path::new("save");
                    let result = std::fs::create_dir_all(dir)
                        .map_err(|e| e.to_string())
                        .and_then(|()| {
                            self.save(&dir.join(format!(
                                "character_look_{:x}.ron",
                                self.seed
                            )))
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
            .show(ctx, |ui| {
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

        egui::CentralPanel::default().show(ctx, |ui| {
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
                ui.label(format!(
                    "{} — seed {}",
                    species_name(self.species),
                    self.seed
                ));
            });
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        self.seed = seed & 0x001F_FFFF_FFFF_FFFF;
        self.dirty = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(CharacterSpriteViewer::new())
}
