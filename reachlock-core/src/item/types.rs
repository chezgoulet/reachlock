//! Item data model (spec §16): the frozen contract. `ItemType` mirrors the
//! spec's hierarchy (Equipment/Consumable/Component/Implant/Cosmetic, each
//! with its subtypes); `ItemStats` is a fixed-point `BTreeMap<StatKey, i64>`
//! with string-stable keys (pinned in `protocol.rs`); `GeneratedItem` is the
//! output of `generate_item`. Downstream sprints (S17 exterior editor, S19
//! combat, S10 economy) hold onto the `ItemSeed` alongside a `GeneratedItem`
//! when they need to know the type back — see S17's `ItemRef`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::generator::GeneratedTexture;

// ---------------------------------------------------------------------
// ItemType hierarchy (spec §16, "Item Type Hierarchy")
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Equipment(EquipmentKind),
    Consumable(ConsumableKind),
    Component(ComponentKind),
    Implant(ImplantKind),
    Cosmetic(CosmeticKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentKind {
    Weapon(WeaponKind),
    Armor,
    Shield,
    Engine,
    Sensor,
    MiningTool,
    RepairTool,
    Cybernetic,
    Augmentation,
    Spacesuit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponKind {
    Energy(EnergyWeapon),
    Kinetic(KineticWeapon),
    Missile(MissileWeapon),
    Melee(MeleeWeapon),
    Boarding(BoardingWeapon),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyWeapon {
    Laser,
    Plasma,
    Tachyon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KineticWeapon {
    Cannon,
    Railgun,
    Autocannon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissileWeapon {
    Torpedo,
    /// The spec's unqualified "missile" leaf — renamed to avoid a
    /// `Missile::Missile` stutter.
    Standard,
    Decoy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeWeapon {
    Blade,
    Baton,
    ArcWelder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardingWeapon {
    BreachingCharge,
    SuppressionTool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumableKind {
    Medkit,
    RepairPack,
    Ammunition,
    FuelCell,
    BatteryPack,
    Booster,
    Grenade,
    Mine,
    DeployableCover,
    DataShard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    Hardpoint,
    HullPlating,
    ArmorSegment,
    PowerPlant,
    Capacitor,
    JumpDriveComponent,
    CraftingMaterial,
    RefinedOre,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplantKind {
    NeuralLace,
    DroidInterface,
    MemoryUpgrade,
    FactionSpecific,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CosmeticKind {
    Costume,
    Hat,
    ShipPaint,
    Decal,
    CrewOutfit,
    PortraitFrame,
    InteriorDecoration,
}

// ---------------------------------------------------------------------
// CLI/manifest tokens: flat `family_subtype` strings, e.g. "energy_laser",
// "kinetic_cannon" — independent of the serde wire shape above.
// ---------------------------------------------------------------------

macro_rules! token_enum {
    ($ty:ty { $($variant:ident => $token:literal),+ $(,)? }) => {
        impl $ty {
            pub const ALL: &'static [$ty] = &[$(<$ty>::$variant),+];

            pub fn token(self) -> &'static str {
                match self {
                    $(<$ty>::$variant => $token),+
                }
            }

            pub fn from_token(s: &str) -> Option<Self> {
                match s {
                    $($token => Some(<$ty>::$variant)),+,
                    _ => None,
                }
            }
        }
    };
}

token_enum!(EnergyWeapon {
    Laser => "laser",
    Plasma => "plasma",
    Tachyon => "tachyon",
});

token_enum!(KineticWeapon {
    Cannon => "cannon",
    Railgun => "railgun",
    Autocannon => "autocannon",
});

token_enum!(MissileWeapon {
    Torpedo => "torpedo",
    Standard => "standard",
    Decoy => "decoy",
});

token_enum!(MeleeWeapon {
    Blade => "blade",
    Baton => "baton",
    ArcWelder => "arc_welder",
});

token_enum!(BoardingWeapon {
    BreachingCharge => "breaching_charge",
    SuppressionTool => "suppression_tool",
});

token_enum!(ConsumableKind {
    Medkit => "medkit",
    RepairPack => "repair_pack",
    Ammunition => "ammunition",
    FuelCell => "fuel_cell",
    BatteryPack => "battery_pack",
    Booster => "booster",
    Grenade => "grenade",
    Mine => "mine",
    DeployableCover => "deployable_cover",
    DataShard => "data_shard",
});

token_enum!(ComponentKind {
    Hardpoint => "hardpoint",
    HullPlating => "hull_plating",
    ArmorSegment => "armor_segment",
    PowerPlant => "power_plant",
    Capacitor => "capacitor",
    JumpDriveComponent => "jump_drive_component",
    CraftingMaterial => "crafting_material",
    RefinedOre => "refined_ore",
});

token_enum!(ImplantKind {
    NeuralLace => "neural_lace",
    DroidInterface => "droid_interface",
    MemoryUpgrade => "memory_upgrade",
    FactionSpecific => "faction_specific",
});

token_enum!(CosmeticKind {
    Costume => "costume",
    Hat => "hat",
    ShipPaint => "ship_paint",
    Decal => "decal",
    CrewOutfit => "crew_outfit",
    PortraitFrame => "portrait_frame",
    InteriorDecoration => "interior_decoration",
});

impl WeaponKind {
    pub fn token(self) -> String {
        match self {
            WeaponKind::Energy(w) => format!("energy_{}", w.token()),
            WeaponKind::Kinetic(w) => format!("kinetic_{}", w.token()),
            WeaponKind::Missile(w) => format!("missile_{}", w.token()),
            WeaponKind::Melee(w) => format!("melee_{}", w.token()),
            WeaponKind::Boarding(w) => format!("boarding_{}", w.token()),
        }
    }

    pub fn from_token(s: &str) -> Option<Self> {
        let (prefix, rest) = s.split_once('_')?;
        match prefix {
            "energy" => EnergyWeapon::from_token(rest).map(WeaponKind::Energy),
            "kinetic" => KineticWeapon::from_token(rest).map(WeaponKind::Kinetic),
            "missile" => MissileWeapon::from_token(rest).map(WeaponKind::Missile),
            "melee" => MeleeWeapon::from_token(rest).map(WeaponKind::Melee),
            "boarding" => BoardingWeapon::from_token(rest).map(WeaponKind::Boarding),
            _ => None,
        }
    }

    pub const ALL: &'static [WeaponKind] = &[
        WeaponKind::Energy(EnergyWeapon::Laser),
        WeaponKind::Energy(EnergyWeapon::Plasma),
        WeaponKind::Energy(EnergyWeapon::Tachyon),
        WeaponKind::Kinetic(KineticWeapon::Cannon),
        WeaponKind::Kinetic(KineticWeapon::Railgun),
        WeaponKind::Kinetic(KineticWeapon::Autocannon),
        WeaponKind::Missile(MissileWeapon::Torpedo),
        WeaponKind::Missile(MissileWeapon::Standard),
        WeaponKind::Missile(MissileWeapon::Decoy),
        WeaponKind::Melee(MeleeWeapon::Blade),
        WeaponKind::Melee(MeleeWeapon::Baton),
        WeaponKind::Melee(MeleeWeapon::ArcWelder),
        WeaponKind::Boarding(BoardingWeapon::BreachingCharge),
        WeaponKind::Boarding(BoardingWeapon::SuppressionTool),
    ];
}

impl EquipmentKind {
    pub fn token(self) -> String {
        match self {
            EquipmentKind::Weapon(w) => w.token(),
            EquipmentKind::Armor => "armor".to_string(),
            EquipmentKind::Shield => "shield".to_string(),
            EquipmentKind::Engine => "engine".to_string(),
            EquipmentKind::Sensor => "sensor".to_string(),
            EquipmentKind::MiningTool => "mining_tool".to_string(),
            EquipmentKind::RepairTool => "repair_tool".to_string(),
            EquipmentKind::Cybernetic => "cybernetic".to_string(),
            EquipmentKind::Augmentation => "augmentation".to_string(),
            EquipmentKind::Spacesuit => "spacesuit".to_string(),
        }
    }

    pub fn from_token(s: &str) -> Option<Self> {
        match s {
            "armor" => Some(EquipmentKind::Armor),
            "shield" => Some(EquipmentKind::Shield),
            "engine" => Some(EquipmentKind::Engine),
            "sensor" => Some(EquipmentKind::Sensor),
            "mining_tool" => Some(EquipmentKind::MiningTool),
            "repair_tool" => Some(EquipmentKind::RepairTool),
            "cybernetic" => Some(EquipmentKind::Cybernetic),
            "augmentation" => Some(EquipmentKind::Augmentation),
            "spacesuit" => Some(EquipmentKind::Spacesuit),
            _ => WeaponKind::from_token(s).map(EquipmentKind::Weapon),
        }
    }

    pub fn all() -> Vec<EquipmentKind> {
        let mut v: Vec<EquipmentKind> = WeaponKind::ALL.iter().copied().map(EquipmentKind::Weapon).collect();
        v.extend([
            EquipmentKind::Armor,
            EquipmentKind::Shield,
            EquipmentKind::Engine,
            EquipmentKind::Sensor,
            EquipmentKind::MiningTool,
            EquipmentKind::RepairTool,
            EquipmentKind::Cybernetic,
            EquipmentKind::Augmentation,
            EquipmentKind::Spacesuit,
        ]);
        v
    }
}

impl ItemType {
    pub fn token(self) -> String {
        match self {
            ItemType::Equipment(k) => k.token(),
            ItemType::Consumable(k) => k.token().to_string(),
            ItemType::Component(k) => k.token().to_string(),
            ItemType::Implant(k) => k.token().to_string(),
            ItemType::Cosmetic(k) => k.token().to_string(),
        }
    }

    pub fn from_token(s: &str) -> Option<Self> {
        EquipmentKind::from_token(s)
            .map(ItemType::Equipment)
            .or_else(|| ConsumableKind::from_token(s).map(ItemType::Consumable))
            .or_else(|| ComponentKind::from_token(s).map(ItemType::Component))
            .or_else(|| ImplantKind::from_token(s).map(ItemType::Implant))
            .or_else(|| CosmeticKind::from_token(s).map(ItemType::Cosmetic))
    }

    /// Every leaf item type — used by exhaustive tests and CLI help.
    pub fn all() -> Vec<ItemType> {
        let mut v: Vec<ItemType> = EquipmentKind::all().into_iter().map(ItemType::Equipment).collect();
        v.extend(ConsumableKind::ALL.iter().copied().map(ItemType::Consumable));
        v.extend(ComponentKind::ALL.iter().copied().map(ItemType::Component));
        v.extend(ImplantKind::ALL.iter().copied().map(ItemType::Implant));
        v.extend(CosmeticKind::ALL.iter().copied().map(ItemType::Cosmetic));
        v
    }

    pub fn family(self) -> ItemFamily {
        match self {
            ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Energy(_))) => ItemFamily::EnergyWeapon,
            ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Kinetic(_))) => ItemFamily::KineticWeapon,
            ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Missile(_))) => ItemFamily::MissileWeapon,
            ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Melee(_))) => ItemFamily::MeleeWeapon,
            ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Boarding(_))) => ItemFamily::BoardingWeapon,
            ItemType::Equipment(EquipmentKind::Armor) => ItemFamily::Armor,
            ItemType::Equipment(EquipmentKind::Shield) => ItemFamily::Shield,
            ItemType::Equipment(EquipmentKind::Engine) => ItemFamily::Engine,
            ItemType::Equipment(EquipmentKind::Sensor) => ItemFamily::Sensor,
            ItemType::Equipment(EquipmentKind::MiningTool) => ItemFamily::MiningTool,
            ItemType::Equipment(EquipmentKind::RepairTool) => ItemFamily::RepairTool,
            ItemType::Equipment(EquipmentKind::Cybernetic) => ItemFamily::Cybernetic,
            ItemType::Equipment(EquipmentKind::Augmentation) => ItemFamily::Augmentation,
            ItemType::Equipment(EquipmentKind::Spacesuit) => ItemFamily::Spacesuit,
            ItemType::Consumable(_) => ItemFamily::Consumable,
            ItemType::Component(_) => ItemFamily::Component,
            ItemType::Implant(_) => ItemFamily::Implant,
            ItemType::Cosmetic(_) => ItemFamily::Cosmetic,
        }
    }
}

