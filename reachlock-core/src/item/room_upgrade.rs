//! Ship room upgrades (S45): widget items, upgrade slots, power budget,
//! and room repurposing. Pure functions — deterministic, wasm-safe.

use serde::{Deserialize, Serialize};

use crate::generator::RoomKind;
use crate::item::types::StatKey;
use crate::util::Fixed;

/// Which kind of upgrade a widget provides. Mirrors the room taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomUpgradeKind {
    // Med Bay
    MedStation,
    PharmacyLocker,
    TriageBed,
    SurgerySuite,
    // Engineering
    ReactorConsole,
    RepairBench,
    ComponentStorage,
    OverclockModule,
    // Hydroponics
    GrowBed,
    IrrigationSystem,
    NutrientProcessor,
    AeroponicArray,
    // Cargo Hold
    CargoExpansion,
    RefrigerationUnit,
    SecureVault,
    HiddenCompartment,
    // Crew Quarters
    Bunk,
    RecreationModule,
    PrivacyScreen,
    LuxurySuite,
    // Galley
    GalleyUnit,
    FoodProcessor,
    EntertainmentSystem,
    ChefStation,
    // Workshop
    Workbench,
    NanoForge,
    MaterialAnalyzer,
    PrototypePrinter,
    // Cockpit / Bridge
    NavComputer,
    SensorArray,
    CommRelay,
    TargetingComputer,
    // Armory
    WeaponRack,
    ArmorLocker,
    AmmoFabricator,
    CombatSimulator,
    // Science Lab
    SampleAnalyzer,
    IsolationChamber,
    ResearchDatabase,
    PredecessorInterface,
}

impl RoomUpgradeKind {
    /// The room kind this upgrade must be installed in.
    pub fn room_kind(self) -> RoomKind {
        match self {
            RoomUpgradeKind::MedStation
            | RoomUpgradeKind::PharmacyLocker
            | RoomUpgradeKind::TriageBed
            | RoomUpgradeKind::SurgerySuite => RoomKind::MedBay,
            RoomUpgradeKind::ReactorConsole
            | RoomUpgradeKind::RepairBench
            | RoomUpgradeKind::ComponentStorage
            | RoomUpgradeKind::OverclockModule => RoomKind::TechBay,
            RoomUpgradeKind::GrowBed
            | RoomUpgradeKind::IrrigationSystem
            | RoomUpgradeKind::NutrientProcessor
            | RoomUpgradeKind::AeroponicArray => RoomKind::Hydroponics,
            RoomUpgradeKind::CargoExpansion
            | RoomUpgradeKind::RefrigerationUnit
            | RoomUpgradeKind::SecureVault
            | RoomUpgradeKind::HiddenCompartment => RoomKind::CargoHold,
            RoomUpgradeKind::Bunk
            | RoomUpgradeKind::RecreationModule
            | RoomUpgradeKind::PrivacyScreen
            | RoomUpgradeKind::LuxurySuite => RoomKind::Quarters,
            RoomUpgradeKind::GalleyUnit
            | RoomUpgradeKind::FoodProcessor
            | RoomUpgradeKind::EntertainmentSystem
            | RoomUpgradeKind::ChefStation => RoomKind::Galley,
            RoomUpgradeKind::Workbench
            | RoomUpgradeKind::NanoForge
            | RoomUpgradeKind::MaterialAnalyzer
            | RoomUpgradeKind::PrototypePrinter => RoomKind::Workshop,
            RoomUpgradeKind::NavComputer
            | RoomUpgradeKind::SensorArray
            | RoomUpgradeKind::CommRelay
            | RoomUpgradeKind::TargetingComputer => RoomKind::Cockpit,
            RoomUpgradeKind::WeaponRack
            | RoomUpgradeKind::ArmorLocker
            | RoomUpgradeKind::AmmoFabricator
            | RoomUpgradeKind::CombatSimulator => RoomKind::Armory,
            RoomUpgradeKind::SampleAnalyzer
            | RoomUpgradeKind::IsolationChamber
            | RoomUpgradeKind::ResearchDatabase
            | RoomUpgradeKind::PredecessorInterface => RoomKind::ScienceLab,
        }
    }

