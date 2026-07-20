//! Character sprite generator (S25).
//! Pure function: seed + species + look config -> pixel-art sprite layers.
//!
//! `CharacterLookConfig` lets callers override individual properties
//! (hair style/color, skin, shirt, pants, jacket, robot chassis/visor)
//! while leaving the rest derived from the seed. Overridden or not, the
//! RNG is always advanced for every property so a given seed produces the
//! same seed-derived values regardless of which overrides are set — this
//! keeps the function deterministic and order-independent.

use super::GeneratedTexture;
use crate::util::SeededRng;

/// Number of hair-style variants the generator can render. Indexes are
/// stable and match the client `Hair` enum vocabulary.
pub const HAIR_STYLE_COUNT: u8 = 7;

/// All user-tweakable visual properties for a character. Every field is
/// `None` by default, meaning "derive from the seed". The editor fills in
/// the fields it wants to pin; anything left `None` stays procedural.
#[derive(Debug, Clone, Default)]
pub struct CharacterLookConfig {
    pub species: String,
    /// None = seed-derived. Some(i) picks style `i % HAIR_STYLE_COUNT`.
    /// 0=Bald, 1=Short, 2=Buzz, 3=Long, 4=Locs, 5=Bun, 6=Crest.
    pub hair_style: Option<u8>,
    /// RGB triple; None = seed-derived.
    pub hair_color: Option<[u8; 3]>,
    /// RGB triple; None = seed-derived.
    pub skin_color: Option<[u8; 3]>,
    /// RGB triple; None = seed-derived.
    pub shirt_color: Option<[u8; 3]>,
    /// RGB triple; None = seed-derived.
    pub pants_color: Option<[u8; 3]>,
    /// None = seed-derived. Some(false) = no jacket. Some(true) = jacket
    /// drawn in `jacket_color` (if that is also Some, else seed-derived).
    pub jacket_enabled: Option<bool>,
    /// RGB triple for the jacket; None = seed-derived.
    pub jacket_color: Option<[u8; 3]>,
    /// Robot chassis hull color; None = seed-derived.
    pub chassis_color: Option<[u8; 3]>,
    /// Robot visor glow color; None = seed-derived.
    pub visor_color: Option<[u8; 3]>,
}