// ---------------------------------------------------------------------
// ItemFamily: the grouping stat bands and icon formulas key off. Coarser
// than `ItemType` — every leaf type belongs to exactly one family.
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemFamily {
    EnergyWeapon,
    KineticWeapon,
    MissileWeapon,
    MeleeWeapon,
    BoardingWeapon,
    Armor,
    Shield,
    Engine,
    Sensor,
    MiningTool,
    RepairTool,
    Cybernetic,
    Augmentation,
    Spacesuit,
    Consumable,
    Component,
    Implant,
    Cosmetic,
}

impl ItemFamily {
    pub const ALL: [ItemFamily; 18] = [
        ItemFamily::EnergyWeapon,
        ItemFamily::KineticWeapon,
        ItemFamily::MissileWeapon,
        ItemFamily::MeleeWeapon,
        ItemFamily::BoardingWeapon,
        ItemFamily::Armor,
        ItemFamily::Shield,
        ItemFamily::Engine,
        ItemFamily::Sensor,
        ItemFamily::MiningTool,
        ItemFamily::RepairTool,
        ItemFamily::Cybernetic,
        ItemFamily::Augmentation,
        ItemFamily::Spacesuit,
        ItemFamily::Consumable,
        ItemFamily::Component,
        ItemFamily::Implant,
        ItemFamily::Cosmetic,
    ];

