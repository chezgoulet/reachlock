//! Soul runtime (S13): pure transitions over live soul state. Authored
//! [`SoulFile`]s never change; everything an event moves — mood, memories,
//! relationships, unlocked secrets, fired mutations/breaks — lives in
//! [`SoulState`], a plain serde struct keyed by soul id that the client
//! stores in the save. No Bevy types, no IO (iron rule #1).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::contract::engine::{condition_holds, EvalContext};
use crate::soul::types::{
    BreakReaction, Goal, Memory, Mood, Relationship, SoulChange, SoulFile, SoulMutation,
};

/// Memories kept per soul. Eviction is by emotional weight: the formative
/// stay, the forgettable go (ties: older goes first).
pub const MEMORY_CAP: usize = 48;
/// Mood-shift history kept per soul (a log, not a database).
const MOOD_HISTORY_CAP: usize = 32;

/// Live, save-persisted state of one soul. Built from the authored file by
/// [`SoulState::from_file`]; moved only by [`apply_event`] and
/// [`apply_mutation`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoulState {
    pub soul_id: String,
    pub mood: Mood,
    /// 0 ..= 1024.
    pub intensity: i64,
    /// Log of `(timestamp, mood, intensity)` shifts, newest last.
    #[serde(default)]
    pub mood_history: Vec<(u64, Mood, i64)>,
    /// Live memory tree: authored formative memories plus everything since.
    #[serde(default)]
    pub memories: Vec<Memory>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    /// Live trait list (mutations add/remove; starts from the authored
    /// personality).
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default)]
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub unlocked_secrets: BTreeSet<String>,
    #[serde(default)]
    pub fired_mutations: BTreeSet<String>,
    #[serde(default)]
    pub fired_breaks: BTreeSet<String>,
}

impl SoulState {
    /// Fresh runtime state from an authored soul.
    pub fn from_file(file: &SoulFile) -> Self {
        SoulState {
            soul_id: file.id.clone(),
            mood: file.emotional_state.dominant_mood,
            intensity: file.emotional_state.intensity,
            mood_history: Vec::new(),
            memories: file.memory_tree.clone(),
            relationships: file.relationship_graph.clone(),
            traits: file.personality.traits.clone(),
            goals: file.goals.clone(),
            unlocked_secrets: BTreeSet::new(),
            fired_mutations: BTreeSet::new(),
            fired_breaks: BTreeSet::new(),
        }
    }

    /// The soul's standing toward `target` ("player", or another soul id).
    pub fn relationship(&self, target: &str) -> Option<&Relationship> {
        self.relationships.iter().find(|r| r.target_id == target)
    }
}

/// One thing that happened to (or near) a soul. `fields` carries the game
/// context trigger conditions compare against; `event.<event_type>` is set
/// to 1 automatically so conditions can gate on the event kind itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoulEvent {
    /// "conversation", "combat", "ship_damage", "asked_about_mark", …
    pub event_type: String,
    #[serde(default)]
    pub player_involved: bool,
    /// 0 (forgettable) ..= 1024 (formative). Weight of the recorded memory.
    pub emotional_weight: i64,
    pub timestamp: u64,
    /// For LLM context assembly (S16) and the memory record.
    pub summary: String,
    /// Extra game-state fields for trigger evaluation (fixed-point).
    #[serde(default)]
    pub fields: BTreeMap<String, i64>,
    /// Relationship moves this event carries: `(target_id, trust_delta,
    /// familiarity_delta)`, clamped into range on apply.
    #[serde(default)]
    pub relationship_deltas: Vec<(String, i64, i64)>,
}

/// What [`apply_event`] reports back to the game layer. S13 delivers the
/// event, never the consequence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SoulOutput {
    MoodShift {
        soul_id: String,
        from: Mood,
        to: Mood,
        intensity: i64,
    },
    SecretUnlocked {
        soul_id: String,
        secret_id: String,
    },
    SoulBreak {
        soul_id: String,
        breaking_point_id: String,
        reaction: BreakReaction,
    },
}

/// Expose a soul's live state to the contract engine (spec §15 soul→contract
/// bridge): `mood.<name>` = 1 for the active mood, `mood.intensity`,
/// `trust.<target>` / `familiarity.<target>` per relationship, and
/// `trait.<name>` = 1 per live trait. Authored contracts gate on these.
pub fn inject_soul_fields(ctx: &mut EvalContext, state: &SoulState) {
    ctx.set(format!("mood.{}", state.mood.as_str()), 1);
    ctx.set("mood.intensity", state.intensity);
    for r in &state.relationships {
        ctx.set(format!("trust.{}", r.target_id), r.trust);
        ctx.set(format!("familiarity.{}", r.target_id), r.familiarity);
    }
    for t in &state.traits {
        ctx.set(format!("trait.{}", t.to_lowercase()), 1);
    }
}

