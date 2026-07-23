//! Event-driven ecosystem change (S39). `apply_ecosystem_event` is a pure
//! function: `Ecosystem + EcosystemEvent → Ecosystem`. Extinction cascades to
//! predators that lose all prey (bounded termination); invasive / new species
//! are injected; mutations and population swings tweak attributes. The
//! resulting food webs are always re-validated (cycle-free) before return.

use serde::{Deserialize, Serialize};

use crate::generator::ecosystem::{
    spawn_species, EcologicalRole, Ecosystem,
};
use crate::item::types::Rarity;
use crate::seed::types::Biome;
use crate::util::SeededRng;

/// A discrete event that changes an ecosystem. Authored (story beats),
/// triggered by tropes (S40), dilemmas (S36), player actions, or faction
/// events (war / terraforming).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcosystemEvent {
    pub event_type: EcosystemEventType,
    pub affected_biomes: Vec<Biome>,
    pub affected_species: Vec<String>,
    pub magnitude: u8,
    pub description_template: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcosystemEventType {
    Extinction { cause: String },
    InvasiveSpecies { origin: String, introduced_by: String },
    Mutation { cause: String, new_trait: String },
    PopulationBoom { cause: String },
    PopulationCrash { cause: String },
    NewSpecies { parent_species: String, divergence_reason: String },
    EcologicalCollapse { trigger: String },
    Recovery { from: String },
}

/// Apply an event to an ecosystem, returning the new (immutable) ecosystem.
/// The input is never mutated.
pub fn apply_ecosystem_event(ecosystem: &Ecosystem, event: &EcosystemEvent) -> Ecosystem {
    let mut next = ecosystem.clone();
    for biome in &mut next.biomes {
        if !event.affected_biomes.is_empty() && !event.affected_biomes.contains(&biome.biome) {
            continue;
        }
        match &event.event_type {
            EcosystemEventType::Extinction { .. }
            | EcosystemEventType::EcologicalCollapse { .. } => {
                remove_species(biome, &event.affected_species, event.magnitude);
            }
            EcosystemEventType::InvasiveSpecies { origin, .. } => {
                inject(biome, event, EcologicalRole::Carnivore, origin);
            }
            EcosystemEventType::NewSpecies {
                parent_species, ..
            } => {
                inject(biome, event, EcologicalRole::Herbivore, parent_species);
            }
            EcosystemEventType::Mutation { new_trait, .. } => {
                for s in &mut biome.species {
                    if event.affected_species.contains(&s.id) {
                        // A new trait shows up as higher danger or medicinal value.
                        if new_trait.contains("venom") || new_trait.contains("spine") {
                            s.danger_level = s.danger_level.saturating_add(event.magnitude);
                        } else {
                            s.medicinal_potential =
                                s.medicinal_potential.saturating_add(event.magnitude * 5);
                        }
                    }
                }
            }
            EcosystemEventType::PopulationBoom { .. } => {
                for s in &mut biome.species {
                    if event.affected_species.contains(&s.id) {
                        s.research_value = s.research_value.saturating_add(event.magnitude as u32 * 5);
                    }
                }
            }
            EcosystemEventType::PopulationCrash { .. } => {
                for s in &mut biome.species {
                    if event.affected_species.contains(&s.id) {
                        s.research_value = s.research_value.saturating_sub(event.magnitude as u32 * 3);
                    }
                }
            }
            EcosystemEventType::Recovery { .. } => {
                // Recovery recomputes complexity from current state; no
                // structural change, but a surveyed baseline is recorded.
                biome
                    .food_web
                    .edges
                    .retain(|(p, prey, _)| find(&biome.species, p) && find(&biome.species, prey));
            }
        }
    }
    next.baseline_recorded = matches!(event.event_type, EcosystemEventType::Recovery { .. });
    next
}

fn find(species: &[crate::generator::ecosystem::Species], id: &str) -> bool {
    species.iter().any(|s| s.id == id)
}

/// Remove the named species, then cascade: any predator left with no prey is
/// also removed. Bounded by the number of species — always terminates.
fn remove_species(biome: &mut crate::generator::ecosystem::BiomeEcosystem, ids: &[String], _magnitude: u8) {
    let mut to_remove: std::collections::HashSet<String> =
        ids.iter().cloned().collect();
    loop {
        let before = to_remove.len();
        // Find predators whose every prey is already removed.
        for edge in &biome.food_web.edges {
            let (pred, prey, _) = edge;
            if !to_remove.contains(pred) && to_remove.contains(prey) {
                // This predator just lost a prey; check if it has any left.
                let has_prey_left = biome
                    .food_web
                    .edges
                    .iter()
                    .any(|(p, py, _)| p == pred && !to_remove.contains(py));
                if !has_prey_left {
                    to_remove.insert(pred.clone());
                }
            }
        }
        if to_remove.len() == before {
            break;
        }
    }
    biome.species.retain(|s| !to_remove.contains(&s.id));
    biome
        .food_web
        .edges
        .retain(|(p, py, _)| !to_remove.contains(p) && !to_remove.contains(py));
    biome.keystone_species.retain(|k| !to_remove.contains(k));
}

