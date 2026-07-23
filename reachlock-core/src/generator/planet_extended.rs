//! Planet scale & culture (S47). Planets become full worlds: physical scale,
//! climate, biomes, resource distribution, generated settlements, and an
//! emergent coherent culture derived from conditions + settlement history +
//! faction influence. `PlanetExtended` *wraps* S04's `GeneratedPlanet` (disc +
//! surface) rather than forking it. Pure & deterministic (iron rule #1/#2:
//! all gameplay values are integers / `Fixed`).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::economy::GoodId;
use crate::faction::FactionId;
use crate::generator::culture::PlanetCulture;
use crate::generator::planet::{generate_planet, GeneratedPlanet};
use crate::seed::types::Biome;
use crate::util::rng::SeededRng;
use crate::util::Fixed;

/// Influence of a faction on a planet (0–255), keyed by faction id.
pub type FactionMap = HashMap<FactionId, u8>;

/// Coarse system context the planet is generated within.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemParams {
    pub kind: String,
    pub threat_level: u8,
}

/// A full world: S04's visual planet plus physical, cultural, and economic
/// layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetExtended {
    pub planet_id: String,
    pub name: String,
    /// Wrapped S04 output (disc mesh + surface texture).
    pub planet: GeneratedPlanet,
    pub physical: PlanetPhysical,
    pub climate: PlanetClimate,
    pub habitability: Habitability,
    pub resources: PlanetResources,
    pub settlements: Vec<Settlement>,
    pub culture: PlanetCulture,
    /// Links to an S39 ecosystem id (filled by the galaxy assembler).
    pub ecosystem_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetPhysical {
    pub diameter_km: u32,
    pub gravity_g: Fixed,
    pub atmosphere: AtmosphereType,
    pub atmosphere_density: Fixed,
    pub surface_water_pct: u8,
    pub tectonic_activity: u8,
    pub magnetic_field: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtmosphereType {
    None,
    Trace,
    Thin,
    Standard,
    Dense,
    Toxic,
    Corrosive,
    Exotic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetClimate {
    pub temperature_range: (i32, i32),
    pub seasons: SeasonIntensity,
    pub weather_patterns: Vec<WeatherPattern>,
    pub climate_zones: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonIntensity {
    None,
    Mild,
    Moderate,
    Extreme,
    Chaotic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherPattern {
    pub pattern_type: WeatherType,
    pub frequency: u8,
    pub severity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherType {
    Rain,
    Storm,
    Hurricane,
    Tornado,
    DustStorm,
    SandStorm,
    Blizzard,
    IceStorm,
    AcidRain,
    MethaneFog,
    SolarFlare,
    RadiationSurge,
    ElectromagneticStorm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Habitability {
    pub human_compatible: bool,
    pub requires_suit: bool,
    pub requires_dome: bool,
    pub terraformable: bool,
    pub habitability_index: u8,
    pub hazards: Vec<Hazard>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hazard {
    ToxicAtmosphere,
    ExtremeTemperature,
    HighGravity,
    LowGravity,
    Radiation,
    AggressiveFauna,
    GeologicalInstability,
    CorrosiveEnvironment,
    ParasiticMicrobes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetResources {
    pub mineral_richness: u8,
    pub mineral_types: Vec<GoodId>,
    pub organic_richness: u8,
    pub energy_potential: u8,
    pub rare_element_presence: u8,
    pub resource_map: ResourceMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceMap {
    pub deposits: Vec<ResourceDeposit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceDeposit {
    pub good_id: GoodId,
    pub location: (Fixed, Fixed),
    pub richness: u8,
    pub accessibility: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settlement {
    pub settlement_id: String,
    pub name: String,
    pub settlement_type: SettlementType,
    pub population: u64,
    pub founded_tick: u64,
    pub founding_faction: FactionId,
    pub founding_wave: SettlementWave,
    pub districts: Vec<District>,
    pub starport_size: u8,
    pub economic_focus: EconomicFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementType {
    Outpost,
    Colony,
    Settlement,
    City,
    Metropolis,
    Megacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementWave {
    FirstWave,
    SecondWave,
    ThirdWave,
    RecentColony,
    FrontierOutpost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct District {
    pub district_type: DistrictType,
    pub name: String,
    pub notable_locations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistrictType {
    Starport,
    Industrial,
    Residential,
    Commercial,
    Administrative,
    Research,
    Military,
    Undercity,
    Agricultural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EconomicFocus {
    Mining,
    Manufacturing,
    Agriculture,
    Trade,
    Research,
    Military,
    Tourism,
    Refugee,
    Mixed,
}

// ---- name + generation helpers -------------------------------------------

const PLANET_PREFIX: &[&str] = &[
    "Vel", "Kor", "Aru", "Tes", "Xan", "Mir", "Oph", "Zel", "Cae", "Nyx", "Bri", "Sol",
];
const PLANET_SUFFIX: &[&str] = &[
    "is", "ar", "os", "en", "ix", "us", "ae", "or", "une", "ara", "oth", "ion",
];

fn planet_name(rng: &mut SeededRng) -> String {
    format!(
        "{}{}",
        PLANET_PREFIX[rng.next_below(PLANET_PREFIX.len() as u64) as usize],
        PLANET_SUFFIX[rng.next_below(PLANET_SUFFIX.len() as u64) as usize]
    )
}

fn atmo_from_water(water: u8) -> AtmosphereType {
    match water {
        0 => AtmosphereType::None,
        1..=20 => AtmosphereType::Trace,
        21..=50 => AtmosphereType::Thin,
        51..=80 => AtmosphereType::Standard,
        _ => AtmosphereType::Dense,
    }
}

/// Generate a full planet. Pure & deterministic.
pub fn generate_planet_extended(
    seed: u64,
    biome: Biome,
    radius: i64,
    system: &SystemParams,
    faction_map: &FactionMap,
) -> PlanetExtended {
    let mut rng = SeededRng::new(seed);
    let planet = generate_planet(seed, radius, biome);

    let diameter_km = 2000u32 + rng.next_below(12000) as u32;
    let gravity_g = Fixed(Fixed::SCALE + (rng.next_below(40) as i64 * Fixed::SCALE / 20));
    let water = rng.next_below(101) as u8;
    let atmosphere = atmo_from_water(water);
    let atmosphere_density = Fixed((water as i64) * Fixed::SCALE / 100);
    let tectonic = rng.next_below(101) as u8;
    let magnetic = rng.next_below(101) as u8;

    let habitability_index = (water.saturating_sub(20) / 3)
        .saturating_add(if matches!(atmosphere, AtmosphereType::Standard | AtmosphereType::Thin) {
            40
        } else {
            0
        })
        .min(100);
    let requires_suit = !matches!(atmosphere, AtmosphereType::Standard);
    let mut hazards = Vec::new();
    if requires_suit {
        hazards.push(Hazard::ToxicAtmosphere);
    }
    if gravity_g.0 > Fixed::SCALE * 12 / 10 {
        hazards.push(Hazard::HighGravity);
    }
    if tectonic > 70 {
        hazards.push(Hazard::GeologicalInstability);
    }
    let habitability = Habitability {
        human_compatible: habitability_index > 50 && !requires_suit,
        requires_suit,
        requires_dome: matches!(
            atmosphere,
            AtmosphereType::None | AtmosphereType::Trace | AtmosphereType::Toxic | AtmosphereType::Corrosive
        ),
        terraformable: habitability_index > 20 && habitability_index < 90,
        habitability_index,
        hazards,
    };

    let climate = PlanetClimate {
        temperature_range: (-20 + rng.next_below(20) as i32, 20 + rng.next_below(60) as i32),
        seasons: match rng.next_below(5) {
            0 => SeasonIntensity::None,
            1 => SeasonIntensity::Mild,
            2 => SeasonIntensity::Moderate,
            3 => SeasonIntensity::Extreme,
            _ => SeasonIntensity::Chaotic,
        },
        weather_patterns: vec![WeatherPattern {
            pattern_type: WeatherType::Rain,
            frequency: rng.next_below(101) as u8,
            severity: rng.next_below(11) as u8,
        }],
        climate_zones: 2 + rng.next_below(6) as u8,
    };

    // Resources: a handful of mineral goods + deposits.
    let n_min = 2 + rng.next_below(5) as usize;
    let mineral_types: Vec<GoodId> = (0..n_min)
        .map(|i| GoodId(format!("mineral_{}", rng.next_below(64) + i as u64)))
        .collect();
    let deposits: Vec<ResourceDeposit> = mineral_types
        .iter()
        .map(|g| ResourceDeposit {
            good_id: g.clone(),
            location: (
                Fixed((rng.next_below(1000) as i64) - 500),
                Fixed((rng.next_below(1000) as i64) - 500),
            ),
            richness: rng.next_below(101) as u8,
            accessibility: rng.next_below(101) as u8,
        })
        .collect();
    let resources = PlanetResources {
        mineral_richness: rng.next_below(101) as u8,
        mineral_types,
        organic_richness: if water > 40 { rng.next_below(80) as u8 } else { 0 },
        energy_potential: rng.next_below(101) as u8,
        rare_element_presence: rng.next_below(50) as u8,
        resource_map: ResourceMap { deposits },
    };

    // Settlements: capital + satellites, scaled by habitability.
    let dominant = faction_map
        .keys()
        .next()
        .cloned()
        .unwrap_or(FactionId("independent".into()));
    let n_settle = if habitability.human_compatible {
        1 + rng.next_below(3) as usize
    } else {
        1
    };
    let mut settlements = Vec::with_capacity(n_settle);
    for i in 0..n_settle {
        let stype = if i == 0 {
            if habitability.human_compatible {
                SettlementType::City
            } else {
                SettlementType::Outpost
            }
        } else {
            SettlementType::Colony
        };
        let population = if stype == SettlementType::City {
            100_000 + rng.next_below(900_000)
        } else {
            500 + rng.next_below(20_000)
        };
        let starport_size = (population.ilog10() as u8).saturating_sub(2).max(1);
        settlements.push(Settlement {
            settlement_id: format!("set-{}-{}", seed % 1000, i),
            name: format!(
                "{} {}",
                planet_name(&mut rng),
                if i == 0 { "Prime" } else { "Station" }
            ),
            settlement_type: stype,
            population,
            founded_tick: rng.next_below(1_000_000),
            founding_faction: dominant.clone(),
            founding_wave: if i == 0 {
                SettlementWave::FirstWave
            } else {
                SettlementWave::FrontierOutpost
            },
            districts: vec![
                District {
                    district_type: DistrictType::Starport,
                    name: "Starport".into(),
                    notable_locations: vec!["Landing Pad".into()],
                },
                District {
                    district_type: DistrictType::Commercial,
                    name: "Market".into(),
                    notable_locations: vec!["Trade Hall".into()],
                },
            ],
            starport_size,
            economic_focus: if resources.mineral_richness > 50 {
                EconomicFocus::Mining
            } else {
                EconomicFocus::Trade
            },
        });
    }

    let culture = crate::generator::culture::generate_culture(
        seed ^ 0x5151,
        habitability_index,
        &habitability.hazards,
        &dominant,
        settlements[0].founding_wave,
        faction_map,
        system.threat_level,
    );

    PlanetExtended {
        planet_id: format!("planet-{}", seed % 100_000),
        name: planet_name(&mut rng),
        planet,
        physical: PlanetPhysical {
            diameter_km,
            gravity_g,
            atmosphere,
            atmosphere_density,
            surface_water_pct: water,
            tectonic_activity: tectonic,
            magnetic_field: magnetic,
        },
        climate,
        habitability,
        resources,
        settlements,
        culture,
        ecosystem_id: format!("eco-{}", seed % 100_000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sys() -> SystemParams {
        SystemParams {
            kind: "frontier".into(),
            threat_level: 30,
        }
    }

    fn fmap() -> FactionMap {
        let mut m = HashMap::new();
        m.insert(FactionId("compact".into()), 120);
        m
    }

    #[test]
    fn deterministic_generation() {
        let a = generate_planet_extended(4242, Biome::Frontier, 100, &sys(), &fmap());
        let b = generate_planet_extended(4242, Biome::Frontier, 100, &sys(), &fmap());
        assert_eq!(a, b);
    }

    #[test]
    fn wraps_s04_planet() {
        let p = generate_planet_extended(7, Biome::Nebula, 80, &sys(), &fmap());
        assert_eq!(p.planet.surface.width, 64);
        assert!(!p.settlements.is_empty());
    }

    #[test]
    fn coherence_hazard_drives_suit() {
        let p = generate_planet_extended(99, Biome::Core, 100, &sys(), &fmap());
        if p.habitability.requires_suit {
            assert!(p.habitability.hazards.contains(&Hazard::ToxicAtmosphere));
        }
    }

    #[test]
    fn starport_scales_with_population() {
        let p = generate_planet_extended(11, Biome::Frontier, 100, &sys(), &fmap());
        let cap = &p.settlements[0];
        assert!(
            cap.starport_size >= (cap.population.ilog10() as u8).saturating_sub(2).max(1) - 1
        );
    }
}
