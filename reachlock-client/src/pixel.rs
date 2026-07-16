//! Procedural pixel-art painter (render layer). Every sprite in the walking
//! modes — floor tiles, walls, furniture, characters — is painted here at
//! Terraria-guide dimensions (tiles 16×16; furniture in the 26×20/26×22/
//! 28×20 band; characters 16×26 with A-Link-to-the-Past / Chrono Trigger
//! proportions: big head, tunic, 2-frame walk, 4 facings) and rendered with
//! nearest sampling so the pixels stay crisp at the 4× interior zoom.
//!
//! No assets, per the iron rules: everything is computed from palette colors
//! and small seeded variations, so every station keeps its seed identity.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use reachlock_core::generator::RoomKind;

/// One world tile, pixels. The whole interior grid hangs off this.
pub const TILE: f32 = 16.0;

// --- tiny paint kit ----------------------------------------------------

type Rgba = [u8; 4];

/// A pixel buffer being painted. y = 0 is the TOP row (painter convention);
/// `into_image` keeps that orientation via a vertical flip into bevy's
/// bottom-up sprite space.
pub struct Px {
    w: usize,
    h: usize,
    data: Vec<Rgba>,
}

fn rgba(c: Color) -> Rgba {
    let s = c.to_srgba();
    [
        (s.red * 255.0) as u8,
        (s.green * 255.0) as u8,
        (s.blue * 255.0) as u8,
        (s.alpha * 255.0) as u8,
    ]
}

/// Multiply a color's brightness (k < 1 darkens, > 1 lightens).
fn shade(c: Rgba, k: f32) -> Rgba {
    [
        ((c[0] as f32 * k).min(255.0)) as u8,
        ((c[1] as f32 * k).min(255.0)) as u8,
        ((c[2] as f32 * k).min(255.0)) as u8,
        c[3],
    ]
}

const OUTLINE: Rgba = [22, 18, 28, 255];

impl Px {
    pub fn new(w: usize, h: usize) -> Self {
        Px {
            w,
            h,
            data: vec![[0, 0, 0, 0]; w * h],
        }
    }

