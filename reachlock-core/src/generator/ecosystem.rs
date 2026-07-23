//! Procedural ecosystem & life generator (S39). Pure function:
//! `PlanetParams + Biomes + Seed → Ecosystem`. No LLM, no I/O — deterministic
//! from input state. Every habitable planet gets a taxonomy tree, ecological
//! roles, a (cycle-free) food web, procedural names/visuals, and an
//! event-driven change layer (see `ecosystem_events.rs`).

use serde::{Deserialize, Serialize};

use crate::editor::exterior::SizeClass;
use crate::item::types::Rarity;
use crate::seed::types::Biome;
use crate::util::color::{hsv, ColorRgba8};
use crate::util::{Fixed, SeededRng};

/// Planet-level parameters that drive species richness. `generate_planet` in
/// S04 produces the physical planet; this is the slice of it the ecosystem
/// generator needs (kept separate so the two generators stay decoupled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanetParams {
    /// 0–255 habitability index.
    pub habitability: u8,
    /// Planetary age in ticks — older worlds have deeper food webs.
    pub age_ticks: u64,
    /// Count of distinct biomes on the planet (drives diversity).
    pub biome_diversity: u8,
}

/// A whole planet's living system: one `BiomeEcosystem` per biome plus global
/// tallies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ecosystem {
    pub planet_seed: u64,
    pub biomes: Vec<BiomeEcosystem>,
    pub global_species_count: u32,
    pub endemic_species_count: u32,
    pub ecological_complexity: EcosystemComplexity,
    pub baseline_recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BiomeEcosystem {
    pub biome: Biome,
    pub species: Vec<Species>,
    pub food_web: FoodWeb,
    pub keystone_species: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcosystemComplexity {
    Barren,
    Simple,
    Developed,
    Rich,
    Verdant,
}

/// Ecological role — defines trophic level and (roughly) what a species eats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcologicalRole {
    PrimaryProducer,
    Herbivore,
    Carnivore,
    Omnivore,
    Decomposer,
    Parasite,
    Symbiote,
    FilterFeeder,
    Scavenger,
}

impl EcologicalRole {
    /// Integer trophic level. Strictly increasing levels guarantee the food
    /// web is a DAG (we only ever connect a higher level to a lower one).
    pub(crate) fn trophic_level(self) -> u8 {
        match self {
            EcologicalRole::PrimaryProducer => 0,
            EcologicalRole::Decomposer => 0,
            EcologicalRole::FilterFeeder => 1,
            EcologicalRole::Herbivore => 1,
            EcologicalRole::Symbiote => 1,
            EcologicalRole::Omnivore => 2,
            EcologicalRole::Scavenger => 2,
            EcologicalRole::Parasite => 3,
            EcologicalRole::Carnivore => 4,
        }
    }

