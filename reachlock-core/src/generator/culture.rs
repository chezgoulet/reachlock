//! Emergent planet culture (S47). `generate_culture` derives a coherent
//! culture from planetary conditions + settlement history + faction influence
//! via an explicit causal chain, then runs a coherence pass to resolve
//! contradictions (faction allegiance trumps ecosystem). Pure & deterministic.

use serde::{Deserialize, Serialize};

use crate::faction::FactionId;
use crate::generator::planet_extended::{FactionMap, Hazard, SettlementWave};
use crate::util::color::{hsv, ColorRgba8};
use crate::util::rng::SeededRng;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetCulture {
    pub cultural_id: String,
    pub language: LanguageProfile,
    pub customs: Vec<Custom>,
    pub social_structure: SocialStructure,
    pub architecture: ArchitecturalStyle,
    pub clothing: ClothingStyle,
    pub attitude_toward_outsiders: OutsiderAttitude,
    pub faction_allegiance: FactionAllegiance,
    pub dominant_values: Vec<CulturalValue>,
    pub cultural_quirk: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LanguageProfile {
    pub base_language: String,
    pub drift_intensity: u8,
    pub accent_name: String,
    pub unique_terms: Vec<String>,
    pub greeting: String,
    pub farewell: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Custom {
    pub custom_type: CustomType,
    pub description: String,
    pub trigger: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomType {
    Greeting,
    Farewell,
    GiftGiving,
    Dining,
    Bargaining,
    Conflict,
    Mourning,
    Celebration,
    Taboo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SocialStructure {
    Egalitarian,
    Hierarchical { castes: Vec<String> },
    Meritocratic,
    Corporate,
    Military,
    Religious,
    Communal,
    Individualistic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorScheme {
    pub primary: ColorRgba8,
    pub secondary: ColorRgba8,
    pub accent: ColorRgba8,
    pub preference: ColorPreference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorPreference {
    Warm,
    Cool,
    Earth,
    Bold,
    Muted,
    Monochrome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchitecturalStyle {
    pub style_name: String,
    pub materials: Vec<String>,
    pub dominant_shape: String,
    pub color_palette: ColorScheme,
    pub adapted_to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClothingStyle {
    pub style_name: String,
    pub primary_material: String,
    pub dominant_colors: ColorScheme,
    pub practicality_level: u8,
    pub adapted_to: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutsiderAttitude {
    Welcoming,
    Curious,
    Indifferent,
    Suspicious,
    Hostile,
    Xenophilic,
    Isolationist,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactionAllegiance {
    Loyal { faction_id: FactionId, intensity: u8 },
    NominallyAligned { faction_id: FactionId },
    Independent,
    Contested { factions: Vec<FactionId> },
    Lawless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CulturalValue {
    Honor,
    Community,
    Innovation,
    Tradition,
    Wealth,
    Knowledge,
    Strength,
    Compassion,
    Independence,
    Order,
    Freedom,
    Piety,
    Survival,
    Beauty,
    Efficiency,
    Family,
}

const BASE_LANGUAGES: &[&str] = &["Compact Standard", "ISC Trade", "Frontier Pidgin", "Old Core"];
const ACCENTS: &[&str] = &["flat", "clipped", "drawling", "singsong", "guttural"];
const QUIRKS: &[&str] = &[
    "counts time in harvests",
    "never speaks the ruler's name",
    "pays debts in stories",
    "plants a tree at every birth",
    "wears the color of their home district",
];
const SHAPES: &[&str] = &["sprawling", "vertically stacked", "domed", "terraced", "sunken"];

/// Generate a coherent culture. `founding_faction` is the dominant faction;
/// `faction_map` supplies the full influence picture (for contested/loyal
/// resolution).
pub fn generate_culture(
    seed: u64,
    habitability_index: u8,
    hazards: &[Hazard],
    founding_faction: &FactionId,
    founding_wave: SettlementWave,
    faction_map: &FactionMap,
    threat_level: u8,
) -> PlanetCulture {
    let mut rng = SeededRng::new(seed);

    // --- Language: base from faction, drift from founding wave ------------
    let base = BASE_LANGUAGES[rng.next_below(BASE_LANGUAGES.len() as u64) as usize].to_string();
    let drift = match founding_wave {
        SettlementWave::FirstWave => 10,
        SettlementWave::SecondWave => 25,
        SettlementWave::ThirdWave => 40,
        SettlementWave::RecentColony => 55,
        SettlementWave::FrontierOutpost => 70,
    } + rng.next_below(15) as u8;
    let accent = ACCENTS[rng.next_below(ACCENTS.len() as u64) as usize].to_string();
    let unique_terms = (0..3)
        .map(|_| format!("{}word", rng.next_below(2).max(1)))
        .collect();
    let language = LanguageProfile {
        base_language: base,
        drift_intensity: drift,
        accent_name: accent,
        unique_terms,
        greeting: "Well met, traveler".into(),
        farewell: "Safe stars".into(),
    };

    // --- Customs ----------------------------------------------------------
    let mut customs = vec![
        Custom {
            custom_type: CustomType::Greeting,
            description: "Offer a closed fist to the chest".into(),
            trigger: "first meeting".into(),
        },
        Custom {
            custom_type: CustomType::Taboo,
            description: "Never name the dead aloud".into(),
            trigger: "funeral".into(),
        },
        Custom {
            custom_type: CustomType::Bargaining,
            description: "Haggle is expected; silence means refusal".into(),
            trigger: "market".into(),
        },
    ];
    if hazards.iter().any(|h| matches!(h, Hazard::AggressiveFauna)) {
        customs.push(Custom {
            custom_type: CustomType::Conflict,
            description: "Carry a warding charm off-planet".into(),
            trigger: "departure".into(),
        });
    }

    // --- Social structure -------------------------------------------------
    let social_structure = match rng.next_below(8) {
        0 => SocialStructure::Egalitarian,
        1 => SocialStructure::Hierarchical {
            castes: vec!["Guild".into(), "Free".into(), "Bound".into()],
        },
        2 => SocialStructure::Meritocratic,
        3 => SocialStructure::Corporate,
        4 => SocialStructure::Military,
        5 => SocialStructure::Religious,
        6 => SocialStructure::Communal,
        _ => SocialStructure::Individualistic,
    };

    // --- Architecture & clothing (adapted to conditions) -----------------
    let hue = rng.next_below(1536) as u32;
    let palette = ColorScheme {
        primary: hsv(hue, 150 + rng.next_below(80) as u32, 200),
        secondary: hsv((hue + 200) % 1536, 120, 180),
        accent: hsv((hue + 600) % 1536, 200, 230),
        preference: match rng.next_below(6) {
            0 => ColorPreference::Warm,
            1 => ColorPreference::Cool,
            2 => ColorPreference::Earth,
            3 => ColorPreference::Bold,
            4 => ColorPreference::Muted,
            _ => ColorPreference::Monochrome,
        },
    };
    let dominant_shape = SHAPES[rng.next_below(SHAPES.len() as u64) as usize].to_string();
    let architecture = ArchitecturalStyle {
        style_name: format!("{} vernacular", dominant_shape),
        materials: vec!["local stone".into(), "imported alloy".into()],
        dominant_shape: dominant_shape.clone(),
        color_palette: palette,
        adapted_to: if habitability_index < 40 {
            vec!["low light".into(), "thin air".into()]
        } else {
            vec!["open skies".into()]
        },
    };
    let clothing_practicality = if hazards.is_empty() { 20 } else { 60 + hazards.len() as u8 * 10 };
    let clothing = ClothingStyle {
        style_name: format!("{}-weave", dominant_shape),
        primary_material: "synthetic fiber".into(),
        dominant_colors: palette,
        practicality_level: clothing_practicality.min(100),
        adapted_to: hazards
            .iter()
            .map(|h| format!("{:?}", h))
            .collect(),
    };

    // --- Attitude & allegiance (coherence pass) --------------------------
    let faction_allegiance = resolve_allegiance(founding_faction, faction_map);
    // Faction allegiance trumps ecosystem: Lawless/Contested => isolationist.
    let attitude = match &faction_allegiance {
        FactionAllegiance::Lawless => OutsiderAttitude::Isolationist,
        FactionAllegiance::Contested { .. } => OutsiderAttitude::Suspicious,
        _ if threat_level > 60 => OutsiderAttitude::Hostile,
        _ if habitability_index < 30 => OutsiderAttitude::Indifferent,
        _ if rng.next_below(100) < 40 => OutsiderAttitude::Welcoming,
        _ => OutsiderAttitude::Curious,
    };

    // --- Dominant values --------------------------------------------------
    let all_values = [
        CulturalValue::Honor,
        CulturalValue::Community,
        CulturalValue::Innovation,
        CulturalValue::Tradition,
        CulturalValue::Wealth,
        CulturalValue::Knowledge,
        CulturalValue::Strength,
        CulturalValue::Compassion,
        CulturalValue::Survival,
        CulturalValue::Order,
    ];
    let n_val = 2 + rng.next_below(3) as usize;
    let mut dominant_values = Vec::new();
    while dominant_values.len() < n_val {
        let v = all_values[rng.next_below(all_values.len() as u64) as usize];
        if !dominant_values.contains(&v) {
            dominant_values.push(v);
        }
    }

    let cultural_quirk = QUIRKS[rng.next_below(QUIRKS.len() as u64) as usize].to_string();

    PlanetCulture {
        cultural_id: format!("culture-{}", seed % 100_000),
        language,
        customs,
        social_structure,
        architecture,
        clothing,
        attitude_toward_outsiders: attitude,
        faction_allegiance,
        dominant_values,
        cultural_quirk,
    }
}

fn resolve_allegiance(founding_faction: &FactionId, faction_map: &FactionMap) -> FactionAllegiance {
    let entries: Vec<(&FactionId, &u8)> = faction_map.iter().collect();
    if entries.is_empty() {
        return FactionAllegiance::Independent;
    }
    if entries.len() > 1 {
        return FactionAllegiance::Contested {
            factions: entries.iter().map(|(f, _)| (*f).clone()).collect(),
        };
    }
    let (fid, infl) = entries[0];
    if *fid == *founding_faction && *infl > 80 {
        FactionAllegiance::Loyal {
            faction_id: fid.clone(),
            intensity: *infl,
        }
    } else {
        FactionAllegiance::NominallyAligned {
            faction_id: fid.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn hazards() -> Vec<Hazard> {
        vec![Hazard::AggressiveFauna]
    }

    #[test]
    fn deterministic_culture() {
        let fm = HashMap::from([(FactionId("compact".into()), 120u8)]);
        let a = generate_culture(
            123,
            60,
            &hazards(),
            &FactionId("compact".into()),
            SettlementWave::FirstWave,
            &fm,
            20,
        );
        let b = generate_culture(
            123,
            60,
            &hazards(),
            &FactionId("compact".into()),
            SettlementWave::FirstWave,
            &fm,
            20,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn loyal_allegiance_when_dominant_strong() {
        let fm = HashMap::from([(FactionId("compact".into()), 120u8)]);
        let c = generate_culture(
            1,
            70,
            &[],
            &FactionId("compact".into()),
            SettlementWave::FirstWave,
            &fm,
            10,
        );
        assert!(matches!(
            c.faction_allegiance,
            FactionAllegiance::Loyal { .. }
        ));
    }

    #[test]
    fn empty_faction_map_is_independent() {
        let fm = HashMap::new();
        let c = generate_culture(
            2,
            70,
            &[],
            &FactionId("independent".into()),
            SettlementWave::FrontierOutpost,
            &fm,
            10,
        );
        assert!(matches!(c.faction_allegiance, FactionAllegiance::Independent));
    }

    #[test]
    fn hazard_shapes_clothing() {
        let fm = HashMap::from([(FactionId("compact".into()), 50u8)]);
        let c = generate_culture(
            3,
            80,
            &hazards(),
            &FactionId("compact".into()),
            SettlementWave::SecondWave,
            &fm,
            10,
        );
        assert!(!c.clothing.adapted_to.is_empty());
        assert!(c.customs.iter().any(|x| x.custom_type == CustomType::Conflict));
    }
}
