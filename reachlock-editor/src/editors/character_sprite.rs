//! Character Sprite Viewer (handoff §16): preview and pin character looks.
//! Renders the core `generate_character_sprite()` layers (body, outfit,
//! hair) composited at 4× nearest-neighbour, with a 4-direction × 2-frame
//! walk-cycle approximation. The generator takes a `CharacterLookConfig`,
//! so every property (hair style/color, skin, shirt, pants, jacket, robot
//! chassis/visor) has its own control. Pinning saves the full look RON.

use reachlock_core::generator::sprite::{
    generate_character_sprite, CharacterLookConfig, HAIR_STYLE_COUNT,
};
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

/// The seven hair styles, in generator-index order (0 = Bald).
const HAIR_STYLES: [&str; HAIR_STYLE_COUNT as usize] = [
    "Bald", "Short", "Buzz", "Long", "Locs", "Bun", "Crest",
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

/// The pinned look — species + seed + every overridden property.
#[derive(serde::Serialize, serde::Deserialize)]
struct CharacterLook {
    species: String,
    seed: u64,
    palette_key: String,
    hair_style: Option<u8>,
    hair_color: Option<[u8; 3]>,
    skin_color: Option<[u8; 3]>,
    shirt_color: Option<[u8; 3]>,
    pants_color: Option<[u8; 3]>,
    jacket_enabled: Option<bool>,
    jacket_color: Option<[u8; 3]>,
    chassis_color: Option<[u8; 3]>,
    visor_color: Option<[u8; 3]>,
}

pub struct CharacterSpriteViewer {
    species: Species,
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

/// A labelled RGB triple of drag values plus a color swatch.
fn color_control(ui: &mut egui::Ui, label: &str, color: &mut [u8; 3]) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut color[0]).range(0..=255).prefix("R"));
        ui.add(egui::DragValue::new(&mut color[1]).range(0..=255).prefix("G"));
        ui.add(egui::DragValue::new(&mut color[2]).range(0..=255).prefix("B"));
        let c = egui::Color32::from_rgb(color[0], color[1], color[2]);
        let mut srgba = [color[0], color[1], color[2], 255u8];
        ui.color_edit_button_srgba_unmultiplied(&mut srgba);
        if srgba[0..3] != *color {
            color[0] = srgba[0];
            color[1] = srgba[1];
            color[2] = srgba[2];
        }
        let _ = c;
    });
}

impl CharacterSpriteViewer {
    fn new() -> Self {
        CharacterSpriteViewer {
            species: Species::Human,
            seed: 42,
            config: CharacterLookConfig::seed_derived("Human"),
            texture: None,
            palette_key: String::new(),
            dirty: true,
            status: String::new(),
        }
    }

    fn sync_config(&mut self) {
        self.config.species = generator_species(self.species).to_string();
    }

    fn regenerate(&mut self, ctx: &egui::Context) {
        self.sync_config();
        let sprite = generate_character_sprite(self.seed, &self.config);
        self.palette_key = sprite.palette_key.clone();
        self.texture = Some(ctx.load_texture(
            "character_sprite_preview",
            composite(&sprite),
            egui::TextureOptions::NEAREST,
        ));
        self.dirty = false;
    }

    fn randomize(&mut self) {
        let mut rng = SeededRng::new(self.seed ^ 0x5EED_F00F);
        self.seed = rng.next_u64() & 0x001F_FFFF_FFFF_FFFF;
        self.config.hair_style = Some((rng.next_u64() % HAIR_STYLE_COUNT as u64) as u8);
        self.config.hair_color = Some([
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
        ]);
        self.config.skin_color = Some([
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
        ]);
        self.config.shirt_color = Some([
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
        ]);
        self.config.pants_color = Some([
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
        ]);
        self.config.jacket_enabled = Some(rng.next_u64().is_multiple_of(2));
        self.config.jacket_color = Some([
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
        ]);
        self.config.chassis_color = Some([
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
        ]);
        self.config.visor_color = Some([
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
            rng.next_below(256) as u8,
        ]);
        self.dirty = true;
    }