/// Build the evaluation context for one event against one soul: the soul's
/// bridge fields, the event's fields, `event.<event_type>` = 1, and
/// `player_involved`.
fn event_context(state: &SoulState, event: &SoulEvent) -> EvalContext {
    let mut ctx = EvalContext::default();
    inject_soul_fields(&mut ctx, state);
    for (k, v) in &event.fields {
        ctx.set(k.clone(), *v);
    }
    ctx.set(format!("event.{}", event.event_type), 1);
    ctx.set("player_involved", i64::from(event.player_involved));
    ctx
}

/// Pure transition: one event moves one soul. Returns the next state plus
/// the outputs the game layer must surface (mood shifts for the log,
/// breaking points for consequences, secret unlocks for the UI). The order
/// is the spec §15 pipeline: triggers → mood → memory → relationships →
/// secrets → breaking points.
pub fn apply_event(
    file: &SoulFile,
    state: &SoulState,
    event: &SoulEvent,
) -> (SoulState, Vec<SoulOutput>) {
    let mut next = state.clone();
    let mut outputs = Vec::new();

    // 1. Emotional triggers, via the contract engine — one predicate
    // language (the gotcha). Highest priority match wins; ties break by
    // authored order.
    let ctx = event_context(state, event);
    let winner = file
        .emotional_state
        .triggers
        .iter()
        .filter(|t| condition_holds(&t.condition, &ctx))
        .max_by_key(|t| t.priority);
    if let Some(t) = winner {
        if t.mood != next.mood || t.intensity != next.intensity {
            outputs.push(SoulOutput::MoodShift {
                soul_id: next.soul_id.clone(),
                from: next.mood,
                to: t.mood,
                intensity: t.intensity,
            });
            next.mood = t.mood;
            next.intensity = t.intensity.clamp(0, 1024);
            next.mood_history
                .push((event.timestamp, t.mood, t.intensity));
            if next.mood_history.len() > MOOD_HISTORY_CAP {
                let drop = next.mood_history.len() - MOOD_HISTORY_CAP;
                next.mood_history.drain(0..drop);
            }
        }
    }

    // 2. Memory, with weight-based eviction: keep the formative, drop the
    // forgettable.
    next.memories.push(Memory {
        id: format!("{}-{}", event.event_type, event.timestamp),
        event_type: event.event_type.clone(),
        player_involved: event.player_involved,
        emotional_weight: event.emotional_weight.clamp(0, 1024),
        timestamp: event.timestamp,
        summary: event.summary.clone(),
    });
    while next.memories.len() > MEMORY_CAP {
        // Evict the lowest-weight memory; among equals the oldest goes.
        let evict = next
            .memories
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| (m.emotional_weight, m.timestamp))
            .map(|(i, _)| i)
            .expect("non-empty above cap");
        next.memories.remove(evict);
    }

    // 3. Relationships move (clamped into range); the memory that moved
    // them is recorded in the relationship's history.
    for (target, trust_delta, familiarity_delta) in &event.relationship_deltas {
        let rel = match next
            .relationships
            .iter_mut()
            .find(|r| r.target_id == *target)
        {
            Some(r) => r,
            None => {
                next.relationships.push(Relationship {
                    target_id: target.clone(),
                    trust: 0,
                    familiarity: 0,
                    history: Vec::new(),
                });
                next.relationships.last_mut().expect("just pushed")
            }
        };
        rel.trust = (rel.trust + trust_delta).clamp(-1024, 1024);
        rel.familiarity = (rel.familiarity + familiarity_delta).clamp(0, 1024);
        rel.history
            .push(format!("{}-{}", event.event_type, event.timestamp));
    }

    // 4. Secret reveals — evaluated against the *post-event* state, so a
    // trust threshold crossed by this very event counts.
    let post_ctx = event_context(&next, event);
    for secret in &file.secrets {
        if !next.unlocked_secrets.contains(&secret.id)
            && condition_holds(&secret.reveal_condition, &post_ctx)
        {
            next.unlocked_secrets.insert(secret.id.clone());
            outputs.push(SoulOutput::SecretUnlocked {
                soul_id: next.soul_id.clone(),
                secret_id: secret.id.clone(),
            });
        }
    }

    // 5. Breaking points — fired at most once, event delivered, consequence
    // left to the game layer.
    for bp in &file.breaking_points {
        if !next.fired_breaks.contains(&bp.id) && condition_holds(&bp.trigger, &post_ctx) {
            next.fired_breaks.insert(bp.id.clone());
            outputs.push(SoulOutput::SoulBreak {
                soul_id: next.soul_id.clone(),
                breaking_point_id: bp.id.clone(),
                reaction: bp.reaction,
            });
        }
    }

    (next, outputs)
}

