use std::collections::HashSet;

use bevy::prelude::*;

use reachlock_core::generator::ecosystem_events::EcosystemEventType;

use crate::states::CurrentLocation;
use crate::systems::contract::ShipLog;
use crate::systems::discovery::EcosystemResource;
use crate::systems::dispatch::EcosystemOverrideRegistry;

/// Tracks which ecosystem events have already been fired per planet to avoid
/// replaying events on every orbit entry.
#[derive(Resource, Default)]
pub struct EcosystemEventTriggerLog {
    pub fired: HashSet<String>,
}

/// Build a human-readable notification string from an event type and a
/// species/planet context. Follows the `description_template` variable
/// substitution pattern from the spec.
fn format_event_notification(
    event_type: &EcosystemEventType,
    species_name: &str,
    planet_name: &str,
) -> String {
    match event_type {
        EcosystemEventType::Extinction { cause } => {
            format!(
                "EXTINCTION — The {species_name} has gone extinct on {planet_name} due to {cause}."
            )
        }
        EcosystemEventType::InvasiveSpecies {
            origin,
            introduced_by,
        } => {
            format!("INVASIVE SPECIES — A species from {origin} has established on {planet_name}, introduced by {introduced_by}.")
        }
        EcosystemEventType::Mutation { cause, new_trait } => {
            format!("MUTATION — The {species_name} on {planet_name} has developed {new_trait} due to {cause}.")
        }
        EcosystemEventType::PopulationBoom { cause } => {
            format!("POPULATION BOOM — The {species_name} population is surging on {planet_name} because of {cause}.")
        }
        EcosystemEventType::PopulationCrash { cause } => {
            format!("POPULATION CRASH — The {species_name} population is collapsing on {planet_name} due to {cause}.")
        }
        EcosystemEventType::NewSpecies {
            parent_species,
            divergence_reason,
        } => {
            format!("NEW SPECIES — A new species diverging from {parent_species} has emerged on {planet_name} ({divergence_reason}).")
        }
        EcosystemEventType::EcologicalCollapse { trigger } => {
            format!("ECOLOGICAL COLLAPSE — The ecosystem on {planet_name} is collapsing after {trigger}.")
        }
        EcosystemEventType::Recovery { from } => {
            format!("RECOVERY — The ecosystem on {planet_name} is recovering from {from}.")
        }
    }
}

/// Run on system arrival (OnEnter SpaceFlight). Looks up ecosystem override
/// data for the current planet, generates events, and fires notifications
/// that haven't been shown yet.
pub fn check_ecosystem_events(
    location: Res<CurrentLocation>,
    eco_registry: Res<EcosystemOverrideRegistry>,
    eco_resource: Res<EcosystemResource>,
    mut trigger_log: ResMut<EcosystemEventTriggerLog>,
    mut log: ResMut<ShipLog>,
) {
    let planet_id = &location.system_id.0;
    let log_key = format!("eco-event/{}", planet_id);
    if trigger_log.fired.contains(&log_key) {
        return;
    }
    trigger_log.fired.insert(log_key.clone());

    // Check for an authored ecosystem override first
    if let Some(eco) = eco_registry.0.get(planet_id) {
        let species_names: Vec<&str> = eco
            .biomes
            .iter()
            .flat_map(|b| b.species.iter())
            .map(|s| s.common_name.as_str())
            .collect();
        if let Some(first_species) = species_names.first() {
            let event_types = [
                EcosystemEventType::Extinction {
                    cause: "mining runoff".into(),
                },
                EcosystemEventType::Mutation {
                    cause: "industrial pollution".into(),
                    new_trait: "venomous spines".into(),
                },
                EcosystemEventType::PopulationBoom {
                    cause: "abundant seasonal rains".into(),
                },
                EcosystemEventType::InvasiveSpecies {
                    origin: "a passing freighter".into(),
                    introduced_by: "stowaway organisms".into(),
                },
            ];
            let idx = (location.system_seed % event_types.len() as u64) as usize;
            let notification =
                format_event_notification(&event_types[idx], first_species, planet_id);
            log.log(notification);
        } else {
            log.log(format!(
                "ECOSYSTEM PROFILE — {}: {} species across {} biome(s), complexity {:?}.",
                planet_id,
                eco.global_species_count,
                eco.biomes.len(),
                eco.ecological_complexity,
            ));
        }
    } else if let Some(eco) = &eco_resource.0 {
        // No override, but the system has a generated ecosystem — log a profile
        log.log(format!(
            "ECOSYSTEM PROFILE — {}: {} species across {} biome(s), complexity {:?}.",
            planet_id,
            eco.global_species_count,
            eco.biomes.len(),
            eco.ecological_complexity,
        ));
    }
}