    pub fn token(self) -> &'static str {
        match self {
            ItemFamily::EnergyWeapon => "energy_weapon",
            ItemFamily::KineticWeapon => "kinetic_weapon",
            ItemFamily::MissileWeapon => "missile_weapon",
            ItemFamily::MeleeWeapon => "melee_weapon",
            ItemFamily::BoardingWeapon => "boarding_weapon",
            ItemFamily::Armor => "armor",
            ItemFamily::Shield => "shield",
            ItemFamily::Engine => "engine",
            ItemFamily::Sensor => "sensor",
            ItemFamily::MiningTool => "mining_tool",
            ItemFamily::RepairTool => "repair_tool",
            ItemFamily::Cybernetic => "cybernetic",
            ItemFamily::Augmentation => "augmentation",
            ItemFamily::Spacesuit => "spacesuit",
            ItemFamily::Consumable => "consumable",
            ItemFamily::Component => "component",
            ItemFamily::Implant => "implant",
            ItemFamily::Cosmetic => "cosmetic",
        }
    }

    /// One representative leaf `ItemType` per family — used by the
    /// determinism manifest and by tests that need "an item of this
    /// family" without enumerating every leaf.
    pub fn representative_item_type(self) -> ItemType {
        match self {
            ItemFamily::EnergyWeapon => {
                ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Energy(EnergyWeapon::Laser)))
            }
            ItemFamily::KineticWeapon => {
                ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Kinetic(KineticWeapon::Cannon)))
            }
            ItemFamily::MissileWeapon => {
                ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Missile(MissileWeapon::Torpedo)))
            }
            ItemFamily::MeleeWeapon => {
                ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Melee(MeleeWeapon::Blade)))
            }
            ItemFamily::BoardingWeapon => ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Boarding(
                BoardingWeapon::BreachingCharge,
            ))),
            ItemFamily::Armor => ItemType::Equipment(EquipmentKind::Armor),
            ItemFamily::Shield => ItemType::Equipment(EquipmentKind::Shield),
            ItemFamily::Engine => ItemType::Equipment(EquipmentKind::Engine),
            ItemFamily::Sensor => ItemType::Equipment(EquipmentKind::Sensor),
            ItemFamily::MiningTool => ItemType::Equipment(EquipmentKind::MiningTool),
            ItemFamily::RepairTool => ItemType::Equipment(EquipmentKind::RepairTool),
            ItemFamily::Cybernetic => ItemType::Equipment(EquipmentKind::Cybernetic),
            ItemFamily::Augmentation => ItemType::Equipment(EquipmentKind::Augmentation),
            ItemFamily::Spacesuit => ItemType::Equipment(EquipmentKind::Spacesuit),
            ItemFamily::Consumable => ItemType::Consumable(ConsumableKind::Medkit),
            ItemFamily::Component => ItemType::Component(ComponentKind::Hardpoint),
            ItemFamily::Implant => ItemType::Implant(ImplantKind::NeuralLace),
            ItemFamily::Cosmetic => ItemType::Cosmetic(CosmeticKind::Decal),
        }
    }
}