/// Apply an authored mutation if its trigger holds against the soul's
/// current bridge fields (plus `extra` game fields) and it has not fired
/// before — fired-once semantics like S11 chapters. Returns the (possibly
/// unchanged) state and whether it fired.
pub fn apply_mutation(
    state: &SoulState,
    mutation: &SoulMutation,
    extra: &BTreeMap<String, i64>,
) -> (SoulState, bool) {
    if state.soul_id != mutation.soul_id || state.fired_mutations.contains(&mutation.id) {
        return (state.clone(), false);
    }
    let mut ctx = EvalContext::default();
    inject_soul_fields(&mut ctx, state);
    for (k, v) in extra {
        ctx.set(k.clone(), *v);
    }
    if !condition_holds(&mutation.trigger, &ctx) {
        return (state.clone(), false);
    }

    let mut next = state.clone();
    next.fired_mutations.insert(mutation.id.clone());
    for change in &mutation.changes {
        match change {
            SoulChange::AddTrait(t) => {
                if !next.traits.contains(t) {
                    next.traits.push(t.clone());
                }
            }
            SoulChange::RemoveTrait(t) => next.traits.retain(|x| x != t),
            SoulChange::SetRelationship {
                target,
                trust,
                familiarity,
            } => {
                let trust = (*trust).clamp(-1024, 1024);
                let familiarity = (*familiarity).clamp(0, 1024);
                match next
                    .relationships
                    .iter_mut()
                    .find(|r| r.target_id == *target)
                {
                    Some(r) => {
                        r.trust = trust;
                        r.familiarity = familiarity;
                    }
                    None => next.relationships.push(Relationship {
                        target_id: target.clone(),
                        trust,
                        familiarity,
                        history: Vec::new(),
                    }),
                }
            }
            SoulChange::UnlockSecret(id) => {
                next.unlocked_secrets.insert(id.clone());
            }
            SoulChange::AddGoal(goal) => {
                if !next.goals.iter().any(|g| g.id == goal.id) {
                    next.goals.push(goal.clone());
                }
            }
        }
    }
    (next, true)
}

