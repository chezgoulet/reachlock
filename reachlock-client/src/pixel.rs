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

/// A pixel buffer being painted. y = 0 is the TOP row — the same layout
/// `Image::new` expects, so `into_image` copies rows straight through.
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

    /// Raw RGBA bytes, top row first — exactly the layout `Image::new`
    /// expects (flipping here is what made every sprite stand on its head).
    fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.w * self.h * 4);
        for p in &self.data {
            bytes.extend_from_slice(p);
        }
        bytes
    }

    pub fn into_image(self) -> Image {
        let bytes = self.bytes();
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
        // Poured deck: hangar / repair bay / tech bay (dither of two shades).
        RoomKind::Hangar | RoomKind::Shipyard | RoomKind::TechBay => {
            px.rect(0, 0, 32, 32, shade(b, 0.8));
            for _ in 0..170 {
                let (x, y) = (n.below(32) as i32, n.below(32) as i32);
                px.set(x, y, shade(b, if n.below(2) == 0 { 0.7 } else { 0.9 }));
            }
        }
        // Metal deck plate: bridge / market / admin, and the ship's clean
        // technical rooms (cockpit, scanner, med bay, cryo) — the per-kind
        // base color carries the distinction.
        RoomKind::Bridge
        | RoomKind::Market
        | RoomKind::Cockpit
        | RoomKind::Scanner
        | RoomKind::MedBay
        | RoomKind::Cryo => {
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

/// What kind of being a figure is. Changes how the painter renders it —
/// androids get alloy skin, lit eyes, and a faceplate seam; a robot (BOR-IS)
/// is a boxy chassis with a sensor visor, unmistakably not a person in a suit
/// (docs/LORE.md §V).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Human,
    Android,
    Robot,
}

/// Hair silhouette — the main variety layer for station crowds. Crew get
/// theirs from the lore via [`crew_look`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hair {
    Short,
    Buzz,
    Long,
    Locs,
    Bun,
    /// Sleek swept-back crest (Prudence).
    Crest,
    Bald,
}

/// The palette and build a figure is painted with. On androids `hair` doubles
/// as the eye-glow color; on a robot `shirt` is the hull, `pants` the joints,
/// and `hair` the sensor visor.
#[derive(Clone, Copy)]
pub struct Look {
    pub skin: Color,
    pub hair: Color,
    pub shirt: Color,
    pub pants: Color,
    /// Worn open over the shirt — the spacer-jacket layer.
    pub jacket: Option<Color>,
    pub hair_style: Hair,
    pub body: BodyKind,
}

impl Look {
    /// Seeded civilian look: varied skin/hair/build, seeded shirt, sometimes
    /// a jacket — and roughly one in ten port civilians is an android (the
    /// personhood question is a live one at every Compact-adjacent port).
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
        let glows = [
            Color::srgb(0.30, 0.85, 0.80),
            Color::srgb(0.95, 0.65, 0.20),
            Color::srgb(0.85, 0.30, 0.60),
        ];
        let styles = [
            Hair::Short,
            Hair::Short,
            Hair::Buzz,
            Hair::Long,
            Hair::Locs,
            Hair::Bun,
        ];
        let android = n.below(10) == 0;
        let jacket = if n.below(3) == 0 {
            Some(Color::srgb(
                0.20 + n.below(35) as f32 / 100.0,
                0.20 + n.below(35) as f32 / 100.0,
                0.20 + n.below(35) as f32 / 100.0,
            ))
        } else {
            None
        };
        Look {
            skin: if android {
                Color::srgb(0.62, 0.66, 0.72)
            } else {
                skins[n.below(skins.len() as u64) as usize]
            },
            hair: if android {
                glows[n.below(glows.len() as u64) as usize]
            } else {
                hairs[n.below(hairs.len() as u64) as usize]
            },
            shirt: Color::srgb(
                0.25 + n.below(60) as f32 / 100.0,
                0.25 + n.below(60) as f32 / 100.0,
                0.25 + n.below(60) as f32 / 100.0,
            ),
            pants: Color::srgb(0.22, 0.22, 0.30),
            jacket,
            hair_style: if android && n.below(2) == 0 {
                Hair::Bald
            } else {
                styles[n.below(styles.len() as u64) as usize]
            },
            body: if android {
                BodyKind::Android
            } else {
                BodyKind::Human
            },
        }
    }
}