    /// Default slot type for this upgrade kind.
    pub fn default_slot_type(self) -> UpgradeSlotType {
        match self {
            RoomUpgradeKind::SurgerySuite
            | RoomUpgradeKind::ReactorConsole
            | RoomUpgradeKind::AeroponicArray
            | RoomUpgradeKind::CargoExpansion
            | RoomUpgradeKind::LuxurySuite
            | RoomUpgradeKind::ChefStation
            | RoomUpgradeKind::PrototypePrinter
            | RoomUpgradeKind::TargetingComputer
            | RoomUpgradeKind::CombatSimulator
            | RoomUpgradeKind::PredecessorInterface => UpgradeSlotType::Primary,
            RoomUpgradeKind::MedStation
            | RoomUpgradeKind::PharmacyLocker
            | RoomUpgradeKind::RepairBench
            | RoomUpgradeKind::GrowBed
            | RoomUpgradeKind::RefrigerationUnit
            | RoomUpgradeKind::RecreationModule
            | RoomUpgradeKind::FoodProcessor
            | RoomUpgradeKind::NanoForge
            | RoomUpgradeKind::SensorArray
            | RoomUpgradeKind::ArmorLocker
            | RoomUpgradeKind::SampleAnalyzer
            | RoomUpgradeKind::NavComputer => UpgradeSlotType::Secondary,
            _ => UpgradeSlotType::Utility,
        }
    }
}

/// Which slot type an upgrade occupies in a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeSlotType {
    Primary,
    Secondary,
    Utility,
}

/// Per-upgrade static stats.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomUpgradeStats {
    pub room_kind: RoomKind,
    pub slot_type: UpgradeSlotType,
    pub stat_bonuses: Vec<StatBonus>,
    pub power_draw: i64,
    pub install_ticks: u64,
}

/// A single stat bonus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatBonus {
    pub stat: StatKey,
    pub value: i64,
    pub is_percentage: bool,
}

/// Result of computing all room bonuses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoomBonuses {
    pub bonuses: Vec<StatBonus>,
    pub total_power_draw: i64,
    pub power_available: i64,
}

impl RoomBonuses {
    /// Effective bonus multiplier (0.0–1.0). If power deficit, effectiveness
    /// scales linearly: `eff = available / draw` capped at 1.0.
    pub fn effectiveness(&self) -> Fixed {
        if self.total_power_draw <= 0 || self.power_available >= self.total_power_draw {
            Fixed::from_int(1)
        } else {
            Fixed(self.power_available * Fixed::SCALE / self.total_power_draw)
        }
    }
}

/// Compute aggregate room bonuses from a set of installed upgrades.
pub fn compute_room_bonuses(upgrades: &[RoomUpgradeStats], power_available: i64) -> RoomBonuses {
    let mut bonuses = Vec::new();
    let mut total_power_draw: i64 = 0;
    for u in upgrades {
        total_power_draw += u.power_draw;
        bonuses.extend(u.stat_bonuses.iter().cloned());
    }
    RoomBonuses {
        bonuses,
        total_power_draw,
        power_available,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::types::StatKey;

    #[test]
    fn room_kind_mapping() {
        assert_eq!(RoomUpgradeKind::SurgerySuite.room_kind(), RoomKind::MedBay);
        assert_eq!(RoomUpgradeKind::CargoExpansion.room_kind(), RoomKind::CargoHold);
        assert_eq!(RoomUpgradeKind::GalleyUnit.room_kind(), RoomKind::Galley);
        assert_eq!(RoomUpgradeKind::Workbench.room_kind(), RoomKind::Workshop);
    }

    #[test]
    fn compute_bonuses_aggregates() {
        let ups = vec![
            RoomUpgradeStats {
                room_kind: RoomKind::MedBay,
                slot_type: UpgradeSlotType::Primary,
                stat_bonuses: vec![StatBonus {
                    stat: StatKey::HealRate,
                    value: 1024,
                    is_percentage: false,
                }],
                power_draw: 10,
                install_ticks: 5,
            },
            RoomUpgradeStats {
                room_kind: RoomKind::MedBay,
                slot_type: UpgradeSlotType::Secondary,
                stat_bonuses: vec![StatBonus {
                    stat: StatKey::HealSpeed,
                    value: 512,
                    is_percentage: true,
                }],
                power_draw: 5,
                install_ticks: 3,
            },
        ];
        let r = compute_room_bonuses(&ups, 100);
        assert_eq!(r.bonuses.len(), 2);
        assert_eq!(r.total_power_draw, 15);
        assert_eq!(r.effectiveness(), Fixed::from_int(1));
    }

    #[test]
    fn power_deficit_reduces_effectiveness() {
        let ups = vec![RoomUpgradeStats {
            room_kind: RoomKind::TechBay,
            slot_type: UpgradeSlotType::Primary,
            stat_bonuses: vec![],
            power_draw: 200,
            install_ticks: 1,
        }];
        let r = compute_room_bonuses(&ups, 100);
        assert!(r.effectiveness() < Fixed::from_int(1));
    }
}