impl CharacterLookConfig {
    /// A fully seed-derived config for the given species — equivalent to the
    /// old two-argument `generate_character_sprite(seed, species)` behavior.
    pub fn seed_derived(species: &str) -> Self {
        Self {
            species: species.to_string(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CharacterSprite {
    pub species: String,
    pub body_layer: GeneratedTexture,
    pub outfit_layer: GeneratedTexture,
    pub hair_layer: GeneratedTexture,
    pub palette_key: String,
    /// Which hair style index was actually rendered (0..HAIR_STYLE_COUNT).
    pub hair_style_index: u8,
}

const W: u32 = 32;
const H: u32 = 48;

fn rgb(color: [u8; 3]) -> [u8; 4] {
    [color[0], color[1], color[2], 255]
}

fn base_color(species: &str, rng: &mut SeededRng) -> [u8; 4] {
    match species {
        "Human" => [
            rng.next_below(40) as u8 + 180,
            rng.next_below(30) as u8 + 160,
            rng.next_below(25) as u8 + 150,
            255,
        ],
        "Synthetic" => [
            rng.next_below(30) as u8 + 100,
            rng.next_below(30) as u8 + 120,
            rng.next_below(40) as u8 + 180,
            255,
        ],
        "Voidborn" => [
            rng.next_below(20) as u8 + 100,
            rng.next_below(20) as u8 + 100,
            rng.next_below(30) as u8 + 140,
            255,
        ],
        "Augmented" => [
            rng.next_below(40) as u8 + 180,
            rng.next_below(40) as u8 + 100,
            rng.next_below(30) as u8 + 100,
            255,
        ],
        "Xenotype" => [
            rng.next_below(60) as u8 + 100,
            rng.next_below(60) as u8 + 180,
            rng.next_below(40) as u8 + 80,
            255,
        ],
        _ => [
            rng.next_below(60) as u8 + 140,
            rng.next_below(60) as u8 + 140,
            rng.next_below(60) as u8 + 140,
            255,
        ],
    }
}

fn outfit_color(rng: &mut SeededRng) -> [u8; 4] {
    let h = rng.next_below(6);
    match h {
        0 => [160, 40, 40, 255],
        1 => [40, 80, 160, 255],
        2 => [40, 120, 60, 255],
        3 => [120, 100, 40, 255],
        4 => [80, 40, 120, 255],
        _ => [60, 60, 60, 255],
    }
}

fn hair_color(rng: &mut SeededRng) -> [u8; 4] {
    let h = rng.next_below(5);
    match h {
        0 => [40, 30, 20, 255],
        1 => [180, 140, 60, 255],
        2 => [160, 80, 40, 255],
        3 => [200, 180, 140, 255],
        _ => [180, 40, 40, 255],
    }
}

fn fill_rect(pixels: &mut [u8], x0: i32, y0: i32, w: i32, h: i32, color: [u8; 4]) {
    for py in y0..y0 + h {
        for px in x0..x0 + w {
            if px >= 0 && px < W as i32 && py >= 0 && py < H as i32 {
                let idx = ((py * W as i32 + px) * 4) as usize;
                pixels[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }
}

fn fill_circle(pixels: &mut [u8], cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    for py in (cy - r).max(0)..=(cy + r).min(H as i32 - 1) {
        for px in (cx - r).max(0)..=(cx + r).min(W as i32 - 1) {
            let dx = px - cx;
            let dy = py - cy;
            if dx * dx + dy * dy <= r * r {
                let idx = ((py * W as i32 + px) * 4) as usize;
                pixels[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }
}

fn head_radius(species: &str) -> i32 {
    match species {
        "Human" | "Augmented" => 6,
        "Synthetic" => 7,
        "Voidborn" => 5,
        "Xenotype" => 8,
        _ => 6,
    }
}

/// Torso/leg proportions per species vocabulary.
fn body_metrics(species: &str) -> (i32, i32, i32, i32, i32) {
    match species {
        "Human" => (6, 12, 14, 10, 4),
        "Synthetic" => (7, 14, 16, 12, 5),
        "Voidborn" => (5, 10, 16, 14, 3),
        "Augmented" => (7, 16, 14, 10, 5),
        "Xenotype" => (8, 10, 12, 8, 4),
        _ => (6, 12, 14, 10, 4),
    }
}

/// Outfit metrics mirror the body plan so the clothes hug the frame.
fn outfit_metrics(species: &str) -> (i32, i32, i32) {
    match species {
        "Human" => (12, 14, 10),
        "Synthetic" => (14, 16, 12),
        "Voidborn" => (10, 16, 14),
        "Augmented" => (16, 14, 10),
        "Xenotype" => (10, 12, 8),
        _ => (12, 14, 10),
    }
}

fn draw_body(
    pixels: &mut [u8],
    species: &str,
    rng: &mut SeededRng,
    skin_override: Option<[u8; 3]>,
) -> [u8; 4] {
    // Always consume the RNG so seed-derived values are order-independent.
    let skin = base_color(species, rng);
    let skin = skin_override.map(rgb).unwrap_or(skin);
    let (head_r, torso_w, torso_h, leg_h, arm_w) = body_metrics(species);
    let cx = (W / 2) as i32;
    let head_y = head_r + 1;
    let torso_y = head_y + head_r;
    let leg_y = torso_y + torso_h;

    fill_circle(pixels, cx, head_y, head_r, skin);
    fill_rect(pixels, cx - torso_w / 2, torso_y, torso_w, torso_h, skin);
    fill_rect(
        pixels,
        cx - torso_w / 2 - arm_w,
        torso_y + 2,
        arm_w,
        torso_h - 4,
        skin,
    );
    fill_rect(
        pixels,
        cx + torso_w / 2,
        torso_y + 2,
        arm_w,
        torso_h - 4,
        skin,
    );
    fill_rect(pixels, cx - 3, leg_y, 5, leg_h, skin);
    fill_rect(pixels, cx + 2, leg_y, 5, leg_h, skin);
    skin
}

fn draw_outfit(
    pixels: &mut [u8],
    species: &str,
    rng: &mut SeededRng,
    shirt_override: Option<[u8; 3]>,
    pants_override: Option<[u8; 3]>,
) {
    // Consume the RNG for both colors even when overridden.
    let shirt = shirt_override.map(rgb).unwrap_or_else(|| outfit_color(rng));
    let pants = pants_override.map(rgb).unwrap_or_else(|| outfit_color(rng));
    let (torso_w, torso_h, leg_h) = outfit_metrics(species);
    let cx = (W / 2) as i32;
    let head_r = head_radius(species);
    let torso_y = head_r + head_r + 1;
    let leg_y = torso_y + torso_h + 1;

    // Shirt covers the torso; pants cover the legs.
    fill_rect(
        pixels,
        cx - torso_w / 2 + 1,
        torso_y + 1,
        torso_w - 2,
        torso_h - 1,
        shirt,
    );
    fill_rect(pixels, cx - 2, leg_y, 4, leg_h - 1, pants);
    fill_rect(pixels, cx + 3, leg_y, 4, leg_h - 1, pants);
}

/// Optional jacket layer drawn over the shirt but under the hair.
fn draw_jacket(
    pixels: &mut [u8],
    species: &str,
    rng: &mut SeededRng,
    enabled: Option<bool>,
    color_override: Option<[u8; 3]>,
) -> bool {
    let enabled = enabled.unwrap_or_else(|| rng.next_below(2) == 0);
    if !enabled {
        // Still consume the RNG for determinism parity.
        let _ = outfit_color(rng);
        return false;
    }
    let color = color_override.map(rgb).unwrap_or_else(|| outfit_color(rng));
    let (torso_w, torso_h, _leg_h) = outfit_metrics(species);
    let cx = (W / 2) as i32;
    let head_r = head_radius(species);
    let torso_y = head_r + head_r + 1;
    // Open jacket: two lapel panels leaving a shirt-colored gap down the
    // centre, plus shoulder caps.
    fill_rect(pixels, cx - torso_w / 2, torso_y, 3, torso_h, color);
    fill_rect(pixels, cx + torso_w / 2 - 3, torso_y, 3, torso_h, color);
    fill_rect(pixels, cx - torso_w / 2, torso_y, torso_w, 2, color);
    true
}

fn draw_hair(
    pixels: &mut [u8],
    species: &str,
    rng: &mut SeededRng,
    style_override: Option<u8>,
    color_override: Option<[u8; 3]>,
) -> u8 {
    let style = style_override
        .map(|s| s % HAIR_STYLE_COUNT)
        .unwrap_or_else(|| rng.next_below(HAIR_STYLE_COUNT as u64) as u8);
    let color = color_override.map(rgb).unwrap_or_else(|| hair_color(rng));
    let head_r = head_radius(species);
    let cx = (W / 2) as i32;
    let head_y = head_r + 1;

    match style {
        // 0 = Bald: nothing drawn.
        0 => {}
        // 1 = Short: flat cap across the top of the head.
        1 => {
            fill_rect(
                pixels,
                cx - head_r,
                head_y - head_r,
                head_r * 2,
                head_r / 2 + 1,
                color,
            );
        }
        // 2 = Buzz: a thin line hugging the crown.
        2 => {
            fill_rect(
                pixels,
                cx - head_r + 1,
                head_y - head_r,
                head_r * 2 - 2,
                2,
                color,
            );
        }
        // 3 = Long: side-swept panel hanging past the ear.
        3 => {
            let off = 1 + rng.next_below(3) as i32;
            fill_rect(
                pixels,
                cx - head_r - off,
                head_y - head_r / 2,
                head_r + off,
                head_r,
                color,
            );
        }
        // 4 = Locs: a cluster of vertical strips.
        4 => {
            for i in 0..4 {
                let x = cx - head_r + i * (head_r / 2);
                fill_rect(pixels, x, head_y - head_r, 2, head_r + 2, color);
            }
        }
        // 5 = Bun: a knot on the back/top of the skull.
        5 => {
            fill_circle(pixels, cx, head_y - head_r - 2, head_r - 1, color);
        }
        // 6 = Crest: a wide ridge (mohawk) down the centre.
        _ => {
            fill_rect(
                pixels,
                cx - head_r / 2,
                head_y - head_r - 1,
                head_r,
                3,
                color,
            );
        }
    }
    style
}

/// Robot chassis replaces the organic body; the visor replaces the eyes.
/// Returns the chassis color actually used (for palette_key).
fn draw_robot(
    pixels: &mut [u8],
    species: &str,
    rng: &mut SeededRng,
    chassis_override: Option<[u8; 3]>,
    visor_override: Option<[u8; 3]>,
) -> ([u8; 4], [u8; 4]) {
    let chassis = chassis_override
        .map(rgb)
        .unwrap_or_else(|| base_color(species, rng));
    let visor = visor_override.map(rgb).unwrap_or_else(|| {
        let h = rng.next_below(5);
        match h {
            0 => [80, 200, 255, 255],
            1 => [255, 120, 60, 255],
            2 => [120, 255, 120, 255],
            3 => [255, 80, 200, 255],
            _ => [200, 200, 220, 255],
        }
    });
    let (head_r, torso_w, torso_h, leg_h, arm_w) = body_metrics(species);
    let cx = (W / 2) as i32;
    let head_y = head_r + 1;
    let torso_y = head_y + head_r;
    let leg_y = torso_y + torso_h;

    // Boxy chassis: square head, blocky torso, stubby legs.
    fill_rect(
        pixels,
        cx - head_r,
        head_y - head_r,
        head_r * 2,
        head_r * 2,
        chassis,
    );
    fill_rect(pixels, cx - torso_w / 2, torso_y, torso_w, torso_h, chassis);
    fill_rect(
        pixels,
        cx - torso_w / 2 - arm_w,
        torso_y + 2,
        arm_w,
        torso_h - 4,
        chassis,
    );
    fill_rect(
        pixels,
        cx + torso_w / 2,
        torso_y + 2,
        arm_w,
        torso_h - 4,
        chassis,
    );
    fill_rect(pixels, cx - 3, leg_y, 5, leg_h, chassis);
    fill_rect(pixels, cx + 2, leg_y, 5, leg_h, chassis);
    // Visor band across the head.
    fill_rect(pixels, cx - head_r + 1, head_y, head_r * 2 - 2, 2, visor);
    (chassis, visor)
}

/// Generate a character sprite. `config` overrides individual properties;
/// any `None` field is derived from `seed`. The RNG is always advanced for
/// every property so seed-derived values stay order-independent.
pub fn generate_character_sprite(seed: u64, config: &CharacterLookConfig) -> CharacterSprite {
    let species = config.species.as_str();
    let mut rng = SeededRng::new(seed);

    let mut body_pixels = vec![0u8; (W * H * 4) as usize];
    let mut outfit_pixels = vec![0u8; (W * H * 4) as usize];
    let mut hair_pixels = vec![0u8; (W * H * 4) as usize];

    let (skin, hair_style_index) = if species == "Robot" {
        let (chassis, _visor) = draw_robot(
            &mut body_pixels,
            species,
            &mut rng,
            config.chassis_color,
            config.visor_color,
        );
        // Robots have no organic hair; draw nothing but record style 0.
        draw_outfit(
            &mut outfit_pixels,
            species,
            &mut rng,
            config.shirt_color,
            config.pants_color,
        );
        let _ = draw_jacket(
            &mut outfit_pixels,
            species,
            &mut rng,
            config.jacket_enabled,
            config.jacket_color,
        );
        let _ = draw_hair(&mut hair_pixels, species, &mut rng, Some(0), None);
        (chassis, 0u8)
    } else {
        let skin = draw_body(&mut body_pixels, species, &mut rng, config.skin_color);
        draw_outfit(
            &mut outfit_pixels,
            species,
            &mut rng,
            config.shirt_color,
            config.pants_color,
        );
        let _ = draw_jacket(
            &mut outfit_pixels,
            species,
            &mut rng,
            config.jacket_enabled,
            config.jacket_color,
        );
        let hair_style_index = draw_hair(
            &mut hair_pixels,
            species,
            &mut rng,
            config.hair_style,
            config.hair_color,
        );
        (skin, hair_style_index)
    };

    let palette_key = format!("{:02x}{:02x}{:02x}", skin[0], skin[1], skin[2]);

    CharacterSprite {
        species: species.to_string(),
        body_layer: GeneratedTexture {
            width: W,
            height: H,
            pixels: body_pixels,
        },
        outfit_layer: GeneratedTexture {
            width: W,
            height: H,
            pixels: outfit_pixels,
        },
        hair_layer: GeneratedTexture {
            width: W,
            height: H,
            pixels: hair_pixels,
        },
        palette_key,
        hair_style_index,
    }
}

/// Convenience wrapper: fully seed-derived sprite (old two-arg API).
pub fn generate_character_sprite_default(seed: u64, species: &str) -> CharacterSprite {
    generate_character_sprite(seed, &CharacterLookConfig::seed_derived(species))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_character_sprite(42, &CharacterLookConfig::seed_derived("Human"));
        let b = generate_character_sprite(42, &CharacterLookConfig::seed_derived("Human"));
        assert_eq!(a.body_layer, b.body_layer);
        assert_eq!(a.outfit_layer, b.outfit_layer);
        assert_eq!(a.hair_layer, b.hair_layer);
    }

    #[test]
    fn species_differ() {
        let a = generate_character_sprite(7, &CharacterLookConfig::seed_derived("Human"));
        let b = generate_character_sprite(7, &CharacterLookConfig::seed_derived("Synthetic"));
        assert_ne!(a.body_layer, b.body_layer);
    }

    #[test]
    fn texture_dimensions() {
        let c = generate_character_sprite(99, &CharacterLookConfig::seed_derived("Voidborn"));
        assert_eq!(c.body_layer.width, 32);
        assert_eq!(c.body_layer.height, 48);
        assert_eq!(c.body_layer.pixels.len(), 32 * 48 * 4);
    }

    #[test]
    fn default_wrapper_matches_seed_derived() {
        let a = generate_character_sprite_default(123, "Human");
        let b = generate_character_sprite(123, &CharacterLookConfig::seed_derived("Human"));
        assert_eq!(a, b);
    }

    #[test]
    fn overrides_pin_colors() {
        let mut cfg = CharacterLookConfig::seed_derived("Human");
        cfg.skin_color = Some([10, 20, 30]);
        cfg.shirt_color = Some([200, 10, 10]);
        cfg.hair_color = Some([5, 5, 5]);
        cfg.hair_style = Some(5);
        let s = generate_character_sprite(42, &cfg);
        assert_eq!(s.hair_style_index, 5);
        // The override colors must appear somewhere in each layer.
        assert!(s
            .body_layer
            .pixels
            .windows(4)
            .any(|w| w == [10, 20, 30, 255]));
        assert!(s
            .outfit_layer
            .pixels
            .windows(4)
            .any(|w| w == [200, 10, 10, 255]));
        assert!(s.hair_layer.pixels.windows(4).any(|w| w == [5, 5, 5, 255]));
    }

    #[test]
    fn overrides_do_not_shift_seed_derived_values() {
        // A hair-style override (the LAST property drawn) must not change the
        // seed-derived skin or outfit — proves RNG consumption order is
        // independent of which properties are overridden.
        let default = generate_character_sprite(42, &CharacterLookConfig::seed_derived("Human"));
        let mut cfg = CharacterLookConfig::seed_derived("Human");
        cfg.hair_style = Some(5);
        let partial = generate_character_sprite(42, &cfg);
        assert_eq!(default.body_layer, partial.body_layer);
        assert_eq!(default.outfit_layer, partial.outfit_layer);
    }

    #[test]
    fn robot_has_no_hair() {
        let mut cfg = CharacterLookConfig::seed_derived("Robot");
        cfg.chassis_color = Some([120, 120, 130]);
        cfg.visor_color = Some([80, 200, 255]);
        let s = generate_character_sprite(42, &cfg);
        assert_eq!(s.hair_style_index, 0);
        assert!(s
            .body_layer
            .pixels
            .windows(4)
            .any(|w| w == [120, 120, 130, 255]));
        // The visor band must be present.
        assert!(s
            .body_layer
            .pixels
            .windows(4)
            .any(|w| w == [80, 200, 255, 255]));
        // No hair layer content should remain (style 0 draws nothing).
        assert!(!s.hair_layer.pixels.windows(4).any(|w| w[3] == 255));
    }

    #[test]
    fn jacket_toggle_changes_outfit() {
        let mut off = CharacterLookConfig::seed_derived("Human");
        off.jacket_enabled = Some(false);
        let mut on = CharacterLookConfig::seed_derived("Human");
        on.jacket_enabled = Some(true);
        on.jacket_color = Some([255, 255, 0]);
        let a = generate_character_sprite(42, &off);
        let b = generate_character_sprite(42, &on);
        assert_ne!(a.outfit_layer, b.outfit_layer);
    }

    #[test]
    fn all_seven_hair_styles_render() {
        for style in 0..HAIR_STYLE_COUNT {
            let mut cfg = CharacterLookConfig::seed_derived("Human");
            cfg.hair_style = Some(style);
            let s = generate_character_sprite(42, &cfg);
            assert_eq!(s.hair_style_index, style);
        }
    }
}
