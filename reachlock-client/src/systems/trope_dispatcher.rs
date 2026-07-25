use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;

use reachlock_core::generator::trope::{
    instantiate_trope, LocationType, TropeInstance, TropeTemplate,
};

use crate::settings::{InputAction, Settings};
use crate::states::CurrentLocation;
use crate::systems::dispatch::stash;
use crate::systems::interaction::ActivePanel;

#[derive(Resource, Default)]
pub struct TropeRegistry(pub HashMap<String, TropeTemplate>);

#[derive(Resource, Default)]
pub struct TropeCooldown {
    pub remaining_entries: u32,
}

#[derive(Resource, Default)]
pub struct ActiveTropePopup(pub Option<TropeInstance>);

pub fn init_trope_registry(mut registry: ResMut<TropeRegistry>) {
    let tropes = stash::take_tropes();
    for t in tropes {
        registry.0.insert(t.id.clone(), t);
    }
}

pub fn trope_dispatcher_system(
    registry: Res<TropeRegistry>,
    mut cooldown: ResMut<TropeCooldown>,
    mut popup: ResMut<ActiveTropePopup>,
    mut panel: ResMut<ActivePanel>,
    location: Res<CurrentLocation>,
) {
    if popup.0.is_some() {
        return;
    }
    if cooldown.remaining_entries > 0 {
        cooldown.remaining_entries -= 1;
        return;
    }
    let location_type = LocationType::SystemEntry;
    let threat_level = match location.system_biome {
        reachlock_core::seed::types::Biome::Frontier => 4u8,
        reachlock_core::seed::types::Biome::DeepSpace => 6,
        reachlock_core::seed::types::Biome::Nebula => 5,
        reachlock_core::seed::types::Biome::Core => 2,
        reachlock_core::seed::types::Biome::Derelict => 5,
    };
    let mut game_state: BTreeMap<String, Vec<String>> = BTreeMap::new();
    game_state.insert("factions".into(), vec!["compact".into()]);
    game_state.insert("species".into(), vec!["human".into()]);
    game_state.insert("planet_names".into(), vec!["unknown".into()]);
    let eligible: Vec<&TropeTemplate> = registry
        .0
        .values()
        .filter(|t| t.location_types.contains(&location_type))
        .filter(|t| threat_level >= t.min_threat_level && threat_level <= t.max_threat_level)
        .collect();
    if eligible.is_empty() {
        cooldown.remaining_entries = 3;
        return;
    }
    let roll_seed = location
        .system_seed
        .wrapping_add(cooldown.remaining_entries as u64);
    let idx = (roll_seed % eligible.len() as u64) as usize;
    if let Some(template) = eligible.get(idx) {
        if roll_seed.wrapping_mul(1103515245) % 1024 >= template.base_frequency.0 as u64 {
            cooldown.remaining_entries = 3;
            return;
        }
        let instance = instantiate_trope(template, roll_seed, &game_state, location_type);
        popup.0 = Some(instance);
        *panel = ActivePanel::TropePopup;
        cooldown.remaining_entries = 3;
    }
}

pub fn trope_input_system(
    mut popup: ResMut<ActiveTropePopup>,
    mut panel: ResMut<ActivePanel>,
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
) {
    if popup.0.is_none() {
        return;
    }
    if keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::Enter)
        || keys.just_pressed(settings.key(InputAction::Interact))
    {
        popup.0 = None;
        *panel = ActivePanel::None;
    }
}

pub fn trope_panel_text(popup: Res<ActiveTropePopup>) -> Option<String> {
    let instance = popup.0.as_ref()?;
    let mut s = format!("══ {} ══\n{}\n\n", instance.title, instance.narrative);
    for branch in &instance.branches {
        s.push_str(&format!("  • {}\n", branch.label));
    }
    if instance.branches.is_empty() {
        s.push_str("\n[Space/Enter] Dismiss");
    } else {
        s.push_str("\n[Space/Enter] Dismiss and skip branches");
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_no_crash() {
        let reg = TropeRegistry(HashMap::new());
        assert!(reg.0.is_empty());
    }
}
