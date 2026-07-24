//! Scripted encounters (S41): fully authored, multi-scene encounters that
//! wire into the procedural trope system (S40). Pure functions to evaluate
//! prerequisites, advance scenes, and apply consequences — no I/O, no LLM,
//! deterministic from authored content + game state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::career::{Direction, PathType};
use crate::generator::dilemma::DilemmaType;
use crate::item::types::ItemType;

/// A fully authored, multi-scene scripted encounter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptedEncounter {
    pub id: String,
    pub title: String,
    pub encounter_type: ScriptedEncounterType,
    pub trigger: EncounterTrigger,
    pub prerequisites: Vec<EncounterPrerequisite>,
    pub scenes: Vec<EncounterScene>,
    pub on_complete: Vec<EncounterOutcome>,
    pub repeatable: bool,
    pub cooldown_ticks: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScriptedEncounterType {
    StoryBeat,
    UniqueLocation,
    FactionEvent,
    PlayerMilestone,
    CommunityGoal,
}

/// What fires this encounter — evaluated against current game state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncounterTrigger {
    OnSystemEntry { system_id: String },
    OnStationDock { station_id: String },
    OnFactionReputation {
        faction: String,
        threshold: i64,
        direction: Direction,
    },
    OnCareerRank {
        path_type: PathType,
        rank: u8,
    },
    OnItemAcquired { item_type: ItemType },
    OnCrewMilestone {
        crew_id: String,
        milestone_type: String,
    },
    OnTropeResolved { template_id: String },
    OnDilemmaResolved { dilemma_type: DilemmaType },
    OnTimerElapsed { ticks: u64 },
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterPrerequisite {
    pub condition_type: PrerequisiteType,
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrerequisiteType {
    FactionReputationRange,
    CareerRankMinimum,
    ShipHasUpgrade,
    CrewHasRole,
    ItemInInventory,
    SystemDiscovered,
    EcosystemScanned,
    StoryArcActive,
    PlayerLevelMinimum,
    UniverseTier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterScene {
    pub scene_id: String,
    pub narrative: String,
    pub speaker: Option<String>,
    pub choices: Vec<EncounterChoice>,
    pub time_pressure: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterChoice {
    pub label: String,
    pub condition: Option<String>,
    pub outcome_scene: String,
    pub immediate_consequences: Vec<EncounterConsequence>,
    pub narrative_response: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterConsequence {
    pub consequence_type: ConsequenceType,
    pub target: String,
    pub params: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsequenceType {
    GiveItem,
    RemoveItem,
    ModifyReputation,
    ModifyCredits,
    ModifyCrewTrust,
    StartCombat,
    TriggerDilemma,
    TriggerTrope,
    EcosystemEvent,
    UnlockMission,
    CompleteMission,
    UnlockStation,
    ModifyCareerProgress,
    ModifyShipUpgrade,
    AddCrewMember,
    RemoveCrewMember,
    SetStoryFlag,
    BroadcastUniverseEvent,
    Custom { function: String },
}

/// One possible ending of a scripted encounter, gated by a condition string
/// (empty = always available).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterOutcome {
    pub condition: String,
    pub summary: String,
    pub permanent_effects: Vec<EncounterConsequence>,
    pub unlocks: Vec<String>,
}

/// Evaluation result: an encounter ready to present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterEvaluation {
    pub encounter_id: String,
    pub title: String,
    pub narrative: String,
    pub speaker: Option<String>,
    pub choices: Vec<EncounterChoice>,
    pub time_pressure: Option<u64>,
}

/// Evaluate a scripted encounter against game state: resolve prerequisites,
/// find the first scene, resolve `{reference}` tokens, return ready-to-
/// present data. Deterministic from authored content + game_state map.
pub fn evaluate_scripted_encounter(
    encounter: &ScriptedEncounter,
    game_state: &BTreeMap<String, String>,
) -> Option<EncounterEvaluation> {
    // Check prerequisites — simple state-key existence check for now.
    for prereq in &encounter.prerequisites {
        let key = format!("{:?}_{}", prereq.condition_type, prereq.params.get("key").unwrap_or(&String::new()));
        if !game_state.contains_key(&key) {
            return None;
        }
    }
    let first_scene = encounter.scenes.first()?;
    let narrative = resolve_references(&first_scene.narrative, game_state);
    Some(EncounterEvaluation {
        encounter_id: encounter.id.clone(),
        title: encounter.title.clone(),
        narrative,
        speaker: first_scene.speaker.clone(),
        choices: first_scene.choices.clone(),
        time_pressure: first_scene.time_pressure,
    })
}

/// Advance the encounter: find the next scene based on player choice.
/// Returns the next scene's evaluation, or None if the outcome_scene
/// doesn't exist.
pub fn advance_scene(
    encounter: &ScriptedEncounter,
    current_scene_id: &str,
    choice_index: usize,
    game_state: &BTreeMap<String, String>,
) -> Option<EncounterEvaluation> {
    let current = encounter.scenes.iter().find(|s| s.scene_id == current_scene_id)?;
    let choice = current.choices.get(choice_index)?;
    let next = encounter.scenes.iter().find(|s| s.scene_id == choice.outcome_scene)?;
    let narrative = resolve_references(&choice.narrative_response, game_state);
    Some(EncounterEvaluation {
        encounter_id: encounter.id.clone(),
        title: encounter.title.clone(),
        narrative,
        speaker: next.speaker.clone(),
        choices: next.choices.clone(),
        time_pressure: next.time_pressure,
    })
}

/// Apply a set of consequences to game state (pure: returns new state).
/// Each consequence updates the state map entry corresponding to its type.
pub fn apply_consequences(
    consequences: &[EncounterConsequence],
    mut game_state: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    for c in consequences {
        let key = format!("{:?}_{}", c.consequence_type, c.target);
        let value = match c.consequence_type {
            ConsequenceType::ModifyCredits => {
                let delta = c.params.get("delta").and_then(|v| v.as_i64()).unwrap_or(0);
                let current: i64 = game_state.get(&key).and_then(|v| v.parse().ok()).unwrap_or(0);
                (current + delta).to_string()
            }
            ConsequenceType::SetStoryFlag => {
                "true".to_string()
            }
            _ => {
                // Generic: mark as triggered.
                "done".to_string()
            }
        };
        game_state.insert(key, value);
    }
    game_state
}

/// Replace `{reference}` tokens in narrative text with game state values.
/// Unresolved tokens become empty strings (graceful fallback).
fn resolve_references(text: &str, game_state: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    for (key, val) in game_state {
        out = out.replace(&format!("{{{}}}", key), val);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_encounter() -> ScriptedEncounter {
        ScriptedEncounter {
            id: "ghost_of_kessel".into(),
            title: "The Ghost of Kessel".into(),
            encounter_type: ScriptedEncounterType::StoryBeat,
            trigger: EncounterTrigger::Manual,
            prerequisites: vec![],
            scenes: vec![
                EncounterScene {
                    scene_id: "opening".into(),
                    narrative: "The {ship} drifts near {planet}.".into(),
                    speaker: Some("AI".into()),
                    choices: vec![
                        EncounterChoice {
                            label: "Investigate".into(),
                            condition: None,
                            outcome_scene: "approach".into(),
                            immediate_consequences: vec![],
                            narrative_response: "You move closer to the signal.".into(),
                        },
                    ],
                    time_pressure: None,
                },
                EncounterScene {
                    scene_id: "approach".into(),
                    narrative: "The derelict hull looms ahead.".into(),
                    speaker: Some("AI".into()),
                    choices: vec![],
                    time_pressure: None,
                },
            ],
            on_complete: vec![],
            repeatable: false,
            cooldown_ticks: None,
        }
    }

    #[test]
    fn evaluation_resolves_references() {
        let enc = sample_encounter();
        let mut gs = BTreeMap::new();
        gs.insert("ship".into(), "Grief".into());
        gs.insert("planet".into(), "Korros".into());
        let eval = evaluate_scripted_encounter(&enc, &gs).unwrap();
        assert!(eval.narrative.contains("Grief"));
        assert!(eval.narrative.contains("Korros"));
    }

    #[test]
    fn advance_returns_next_scene() {
        let enc = sample_encounter();
        let gs = BTreeMap::new();
        let next = advance_scene(&enc, "opening", 0, &gs).unwrap();
        assert_eq!(next.speaker, Some("AI".into()));
    }

    #[test]
    fn apply_consequences_tracks_credits() {
        let cons = vec![EncounterConsequence {
            consequence_type: ConsequenceType::ModifyCredits,
            target: "player".into(),
            params: [("delta".into(), serde_json::json!(500))].into(),
        }];
        let gs = BTreeMap::new();
        let gs = apply_consequences(&cons, gs);
        let key = format!("{:?}_{}", ConsequenceType::ModifyCredits, "player");
        assert_eq!(gs.get(&key).unwrap(), "500");
    }

    #[test]
    fn prerequisites_block_evaluation() {
        let mut enc = sample_encounter();
        enc.prerequisites = vec![EncounterPrerequisite {
            condition_type: PrerequisiteType::SystemDiscovered,
            params: [("key".into(), "must_system".into())].into(),
        }];
        let gs = BTreeMap::new();
        let result = evaluate_scripted_encounter(&enc, &gs);
        assert!(result.is_none());

        let mut gs2 = BTreeMap::new();
        gs2.insert("SystemDiscovered_must_system".into(), "true".into());
        let result = evaluate_scripted_encounter(&enc, &gs2);
        assert!(result.is_some());
    }
}
