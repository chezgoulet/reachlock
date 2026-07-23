//! Trope engine: templates (S40). Authored narrative beats with procedural
//! slot filling — the procedural "seasoning" layer of exploration. Pure &
//! deterministic: `seed + game_state → TropeInstance`. No I/O, no LLM.

use serde::{Deserialize, Serialize};

use crate::generator::dilemma::DilemmaType;
use crate::generator::ecosystem_events::EcosystemEventType;
use crate::util::rng::SeededRng;
use crate::util::Fixed;

/// Where a trope can fire — defines which location types are valid triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationType {
    SystemEntry,
    DeepSpace,
    AsteroidBelt,
    Anomaly,
    Beacon,
    StationApproach,
    Resupply,
    LongSilence,
    DebrisField,
    GasGiantRing,
}

/// A trope template — authored as `.ron` content files. Slots are filled
/// from game state at instantiation time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TropeTemplate {
    pub id: String,
    pub trope_type: TropeType,
    pub title_template: String,
    pub narrative_template: String,
    pub slots: Vec<TropeSlot>,
    pub branches: Vec<TropeBranch>,
    /// Base frequency in Fixed (1/1024, 1024 = 100% probability per trigger
    /// roll). Iron rule #2: this is Fixed, not f64.
    pub base_frequency: Fixed,
    pub location_types: Vec<LocationType>,
    pub min_threat_level: u8,
    pub max_threat_level: u8,
    /// Chance that this trope triggers a dilemma on resolution, Fixed.
    pub dilemma_chance: Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TropeType {
    DerelictShip,
    DistressBeacon,
    AnomalousSignal,
    AbandonedStation,
    PredecessorArtifact,
    SmugglerCache,
    RefugeeConvoy,
    ScienceOutpost,
    PirateAmbush,
    TradeOpportunity,
    WeirdSpacePhenomenon,
    ColonyGoneWrong,
    AIShip,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TropeSlot {
    pub slot_name: String,
    pub slot_kind: SlotKind,
    pub constraints: Vec<SlotConstraint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    ShipClass,
    Faction,
    Item,
    Species,
    CrewRole,
    PlanetName,
    StationName,
    SecretType,
    FateDescription,
    ClueItem,
    Number { min: u32, max: u32 },
    Text { options: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotConstraint {
    pub field: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TropeBranch {
    pub label: String,
    pub condition: Option<String>,
    pub action: TropeAction,
    pub consequences: Vec<TropeConsequence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TropeAction {
    GiveItem { item_seed: crate::item::types::ItemSeed },
    StartCombat { enemies: Vec<String>, difficulty: u8 },
    TriggerDilemma { dilemma_type: DilemmaType },
    TriggerEcosystemEvent { event: EcosystemEventType },
    ModifyReputation { faction: String, delta: i64 },
    UnlockMission { mission_template_id: String },
    TextOnly { text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TropeConsequence {
    pub kind: TropeConsequenceKind,
    pub target: String,
    pub magnitude: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TropeConsequenceKind {
    FactionReputation,
    CrewTrust,
    Credits,
    CargoSpace,
    ShipDamage,
    CrewInjury,
    EcosystemImpact,
    MissionProgress,
}

/// A fully instantiated trope — slots filled, branch conditions evaluated,
/// narrative rendered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TropeInstance {
    pub template_id: String,
    pub seed: u64,
    pub trope_type: TropeType,
    pub title: String,
    pub narrative: String,
    pub filled_slots: std::collections::BTreeMap<String, String>,
    pub branches: Vec<TropeBranch>,
    pub location: LocationType,
    pub resolved: bool,
    pub player_choice: Option<String>,
}

/// Fill every slot in a template from the game state map. Slots that can't
/// be filled from the map fall back to seed-driven random generation (text
/// options, number ranges, or a placeholder).
pub fn fill_trope_slots(
    template: &TropeTemplate,
    seed: u64,
    game_state: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, String> {
    let mut rng = SeededRng::new(seed);
    let mut filled = std::collections::BTreeMap::new();

    for slot in &template.slots {
        let value: Option<String> = match &slot.slot_kind {
            SlotKind::Text { options } if !options.is_empty() => {
                Some(options[rng.next_below(options.len() as u64) as usize].clone())
            }
            SlotKind::Number { min, max } => {
                if min >= max {
                    Some(min.to_string())
                } else {
                    Some((min + rng.next_below((max - min) as u64) as u32).to_string())
                }
            }
            _ => {
                // Look up from game state by slot_kind name.
                let key = match &slot.slot_kind {
                    SlotKind::ShipClass => "ship_classes",
                    SlotKind::Faction => "factions",
                    SlotKind::Item => "items",
                    SlotKind::Species => "species",
                    SlotKind::CrewRole => "crew_roles",
                    SlotKind::PlanetName => "planet_names",
                    SlotKind::StationName => "station_names",
                    SlotKind::SecretType => "secret_types",
                    SlotKind::FateDescription => "fate_descriptions",
                    SlotKind::ClueItem => "clue_items",
                    _ => continue,
                };
                game_state.get(key).and_then(|list| {
                    if list.is_empty() {
                        None
                    } else {
                        Some(list[rng.next_below(list.len() as u64) as usize].clone())
                    }
                })
            }
        };
        if let Some(v) = value.or_else(|| Some(format!("<{}>", slot.slot_name))) {
            filled.insert(slot.slot_name.clone(), v);
        }
    }
    filled
}

/// Instantiate a trope: fill slots, render title/narrative templates, clone
/// branches. Deterministic: same seed + game state always yields the same
/// instance.
pub fn instantiate_trope(
    template: &TropeTemplate,
    seed: u64,
    game_state: &std::collections::BTreeMap<String, Vec<String>>,
    location: LocationType,
) -> TropeInstance {
    let filled = fill_trope_slots(template, seed, game_state);

    let title = render_template(&template.title_template, &filled);
    let narrative = render_template(&template.narrative_template, &filled);

    TropeInstance {
        template_id: template.id.clone(),
        seed,
        trope_type: template.trope_type,
        title,
        narrative,
        filled_slots: filled,
        branches: template.branches.clone(),
        location,
        resolved: false,
        player_choice: None,
    }
}

/// Replace `{slot_name}` tokens in a template string with filled values.
/// Unresolved tokens remain as-is (graceful fallback for missing slots).
fn render_template(template: &str, filled: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = template.to_string();
    for (key, val) in filled {
        out = out.replace(&format!("{{{}}}", key), val);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample_template() -> TropeTemplate {
        TropeTemplate {
            id: "derelict_ship_01".into(),
            trope_type: TropeType::DerelictShip,
            title_template: "The {ship_name}".into(),
            narrative_template: "You find the {ship_name}, a {condition} vessel adrift in {location}. {detail}".into(),
            slots: vec![
                TropeSlot {
                    slot_name: "ship_name".into(),
                    slot_kind: SlotKind::Text {
                        options: vec!["Grief of Ages".into(), "Last Candle".into(), "Iron Requiem".into()],
                    },
                    constraints: vec![],
                },
                TropeSlot {
                    slot_name: "condition".into(),
                    slot_kind: SlotKind::Text {
                        options: vec!["hollow".into(), "ice-rimed".into(), "buckled".into()],
                    },
                    constraints: vec![],
                },
                TropeSlot {
                    slot_name: "detail".into(),
                    slot_kind: SlotKind::FateDescription,
                    constraints: vec![],
                },
                TropeSlot {
                    slot_name: "location".into(),
                    slot_kind: SlotKind::PlanetName,
                    constraints: vec![],
                },
            ],
            branches: vec![TropeBranch {
                label: "Board".into(),
                condition: None,
                action: TropeAction::TextOnly { text: "You dock with the derelict.".into() },
                consequences: vec![TropeConsequence {
                    kind: TropeConsequenceKind::CrewTrust,
                    target: "all".into(),
                    magnitude: -50,
                }],
            }],
            base_frequency: Fixed::from_int(1),
            location_types: vec![LocationType::DeepSpace, LocationType::AsteroidBelt],
            min_threat_level: 1,
            max_threat_level: 5,
            dilemma_chance: Fixed(2 * Fixed::SCALE / 10),
        }
    }

    #[test]
    fn deterministic_instantiation() {
        let template = sample_template();
        let mut gs = BTreeMap::new();
        gs.insert("fate_descriptions".into(), vec!["no life signs".into()]);
        gs.insert("planet_names".into(), vec!["Korvos".into()]);
        let a = instantiate_trope(&template, 12345, &gs, LocationType::DeepSpace);
        let b = instantiate_trope(&template, 12345, &gs, LocationType::DeepSpace);
        assert_eq!(a, b);
    }

    #[test]
    fn slot_filling_populates_all_slots() {
        let template = sample_template();
        let mut gs = BTreeMap::new();
        gs.insert("fate_descriptions".into(), vec!["cold".into()]);
        gs.insert("planet_names".into(), vec!["Ara".into()]);
        let filled = fill_trope_slots(&template, 99, &gs);
        for slot in &template.slots {
            assert!(filled.contains_key(&slot.slot_name), "missing slot {}", slot.slot_name);
        }
    }

    #[test]
    fn narrative_renders_slot_values() {
        let template = sample_template();
        let mut gs = BTreeMap::new();
        gs.insert("fate_descriptions".into(), vec!["empty".into()]);
        gs.insert("planet_names".into(), vec!["Rom".into()]);
        let instance = instantiate_trope(&template, 42, &gs, LocationType::AsteroidBelt);
        assert!(instance.narrative.contains(&instance.filled_slots["ship_name"]));
        assert!(instance.narrative.contains(&instance.filled_slots["condition"]));
    }

    #[test]
    fn different_seeds_different_text_slots() {
        let template = sample_template();
        let gs = BTreeMap::new();
        let a = instantiate_trope(&template, 1, &gs, LocationType::DeepSpace);
        let b = instantiate_trope(&template, 2, &gs, LocationType::DeepSpace);
        assert_ne!(a, b);
    }
}
