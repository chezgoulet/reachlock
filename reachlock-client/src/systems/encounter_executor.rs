use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;

use reachlock_core::generator::scripted_encounter::{
    advance_scene, apply_consequences, evaluate_scripted_encounter, EncounterTrigger,
    ScriptedEncounter,
};

use crate::states::GameMode;
use crate::systems::dispatch::stash;
use crate::systems::interaction::ActivePanel;
use crate::systems::ticker::UniverseTicker;

#[derive(Resource, Default)]
pub struct EncounterRegistry(pub HashMap<String, ScriptedEncounter>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveEncounterData {
    pub encounter_id: String,
    pub current_scene_id: String,
    pub title: String,
    pub narrative: String,
    pub choices: Vec<String>,
}

#[derive(Resource, Default)]
pub struct ActiveEncounter(pub Option<ActiveEncounterData>);

#[derive(Resource, Default)]
pub struct EncounterCooldowns(pub HashMap<String, u64>);

pub fn init_encounter_registry(mut registry: ResMut<EncounterRegistry>) {
    let encounters = stash::take_encounters();
    for e in encounters {
        registry.0.insert(e.id.clone(), e);
    }
}

pub fn encounter_trigger_system(
    registry: Res<EncounterRegistry>,
    mut active: ResMut<ActiveEncounter>,
    mut panel: ResMut<ActivePanel>,
    mode: Res<State<GameMode>>,
    ticker: Res<UniverseTicker>,
    mut cooldowns: ResMut<EncounterCooldowns>,
) {
    if active.0.is_some() {
        return;
    }
    let tick = ticker.state.factions.tick;
    let game_state = build_game_state(&ticker, &mode);
    for (id, encounter) in &registry.0 {
        if !trigger_matches(&encounter.trigger, &mode) {
            continue;
        }
        if !encounter.repeatable {
            if let Some(cooldown_end) = cooldowns.0.get(id) {
                if tick < *cooldown_end {
                    continue;
                }
            }
        }
        if let Some(eval) = evaluate_scripted_encounter(encounter, &game_state) {
            let next_scene_id = encounter
                .scenes
                .first()
                .map(|s| s.scene_id.clone())
                .unwrap_or_default();
            active.0 = Some(ActiveEncounterData {
                encounter_id: eval.encounter_id.clone(),
                current_scene_id: next_scene_id,
                title: eval.title,
                narrative: eval.narrative,
                choices: eval.choices.iter().map(|c| c.label.clone()).collect(),
            });
            if !encounter.repeatable {
                if let Some(ticks) = encounter.cooldown_ticks {
                    cooldowns.0.insert(id.clone(), tick + ticks);
                } else {
                    cooldowns.0.insert(id.clone(), u64::MAX);
                }
            }
            *panel = ActivePanel::Encounter;
            return;
        }
    }
}

pub fn encounter_choice_system(
    mut active: ResMut<ActiveEncounter>,
    registry: Res<EncounterRegistry>,
    ticker: Res<UniverseTicker>,
    mut panel: ResMut<ActivePanel>,
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<State<GameMode>>,
    focus_stack: Res<crate::focus_stack::FocusStack>,
) {
    if focus_stack.top_captures_input() {
        return;
    }
    let Some(ref data) = active.0 else { return };
    let Some(encounter) = registry.0.get(&data.encounter_id) else {
        active.0 = None;
        *panel = ActivePanel::None;
        return;
    };
    let choice_count = data.choices.len().min(9);
    let chosen = (0..choice_count).find(|&i| {
        let digit = match i {
            0 => KeyCode::Digit1,
            1 => KeyCode::Digit2,
            2 => KeyCode::Digit3,
            3 => KeyCode::Digit4,
            4 => KeyCode::Digit5,
            5 => KeyCode::Digit6,
            6 => KeyCode::Digit7,
            7 => KeyCode::Digit8,
            _ => KeyCode::Digit9,
        };
        keys.just_pressed(digit)
    });
    if let Some(choice_idx) = chosen {
        let game_state = build_game_state(&ticker, &mode);
        if let Some(next) =
            advance_scene(encounter, &data.current_scene_id, choice_idx, &game_state)
        {
            let next_scene_id = encounter
                .scenes
                .iter()
                .find(|s| s.narrative == next.narrative)
                .map(|s| s.scene_id.clone())
                .unwrap_or_default();
            let sc = encounter
                .scenes
                .iter()
                .find(|s| s.scene_id == data.current_scene_id);
            if let Some(scene) = sc {
                if let Some(choice) = scene.choices.get(choice_idx) {
                    let _ = apply_consequences(&choice.immediate_consequences, game_state);
                }
            }
            if next.choices.is_empty() {
                active.0 = None;
                *panel = ActivePanel::None;
            } else {
                active.0 = Some(ActiveEncounterData {
                    encounter_id: next.encounter_id,
                    current_scene_id: next_scene_id,
                    title: next.title,
                    narrative: next.narrative,
                    choices: next.choices.iter().map(|c| c.label.clone()).collect(),
                });
            }
        } else {
            active.0 = None;
            *panel = ActivePanel::None;
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        active.0 = None;
        *panel = ActivePanel::None;
    }
}

pub fn encounter_panel_text(active: Res<ActiveEncounter>) -> Option<String> {
    let data = active.0.as_ref()?;
    let mut s = format!("══ {} ══\n{}\n\n", data.title, data.narrative);
    for (i, choice) in data.choices.iter().enumerate() {
        s.push_str(&format!("  {}: {}\n", i + 1, choice));
    }
    if !data.choices.is_empty() {
        s.push_str("\n[1-9] Choose  [Esc] Close");
    } else {
        s.push_str("\n[Esc] Close");
    }
    Some(s)
}

fn trigger_matches(trigger: &EncounterTrigger, mode: &State<GameMode>) -> bool {
    match trigger {
        EncounterTrigger::OnSystemEntry { .. } => **mode == GameMode::SpaceFlight,
        EncounterTrigger::OnStationDock { .. } => {
            matches!(**mode, GameMode::Landed | GameMode::OnBoard)
        }
        EncounterTrigger::Manual => false,
        _ => false,
    }
}

fn build_game_state(ticker: &UniverseTicker, _mode: &State<GameMode>) -> BTreeMap<String, String> {
    let _ = _mode;
    let mut state = BTreeMap::new();
    for faction in &ticker.state.factions.catalog.factions {
        let rep = ticker.state.factions.rep(&faction.id);
        state.insert(
            format!("Reputation_{}", faction.id.as_str()),
            rep.trust.to_string(),
        );
    }
    state.insert("tick".into(), ticker.state.factions.tick.to_string());
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_no_crash() {
        let reg = EncounterRegistry(HashMap::new());
        assert!(reg.0.is_empty());
    }
}