// ---------------------------------------------------------------------
// Stats, rarity, seed, and the generated item itself.
// ---------------------------------------------------------------------

/// Stat keys are string-stable: renaming or removing one is a wire/save
/// format break (pinned in `protocol.rs`). Add new keys freely; they land
/// at the end of the `BTreeMap` iteration order harmlessly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatKey {
    Damage,
    Range,
    FireRate,
    ShieldHp,
    Recharge,
    Thrust,
    Turn,
    SensorRange,
    MiningRate,
    RepairRate,
    /// Fixed-point (1 unit = 1/1024), like every other stat here — the spec
    /// writes `weight: f32` on `GeneratedItem` directly, but the codebase's
    /// no-floats-in-gameplay-values rule (index, rule 2) wins: weight lives
    /// in the stat map instead.
    Weight,
}

/// Fixed-point item stats (1 unit = 1/1024, matching `util::rng::Fixed`).
/// Newtype over the map so it round-trips as a plain JSON object
/// (`{"damage": 102400, ...}`) — see `SystemId`/`PlayerId` for the same
/// `#[serde(transparent)]` pattern.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ItemStats(pub BTreeMap<StatKey, i64>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

/// Generation input (spec §16, `ItemSeed`). `faction`/`biome` are plain
/// strings (not `crate::seed::types::Biome`) per the spec's own struct —
/// the item generator is self-contained and doesn't share the world biome
/// vocabulary; `biome` here is loose flavor for crafting materials/text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ItemSeed {
    pub seed: u64,
    pub item_type: ItemType,
    pub tier: u8,
    pub faction: String,
    pub biome: String,
}