/// Canonical looks for the Loup-Garou's crew, keyed by roster id
/// (docs/LORE.md §V). Unknown ids fall back to a seeded civilian look so a
/// modded roster still paints.
pub fn crew_look(id: &str) -> Look {
    let human = |skin, hair, shirt, pants, jacket, hair_style| Look {
        skin,
        hair,
        shirt,
        pants,
        jacket,
        hair_style,
        body: BodyKind::Human,
    };
    match id {
        // Tib: Québécois captain — dark hair, worn brown flight jacket.
        "tib" => human(
            Color::srgb(0.82, 0.62, 0.45),
            Color::srgb(0.16, 0.12, 0.10),
            Color::srgb(0.45, 0.44, 0.42),
            Color::srgb(0.25, 0.22, 0.18),
            Some(Color::srgb(0.42, 0.28, 0.16)),
            Hair::Short,
        ),
        // Tove: engineer — ash-blond buzz, orange drive-deck coveralls.
        "tove" => human(
            Color::srgb(0.93, 0.78, 0.66),
            Color::srgb(0.72, 0.66, 0.50),
            Color::srgb(0.75, 0.42, 0.16),
            Color::srgb(0.55, 0.32, 0.14),
            None,
            Hair::Buzz,
        ),
        // Doc Keene: trauma surgeon — teal scrubs under a white coat.
        "keene" => human(
            Color::srgb(0.42, 0.28, 0.20),
            Color::srgb(0.10, 0.08, 0.08),
            Color::srgb(0.20, 0.55, 0.52),
            Color::srgb(0.16, 0.30, 0.30),
            Some(Color::srgb(0.88, 0.88, 0.90)),
            Hair::Bun,
        ),
        // Bardo: linguist — locs, warm gold shirt, deep red jacket.
        "bardo" => human(
            Color::srgb(0.55, 0.38, 0.26),
            Color::srgb(0.12, 0.10, 0.09),
            Color::srgb(0.80, 0.60, 0.20),
            Color::srgb(0.35, 0.25, 0.30),
            Some(Color::srgb(0.48, 0.18, 0.20)),
            Hair::Locs,
        ),
        // Prudence: femme android — pearl alloy, magenta crest and eyes,
        // navy flight suit.
        "prudence" => Look {
            skin: Color::srgb(0.80, 0.82, 0.88),
            hair: Color::srgb(0.85, 0.30, 0.60),
            shirt: Color::srgb(0.18, 0.24, 0.42),
            pants: Color::srgb(0.12, 0.16, 0.30),
            jacket: None,
            hair_style: Hair::Crest,
            body: BodyKind::Android,
        },
        // Risc: angular android — gunmetal alloy, amber eyes, utility rig.
        "risc" => Look {
            skin: Color::srgb(0.45, 0.48, 0.52),
            hair: Color::srgb(0.95, 0.65, 0.20),
            shirt: Color::srgb(0.30, 0.33, 0.24),
            pants: Color::srgb(0.20, 0.22, 0.18),
            jacket: None,
            hair_style: Hair::Bald,
            body: BodyKind::Android,
        },
        // BOR-IS: EVA robot — grey hull, dark joints, amber sensor visor.
        "boris" => Look {
            skin: Color::srgb(0.55, 0.56, 0.58),
            hair: Color::srgb(0.95, 0.62, 0.18),
            shirt: Color::srgb(0.55, 0.56, 0.58),
            pants: Color::srgb(0.28, 0.30, 0.33),
            jacket: None,
            hair_style: Hair::Bald,
            body: BodyKind::Robot,
        },
        other => Look::seeded(
            other
                .bytes()
                .fold(0u64, |a, b| a.wrapping_mul(31) + b as u64),
        ),
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

/// Per-style hair over the skull base. `dir` picks the visible silhouette:
/// facing down shows the cap and framing, up shows the back, left the sweep.
fn paint_hair(px: &mut Px, dir: usize, style: Hair, hair: Rgba) {
    match style {
        Hair::Bald => {}
        Hair::Buzz => {
            px.rect(3, 1, 10, 3, hair);
            if dir == DIR_UP {
                px.rect(3, 3, 10, 4, hair);
            }
            if dir == DIR_LEFT {
                px.rect(9, 3, 4, 3, hair);
            }
        }
        Hair::Short => {
            px.rect(3, 1, 10, 4, hair);
            px.rect(2, 3, 12, 2, hair);
            match dir {
                DIR_UP => px.rect(3, 5, 10, 5, hair),
                DIR_DOWN => {
                    px.rect(3, 5, 1, 3, hair); // sideburns
                    px.rect(12, 5, 1, 3, hair);
                }
                _ => px.rect(9, 4, 4, 5, hair), // falls to the back
            }
        }
        Hair::Long => {
            px.rect(3, 1, 10, 4, hair);
            px.rect(2, 3, 12, 2, hair);
            match dir {
                DIR_UP => {
                    px.rect(3, 5, 10, 5, hair);
                    px.rect(4, 10, 8, 4, hair); // down past the shoulders
                }
                DIR_DOWN => {
                    px.rect(2, 5, 2, 7, hair); // framing falls
                    px.rect(12, 5, 2, 7, hair);
                }
                _ => {
                    px.rect(9, 4, 4, 6, hair);
                    px.rect(10, 10, 3, 4, hair);
                }
            }
        }
        Hair::Locs => {
            px.rect(3, 1, 10, 4, hair);
            match dir {
                DIR_UP => {
                    px.rect(3, 5, 10, 4, hair);
                    for x in [3, 5, 8, 10, 12] {
                        px.rect(x, 9, 1, 3, hair); // strands
                    }
                }
                DIR_DOWN => {
                    px.rect(2, 4, 1, 5, hair);
                    px.rect(13, 4, 1, 5, hair);
                    px.rect(3, 4, 1, 3, hair);
                    px.rect(12, 4, 1, 3, hair);
                }
                _ => {
                    px.rect(9, 4, 4, 4, hair);
                    for x in [10, 12] {
                        px.rect(x, 8, 1, 4, hair);
                    }
                }
            }
        }
        Hair::Bun => {
            px.rect(3, 1, 10, 4, hair);
            px.rect(2, 3, 12, 2, hair);
            match dir {
                DIR_UP => {
                    px.rect(3, 5, 10, 4, hair);
                    px.rect(6, 0, 4, 3, hair); // the bun
                }
                DIR_DOWN => {
                    px.rect(3, 5, 1, 2, hair);
                    px.rect(12, 5, 1, 2, hair);
                }
                _ => {
                    px.rect(9, 4, 4, 4, hair);
                    px.rect(11, 0, 4, 4, hair); // bun at the back
                }
            }
        }
        Hair::Crest => {
            px.rect(4, 0, 8, 3, hair);
            px.rect(3, 2, 10, 2, hair);
            match dir {
                DIR_UP => px.rect(4, 4, 8, 5, hair),
                DIR_DOWN => {}
                _ => {
                    px.rect(8, 2, 6, 3, hair); // swept back
                    px.rect(11, 5, 3, 3, hair);
                }
            }
        }
    }
}

/// 16×26 character in SNES-JRPG proportions: oversized head, layered torso,
/// short legs. `frame` 1 is mid-stride. Robots take their own painter.
fn paint_character(dir: usize, frame: usize, look: Look) -> Px {
    if look.body == BodyKind::Robot {
        return paint_robot(dir, frame, look);
    }
    if dir == DIR_RIGHT {
        return paint_character(DIR_LEFT, frame, look).mirrored();
    }
    let skin = rgba(look.skin);
    let hair = rgba(look.hair);
    let shirt = rgba(look.shirt);
    let pants = rgba(look.pants);
    let boots = shade(pants, 0.6);
    let android = look.body == BodyKind::Android;
    // Android eyes glow their signature color; human eyes are dark.
    let eye = if android { shade(hair, 1.2) } else { OUTLINE };
    let mut px = Px::new(16, 26);

    // Head (rows 0..10): skull base in skin, face by direction, hair on top.
    px.rect(3, 1, 10, 9, skin);
    px.rect(2, 3, 12, 5, skin);
    match dir {
        DIR_UP => {}
        DIR_DOWN => {
            px.set(5, 7, eye);
            px.set(6, 7, eye);
            px.set(9, 7, eye);
            px.set(10, 7, eye);
            if android {
                px.rect(8, 5, 1, 4, shade(skin, 0.8)); // faceplate seam
            }
        }
        _ => {
            // Left profile.
            px.set(5, 7, eye);
            px.set(4, 8, shade(skin, 0.85)); // nose hint
        }
    }
    paint_hair(&mut px, dir, look.hair_style, hair);

    // Torso (rows 10..19): shirt, optionally under an open jacket; arms in
    // the outermost layer.
    let sway = if frame == 1 { 1 } else { 0 };
    let sleeve = look.jacket.map(rgba).unwrap_or(shirt);
    match dir {
        DIR_LEFT => {
            px.rect(4, 10, 8, 9, sleeve);
            if look.jacket.is_some() {
                px.rect(4, 11, 1, 7, shirt); // shirt at the open front edge
            }
            // Leading arm swings with the stride.
            px.rect(6 - sway * 2, 12, 2, 6, sleeve);
            px.rect(6 - sway * 2, 17, 2, 2, skin); // hand
        }
        _ => {
            px.rect(3, 10, 10, 9, shirt);
            if let Some(j) = look.jacket.map(rgba) {
                px.rect(3, 10, 3, 9, j); // open jacket panels
                px.rect(10, 10, 3, 9, j);
                px.rect(3, 10, 10, 1, shade(j, 1.15)); // collar
            }
            // Arms at the sides; the stride frame swings them apart.
            px.rect(2, 11 + sway, 1, 6, sleeve);
            px.rect(13, 12 - sway, 1, 6, sleeve);
            px.rect(2, 17 + sway, 1, 2, skin); // hands
            px.rect(13, 18 - sway, 1, 2, skin);
        }
    }
    px.rect(3, 18, 10, 1, shade(pants, 0.8)); // belt

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

/// 16×26 EVA robot (BOR-IS): boxy sensor head, plated chassis, piston limbs.
/// Clearly mechanical — not a person in a suit. `look.shirt` is the hull,
/// `look.pants` the joints, `look.hair` the visor glow. Facing down, the
/// small pale mark on the inside of his left forearm is visible; no one has
/// asked about it yet.
fn paint_robot(dir: usize, frame: usize, look: Look) -> Px {
    if dir == DIR_RIGHT {
        return paint_robot(DIR_LEFT, frame, look).mirrored();
    }
    let hull = rgba(look.shirt);
    let joint = rgba(look.pants);
    let visor = rgba(look.hair);
    let mut px = Px::new(16, 26);

    // Head (rows 0..9): a sensor block, not a face.
    px.rect(4, 1, 8, 7, hull);
    px.rect(4, 1, 8, 1, shade(hull, 1.25));
    match dir {
        DIR_DOWN => {
            px.rect(5, 3, 6, 2, visor);
            px.set(10, 3, shade(visor, 1.3)); // tracking glint
        }
        DIR_UP => {
            px.rect(5, 3, 6, 3, shade(hull, 0.7)); // vent panel
            px.rect(6, 4, 4, 1, shade(hull, 0.5));
        }
        _ => px.rect(4, 3, 4, 2, visor),
    }
    px.set(12, 0, shade(visor, 1.2)); // antenna nub
    px.rect(6, 8, 4, 1, joint); // neck

    // Torso (rows 9..19): plated chassis, shoulder blocks, chest plate.
    px.rect(3, 9, 10, 10, hull);
    px.rect(2, 9, 2, 4, shade(hull, 0.85));
    px.rect(12, 9, 2, 4, shade(hull, 0.85));
    if dir == DIR_UP {
        px.rect(5, 10, 6, 6, shade(hull, 0.7)); // power pack
    } else {
        px.rect(5, 11, 6, 4, shade(hull, 1.15));
        px.set(5, 11, joint); // bolts
        px.set(10, 11, joint);
    }
    px.rect(3, 16, 10, 1, joint); // waist seam

    // Piston arms with clamp hands; the stride frame swings them.
    let sway = if frame == 1 { 1 } else { 0 };
    match dir {
        DIR_LEFT => {
            px.rect(5 - sway * 2, 11, 2, 7, joint);
            px.rect(5 - sway * 2, 16, 2, 2, shade(hull, 0.9));
        }
        _ => {
            px.rect(1, 10 + sway, 2, 7, joint);
            px.rect(13, 11 - sway, 2, 7, joint);
            px.rect(1, 16 + sway, 2, 2, shade(hull, 0.9));
            px.rect(13, 17 - sway, 2, 2, shade(hull, 0.9));
            if dir == DIR_DOWN {
                // The mark: his left forearm is the viewer's right.
                px.set(13, 15 - sway, [235, 225, 180, 255]);
            }
        }
    }

    // Legs (rows 19..26): hydraulic, flat-footed.
    match dir {
        DIR_LEFT => {
            if frame == 0 {
                px.rect(5, 19, 5, 5, joint);
                px.rect(4, 24, 7, 2, shade(hull, 0.8));
            } else {
                px.rect(3, 19, 4, 5, joint);
                px.rect(2, 24, 5, 2, shade(hull, 0.8));
                px.rect(9, 19, 4, 4, joint);
                px.rect(9, 23, 5, 2, shade(hull, 0.8));
            }
        }
        _ => {
            let lift = if frame == 1 { 1 } else { 0 };
            px.rect(3, 19, 4, 5 - lift, joint);
            px.rect(3, 24 - lift, 4, 2, shade(hull, 0.8));
            px.rect(9, 19 + lift, 4, 5 - lift, joint);
            px.rect(9, 24, 4, 2, shade(hull, 0.8));
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

/// 40×56 parked ship, top-down, nose up — the Loup-Garou on the dock pad.
/// A working tool with weapons bolted on: grey hull, glass canopy, accent
/// stripe across the wings, twin drives aft, and a darker patch where the
/// old registry was scrubbed.
pub fn ship_sprite(accent: Color) -> Image {
    let hull = rgba(Color::srgb(0.36, 0.38, 0.44));
    let a = rgba(accent);
    let glass = rgba(Color::srgb(0.30, 0.50, 0.70));
    let mut px = Px::new(40, 56);
    // Nose taper (rows 0..12).
    for y in 0..12 {
        let half = 3 + y * 7 / 12;
        px.rect(20 - half, y, half * 2, 1, hull);
    }
    // Fuselage.
    px.rect(10, 12, 20, 30, hull);
    px.rect(12, 12, 2, 30, shade(hull, 1.15)); // spine highlight
                                               // Canopy.
    px.rect(16, 8, 8, 7, glass);
    px.rect(17, 9, 2, 3, shade(glass, 1.4)); // glint
                                             // Delta wings (rows 28..42), widening to the canvas edge.
    for y in 28..42 {
        let reach = 4 + (y - 28) * 6 / 13;
        px.rect(10 - reach, y, reach, 1, shade(hull, 0.9));
        px.rect(30, y, reach, 1, shade(hull, 0.9));
    }
    // Accent stripe across wings and fuselage.
    px.rect(2, 36, 36, 3, shade(a, 0.9));
    // Scrubbed registry patch.
    px.rect(13, 20, 7, 5, shade(hull, 0.72));
    // Engine block + nozzles.
    px.rect(8, 42, 24, 10, shade(hull, 0.8));
    for x in [10, 23] {
        px.rect(x, 52, 7, 3, shade(hull, 0.6));
        px.rect(x + 2, 53, 3, 2, [255, 200, 120, 255]); // idle burn
    }
    // Forward mass driver hint.
    px.rect(19, 0, 2, 4, shade(hull, 0.6));
    px.outline();
    px.into_image()
}

/// 26×30 airlock hatch: framed door, porthole to the void, hazard sill.
pub fn hatch_sprite(accent: Color) -> Image {
    let frame = rgba(Color::srgb(0.30, 0.32, 0.38));
    let door = shade(frame, 1.35);
    let a = rgba(accent);
    let mut px = Px::new(26, 30);
    px.rect(0, 0, 26, 30, frame);
    px.rect(2, 2, 22, 24, door);
    // Porthole.
    for y in 0..30 {
        for x in 0..26 {
            let dx = x as f32 - 12.5;
            let dy = y as f32 - 9.5;
            let d = (dx * dx + dy * dy).sqrt();
            if d < 4.5 {
                px.set(x, y, [10, 14, 34, 255]);
            } else if d < 6.0 {
                px.set(x, y, shade(frame, 0.7));
            }
        }
    }
    px.set(11, 8, [220, 220, 240, 255]); // one star through the glass
    px.rect(8, 19, 10, 2, shade(frame, 0.6)); // handle bar
                                              // Hazard sill.
    for x in (0..26).step_by(6) {
        px.rect(x, 26, 3, 3, a);
        px.rect(x + 3, 26, 3, 3, OUTLINE);
    }
    px.outline();
    px.into_image()
}

/// 18×24 pilot seat, seen from behind: headrest, harness straps, armrests.
pub fn seat_sprite(accent: Color) -> Image {
    let pad = rgba(Color::srgb(0.34, 0.28, 0.28));
    let a = rgba(accent);
    let mut px = Px::new(18, 24);
    px.rect(5, 0, 8, 3, shade(pad, 1.2)); // headrest
    px.rect(3, 3, 12, 13, pad); // seat back
    px.rect(5, 4, 2, 11, shade(a, 0.8)); // harness straps
    px.rect(11, 4, 2, 11, shade(a, 0.8));
    px.rect(1, 9, 2, 8, shade(pad, 0.75)); // armrests
    px.rect(15, 9, 2, 8, shade(pad, 0.75));
    px.rect(2, 16, 14, 6, shade(pad, 0.9)); // seat pan
    px.rect(2, 21, 14, 1, shade(pad, 0.6));
    px.outline();
    px.into_image()
}

/// 20×28 deck ladder: rails, rungs, and a hazard-marked floor hatch.
pub fn ladder_sprite(accent: Color) -> Image {
    let rail = rgba(Color::srgb(0.55, 0.58, 0.64));
    let a = rgba(accent);
    let mut px = Px::new(20, 28);
    // Hatch plate behind the rails.
    px.rect(1, 20, 18, 7, shade(rail, 0.45));
    for x in (1..19).step_by(4) {
        px.rect(x, 21, 2, 2, a); // hazard studs
    }
    // Rails.
    px.rect(4, 0, 2, 24, rail);
    px.rect(14, 0, 2, 24, rail);
    px.rect(4, 0, 2, 1, shade(rail, 1.3));
    px.rect(14, 0, 2, 1, shade(rail, 1.3));
    // Rungs.
    for y in (2..24).step_by(4) {
        px.rect(6, y, 8, 2, shade(rail, 1.15));
    }
    px.outline();
    px.into_image()
}

/// 18×30 cryo pod: frosted canopy over a pale sleeper glow, status strip.
/// Ten of these line the cryo chamber — the only way living crew survive a
/// self-generated jump (docs/SHIPS.md §3).
pub fn cryo_pod_sprite(occupied: bool) -> Image {
    let shell = rgba(Color::srgb(0.72, 0.76, 0.82));
    let glass = rgba(Color::srgb(0.55, 0.75, 0.85));
    let mut px = Px::new(18, 30);
    // Rounded shell.
    px.rect(2, 1, 14, 28, shell);
    px.rect(3, 0, 12, 30, shell);
    px.rect(3, 0, 12, 1, shade(shell, 1.2));
    // Canopy window.
    px.rect(5, 3, 8, 16, shade(glass, 0.8));
    px.rect(6, 4, 2, 6, shade(glass, 1.25)); // frost glint
    if occupied {
        px.rect(7, 7, 4, 3, rgba(Color::srgb(0.90, 0.76, 0.62))); // sleeper
        px.rect(7, 10, 4, 7, shade(glass, 0.55));
    }
    // Status strip + base machinery.
    let status = if occupied {
        [120, 220, 160, 255]
    } else {
        [90, 110, 130, 255]
    };
    px.rect(5, 21, 8, 2, status);
    px.rect(4, 24, 10, 4, shade(shell, 0.6));
    px.rect(5, 25, 2, 1, shade(shell, 1.1)); // vents
    px.rect(8, 25, 2, 1, shade(shell, 1.1));
    px.rect(11, 25, 2, 1, shade(shell, 1.1));
    px.outline();
    px.into_image()
}

/// 30×40 support shuttle parked on the tech-bay pad: stubby fuselage, canopy,
/// twin engines. Not jump-capable; it docks before every jump.
pub fn shuttle_sprite(accent: Color) -> Image {
    let hull = rgba(Color::srgb(0.44, 0.46, 0.52));
    let a = rgba(accent);
    let glass = rgba(Color::srgb(0.30, 0.50, 0.70));
    let mut px = Px::new(30, 40);
    // Nose taper.
    for y in 0..8 {
        let half = 3 + y * 5 / 8;
        px.rect(15 - half, y, half * 2, 1, hull);
    }
    // Fuselage.
    px.rect(7, 8, 16, 22, hull);
    px.rect(9, 8, 2, 22, shade(hull, 1.15));
    // Canopy.
    px.rect(11, 6, 8, 6, glass);
    px.set(12, 7, shade(glass, 1.4));
    // Stub wings with accent tips.
    px.rect(1, 20, 6, 8, shade(hull, 0.9));
    px.rect(23, 20, 6, 8, shade(hull, 0.9));
    px.rect(1, 26, 6, 2, a);
    px.rect(23, 26, 6, 2, a);
    // Engines.
    px.rect(9, 30, 5, 7, shade(hull, 0.7));
    px.rect(16, 30, 5, 7, shade(hull, 0.7));
    px.rect(10, 37, 3, 2, [255, 200, 120, 255]);
    px.rect(17, 37, 3, 2, [255, 200, 120, 255]);
    px.outline();
    px.into_image()
}

/// 36×36 interaction ring (hollow circle, drawn over the highlight target).
/// A compartment fire (S09f, docs/SHIPS.md §4): a 14×18 flame painted in
/// code like everything else — dark ember base, orange body, yellow core.
pub fn fire_sprite() -> Image {
    let mut px = Px::new(14, 18);
    for y in 0..18i32 {
        for x in 0..14i32 {
            let cx = (x as f32 - 6.5).abs();
            // Flame silhouette: wide at the base, tapering to a licking tip.
            let h = y as f32 / 18.0;
            let width = 6.0 * (1.0 - h) + 1.0;
            let flicker = ((x * 7 + y * 13) % 5) as f32 * 0.35;
            if cx < width - flicker {
                let color = if h > 0.85 {
                    [255, 246, 160, 235] // tip
                } else if cx < width * 0.45 && h > 0.2 {
                    [255, 214, 90, 240] // core
                } else if h < 0.12 {
                    [120, 40, 20, 220] // ember base
                } else {
                    [235, 120, 30, 235] // body
                };
                // Painted top-down; flames rise, so flip vertically.
                px.set(x, 17 - y, color);
            }
        }
    }
    px.into_image()
}

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
    fn image_bytes_keep_top_row_first() {
        // Regression: a vertical flip here rendered every figure head-down.
        let mut px = Px::new(2, 2);
        px.set(0, 0, [255, 0, 0, 255]); // top-left
        let bytes = px.bytes();
        assert_eq!(&bytes[0..4], &[255, 0, 0, 255]);
        assert!(bytes[4..].iter().all(|&b| b == 0));
    }

    #[test]
    fn crew_looks_paint_and_differ() {
        let ids = ["tib", "tove", "keene", "bardo", "prudence", "risc", "boris"];
        let mut sprites = Vec::new();
        for id in ids {
            let px = paint_character(DIR_DOWN, 0, crew_look(id));
            assert_eq!((px.w, px.h), (16, 26), "{id}");
            assert!(filled(&px) > 100, "{id} barely painted");
            sprites.push(px.data);
        }
        for i in 0..sprites.len() {
            for j in i + 1..sprites.len() {
                assert_ne!(sprites[i], sprites[j], "{} == {}", ids[i], ids[j]);
            }
        }
    }

    #[test]
    fn robot_walks_and_mirrors_like_everyone_else() {
        let look = crew_look("boris");
        let idle = paint_character(DIR_DOWN, 0, look);
        let stride = paint_character(DIR_DOWN, 1, look);
        assert_ne!(idle.data, stride.data);
        let left = paint_character(DIR_LEFT, 0, look);
        let right = paint_character(DIR_RIGHT, 0, look);
        assert_eq!(left.mirrored().data, right.data);
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