/// The authored soul-mutation arcs shipped with the game (embedded like
/// `faction::load_storylines`, so offline is first-class).
pub fn load_soul_mutations() -> Vec<SoulMutation> {
    ron::from_str(SOUL_MUTATIONS_RON).expect("embedded loup_garou_souls.ron")
}
const SOUL_MUTATIONS_RON: &str = include_str!("../../../mods/reachlock/storylines/loup_garou_souls.ron");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::engine::evaluate;
    use crate::contract::types::{
        Action, Comparison, Condition, Contract, Rule, Trigger as ContractTrigger,
    };
    use crate::soul::types::*;

    fn boris_like() -> SoulFile {
        SoulFile {
            id: "boris".into(),
            name: "Boris".into(),
            species: Species::Robot,
            portrait_id: String::new(),
            identity: Identity {
                origin: "Built for interstellar EVA work".into(),
                faction_affiliation: "crew".into(),
                role: "EVA robot".into(),
                public_bio: "First out, last back.".into(),
            },
            personality: Personality {
                traits: vec!["Consistent".into(), "Protective".into()],
                values: vec!["CrewSafety".into()],
                speaking_style: SpeakingStyle::Formal,
                quirks: vec![],
            },
            emotional_state: EmotionalState {
                dominant_mood: Mood::Stable,
                intensity: 307,
                triggers: vec![
                    Trigger {
                        condition: Condition::Compare {
                            field: "ship.damage".into(),
                            op: Comparison::Gt,
                            value: 307,
                        },
                        mood: Mood::Anxious,
                        intensity: 640,
                        priority: 5,
                    },
                    Trigger {
                        condition: Condition::Compare {
                            field: "event.asked_about_mark".into(),
                            op: Comparison::Eq,
                            value: 1,
                        },
                        mood: Mood::Defensive,
                        intensity: 717,
                        priority: 10,
                    },
                ],
            },
            memory_tree: vec![],
            relationship_graph: vec![Relationship {
                target_id: "player".into(),
                trust: 512,
                familiarity: 512,
                history: vec![],
            }],
            goals: vec![],
            breaking_points: vec![BreakingPoint {
                id: "crew_abandoned".into(),
                trigger: Condition::Compare {
                    field: "event.captain_abandons_crew".into(),
                    op: Comparison::Eq,
                    value: 1,
                },
                reaction: BreakReaction::LeaveShip,
            }],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![Secret {
                id: "the_mark".into(),
                reveal_condition: Condition::All(vec![
                    Condition::Compare {
                        field: "trust.player".into(),
                        op: Comparison::Gt,
                        value: 819, // 0.8
                    },
                    Condition::Compare {
                        field: "event.asked_about_mark".into(),
                        op: Comparison::Eq,
                        value: 1,
                    },
                ]),
                content: "A letter, in an alphabet no one asked about.".into(),
            }],
            dialogue: None,
            deflections: vec![],
        }
    }

    fn event(event_type: &str, t: u64) -> SoulEvent {
        SoulEvent {
            event_type: event_type.into(),
            player_involved: true,
            emotional_weight: 256,
            timestamp: t,
            summary: format!("{event_type} at {t}"),
            fields: BTreeMap::new(),
            relationship_deltas: vec![],
        }
    }

    /// The spec §15 flow, end to end: asked about the mark → trigger fires →
    /// mood shifts Defensive → a contract rule gating on `mood.defensive`
    /// deflects the conversation.
    #[test]
    fn asked_about_the_mark_deflects() {
        let file = boris_like();
        let state = SoulState::from_file(&file);
        assert_eq!(state.mood, Mood::Stable);

        let (state, outputs) = apply_event(&file, &state, &event("asked_about_mark", 10));
        assert_eq!(state.mood, Mood::Defensive);
        assert!(outputs.iter().any(|o| matches!(
            o,
            SoulOutput::MoodShift {
                to: Mood::Defensive,
                ..
            }
        )));
        // Trust is nowhere near the reveal threshold: the secret stays put.
        assert!(state.unlocked_secrets.is_empty());

        // The conversation contract deflects while Defensive.
        let contract = Contract {
            id: "conversation_boris".into(),
            label: "Boris — conversation".into(),
            trigger: ContractTrigger::Manual,
            rules: vec![Rule {
                condition: Condition::Compare {
                    field: "mood.defensive".into(),
                    op: Comparison::Eq,
                    value: 1,
                },
                action: Action::verb("deflect_conversation"),
                priority: 10,
            }],
            llm_authority: None,
        };
        let mut ctx = EvalContext::default();
        inject_soul_fields(&mut ctx, &state);
        match evaluate(&contract, &ctx) {
            crate::contract::engine::Outcome::Rule { action, .. } => {
                assert_eq!(action.kind, "deflect_conversation")
            }
            other => panic!("expected the deflect rule, got {other:?}"),
        }
    }

    #[test]
    fn ship_damage_shifts_mood_by_field() {
        let file = boris_like();
        let state = SoulState::from_file(&file);
        let mut ev = event("ship_damage", 5);
        ev.fields.insert("ship.damage".into(), 400); // > 0.3
        let (state, _) = apply_event(&file, &state, &ev);
        assert_eq!(state.mood, Mood::Anxious);
        // Below the threshold nothing happens.
        let fresh = SoulState::from_file(&file);
        let mut ev = event("ship_damage", 6);
        ev.fields.insert("ship.damage".into(), 100);
        let (fresh, outputs) = apply_event(&file, &fresh, &ev);
        assert_eq!(fresh.mood, Mood::Stable);
        assert!(outputs.is_empty());
    }

    #[test]
    fn memories_evict_by_weight_keeping_the_formative() {
        let file = boris_like();
        let mut state = SoulState::from_file(&file);
        // One formative memory early…
        let mut formative = event("rescue", 1);
        formative.emotional_weight = 1000;
        (state, _) = apply_event(&file, &state, &formative);
        // …then flood with forgettable ones, well past the cap.
        for t in 2..(MEMORY_CAP as u64 + 30) {
            let mut e = event("smalltalk", t);
            e.emotional_weight = 10;
            (state, _) = apply_event(&file, &state, &e);
        }
        assert_eq!(state.memories.len(), MEMORY_CAP, "cap holds");
        assert!(
            state.memories.iter().any(|m| m.event_type == "rescue"),
            "the formative memory survived eviction"
        );
        // The evicted ones are the oldest forgettables.
        assert!(!state.memories.iter().any(|m| m.timestamp == 2));
    }

    #[test]
    fn relationships_move_and_clamp() {
        let file = boris_like();
        let state = SoulState::from_file(&file);
        let mut ev = event("betrayal", 3);
        ev.relationship_deltas = vec![("player".into(), -2000, -100)];
        let (state, _) = apply_event(&file, &state, &ev);
        let rel = state.relationship("player").unwrap();
        assert_eq!(rel.trust, -1024, "trust clamps at the floor");
        assert_eq!(rel.familiarity, 412, "familiarity moved");
        assert_eq!(rel.history.len(), 1);
    }

    #[test]
    fn secret_unlocks_when_trust_and_question_align() {
        let file = boris_like();
        let state = SoulState::from_file(&file);
        // Build trust past 0.8 (starts 512; needs > 819).
        let mut ev = event("crisis_shared", 20);
        ev.relationship_deltas = vec![("player".into(), 400, 200)];
        let (state, _) = apply_event(&file, &state, &ev);
        // Now the question unlocks the mark instead of just deflecting.
        let (state, outputs) = apply_event(&file, &state, &event("asked_about_mark", 21));
        assert!(state.unlocked_secrets.contains("the_mark"));
        assert!(outputs
            .iter()
            .any(|o| matches!(o, SoulOutput::SecretUnlocked { .. })));
        // Asking again does not re-emit the unlock.
        let (_, outputs) = apply_event(&file, &state, &event("asked_about_mark", 22));
        assert!(!outputs
            .iter()
            .any(|o| matches!(o, SoulOutput::SecretUnlocked { .. })));
    }

    #[test]
    fn breaking_point_fires_once_and_delivers_the_event() {
        let file = boris_like();
        let state = SoulState::from_file(&file);
        let (state, outputs) = apply_event(&file, &state, &event("captain_abandons_crew", 30));
        assert!(outputs.iter().any(|o| matches!(
            o,
            SoulOutput::SoulBreak {
                reaction: BreakReaction::LeaveShip,
                ..
            }
        )));
        // Same line crossed again: already broken, no second event.
        let (_, outputs) = apply_event(&file, &state, &event("captain_abandons_crew", 31));
        assert!(!outputs
            .iter()
            .any(|o| matches!(o, SoulOutput::SoulBreak { .. })));
    }

    #[test]
    fn mutations_fire_once_and_apply_all_changes() {
        let file = boris_like();
        let state = SoulState::from_file(&file);
        let mutation = SoulMutation {
            id: "boris_devotion".into(),
            soul_id: "boris".into(),
            trigger: Condition::Compare {
                field: "event.player_showed_trust_during_crisis".into(),
                op: Comparison::Eq,
                value: 1,
            },
            changes: vec![
                SoulChange::AddTrait("Devoted".into()),
                SoulChange::RemoveTrait("Consistent".into()),
                SoulChange::SetRelationship {
                    target: "player".into(),
                    trust: 900,
                    familiarity: 700,
                },
                SoulChange::UnlockSecret("the_mark".into()),
                SoulChange::AddGoal(Goal {
                    id: "protect_player".into(),
                    priority: GoalPriority::Constant,
                    description: "Specifically.".into(),
                }),
            ],
        };
        let mut extra = BTreeMap::new();
        // Trigger not met: nothing happens.
        let (unchanged, fired) = apply_mutation(&state, &mutation, &extra);
        assert!(!fired);
        assert_eq!(unchanged, state);
        // Trigger met: everything applies…
        extra.insert("event.player_showed_trust_during_crisis".into(), 1);
        let (next, fired) = apply_mutation(&state, &mutation, &extra);
        assert!(fired);
        assert!(next.traits.contains(&"Devoted".to_string()));
        assert!(!next.traits.contains(&"Consistent".to_string()));
        assert_eq!(next.relationship("player").unwrap().trust, 900);
        assert!(next.unlocked_secrets.contains("the_mark"));
        assert!(next.goals.iter().any(|g| g.id == "protect_player"));
        // …and never twice (fired-once, like S11 chapters).
        let (again, fired) = apply_mutation(&next, &mutation, &extra);
        assert!(!fired);
        assert_eq!(again, next);
    }

    #[test]
    fn embedded_mutation_arcs_parse() {
        let mutations = load_soul_mutations();
        assert!(!mutations.is_empty());
        for m in &mutations {
            assert!(!m.id.is_empty() && !m.soul_id.is_empty() && !m.changes.is_empty());
        }
    }

    #[test]
    fn soul_state_round_trips_serde() {
        let file = boris_like();
        let mut state = SoulState::from_file(&file);
        (state, _) = apply_event(&file, &state, &event("asked_about_mark", 1));
        let text = ron::to_string(&state).expect("to ron");
        let back: SoulState = ron::from_str(&text).expect("from ron");
        assert_eq!(state, back);
    }
}
