//! Name and description generation (spec §16): `{adjective} {material}
//! {base}` templates, e.g. "Scorched Ferrite Autocannon", "Cryo-Treated
//! Titanium Plating", "Bleached Bone Neural Lace". Word tables are Rust
//! const arrays — no content files. Authored item content arrives via S01's
//! pipeline later; this generator must stay self-contained (index gotcha).

use super::types::{
    BoardingWeapon, ComponentKind, ConsumableKind, CosmeticKind, EnergyWeapon, EquipmentKind,
    ImplantKind, ItemSeed, ItemType, KineticWeapon, MeleeWeapon, MissileWeapon, WeaponKind,
};
use crate::util::rng::SeededRng;

const ADJECTIVES: &[&str] = &[
    "Scorched",
    "Gleaming",
    "Battered",
    "Pristine",
    "Corroded",
    "Volatile",
    "Silent",
    "Jagged",
    "Hollow",
    "Weathered",
    "Reinforced",
    "Cracked",
    "Polished",
    "Ashen",
    "Feral",
    "Dormant",
    "Blazing",
    "Frost-Bitten",
    "Ancient",
    "Cryo-Treated",
    "Bleached",
    "Salvaged",
    "Overcharged",
    "Tempered",
];

const MATERIALS: &[&str] = &[
    "Ferrite",
    "Titanium",
    "Carbon",
    "Ceramic",
    "Tungsten",
    "Graphene",
    "Chromium",
    "Obsidian",
    "Bone",
    "Copper",
    "Silicate",
    "Polymer",
    "Adamant",
    "Nickel",
    "Cobalt",
    "Basalt",
];

/// Base noun per leaf `ItemType` — the fixed, meaningful part of the name
/// template; only the adjective and material roll from the seed.
fn base_noun(item_type: ItemType) -> &'static str {
    match item_type {
        ItemType::Equipment(EquipmentKind::Weapon(w)) => weapon_noun(w),
        ItemType::Equipment(EquipmentKind::Armor) => "Plating",
        ItemType::Equipment(EquipmentKind::Shield) => "Shield Generator",
        ItemType::Equipment(EquipmentKind::Engine) => "Thruster",
        ItemType::Equipment(EquipmentKind::Sensor) => "Sensor Array",
        ItemType::Equipment(EquipmentKind::MiningTool) => "Mining Drill",
        ItemType::Equipment(EquipmentKind::RepairTool) => "Repair Rig",
        ItemType::Equipment(EquipmentKind::Cybernetic) => "Cybernetic Implant",
        ItemType::Equipment(EquipmentKind::Augmentation) => "Augmentation Module",
        ItemType::Equipment(EquipmentKind::Spacesuit) => "Spacesuit",
        ItemType::Consumable(c) => consumable_noun(c),
        ItemType::Component(c) => component_noun(c),
        ItemType::Implant(i) => implant_noun(i),
        ItemType::Cosmetic(c) => cosmetic_noun(c),
    }
}

fn weapon_noun(w: WeaponKind) -> &'static str {
    match w {
        WeaponKind::Energy(EnergyWeapon::Laser) => "Laser",
        WeaponKind::Energy(EnergyWeapon::Plasma) => "Plasma Emitter",
        WeaponKind::Energy(EnergyWeapon::Tachyon) => "Tachyon Lance",
        WeaponKind::Kinetic(KineticWeapon::Cannon) => "Cannon",
        WeaponKind::Kinetic(KineticWeapon::Railgun) => "Railgun",
        WeaponKind::Kinetic(KineticWeapon::Autocannon) => "Autocannon",
        WeaponKind::Missile(MissileWeapon::Torpedo) => "Torpedo Launcher",
        WeaponKind::Missile(MissileWeapon::Standard) => "Missile Rack",
        WeaponKind::Missile(MissileWeapon::Decoy) => "Decoy Pod",
        WeaponKind::Melee(MeleeWeapon::Blade) => "Blade",
        WeaponKind::Melee(MeleeWeapon::Baton) => "Shock Baton",
        WeaponKind::Melee(MeleeWeapon::ArcWelder) => "Arc-Welder",
        WeaponKind::Boarding(BoardingWeapon::BreachingCharge) => "Breaching Charge",
        WeaponKind::Boarding(BoardingWeapon::SuppressionTool) => "Suppression Tool",
    }
}

fn consumable_noun(c: ConsumableKind) -> &'static str {
    match c {
        ConsumableKind::Medkit => "Medkit",
        ConsumableKind::RepairPack => "Repair Pack",
        ConsumableKind::Ammunition => "Ammunition Crate",
        ConsumableKind::FuelCell => "Fuel Cell",
        ConsumableKind::BatteryPack => "Battery Pack",
        ConsumableKind::Booster => "Booster Shot",
        ConsumableKind::Grenade => "Grenade",
        ConsumableKind::Mine => "Mine",
        ConsumableKind::DeployableCover => "Deployable Cover",
        ConsumableKind::DataShard => "Data Shard",
    }
}

fn component_noun(c: ComponentKind) -> &'static str {
    match c {
        ComponentKind::Hardpoint => "Hardpoint Mount",
        ComponentKind::HullPlating => "Hull Plating",
        ComponentKind::ArmorSegment => "Armor Segment",
        ComponentKind::PowerPlant => "Power Plant",
        ComponentKind::Capacitor => "Capacitor",
        ComponentKind::JumpDriveComponent => "Jump Drive Component",
        ComponentKind::CraftingMaterial => "Crafting Material",
        ComponentKind::RefinedOre => "Refined Ore",
    }
}

