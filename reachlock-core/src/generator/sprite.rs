//! Character sprite generator (S25).
//! Pure function: seed + species -> pixel-art sprite layers.

use super::GeneratedTexture;
use crate::util::SeededRng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CharacterSprite {
    pub species: String,
    pub body_layer: GeneratedTexture,
    pub outfit_layer: GeneratedTexture,
    pub hair_layer: GeneratedTexture,
    pub palette_key: String,
}

const W: u32 = 32;
const H: u32 = 48;

fn base_color(species: &str, rng: &mut SeededRng) -> [u8; 4] {
    match species {
        "Human" => [rng.next_below(40) as u8 + 180, rng.next_below(30) as u8 + 160, rng.next_below(25) as u8 + 150, 255],
        "Synthetic" => [rng.next_below(30) as u8 + 100, rng.next_below(30) as u8 + 120, rng.next_below(40) as u8 + 180, 255],
        "Voidborn" => [rng.next_below(20) as u8 + 100, rng.next_below(20) as u8 + 100, rng.next_below(30) as u8 + 140, 255],
        "Augmented" => [rng.next_below(40) as u8 + 180, rng.next_below(40) as u8 + 100, rng.next_below(30) as u8 + 100, 255],
        "Xenotype" => [rng.next_below(60) as u8 + 100, rng.next_below(60) as u8 + 180, rng.next_below(40) as u8 + 80, 255],
        _ => [rng.next_below(60) as u8 + 140, rng.next_below(60) as u8 + 140, rng.next_below(60) as u8 + 140, 255],
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

fn draw_body(pixels: &mut [u8], species: &str, rng: &mut SeededRng) -> [u8; 4] {
    let skin = base_color(species, rng);
    let (head_r, torso_w, torso_h, leg_h, arm_w) = match species {
        "Human" => (6, 12, 14, 10, 4),
        "Synthetic" => (7, 14, 16, 12, 5),
        "Voidborn" => (5, 10, 16, 14, 3),
        "Augmented" => (7, 16, 14, 10, 5),
        "Xenotype" => (8, 10, 12, 8, 4),
        _ => (6, 12, 14, 10, 4),
    };
    let cx = (W / 2) as i32;
    let head_y = head_r + 1;
    let torso_y = head_y + head_r;
    let leg_y = torso_y + torso_h;

    fill_circle(pixels, cx, head_y, head_r, skin);
    fill_rect(pixels, cx - torso_w / 2, torso_y, torso_w, torso_h, skin);
    fill_rect(pixels, cx - torso_w / 2 - arm_w, torso_y + 2, arm_w, torso_h - 4, skin);
    fill_rect(pixels, cx + torso_w / 2, torso_y + 2, arm_w, torso_h - 4, skin);
    fill_rect(pixels, cx - 3, leg_y, 5, leg_h, skin);
    fill_rect(pixels, cx + 2, leg_y, 5, leg_h, skin);
    skin
}

fn draw_outfit(pixels: &mut [u8], species: &str, rng: &mut SeededRng) {
    let color = outfit_color(rng);
    let (torso_w, torso_h, leg_h) = match species {
        "Human" => (12, 14, 10),
        "Synthetic" => (14, 16, 12),
        "Voidborn" => (10, 16, 14),
        "Augmented" => (16, 14, 10),
        "Xenotype" => (10, 12, 8),
        _ => (12, 14, 10),
    };
    let cx = (W / 2) as i32;
    let head_r: i32 = match species {
        "Human" | "Augmented" => 6,
        "Synthetic" => 7,
        "Voidborn" => 5,
        "Xenotype" => 8,
        _ => 6,
    };
    let torso_y = head_r + head_r + 1;
    let leg_y = torso_y + torso_h + 1;

    fill_rect(pixels, cx - torso_w / 2 + 1, torso_y + 1, torso_w - 2, torso_h - 1, color);
    fill_rect(pixels, cx - 2, leg_y, 4, leg_h - 1, color);
    fill_rect(pixels, cx + 3, leg_y, 4, leg_h - 1, color);
}

fn draw_hair(pixels: &mut [u8], species: &str, rng: &mut SeededRng) {
    let color = hair_color(rng);
    let head_r: i32 = match species {
        "Human" | "Augmented" => 6,
        "Synthetic" => 7,
        "Voidborn" => 5,
        "Xenotype" => 8,
        _ => 6,
    };
    let cx = (W / 2) as i32;
    let head_y = head_r + 1;
    let style = rng.next_below(4);
    match style {
        0 => {
            fill_rect(pixels, cx - head_r, head_y - head_r, head_r * 2, head_r / 2 + 1, color);
        }
        1 => {
            let off = 1 + rng.next_below(3) as i32;
            fill_rect(pixels, cx - head_r - off, head_y - head_r / 2, head_r + off, head_r, color);
        }
        2 => {
            fill_circle(pixels, cx, head_y - head_r - 2, head_r - 1, color);
        }
        _ => {
            fill_rect(pixels, cx - head_r / 2, head_y - head_r - 1, head_r, 3, color);
        }
    }
}

pub fn generate_character_sprite(seed: u64, species: &str) -> CharacterSprite {
    let mut rng = SeededRng::new(seed);

    let mut body_pixels = vec![0u8; (W * H * 4) as usize];
    let skin = draw_body(&mut body_pixels, species, &mut rng);

    let mut outfit_pixels = vec![0u8; (W * H * 4) as usize];
    draw_outfit(&mut outfit_pixels, species, &mut rng);

    let mut hair_pixels = vec![0u8; (W * H * 4) as usize];
    draw_hair(&mut hair_pixels, species, &mut rng);

    let palette_key = format!("{:02x}{:02x}{:02x}", skin[0], skin[1], skin[2]);

    CharacterSprite {
        species: species.to_string(),
        body_layer: GeneratedTexture { width: W, height: H, pixels: body_pixels },
        outfit_layer: GeneratedTexture { width: W, height: H, pixels: outfit_pixels },
        hair_layer: GeneratedTexture { width: W, height: H, pixels: hair_pixels },
        palette_key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_character_sprite(42, "Human");
        let b = generate_character_sprite(42, "Human");
        assert_eq!(a.body_layer, b.body_layer);
        assert_eq!(a.outfit_layer, b.outfit_layer);
        assert_eq!(a.hair_layer, b.hair_layer);
    }

    #[test]
    fn species_differ() {
        let a = generate_character_sprite(7, "Human");
        let b = generate_character_sprite(7, "Synthetic");
        assert_ne!(a.body_layer, b.body_layer);
    }

    #[test]
    fn texture_dimensions() {
        let c = generate_character_sprite(99, "Voidborn");
        assert_eq!(c.body_layer.width, 32);
        assert_eq!(c.body_layer.height, 48);
        assert_eq!(c.body_layer.pixels.len(), 32 * 48 * 4);
    }
}