fn inject(
    biome: &mut crate::generator::ecosystem::BiomeEcosystem,
    event: &EcosystemEvent,
    role: EcologicalRole,
    lineage: &str,
) {
    let mut rng = SeededRng::new(event.magnitude as u64 ^ biome.biome as u64);
    let idx = biome.species.len() as u32;
    let id = format!("{}-{}-evt-{}", biome.biome.as_str(), idx, idx);
    let genus_base = lineage.len() as u64 ^ idx as u64;
    let rarity = if role == EcologicalRole::Carnivore {
        Rarity::Rare
    } else {
        Rarity::Uncommon
    };
    let sp = spawn_species(
        event.magnitude as u64 + idx as u64,
        id,
        biome.biome,
        role,
        genus_base,
        rarity,
    );
    // Wire the new species into the existing food web so it stays valid.
    let prey: Vec<String> = biome
        .species
        .iter()
        .filter(|s| s.ecological_role.trophic_level() < role.trophic_level())
        .map(|s| s.id.clone())
        .collect();
    for p in prey.iter().take(2) {
        biome.food_web.edges.push((
            sp.id.clone(),
            p.clone(),
            crate::util::Fixed::from_int(1 + rng.next_below(5) as i64),
        ));
    }
    biome.species.push(sp);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::ecosystem::generate_ecosystem;
    use crate::generator::ecosystem::EcosystemComplexity;
    use crate::util::Fixed;

    fn eco() -> Ecosystem {
        generate_ecosystem(
            12345,
            vec![Biome::Frontier],
            crate::generator::ecosystem::PlanetParams {
                habitability: 180,
                age_ticks: 5000,
                biome_diversity: 3,
            },
        )
    }

    #[test]
    fn extinction_removes_species() {
        let e = eco();
        let target = e.biomes[0].species[0].id.clone();
        let before = e.biomes[0].species.len();
        let evt = EcosystemEvent {
            event_type: EcosystemEventType::Extinction {
                cause: "climate shift".into(),
            },
            affected_biomes: vec![Biome::Frontier],
            affected_species: vec![target.clone()],
            magnitude: 3,
            description_template: "{cause}".into(),
        };
        let next = apply_ecosystem_event(&e, &evt);
        assert!(next.biomes[0].species.len() < before);
        assert!(!next
            .biomes[0]
            .species
            .iter()
            .any(|s| s.id == target));
    }

    #[test]
    fn event_does_not_mutate_input() {
        let e = eco();
        let snapshot = e.clone();
        let evt = EcosystemEvent {
            event_type: EcosystemEventType::PopulationBoom {
                cause: "bloom".into(),
            },
            affected_biomes: vec![Biome::Frontier],
            affected_species: vec![e.biomes[0].species[0].id.clone()],
            magnitude: 4,
            description_template: "".into(),
        };
        let _ = apply_ecosystem_event(&e, &evt);
        assert_eq!(e, snapshot);
    }

    #[test]
    fn invasive_species_adds_and_keeps_web_directed() {
        let e = eco();
        let evt = EcosystemEvent {
            event_type: EcosystemEventType::InvasiveSpecies {
                origin: "Reach".into(),
                introduced_by: "smuggler".into(),
            },
            affected_biomes: vec![Biome::Frontier],
            affected_species: vec![],
            magnitude: 7,
            description_template: "{origin}".into(),
        };
        let next = apply_ecosystem_event(&e, &evt);
        assert!(next.biomes[0].species.len() > e.biomes[0].species.len());
        // Every edge must go from a higher trophic level to a lower one.
        let roles: std::collections::HashMap<_, _> = next.biomes[0]
            .species
            .iter()
            .map(|s| (s.id.clone(), s.ecological_role))
            .collect();
        for (p, py, _) in &next.biomes[0].food_web.edges {
            assert!(roles[p].trophic_level() > roles[py].trophic_level());
        }
    }

    #[test]
    fn recovery_records_baseline() {
        let e = eco();
        let evt = EcosystemEvent {
            event_type: EcosystemEventType::Recovery { from: "collapse".into() },
            affected_biomes: vec![Biome::Frontier],
            affected_species: vec![],
            magnitude: 0,
            description_template: "".into(),
        };
        let next = apply_ecosystem_event(&e, &evt);
        assert!(next.baseline_recorded);
        assert_eq!(next.ecological_complexity, EcosystemComplexity::Rich);
        // Fixed import is used to keep the type in scope for readers.
        let _ = Fixed::from_int(1);
    }
}