/// Generation output (spec §16, `GeneratedItem`). Deliberately does NOT
/// carry `item_type`/`tier`: downstream consumers keep the `ItemSeed`
/// alongside (see S17's `ItemRef` = seed + `ItemSeed` params) rather than
/// have this contract duplicate it — same shape the spec itself defines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedItem {
    pub id: String,
    pub seed: u64,
    pub display_name: String,
    pub description: String,
    pub icon: GeneratedTexture,
    pub stats: ItemStats,
    pub rarity: Rarity,
}

/// Deterministic id string: `item-<16 hex>`, an FNV-1a hash over every
/// `ItemSeed` field. Two identical `ItemSeed`s always produce the same id;
/// changing any field (including `item_type`) changes it.
pub fn item_id(item_seed: &ItemSeed) -> String {
    const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut h = FNV_OFFSET;
    let mut write = |bytes: &[u8]| {
        for &b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
    };
    write(&item_seed.seed.to_le_bytes());
    write(item_seed.item_type.token().as_bytes());
    write(&[item_seed.tier]);
    write(item_seed.faction.as_bytes());
    write(item_seed.biome.as_bytes());
    format!("item-{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_round_trips_every_leaf() {
        for item_type in ItemType::all() {
            let token = item_type.token();
            assert_eq!(
                ItemType::from_token(&token),
                Some(item_type),
                "token round-trip failed for {token}"
            );
        }
    }

    #[test]
    fn cli_examples_parse() {
        assert_eq!(
            ItemType::from_token("energy_laser"),
            Some(ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Energy(
                EnergyWeapon::Laser
            ))))
        );
        assert_eq!(
            ItemType::from_token("kinetic_cannon"),
            Some(ItemType::Equipment(EquipmentKind::Weapon(WeaponKind::Kinetic(
                KineticWeapon::Cannon
            ))))
        );
    }

    #[test]
    fn every_leaf_has_a_family() {
        for item_type in ItemType::all() {
            let _ = item_type.family(); // exhaustive match; panics if a leaf is unmapped
        }
    }

    #[test]
    fn family_representatives_map_back_to_family() {
        for family in ItemFamily::ALL {
            assert_eq!(family.representative_item_type().family(), family);
        }
    }

    #[test]
    fn no_duplicate_leaf_tokens() {
        let all = ItemType::all();
        let mut tokens: Vec<String> = all.iter().map(|t| t.token()).collect();
        tokens.sort();
        let before = tokens.len();
        tokens.dedup();
        assert_eq!(tokens.len(), before, "duplicate item type token");
    }

    #[test]
    fn expected_leaf_count() {
        // 14 weapons + 9 non-weapon equipment + 10 consumable + 8 component
        // + 4 implant + 7 cosmetic = 52 (spec §16 hierarchy, flattened).
        assert_eq!(ItemType::all().len(), 52);
    }

    #[test]
    fn item_id_is_deterministic_and_sensitive_to_every_field() {
        let base = ItemSeed {
            seed: 7,
            item_type: ItemType::Equipment(EquipmentKind::Shield),
            tier: 3,
            faction: "Compact".into(),
            biome: "frontier".into(),
        };
        assert_eq!(item_id(&base), item_id(&base));

        let mut other = base.clone();
        other.seed = 8;
        assert_ne!(item_id(&base), item_id(&other));

        let mut other = base.clone();
        other.item_type = ItemType::Equipment(EquipmentKind::Armor);
        assert_ne!(item_id(&base), item_id(&other));

        let mut other = base.clone();
        other.tier = 4;
        assert_ne!(item_id(&base), item_id(&other));

        let mut other = base.clone();
        other.faction = "ISC".into();
        assert_ne!(item_id(&base), item_id(&other));

        let mut other = base.clone();
        other.biome = "nebula".into();
        assert_ne!(item_id(&base), item_id(&other));
    }
}
