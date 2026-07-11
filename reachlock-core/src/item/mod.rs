//! Item generation (spec §16): `(seed, item_type, tier, faction, biome)` →
//! a named, stat-banded, procedurally-iconed item. Pure functions, integer
//! math, plain-data output — like every generator in this crate.
//!
//! Split: `types` (the `ItemType` tree + `GeneratedItem`/`ItemSeed`
//! contracts), `stats` (tier stat bands + rolls), `names` (name/description
//! templates). This module composes them and adds the icon.

pub mod names;
pub mod stats;
pub mod types;

pub use types::{
    EquipmentKind, GeneratedItem, ItemFamily, ItemSeed, ItemStats, ItemType, Rarity, StatKey,
};

use crate::generator::GeneratedTexture;
use crate::util::color::{generate_palette, ColorRgba8};
use crate::util::noise::value_noise;
use crate::util::rng::SeededRng;

/// Generate a complete item from its seed parameters. Single deterministic
/// RNG stream: stats → rarity → name/description, so the same `ItemSeed`
/// always yields the same item everywhere (the determinism harness pins it).
pub fn generate_item(item_seed: &ItemSeed) -> GeneratedItem {
    let family = item_seed.item_type.family();
    let mut rng = SeededRng::new(item_seed.seed);

    let stats = stats::roll_stats(&mut rng, family, item_seed.tier);
    let rarity = stats::roll_rarity(&mut rng, item_seed.tier);
    let (display_name, description) = names::generate_naming(&mut rng, item_seed, rarity);
    let icon = generate_icon(item_seed.seed, family);

    GeneratedItem {
        id: types::item_id(item_seed),
        seed: item_seed.seed,
        display_name,
        description,
        icon,
        stats,
        rarity,
    }
}

const ICON_SIZE: u32 = 24;

/// Procedural icon (spec §16): a small RGBA sprite whose palette comes from
/// the item's seed and whose motif comes from its family. Energy families
/// read as glowing cores, kinetic as angular blocks, shields as concentric
/// rings — a coarse-but-deterministic silhouette, not art.
fn generate_icon(seed: u64, family: ItemFamily) -> GeneratedTexture {
    // Family perturbs the palette so families read differently at a glance.
    let palette = generate_palette(seed ^ (family as u64).wrapping_mul(0x9E37_79B9));
    let shades = [palette.structure, palette.primary, palette.accent];
    let motif = IconMotif::of(family);

    let center = ICON_SIZE as i32 / 2;
    let mut pixels = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE as i32 {
        for x in 0..ICON_SIZE as i32 {
            let dx = x - center;
            let dy = y - center;
            // Chebyshev + Euclidean-ish radius in integers.
            let r2 = dx * dx + dy * dy;
            let noise = value_noise(seed, x as i64 * 40, y as i64 * 40); // [-32768,32768]

            let shade = match motif {
                IconMotif::Core => {
                    // Bright center falling off to structure at the rim.
                    if r2 < 16 {
                        2
                    } else if r2 < 64 && noise > -8000 {
                        1
                    } else {
                        0
                    }
                }
                IconMotif::Angular => {
                    // Blocky diagonal barrel.
                    if (dx + dy).abs() < 5 {
                        2
                    } else if dx.abs().max(dy.abs()) < 9 {
                        1
                    } else {
                        0
                    }
                }
                IconMotif::Rings => {
                    // Concentric hex-ish rings by radius bands.
                    match r2 {
                        _ if r2 < 9 => 2,
                        _ if r2 < 36 => 0,
                        _ if r2 < 81 => 1,
                        _ => 0,
                    }
                }
                IconMotif::Vial => {
                    // A centered capsule, noise-flecked.
                    if dx.abs() < 4 && dy.abs() < 8 {
                        if noise > 0 {
                            2
                        } else {
                            1
                        }
                    } else {
                        0
                    }
                }
                IconMotif::Circuitry => {
                    // Orthogonal traces.
                    if dx.abs() < 2 || dy.abs() < 2 || (noise.abs() < 3000) {
                        1
                    } else if r2 < 20 {
                        2
                    } else {
                        0
                    }
                }
            };

            let c: ColorRgba8 = shades[shade];
            // Border ring for readability at small sizes.
            let on_border =
                x == 0 || y == 0 || x == ICON_SIZE as i32 - 1 || y == ICON_SIZE as i32 - 1;
            if on_border {
                pixels.extend_from_slice(&[
                    palette.structure.r / 2,
                    palette.structure.g / 2,
                    palette.structure.b / 2,
                    255,
                ]);
            } else {
                pixels.extend_from_slice(&[c.r, c.g, c.b, 255]);
            }
        }
    }

    GeneratedTexture {
        width: ICON_SIZE,
        height: ICON_SIZE,
        pixels,
    }
}

/// The icon silhouette a family renders with.
#[derive(Clone, Copy)]
enum IconMotif {
    Core,
    Angular,
    Rings,
    Vial,
    Circuitry,
}

impl IconMotif {
    fn of(family: ItemFamily) -> Self {
        use ItemFamily::*;
        match family {
            EnergyWeapon | MissileWeapon => IconMotif::Core,
            KineticWeapon | MeleeWeapon | BoardingWeapon => IconMotif::Angular,
            Shield | Armor => IconMotif::Rings,
            Consumable => IconMotif::Vial,
            Implant | Cybernetic | Sensor => IconMotif::Circuitry,
            // Engines, tools, components, cosmetics: reuse the closest read.
            Engine | MiningTool | RepairTool => IconMotif::Angular,
            _ => IconMotif::Core,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(item_type: ItemType, tier: u8) -> ItemSeed {
        ItemSeed {
            seed: 0x1234_5678,
            item_type,
            tier,
            faction: "compact".into(),
            biome: "frontier".into(),
        }
    }

    fn a_kinetic() -> ItemType {
        // Any concrete kinetic weapon variant; exact variant doesn't matter
        // for these structural checks.
        ItemType::all()
            .into_iter()
            .find(|t| t.family() == ItemFamily::KineticWeapon)
            .expect("a kinetic weapon type exists")
    }

    #[test]
    fn deterministic() {
        let s = seed(a_kinetic(), 4);
        assert_eq!(generate_item(&s), generate_item(&s));
    }

    #[test]
    fn different_seeds_differ() {
        let mut s = seed(a_kinetic(), 4);
        let a = generate_item(&s);
        s.seed = 0x8765_4321;
        assert_ne!(a, generate_item(&s));
    }

    #[test]
    fn icon_is_full_rgba() {
        let item = generate_item(&seed(a_kinetic(), 4));
        assert_eq!(item.icon.pixels.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    #[test]
    fn id_is_stable_and_prefixed() {
        let item = generate_item(&seed(a_kinetic(), 4));
        assert!(item.id.starts_with("item-"));
    }

    #[test]
    fn every_family_generates() {
        // Smoke: one representative item per family generates without panic
        // and produces a full icon.
        for family in ItemFamily::ALL {
            let it = family.representative_item_type();
            let item = generate_item(&seed(it, 5));
            assert_eq!(item.icon.width, ICON_SIZE);
            assert!(!item.display_name.is_empty());
        }
    }
}