    fn is_producer(self) -> bool {
        matches!(self, EcologicalRole::PrimaryProducer | EcologicalRole::Decomposer)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Species {
    pub id: String,
    pub taxonomy: Taxonomy,
    pub common_name: String,
    pub scientific_name: String,
    pub ecological_role: EcologicalRole,
    pub size_class: SizeClass,
    pub habitat: String,
    pub rarity: Rarity,
    pub visual: SpeciesVisual,
    pub discoverable: bool,
    pub research_value: u32,
    pub edibility: Edibility,
    pub medicinal_potential: u8,
    pub danger_level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Taxonomy {
    pub kingdom: String,
    pub phylum: String,
    pub class: String,
    pub order: String,
    pub family: String,
    pub genus: String,
    pub species: String,
}

/// Cycle-free predation graph. Edges are `(predator_id, prey_id, strength)`
/// with `strength` in fixed-point (1/1024) — never a float (iron rule #2), so
/// the determinism manifest hashes it byte-identically on every target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoodWeb {
    pub edges: Vec<(String, String, Fixed)>,
}

impl FoodWeb {
    /// Every non-producer must appear as a predator with at least one prey,
    /// and no edge may point from a lower trophic level to a higher one.
    pub fn is_valid(&self, roles: &std::collections::HashMap<String, EcologicalRole>) -> bool {
        use std::collections::HashSet;
        let mut predators_with_prey = HashSet::new();
        for (pred, prey, _) in &self.edges {
            if let (Some(&pr), Some(&py)) = (roles.get(pred), roles.get(prey)) {
                if pr.trophic_level() <= py.trophic_level() {
                    return false;
                }
            } else {
                return false;
            }
            predators_with_prey.insert(pred.clone());
        }
        // Every non-producer must eat something.
        for (id, role) in roles {
            if !role.is_producer() && !predators_with_prey.contains(id) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyPlan {
    Radial,
    Bilateral,
    Asymmetric,
    Colonial,
    Amorphous,
    Segmented,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeciesVisual {
    pub silhouette: u8,
    pub primary_color: ColorRgba8,
    pub secondary_color: ColorRgba8,
    pub body_plan: BodyPlan,
    /// Free-text size hint (e.g. "fist-sized"…"building-sized") — render only.
    pub size_hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edibility {
    Toxic,
    Inedible,
    Edible { nutrition_value: u32 },
    Delicacy { nutrition_value: u32, market_value: u32 },
}

// --------------------------------------------------------------------------
// Name generation
// --------------------------------------------------------------------------

const ADJECTIVES: &[&str] = &[
    "azure", "crimson", "hollow", "gilded", "pale", "iron", "verdant", "sable", "luminous",
    "thorned", "silent", "drifting", "ember", "frost", "mottled", "song", "glass", "amber",
    "coiled", "veiled",
];

const NOUNS: &[&str] = &[
    "lurker", "bloom", "crawler", "wing", "maw", "husk", "spire", "fin", "thresher", "mote",
    "creeper", "lantern", "drifter", "shell", "biter", "web", "horn", "tendril", "gleam", "root",
];

const SYLLABLES: &[&str] = &[
    "xa", "trin", "vore", "lyx", "qel", "mos", "an", "thu", "gre", "spi", "no", "vax", "ul",
    "cor", "phen", "zy", "dra", "kel", "oph", "myr",
];

pub(crate) fn pick<'a>(rng: &mut SeededRng, list: &'a [&'a str]) -> &'a str {
    list[rng.next_below(list.len() as u64) as usize]
}

pub(crate) fn common_name(rng: &mut SeededRng) -> String {
    format!("{} {}", pick(rng, ADJECTIVES), pick(rng, NOUNS))
}

/// Pseudo-Latin genus/species from seed. The genus is seeded from a per-biome
/// base so same-genus species (same `genus` string) share syllable ancestry.
pub(crate) fn scientific_name(rng: &mut SeededRng, genus_base: u64) -> (String, String) {
    let mut grng = SeededRng::new(genus_base ^ rng.next_u64());
    let genus = format!(
        "{}{}",
        pick(&mut grng, SYLLABLES),
        pick(&mut grng, SYLLABLES)
    );
    let species = format!(
        "{}{}",
        pick(rng, SYLLABLES),
        pick(rng, SYLLABLES)
    );
    (genus.clone(), species)
}

// --------------------------------------------------------------------------
// Generation
// --------------------------------------------------------------------------

fn body_plan(rng: &mut SeededRng) -> BodyPlan {
    const ALL: [BodyPlan; 6] = [
        BodyPlan::Radial,
        BodyPlan::Bilateral,
        BodyPlan::Asymmetric,
        BodyPlan::Colonial,
        BodyPlan::Amorphous,
        BodyPlan::Segmented,
    ];
    ALL[rng.next_below(ALL.len() as u64) as usize]
}

fn size_hint(rng: &mut SeededRng) -> String {
    const HINTS: &[&str] = &[
        "fist-sized",
        "boot-sized",
        "dog-sized",
        "human-sized",
        "cart-sized",
        "building-sized",
    ];
    HINTS[rng.next_below(HINTS.len() as u64) as usize].to_string()
}

/// Deterministic species visual. Colors come from integer HSV so the same
/// seed always yields the same creature.
pub fn generate_species_visual(seed: u64, body_plan: BodyPlan) -> SpeciesVisual {
    let mut rng = SeededRng::new(seed);
    let hue = rng.next_below(1536) as u32;
    let accent_hue = (hue + 256 + rng.next_below(256) as u32) % 1536;
    SpeciesVisual {
        silhouette: rng.next_below(256) as u8,
        primary_color: hsv(hue, 160 + rng.next_below(80) as u32, 200),
        secondary_color: hsv(accent_hue, 180, 230),
        body_plan,
        size_hint: size_hint(&mut rng),
    }
}

/// Build a single species (used by event application to inject invasive /
/// mutated / new species). Deterministic from `seed` + `id` + `role`.
pub(crate) fn spawn_species(
    seed: u64,
    id: String,
    biome: Biome,
    role: EcologicalRole,
    genus_base: u64,
    rarity: Rarity,
) -> Species {
    let mut rng = SeededRng::new(seed);
    let bp = body_plan(&mut rng);
    let (genus, sp) = scientific_name(&mut rng, genus_base);
    let visual = generate_species_visual(seed, bp);
    let size_class = match bp {
        BodyPlan::Amorphous | BodyPlan::Radial => SizeClass::Small,
        BodyPlan::Bilateral | BodyPlan::Segmented => SizeClass::Medium,
        _ => SizeClass::Large,
    };
    let common = common_name(&mut rng);
    Species {
        id,
        taxonomy: Taxonomy {
            kingdom: "Animalia".to_string(),
            phylum: format!("{}a", pick(&mut rng, SYLLABLES)),
            class: format!("{}ia", pick(&mut rng, SYLLABLES)),
            order: format!("{}iformes", pick(&mut rng, SYLLABLES)),
                family: format!("{}idae", genus),
                genus: genus.clone(),
                species: sp.clone(),
            },
            common_name: common,
            scientific_name: format!("{} {}", genus, sp),
        ecological_role: role,
        size_class,
        habitat: biome.as_str().to_string(),
        rarity,
        visual,
        discoverable: true,
        research_value: 5 + rng.next_below(60) as u32,
        edibility: edibility_for(&mut rng, role),
        medicinal_potential: if role.is_producer() {
            rng.next_below(100) as u8
        } else {
            0
        },
        danger_level: if matches!(role, EcologicalRole::Carnivore | EcologicalRole::Parasite) {
            1 + rng.next_below(9) as u8
        } else {
            0
        },
    }
}

/// Role distribution per biome: producers first, then consumers. Number of
/// species scales with habitability, age, and biome diversity.
fn species_count(params: PlanetParams, biome_index: u8) -> u32 {
    let habit = params.habitability as u32;
    let age_factor = (params.age_ticks / 1000).clamp(1, 8) as u32;
    let diversity = (params.biome_diversity as u32).clamp(1, 8);
    let base = (habit / 16 + 2) * age_factor * diversity / 2;
    // Slight per-biome variation, deterministic.
    let jitter = (biome_index as u32 * 7) % 3;
    (base + jitter).clamp(3, 40)
}

fn role_for_index(rng: &mut SeededRng, index: u32, total: u32) -> EcologicalRole {
    // First ~40% are producers, next ~30% herbivores, remainder carnivores/etc.
    let frac = index * 100 / total.max(1);
    match frac {
        0..=39 => EcologicalRole::PrimaryProducer,
        40..=69 => EcologicalRole::Herbivore,
        _ => {
            const CONSUMERS: [EcologicalRole; 6] = [
                EcologicalRole::Carnivore,
                EcologicalRole::Omnivore,
                EcologicalRole::Scavenger,
                EcologicalRole::FilterFeeder,
                EcologicalRole::Parasite,
                EcologicalRole::Symbiote,
            ];
            CONSUMERS[rng.next_below(CONSUMERS.len() as u64) as usize]
        }
    }
}

fn edibility_for(rng: &mut SeededRng, role: EcologicalRole) -> Edibility {
    match role {
        EcologicalRole::PrimaryProducer | EcologicalRole::Herbivore => {
            let roll = rng.next_below(100);
            if roll < 50 {
                Edibility::Edible {
                    nutrition_value: 10 + rng.next_below(90) as u32,
                }
            } else if roll < 65 {
                Edibility::Delicacy {
                    nutrition_value: 30 + rng.next_below(70) as u32,
                    market_value: 50 + rng.next_below(200) as u32,
                }
            } else {
                Edibility::Inedible
            }
        }
        EcologicalRole::Carnivore | EcologicalRole::Parasite => {
            if rng.next_below(100) < 30 {
                Edibility::Toxic
            } else {
                Edibility::Inedible
            }
        }
        _ => Edibility::Inedible,
    }
}

/// Build one biome's species list + food web.
fn generate_biome(
    seed: u64,
    biome: Biome,
    biome_index: u8,
    params: PlanetParams,
) -> BiomeEcosystem {
    let count = species_count(params, biome_index);
    let mut rng = SeededRng::new(seed ^ (biome_index as u64).wrapping_mul(0x9E37_79B9));
    let genus_base = seed ^ (biome as u64).wrapping_mul(0x85EB_CA6B);

    let mut species = Vec::with_capacity(count as usize);
    let mut roles = std::collections::HashMap::new();
    let mut producers = Vec::new();
    let mut used_names = std::collections::HashSet::new();

    for i in 0..count {
        let role = role_for_index(&mut rng, i, count);
        let bp = body_plan(&mut rng);
        let (genus, sp) = scientific_name(&mut rng, genus_base.wrapping_add(i as u64));
        let id = format!("{}-{}-{}", biome.as_str(), biome_index, i);
        let visual = generate_species_visual(seed ^ i as u64, bp);
        let rarity = match i {
            0 => Rarity::Common,
            _ if i < 5 => Rarity::Uncommon,
            _ if i < 12 => Rarity::Rare,
            _ if i < 20 => Rarity::Epic,
            _ => Rarity::Legendary,
        };
        let size_class = match bp {
            BodyPlan::Amorphous | BodyPlan::Radial => SizeClass::Small,
            BodyPlan::Bilateral | BodyPlan::Segmented => SizeClass::Medium,
            _ => SizeClass::Large,
        };
        // Common names must be unique within a biome (brief requirement).
        let mut common = common_name(&mut rng);
        while used_names.contains(&common) {
            common = common_name(&mut rng);
        }
        used_names.insert(common.clone());
        if role.is_producer() {
            producers.push(id.clone());
        }
        roles.insert(id.clone(), role);
        species.push(Species {
            id: id.clone(),
            taxonomy: Taxonomy {
                kingdom: "Animalia".to_string(),
                phylum: format!("{}a", pick(&mut rng, SYLLABLES)),
                class: format!("{}ia", pick(&mut rng, SYLLABLES)),
                order: format!("{}iformes", pick(&mut rng, SYLLABLES)),
                family: format!("{}idae", genus),
                genus: genus.clone(),
                species: sp.clone(),
            },
            common_name: common,
            scientific_name: format!("{} {}", genus, sp),
            ecological_role: role,
            size_class,
            habitat: biome.as_str().to_string(),
            rarity,
            visual,
            discoverable: true,
            research_value: 5 + rng.next_below(60) as u32,
            edibility: edibility_for(&mut rng, role),
            medicinal_potential: if role.is_producer() {
                rng.next_below(100) as u8
            } else {
                0
            },
            danger_level: if matches!(role, EcologicalRole::Carnivore | EcologicalRole::Parasite) {
                1 + rng.next_below(9) as u8
            } else {
                0
            },
        });
    }

    let food_web = build_food_web(&mut rng, &species, &roles, &producers);
    let keystone = pick_keystone(&species, &roles);

    BiomeEcosystem {
        biome,
        species,
        food_web,
        keystone_species: keystone,
    }
}

/// Build a cycle-free food web: every non-producer preys on at least one
/// lower-trophic-level species.
fn build_food_web(
    rng: &mut SeededRng,
    species: &[Species],
    roles: &std::collections::HashMap<String, EcologicalRole>,
    producers: &[String],
) -> FoodWeb {
    let mut edges = Vec::new();
    let consumers: Vec<&Species> = species
        .iter()
        .filter(|s| !s.ecological_role.is_producer())
        .collect();
    for c in &consumers {
        // Prefer prey one trophic level down; fall back to any lower level.
        let ct = c.ecological_role.trophic_level();
        let mut prey_pool: Vec<&String> = species
            .iter()
            .filter(|p| {
                roles
                    .get(&p.id)
                    .map(|r| r.trophic_level() < ct)
                    .unwrap_or(false)
            })
            .map(|p| &p.id)
            .collect();
        if prey_pool.is_empty() {
            prey_pool = producers.iter().collect();
        }
        if prey_pool.is_empty() {
            continue;
        }
        let n = 1 + rng.next_below(3).min(prey_pool.len() as u64) as usize;
        for _ in 0..n {
            let prey = prey_pool[rng.next_below(prey_pool.len() as u64) as usize].clone();
            let strength = Fixed::from_int(1 + rng.next_below(10) as i64);
            edges.push((c.id.clone(), prey.clone(), strength));
        }
    }
    FoodWeb { edges }
}

fn pick_keystone(species: &[Species], roles: &std::collections::HashMap<String, EcologicalRole>) -> Vec<String> {
    // Keystone = a high-trophic carnivore with many prey in the web.
    let mut best: Option<&Species> = None;
    let mut best_deg = 0u32;
    for s in species {
        if !matches!(s.ecological_role, EcologicalRole::Carnivore) {
            continue;
        }
        let deg = roles
            .values()
            .filter(|r| r.trophic_level() < s.ecological_role.trophic_level())
            .count() as u32;
        if deg > best_deg {
            best_deg = deg;
            best = Some(s);
        }
    }
    best.map(|s| vec![s.id.clone()]).unwrap_or_default()
}

/// Generate a full ecosystem for a planet. Pure & deterministic: identical
/// `(planet_seed, biomes, params)` always yields an identical `Ecosystem`.
pub fn generate_ecosystem(
    planet_seed: u64,
    biomes: Vec<Biome>,
    params: PlanetParams,
) -> Ecosystem {
    let mut eco_biomes = Vec::with_capacity(biomes.len());
    for (i, &biome) in biomes.iter().enumerate() {
        eco_biomes.push(generate_biome(planet_seed, biome, i as u8, params));
    }
    let global = eco_biomes.iter().map(|b| b.species.len() as u32).sum();
    let endemic = eco_biomes
        .iter()
        .filter(|b| b.species.len() <= 4)
        .count() as u32;
    let complexity = classify_complexity(global, &eco_biomes);
    Ecosystem {
        planet_seed,
        biomes: eco_biomes,
        global_species_count: global,
        endemic_species_count: endemic,
        ecological_complexity: complexity,
        baseline_recorded: false,
    }
}

fn classify_complexity(global: u32, biomes: &[BiomeEcosystem]) -> EcosystemComplexity {
    let deepest = biomes
        .iter()
        .map(|b| b.food_web.edges.len() as u32)
        .max()
        .unwrap_or(0);
    match (global, deepest) {
        (0, _) => EcosystemComplexity::Barren,
        (1..=8, _) => EcosystemComplexity::Simple,
        (9..=20, _) => EcosystemComplexity::Developed,
        (21..=40, _) => EcosystemComplexity::Rich,
        _ => EcosystemComplexity::Verdant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_params() -> PlanetParams {
        PlanetParams {
            habitability: 180,
            age_ticks: 5000,
            biome_diversity: 4,
        }
    }

    fn sample_biomes() -> Vec<Biome> {
        vec![Biome::Frontier, Biome::Nebula]
    }

    fn roles_of(b: &BiomeEcosystem) -> std::collections::HashMap<String, EcologicalRole> {
        b.species
            .iter()
            .map(|s| (s.id.clone(), s.ecological_role))
            .collect()
    }

    #[test]
    fn deterministic_generation() {
        let a = generate_ecosystem(12345, sample_biomes(), sample_params());
        let b = generate_ecosystem(12345, sample_biomes(), sample_params());
        assert_eq!(a, b);
    }

    #[test]
    fn food_web_is_valid_and_acyclic() {
        let eco = generate_ecosystem(777, sample_biomes(), sample_params());
        for b in &eco.biomes {
            let roles = roles_of(b);
            assert!(
                b.food_web.is_valid(&roles),
                "food web invalid in biome {:?}",
                b.biome
            );
        }
    }

    #[test]
    fn every_non_producer_has_prey() {
        let eco = generate_ecosystem(42, sample_biomes(), sample_params());
        for b in &eco.biomes {
            let predators: std::collections::HashSet<&String> =
                b.food_web.edges.iter().map(|(p, _, _)| p).collect();
            for s in &b.species {
                if !s.ecological_role.is_producer() {
                    assert!(
                        predators.contains(&s.id),
                        "species {} has no prey",
                        s.id
                    );
                }
            }
        }
    }

    #[test]
    fn names_unique_within_biome() {
        let eco = generate_ecosystem(99, sample_biomes(), sample_params());
        for b in &eco.biomes {
            let mut seen = std::collections::HashSet::new();
            for s in &b.species {
                assert!(seen.insert(s.common_name.clone()), "dup name {}", s.common_name);
            }
        }
    }

    #[test]
    fn visual_is_deterministic() {
        assert_eq!(
            generate_species_visual(5, BodyPlan::Radial),
            generate_species_visual(5, BodyPlan::Radial)
        );
    }
}
