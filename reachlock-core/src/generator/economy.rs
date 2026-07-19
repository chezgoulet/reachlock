//! Economy catalog generator (S25): seed -> list of tradeable goods.

use crate::util::SeededRng;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EconomyGood {
    pub name: String,
    pub category: String,
    pub base_price: u32,
}

fn pick<'a>(rng: &mut SeededRng, table: &'a [&str]) -> &'a str {
    table[rng.next_below(table.len() as u64) as usize]
}

const NAMES: &[&str] = &[
    "Food Rations", "Water Canisters", "Oxygen Tanks", "Medical Kits",
    "Fuel Cells", "Plasma Coils", "Conduit Wire", "Circuit Boards",
    "Structural Alloy", "Reinforced Glass", "Ceramic Plate", "Nano Paste",
    "Optical Lens", "Sensor Array", "Coolant Fluid", "Lubricant Gel",
    "Auto-Suture Kit", "Broad-Spectrum Antibiotic", "Nerve Staple",
    "Starship Battery", "Fusion Core", "Solar Panel", "Thruster Nozzle",
    "Cargo Container", "Magnetic Clamp", "Tractor Beam Emitter",
    "Data Crystal", "Encryption Module", "Comm Relay", "Navigation Chart",
    "Weapon Capacitor", "Shield Emitter", "Targeting Matrix", "Ammo Box",
    "Chitin Plate", "Xeno Silk", "Alien Seed Pod", "Bio-Luminescent Ink",
    "Artifact Fragment", "Ancient Cog", "Precursor Key", "Glyph Tablet",
];

const CATEGORIES: &[&str] = &[
    "consumable", "component", "medical", "fuel", "electronic",
    "structural", "ship_part", "cargo", "weapon", "shield",
    "data", "organic", "artifact", "xenotech",
];

pub fn generate_economy_catalog(seed: u64) -> Vec<EconomyGood> {
    let mut rng = SeededRng::new(seed);
    let count = 12 + rng.next_below(12) as usize;

    let mut goods = Vec::with_capacity(count);
    for _ in 0..count {
        let name = pick(&mut rng, NAMES).to_string();
        let category = pick(&mut rng, CATEGORIES).to_string();
        let base_price = 10 + rng.next_below(9991) as u32;
        goods.push(EconomyGood { name, category, base_price });
    }
    goods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_economy_catalog(42);
        let b = generate_economy_catalog(42);
        assert_eq!(a, b);
    }

    #[test]
    fn seeds_differ() {
        let a = generate_economy_catalog(1);
        let b = generate_economy_catalog(2);
        assert_ne!(a, b);
    }

    #[test]
    fn catalog_size_in_range() {
        let c = generate_economy_catalog(99);
        assert!((12..=23).contains(&c.len()));
    }

    #[test]
    fn prices_positive() {
        let c = generate_economy_catalog(7);
        for good in &c {
            assert!(good.base_price >= 10);
        }
    }
}
