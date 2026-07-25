//! Seeded color palettes. 8-bit RGBA — plain data, converted by the client
//! bridge. Palette construction is integer HSV→RGB so it is deterministic.

use serde::{Deserialize, Serialize};

use super::rng::SeededRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Integer HSV → RGB. h in [0, 1536) (256 per sextant), s and v in [0, 255].
pub fn hsv(h: u32, s: u32, v: u32) -> ColorRgba8 {
    let h = h % 1536;
    let sextant = h / 256;
    let f = h % 256;
    let p = v * (255 - s) / 255;
    let q = v * (255 * 256 - s * f) / (255 * 256);
    let t = v * (255 * 256 - s * (256 - f)) / (255 * 256);
    let (r, g, b) = match sextant {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ColorRgba8 {
        r: r.min(255) as u8,
        g: g.min(255) as u8,
        b: b.min(255) as u8,
        a: 255,
    }
}

/// A faction/biome palette: base hue plus analogous accent and a dimmed
/// structural tone. Same seed, same palette, everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub primary: ColorRgba8,
    pub accent: ColorRgba8,
    pub structure: ColorRgba8,
}

pub fn generate_palette(seed: u64) -> Palette {
    let mut rng = SeededRng::new(seed);
    let hue = rng.next_below(1536) as u32;
    let accent_hue = (hue + 256 + rng.next_below(256) as u32) % 1536;
    Palette {
        primary: hsv(hue, 140 + rng.next_below(80) as u32, 200),
        accent: hsv(accent_hue, 180, 230),
        structure: hsv(hue, 60, 90),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        assert_eq!(generate_palette(99), generate_palette(99));
    }

    #[test]
    fn hsv_pure_hues() {
        assert_eq!(
            hsv(0, 255, 255),
            ColorRgba8 {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            }
        );
        assert_eq!(
            hsv(512, 255, 255),
            ColorRgba8 {
                r: 0,
                g: 255,
                b: 0,
                a: 255
            }
        );
    }
}