    fn pinned_look(&self) -> CharacterLook {
        CharacterLook {
            species: species_name(self.species).into(),
            seed: self.seed,
            palette_key: self.palette_key.clone(),
            hair_style: self.config.hair_style,
            hair_color: self.config.hair_color,
            skin_color: self.config.skin_color,
            shirt_color: self.config.shirt_color,
            pants_color: self.config.pants_color,
            jacket_enabled: self.config.jacket_enabled,
            jacket_color: self.config.jacket_color,
            chassis_color: self.config.chassis_color,
            visor_color: self.config.visor_color,
        }
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
        self.config = CharacterLookConfig {
            species: generator_species(self.species).to_string(),
            hair_style: look.hair_style,
            hair_color: look.hair_color,
            skin_color: look.skin_color,
            shirt_color: look.shirt_color,
            pants_color: look.pants_color,
            jacket_enabled: look.jacket_enabled,
            jacket_color: look.jacket_color,
            chassis_color: look.chassis_color,
            visor_color: look.visor_color,
        };
        self.dirty = true;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.pinned_look())
    }

    fn validate(&self) -> Vec<String> {
        Vec::new()
    }

    fn ui(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.regenerate(ctx);
        }

        let is_robot = self.species == Species::Robot;

        egui::SidePanel::left("sprite_controls")
            .resizable(true)
            .default_width(280.0)
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

                ui.separator();
                // Hair style cycler.
                let idx = self.config.hair_style.unwrap_or(0) as usize % HAIR_STYLES.len();
                ui.horizontal(|ui| {
                    ui.label("Hair:");
                    if ui.button("◀").clicked() {
                        let cur = self.config.hair_style.unwrap_or(0) as i32;
                        let next = if cur <= 0 {
                            HAIR_STYLE_COUNT as i32 - 1
                        } else {
                            cur - 1
                        };
                        self.config.hair_style = Some(next as u8);
                        self.dirty = true;
                    }
                    ui.label(HAIR_STYLES[idx]);
                    if ui.button("▶").clicked() {
                        let cur = self.config.hair_style.unwrap_or(0) as u32;
                        let next = (cur + 1) % HAIR_STYLE_COUNT as u32;
                        self.config.hair_style = Some(next as u8);
                        self.dirty = true;
                    }
                });

                if is_robot {
                    ui.small("Robot: chassis + visor replace hair/skin tones.");
                    ui.separator();
                    let mut chassis = self
                        .config
                        .chassis_color
                        .unwrap_or([120, 120, 130]);
                    color_control(ui, "Chassis", &mut chassis);
                    if self.config.chassis_color != Some(chassis) {
                        self.config.chassis_color = Some(chassis);
                        self.dirty = true;
                    }
                    let mut visor = self.config.visor_color.unwrap_or([80, 200, 255]);
                    color_control(ui, "Visor", &mut visor);
                    if self.config.visor_color != Some(visor) {
                        self.config.visor_color = Some(visor);
                        self.dirty = true;
                    }
                } else {
                    let mut hair = self.config.hair_color.unwrap_or([40, 30, 20]);
                    color_control(ui, "Hair", &mut hair);
                    if self.config.hair_color != Some(hair) {
                        self.config.hair_color = Some(hair);
                        self.dirty = true;
                    }
                    let mut skin = self.config.skin_color.unwrap_or([200, 170, 150]);
                    color_control(ui, "Skin", &mut skin);
                    if self.config.skin_color != Some(skin) {
                        self.config.skin_color = Some(skin);
                        self.dirty = true;
                    }
                }

                ui.separator();
                let mut shirt = self.config.shirt_color.unwrap_or([160, 40, 40]);
                color_control(ui, "Shirt", &mut shirt);
                if self.config.shirt_color != Some(shirt) {
                    self.config.shirt_color = Some(shirt);
                    self.dirty = true;
                }
                let mut pants = self.config.pants_color.unwrap_or([40, 40, 40]);
                color_control(ui, "Pants", &mut pants);
                if self.config.pants_color != Some(pants) {
                    self.config.pants_color = Some(pants);
                    self.dirty = true;
                }

                ui.separator();
                let enabled = self.config.jacket_enabled.unwrap_or(false);
                let mut new_enabled = enabled;
                ui.checkbox(&mut new_enabled, "Jacket");
                if new_enabled != enabled {
                    self.config.jacket_enabled = Some(new_enabled);
                    self.dirty = true;
                }
                if new_enabled {
                    let mut jacket = self.config.jacket_color.unwrap_or([200, 40, 40]);
                    color_control(ui, "Jacket", &mut jacket);
                    if self.config.jacket_color != Some(jacket) {
                        self.config.jacket_color = Some(jacket);
                        self.dirty = true;
                    }
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
                ui.horizontal(|ui| {
                    if ui.button("Randomize").clicked() {
                        self.randomize();
                    }
                    if ui.button("Re-roll Seed").clicked() {
                        self.seed = (self.seed + 1) & 0x001F_FFFF_FFFF_FFFF;
                        self.dirty = true;
                    }
                });
                ui.separator();
                ui.label(format!("Palette key: {}", self.palette_key));
                if ui.button("Pin Look").clicked() {
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
                let style = self.config.hair_style.map(|s| HAIR_STYLES
                    [s as usize % HAIR_STYLES.len()])
                    .unwrap_or("Seed-derived");
                ui.label(format!(
                    "{} — seed {} — hair: {}",
                    species_name(self.species),
                    self.seed,
                    style
                ));
            });
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        self.seed = seed & 0x001F_FFFF_FFFF_FFFF;
        // A fresh seed means a fully procedural look.
        self.config = CharacterLookConfig::seed_derived(generator_species(self.species));
        self.dirty = true;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(CharacterSpriteViewer::new())
}