    fn set(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.data[y as usize * self.w + x as usize] = c;
        }
    }

    fn get(&self, x: i32, y: i32) -> Rgba {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.data[y as usize * self.w + x as usize]
        } else {
            [0, 0, 0, 0]
        }
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgba) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    /// Darken every filled pixel that touches transparency — the classic
    /// SNES-era sprite outline.
    fn outline(&mut self) {
        let mut edges = Vec::new();
        for y in 0..self.h as i32 {
            for x in 0..self.w as i32 {
                if self.get(x, y)[3] == 0 {
                    continue;
                }
                let open = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dy)| self.get(x + dx, y + dy)[3] == 0);
                if open {
                    edges.push((x, y));
                }
            }
        }
        for (x, y) in edges {
            self.set(x, y, OUTLINE);
        }
    }

    /// Horizontal mirror (paint left-facing, mirror for right-facing).
    fn mirrored(&self) -> Px {
        let mut out = Px::new(self.w, self.h);
        for y in 0..self.h {
            for x in 0..self.w {
                out.data[y * self.w + (self.w - 1 - x)] = self.data[y * self.w + x];
            }
        }
        out
    }

    pub fn into_image(self) -> Image {
        let mut bytes = Vec::with_capacity(self.w * self.h * 4);
        // Flip vertically: painter rows are top-down, sprites sample
        // bottom-up.
        for y in (0..self.h).rev() {
            for x in 0..self.w {
                bytes.extend_from_slice(&self.data[y * self.w + x]);
            }
        }
        let mut image = Image::new(
            Extent3d {
                width: self.w as u32,
                height: self.h as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            bytes,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = ImageSampler::nearest();
        image
    }
}

/// Tiny deterministic noise stream for dither/scratches.
struct Noise(u64);
impl Noise {
    fn next(&mut self) -> u64 {
        let mut x = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        self.0 = x;
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 27;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

// --- floor & wall tiles -------------------------------------------------

/// Paint one 32×32 floor texture (a 2×2 block of 16px tiles, so the repeat
/// is less obvious) in the pattern that fits the room kind.
pub fn floor_texture(kind: RoomKind, base: Color, seed: u64) -> Image {
    let b = rgba(base);
    let mut px = Px::new(32, 32);
    let mut n = Noise(seed);
    match kind {
        // Wood planks: the bar/galley (Stardew tavern floors).
        RoomKind::Bar => {
            for row in 0..6 {
                let y = row * 6;
                let tone = if row % 2 == 0 { 0.95 } else { 0.8 };
                px.rect(0, y, 32, 6, shade(b, tone));
                px.rect(0, y, 32, 1, shade(b, 0.55)); // plank seam
                let joint = n.below(28) as i32 + 2;
                px.rect(joint, y + 1, 1, 5, shade(b, 0.6)); // board joint
            }
        }
        // Carpet: quarters/habs.
        RoomKind::Quarters => {
            px.rect(0, 0, 32, 32, shade(b, 0.85));
            for ty in [0, 16] {
                for tx in [0, 16] {
                    px.rect(tx + 1, ty + 1, 14, 14, shade(b, 0.95));
                    px.rect(tx + 6, ty + 6, 4, 4, shade(b, 0.8)); // motif
                }
            }
        }
        // Grating: corridors and the reactor room.
        RoomKind::Corridor | RoomKind::Reactor => {
            px.rect(0, 0, 32, 32, shade(b, 0.55));
            for i in (0..32).step_by(4) {
                px.rect(i, 0, 1, 32, shade(b, 0.85));
                px.rect(0, i, 32, 1, shade(b, 0.85));
            }
            for _ in 0..6 {
                let (x, y) = (n.below(32) as i32, n.below(32) as i32);
                px.set(x, y, shade(b, 0.4)); // wear
            }
        }
        // Poured deck: hangar / repair bay (dither of two shades).
        RoomKind::Hangar | RoomKind::Shipyard => {
            px.rect(0, 0, 32, 32, shade(b, 0.8));
            for _ in 0..170 {
                let (x, y) = (n.below(32) as i32, n.below(32) as i32);
                px.set(x, y, shade(b, if n.below(2) == 0 { 0.7 } else { 0.9 }));
            }
        }
        // Metal deck plate: bridge / market / admin.
        RoomKind::Bridge | RoomKind::Market => {
            px.rect(0, 0, 32, 32, shade(b, 0.9));
            for ty in [0, 16] {
                for tx in [0, 16] {
                    px.rect(tx, ty, 16, 1, shade(b, 0.65));
                    px.rect(tx, ty, 1, 16, shade(b, 0.65));
                    px.rect(tx + 1, ty + 1, 15, 1, shade(b, 1.1));
                    px.set(tx + 2, ty + 13, shade(b, 0.6)); // rivets
                    px.set(tx + 13, ty + 2, shade(b, 0.6));
                }
            }
        }
    }
    px.into_image()
}

/// 16×16 wall tile: dark plating with a lit top edge and a vertical seam.
pub fn wall_texture(base: Color) -> Image {
    let b = rgba(base);
    let mut px = Px::new(16, 16);
    px.rect(0, 0, 16, 16, shade(b, 0.30));
    px.rect(0, 0, 16, 2, shade(b, 0.5)); // catch the light on top
    px.rect(0, 15, 16, 1, shade(b, 0.18));
    px.rect(7, 0, 1, 16, shade(b, 0.22)); // seam
    px.into_image()
}

/// Door threshold: light plate with hazard-stripe edges (48×36 spans a
/// 3-tile-wide opening across the party wall).
pub fn threshold_sprite(base: Color, accent: Color) -> Image {
    let b = rgba(base);
    let a = rgba(accent);
    let mut px = Px::new(48, 36);
    px.rect(0, 0, 48, 36, shade(b, 1.05));
    for y in [0, 33] {
        for x in (0..48).step_by(6) {
            px.rect(x, y, 3, 3, a);
            px.rect(x + 3, y, 3, 3, OUTLINE);
        }
    }
    px.into_image()
}

// --- characters ----------------------------------------------------------

/// The palette a figure is painted with.
#[derive(Clone, Copy)]
pub struct Look {
    pub skin: Color,
    pub hair: Color,
    pub shirt: Color,
    pub pants: Color,
}

impl Look {
    /// Seeded civilian look: varied skin/hair, seeded shirt.
    pub fn seeded(seed: u64) -> Look {
        let mut n = Noise(seed);
        let skins = [
            Color::srgb(0.96, 0.80, 0.64),
            Color::srgb(0.87, 0.67, 0.48),
            Color::srgb(0.62, 0.44, 0.30),
            Color::srgb(0.45, 0.31, 0.22),
        ];
        let hairs = [
            Color::srgb(0.15, 0.12, 0.10),
            Color::srgb(0.35, 0.22, 0.10),
            Color::srgb(0.75, 0.65, 0.35),
            Color::srgb(0.45, 0.45, 0.50),
            Color::srgb(0.55, 0.25, 0.15),
        ];
        Look {
            skin: skins[n.below(skins.len() as u64) as usize],
            hair: hairs[n.below(hairs.len() as u64) as usize],
            shirt: Color::srgb(
                0.25 + n.below(60) as f32 / 100.0,
                0.25 + n.below(60) as f32 / 100.0,
                0.25 + n.below(60) as f32 / 100.0,
            ),
            pants: Color::srgb(0.22, 0.22, 0.30),
        }
    }
}

/// Facing directions, indexing [`character_frames`]' outer array.
pub const DIR_DOWN: usize = 0;
pub const DIR_UP: usize = 1;
pub const DIR_LEFT: usize = 2;
pub const DIR_RIGHT: usize = 3;

/// Paint the full 4-direction × 2-frame walk set for a look. Frame 0 doubles
/// as the idle pose.
pub fn character_frames(images: &mut Assets<Image>, look: Look) -> [[Handle<Image>; 2]; 4] {
    let f = |images: &mut Assets<Image>, dir: usize, frame: usize| {
        images.add(paint_character(dir, frame, look).into_image())
    };
    [
        [f(images, DIR_DOWN, 0), f(images, DIR_DOWN, 1)],
        [f(images, DIR_UP, 0), f(images, DIR_UP, 1)],
        [f(images, DIR_LEFT, 0), f(images, DIR_LEFT, 1)],
        [f(images, DIR_RIGHT, 0), f(images, DIR_RIGHT, 1)],
    ]
}

/// 16×26 character in SNES-JRPG proportions: oversized head, tunic torso,
/// short legs. `frame` 1 is mid-stride.
fn paint_character(dir: usize, frame: usize, look: Look) -> Px {
    if dir == DIR_RIGHT {
        return paint_character(DIR_LEFT, frame, look).mirrored();
    }
    let skin = rgba(look.skin);
    let hair = rgba(look.hair);
    let shirt = rgba(look.shirt);
    let pants = rgba(look.pants);
    let boots = shade(pants, 0.6);
    let mut px = Px::new(16, 26);

    // Head (rows 0..10): hair cap over a face — or the back of the head.
    px.rect(3, 1, 10, 5, hair);
    px.rect(2, 3, 12, 3, hair);
    match dir {
        DIR_UP => {
            px.rect(3, 5, 10, 5, hair); // back of head: all hair
        }
        DIR_DOWN => {
            px.rect(4, 5, 8, 5, skin);
            px.rect(3, 5, 1, 3, hair); // sideburns
            px.rect(12, 5, 1, 3, hair);
            px.set(5, 7, OUTLINE); // eyes
            px.set(6, 7, OUTLINE);
            px.set(9, 7, OUTLINE);
            px.set(10, 7, OUTLINE);
        }
        _ => {
            // Left profile.
            px.rect(4, 5, 7, 5, skin);
            px.rect(9, 5, 4, 5, hair); // hair falls to the back
            px.set(5, 7, OUTLINE); // one eye
            px.set(4, 8, shade(skin, 0.85)); // nose hint
        }
    }

    // Torso (rows 10..19): tunic with arms.
    let sway = if frame == 1 { 1 } else { 0 };
    match dir {
        DIR_LEFT => {
            px.rect(4, 10, 8, 9, shirt);
            // Leading arm swings with the stride.
            px.rect(6 - sway * 2, 12, 2, 6, shirt);
            px.rect(6 - sway * 2, 17, 2, 2, skin); // hand
        }
        _ => {
            px.rect(3, 10, 10, 9, shirt);
            // Arms at the sides; the stride frame swings them apart.
            px.rect(2, 11 + sway, 1, 6, shirt);
            px.rect(13, 12 - sway, 1, 6, shirt);
            px.rect(2, 17 + sway, 1, 2, skin); // hands
            px.rect(13, 18 - sway, 1, 2, skin);
        }
    }
    px.rect(3, 18, 10, 1, shade(shirt, 0.6)); // belt

    // Legs (rows 19..26).
    match dir {
        DIR_LEFT => {
            if frame == 0 {
                px.rect(6, 19, 4, 5, pants);
                px.rect(6, 24, 4, 2, boots);
            } else {
                px.rect(4, 19, 3, 5, pants); // front leg
                px.rect(4, 24, 3, 2, boots);
                px.rect(9, 19, 3, 4, pants); // back leg, lifted
                px.rect(9, 23, 3, 2, boots);
            }
        }
        _ => {
            let lift = if frame == 1 { 1 } else { 0 };
            px.rect(4, 19, 3, 5 - lift, pants);
            px.rect(4, 24 - lift, 3, 2, boots);
            px.rect(9, 19 + lift, 3, 5 - lift, pants);
            px.rect(9, 24, 3, 2, boots);
        }
    }

    px.outline();
    px
}

/// Soft drop shadow under figures (16×6, alpha ellipse).
pub fn shadow_sprite() -> Image {
    let mut px = Px::new(16, 6);
    for y in 0..6 {
        for x in 0..16 {
            let dx = (x as f32 - 7.5) / 8.0;
            let dy = (y as f32 - 2.5) / 3.0;
            if dx * dx + dy * dy <= 1.0 {
                px.set(x, y, [0, 0, 0, 70]);
            }
        }
    }
    px.into_image()
}

// --- furniture & props (Terraria furniture dimensions) -------------------

/// 26×22 crate/chest.
pub fn crate_sprite(seed: u64) -> Image {
    let mut n = Noise(seed);
    let wood = rgba(Color::srgb(0.55, 0.42, 0.26));
    let wood = shade(wood, 0.9 + n.below(20) as f32 / 100.0);
    let mut px = Px::new(26, 22);
    px.rect(0, 0, 26, 22, wood);
    px.rect(0, 0, 26, 2, shade(wood, 1.2));
    px.rect(0, 20, 26, 2, shade(wood, 0.7));
    // Cross braces.
    for i in 0..22 {
        px.set(2 + i, i, shade(wood, 0.65));
        px.set(3 + i, i, shade(wood, 0.65));
        px.set(23 - i, i, shade(wood, 0.65));
        px.set(22 - i, i, shade(wood, 0.65));
    }
    px.rect(0, 0, 2, 22, shade(wood, 0.75));
    px.rect(24, 0, 2, 22, shade(wood, 0.75));
    px.outline();
    px.into_image()
}

/// 26×20 table.
pub fn table_sprite() -> Image {
    let wood = rgba(Color::srgb(0.5, 0.36, 0.22));
    let mut px = Px::new(26, 20);
    px.rect(0, 0, 26, 10, shade(wood, 1.15)); // top
    px.rect(0, 0, 26, 2, shade(wood, 1.3));
    px.rect(1, 10, 3, 9, shade(wood, 0.7)); // legs
    px.rect(22, 10, 3, 9, shade(wood, 0.7));
    px.outline();
    px.into_image()
}

/// 28×20 bunk/bed.
pub fn bunk_sprite(blanket: Color) -> Image {
    let frame = rgba(Color::srgb(0.35, 0.35, 0.42));
    let sheet = rgba(Color::srgb(0.85, 0.85, 0.9));
    let cover = rgba(blanket);
    let mut px = Px::new(28, 20);
    px.rect(0, 0, 28, 20, frame);
    px.rect(1, 2, 26, 16, sheet);
    px.rect(2, 3, 7, 6, shade(sheet, 1.1)); // pillow
    px.rect(10, 2, 17, 16, cover);
    px.rect(10, 2, 17, 2, shade(cover, 1.2)); // fold
    px.outline();
    px.into_image()
}

/// 24×20 systems console: glowing screen over a keyboard deck.
pub fn console_sprite(glow: Color) -> Image {
    let body = rgba(Color::srgb(0.30, 0.33, 0.38));
    let g = rgba(glow);
    let mut px = Px::new(24, 20);
    px.rect(0, 2, 24, 18, body);
    px.rect(2, 0, 20, 12, shade(body, 0.7)); // screen bezel
    px.rect(3, 1, 18, 10, shade(g, 0.55)); // screen
    px.rect(4, 2, 8, 1, g); // readout lines
    px.rect(4, 4, 12, 1, shade(g, 0.8));
    px.rect(4, 6, 6, 1, shade(g, 0.9));
    for x in (3..21).step_by(3) {
        px.rect(x, 15, 2, 1, shade(body, 1.3)); // keys
        px.rect(x, 17, 2, 1, shade(body, 1.2));
    }
    px.outline();
    px.into_image()
}

/// 32×32 reactor core: framed ring around a hot center.
pub fn core_sprite(glow: Color) -> Image {
    let frame = rgba(Color::srgb(0.32, 0.33, 0.4));
    let g = rgba(glow);
    let mut px = Px::new(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let d = (dx * dx + dy * dy).sqrt();
            if d < 6.0 {
                px.set(x, y, [255, 250, 220, 255]);
            } else if d < 10.0 {
                px.set(x, y, g);
            } else if d < 12.0 {
                px.set(x, y, shade(g, 0.5));
            } else if d < 15.0 {
                px.set(x, y, frame);
            }
        }
    }
    // Emitter studs.
    for (x, y) in [(15, 0), (15, 30), (0, 15), (30, 15)] {
        px.rect(x, y, 2, 2, shade(frame, 1.3));
    }
    px.outline();
    px.into_image()
}

/// 48×20 service counter (market/bar).
pub fn counter_sprite(base: Color) -> Image {
    let b = rgba(base);
    let mut px = Px::new(48, 20);
    px.rect(0, 0, 48, 8, shade(b, 1.2)); // top
    px.rect(0, 0, 48, 2, shade(b, 1.4));
    px.rect(0, 8, 48, 12, shade(b, 0.75)); // front panel
    px.rect(0, 12, 48, 2, shade(b, 0.55)); // trim stripe
    px.outline();
    px.into_image()
}

/// 16×22 potted plant — the mandatory JRPG interior foliage.
pub fn plant_sprite(seed: u64) -> Image {
    let mut n = Noise(seed);
    let pot = rgba(Color::srgb(0.62, 0.35, 0.22));
    let leaf = rgba(Color::srgb(0.25, 0.55, 0.28));
    let mut px = Px::new(16, 22);
    px.rect(4, 15, 8, 6, pot);
    px.rect(3, 14, 10, 2, shade(pot, 1.2));
    for _ in 0..26 {
        let x = 3 + n.below(10) as i32;
        let y = 2 + n.below(12) as i32;
        px.set(x, y, shade(leaf, 0.8 + n.below(40) as f32 / 100.0));
    }
    px.rect(7, 6, 2, 9, shade(leaf, 0.6)); // stem
    px.outline();
    px.into_image()
}

/// 64×64 landing pad marking (painted on the hangar floor).
pub fn pad_sprite(accent: Color) -> Image {
    let a = rgba(accent);
    let mut px = Px::new(64, 64);
    for y in 0..64 {
        for x in 0..64 {
            let dx = x as f32 - 31.5;
            let dy = y as f32 - 31.5;
            let d = (dx * dx + dy * dy).sqrt();
            if (28.0..31.0).contains(&d) {
                px.set(x, y, shade(a, 0.9));
            }
        }
    }
    // Center cross.
    px.rect(29, 20, 6, 24, shade(a, 0.8));
    px.rect(20, 29, 24, 6, shade(a, 0.8));
    px.into_image()
}

/// 48×16 viewport strip: stars through armored glass.
pub fn viewport_sprite(seed: u64) -> Image {
    let mut n = Noise(seed);
    let mut px = Px::new(48, 16);
    px.rect(0, 0, 48, 16, [10, 14, 34, 255]);
    for _ in 0..22 {
        let x = 1 + n.below(46) as i32;
        let y = 1 + n.below(14) as i32;
        let b = 120 + n.below(135) as u8;
        px.set(x, y, [b, b, b.saturating_add(20), 255]);
    }
    px.rect(0, 0, 48, 1, [70, 80, 100, 255]);
    px.rect(0, 15, 48, 1, [70, 80, 100, 255]);
    for x in [0, 15, 31, 47] {
        px.rect(x, 0, 1, 16, [70, 80, 100, 255]);
    }
    px.into_image()
}

/// 36×36 interaction ring (hollow circle, drawn over the highlight target).
pub fn ring_sprite() -> Image {
    let mut px = Px::new(36, 36);
    for y in 0..36 {
        for x in 0..36 {
            let dx = x as f32 - 17.5;
            let dy = y as f32 - 17.5;
            let d = (dx * dx + dy * dy).sqrt();
            if (14.0..17.0).contains(&d) {
                px.set(x, y, [255, 240, 140, 190]);
            }
        }
    }
    px.into_image()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(px: &Px) -> usize {
        px.data.iter().filter(|p| p[3] != 0).count()
    }

    #[test]
    fn characters_are_16x26_and_painted() {
        for dir in 0..4 {
            for frame in 0..2 {
                let px = paint_character(dir, frame, Look::seeded(7));
                assert_eq!((px.w, px.h), (16, 26));
                // A real figure fills a meaningful chunk of the canvas.
                assert!(filled(&px) > 120, "dir {dir} frame {frame}");
            }
        }
    }

    #[test]
    fn stride_frame_differs_from_idle() {
        let look = Look::seeded(7);
        let a = paint_character(DIR_DOWN, 0, look);
        let b = paint_character(DIR_DOWN, 1, look);
        assert_ne!(a.data, b.data);
    }

    #[test]
    fn right_is_mirror_of_left() {
        let look = Look::seeded(7);
        let left = paint_character(DIR_LEFT, 0, look);
        let right = paint_character(DIR_RIGHT, 0, look);
        assert_eq!(left.mirrored().data, right.data);
    }

    #[test]
    fn looks_are_seed_stable_and_varied() {
        let a = paint_character(DIR_DOWN, 0, Look::seeded(1));
        let b = paint_character(DIR_DOWN, 0, Look::seeded(1));
        assert_eq!(a.data, b.data);
        let c = paint_character(DIR_DOWN, 0, Look::seeded(2));
        assert_ne!(a.data, c.data);
    }

    #[test]
    fn outline_darkens_silhouette_edges() {
        let mut px = Px::new(4, 4);
        px.rect(1, 1, 2, 2, [200, 0, 0, 255]);
        px.outline();
        // Every filled pixel of a 2×2 block borders transparency.
        assert_eq!(px.get(1, 1), OUTLINE);
        assert_eq!(px.get(2, 2), OUTLINE);
        assert_eq!(px.get(0, 0)[3], 0);
    }
}