fn implant_noun(i: ImplantKind) -> &'static str {
    match i {
        ImplantKind::NeuralLace => "Neural Lace",
        ImplantKind::DroidInterface => "Droid Interface",
        ImplantKind::MemoryUpgrade => "Memory Upgrade",
        ImplantKind::FactionSpecific => "Loyalty Chip",
    }
}

fn cosmetic_noun(c: CosmeticKind) -> &'static str {
    match c {
        CosmeticKind::Costume => "Costume",
        CosmeticKind::Hat => "Hat",
        CosmeticKind::ShipPaint => "Ship Paint",
        CosmeticKind::Decal => "Decal",
        CosmeticKind::CrewOutfit => "Crew Outfit",
        CosmeticKind::PortraitFrame => "Portrait Frame",
        CosmeticKind::InteriorDecoration => "Interior Decoration",
    }
}

const DESCRIPTION_TEMPLATES: &[&str] = &[
    "A {rarity} {base}, tier {tier} make. Faction markings suggest {faction} origin.",
    "Recovered from the {biome} reaches; {rarity} condition for its age.",
    "{faction} engineers rate this {base} {rarity} grade — tier {tier} spec.",
    "Bears the wear of the {biome} frontier. {rarity} tier-{tier} craftsmanship.",
    "A tier-{tier} {base}, {rarity} by any {faction} appraiser's standard.",
];

fn render_template(template: &str, base: &str, rarity: &str, tier: u8, faction: &str, biome: &str) -> String {
    template
        .replace("{base}", base)
        .replace("{rarity}", rarity)
        .replace("{tier}", &tier.to_string())
        .replace("{faction}", faction)
        .replace("{biome}", biome)
}

/// Generate `(display_name, description)` for an item. Draws from the same
/// `rng` stream `generate_item` uses for stats/rarity — call this AFTER
/// those rolls if you want name text to reflect the rolled rarity (see
/// `mod.rs`, which passes the already-rolled rarity in).
pub fn generate_naming(
    rng: &mut SeededRng,
    item_seed: &ItemSeed,
    rarity: super::types::Rarity,
) -> (String, String) {
    let adjective = ADJECTIVES[rng.next_below(ADJECTIVES.len() as u64) as usize];
    let material = MATERIALS[rng.next_below(MATERIALS.len() as u64) as usize];
    let base = base_noun(item_seed.item_type);

    let display_name = format!("{adjective} {material} {base}");

    let template = DESCRIPTION_TEMPLATES[rng.next_below(DESCRIPTION_TEMPLATES.len() as u64) as usize];
    let rarity_str = format!("{rarity:?}").to_lowercase();
    let description = render_template(
        template,
        base,
        &rarity_str,
        item_seed.tier,
        &item_seed.faction,
        &item_seed.biome,
    );

    (display_name, description)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::{EquipmentKind, ItemFamily, Rarity};

    fn seed(item_type: ItemType) -> ItemSeed {
        ItemSeed {
            seed: 5,
            item_type,
            tier: 3,
            faction: "Compact".into(),
            biome: "frontier".into(),
        }
    }

    #[test]
    fn deterministic() {
        let s = seed(ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Kinetic(
            KineticWeapon::Autocannon,
        ))));
        let mut a = SeededRng::new(1);
        let mut b = SeededRng::new(1);
        assert_eq!(
            generate_naming(&mut a, &s, Rarity::Rare),
            generate_naming(&mut b, &s, Rarity::Rare)
        );
    }

    #[test]
    fn every_leaf_type_names_non_empty() {
        for item_type in ItemType::all() {
            let s = seed(item_type);
            let mut rng = SeededRng::new(11);
            let (name, desc) = generate_naming(&mut rng, &s, Rarity::Common);
            assert!(!name.is_empty());
            assert!(!desc.is_empty());
            assert!(!name.contains("{{"), "unrendered template in name: {name}");
            assert!(!desc.contains('{'), "unrendered placeholder in description: {desc}");
        }
    }

    #[test]
    fn different_seeds_vary_the_name() {
        let s = seed(ItemType::Implant(ImplantKind::NeuralLace));
        let mut a = SeededRng::new(1);
        let mut b = SeededRng::new(2);
        let (name_a, _) = generate_naming(&mut a, &s, Rarity::Common);
        let (name_b, _) = generate_naming(&mut b, &s, Rarity::Common);
        assert_ne!(name_a, name_b);
    }

    #[test]
    fn spec_example_shape() {
        // "Scorched Ferrite Autocannon" — three words, title-cased.
        let s = seed(ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Kinetic(
            KineticWeapon::Autocannon,
        ))));
        let mut rng = SeededRng::new(0);
        let (name, _) = generate_naming(&mut rng, &s, Rarity::Common);
        assert!(name.ends_with("Autocannon"));
        assert_eq!(name.split(' ').count(), 3);
    }

    #[test]
    fn families_have_reasonable_nouns() {
        for family in ItemFamily::ALL {
            let noun = base_noun(family.representative_item_type());
            assert!(!noun.is_empty());
        }
    }
}
