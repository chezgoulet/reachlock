//! Frozen soul shapes (spec §15, "Freeze first"). These are the
//! compatibility surface for every file under `content/souls/` — field
//! names are pinned by the wire-shape test at the foot of this file.
//!
//! Everything the spec wrote as `f32` is fixed-point `i64` here
//! (1024 = 1.0): trust, familiarity, intensity, emotional_weight, loyalty.

use serde::{Deserialize, Serialize};

use crate::contract::types::Condition;

/// What kind of body carries this soul. Mirrors the client's `BodyKind`
/// (zero-g movement). Five canonical species:
///
/// - Human: includes cybernetically enhanced humans. The baseline.
/// - Android: any synthetic humanoid.
/// - Robot: industrial or non-humanoid synthetics.
/// - Voidborn: space-dwelling creatures, mystical beings tied to Predecessor
///   lore and special events. Not bound to any planetary ecosystem.
/// - Xenotype: creatures bound to and part of a planet's ecosystem.
///   The galaxy's planetary life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Species {
    Human,
    Android,
    Robot,
    Voidborn,
    Xenotype,
}

/// How a soul talks. Consumed by S16's context assembly; inert data here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakingStyle {
    Terse,
    Elaborate,
    Technical,
    Lyrical,
    Sarcastic,
    Blunt,
    Formal,
    Wry,
    Warm,
}

/// A named mood. The bridge exposes the active one to contracts as
/// `mood.<name>` (snake_case), so authored rules can gate on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    Stable,
    Happy,
    Tense,
    Grieving,
    Suspicious,
    Grateful,
    Anxious,
    Protective,
    Defensive,
    Focused,
    Withdrawn,
}

impl Mood {
    /// The `mood.<name>` bridge field suffix. Matches the serde rename.
    pub fn as_str(self) -> &'static str {
        match self {
            Mood::Stable => "stable",
            Mood::Happy => "happy",
            Mood::Tense => "tense",
            Mood::Grieving => "grieving",
            Mood::Suspicious => "suspicious",
            Mood::Grateful => "grateful",
            Mood::Anxious => "anxious",
            Mood::Protective => "protective",
            Mood::Defensive => "defensive",
            Mood::Focused => "focused",
            Mood::Withdrawn => "withdrawn",
        }
    }
}

/// Who this NPC is — the authored root (spec §15). Immutable at runtime;
/// live state lives in [`super::runtime::SoulState`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoulFile {
    pub id: String,
    pub name: String,
    pub species: Species,
    /// References a generated or authored portrait asset (unused until the
    /// portrait pass; pinned now so files don't churn later).
    #[serde(default)]
    pub portrait_id: String,
    pub identity: Identity,
    pub personality: Personality,
    /// The authored emotional baseline; runtime mood starts here.
    pub emotional_state: EmotionalState,
    /// Formative authored memories the NPC starts with.
    #[serde(default)]
    pub memory_tree: Vec<Memory>,
    #[serde(default)]
    pub relationship_graph: Vec<Relationship>,
    #[serde(default)]
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub breaking_points: Vec<BreakingPoint>,
    /// Contract ids this soul can execute (spec §15 contract integration).
    #[serde(default)]
    pub contracts: Vec<String>,
    /// Narrative reference for authors — never an LLM prompt.
    #[serde(default)]
    pub backstory: String,
    #[serde(default)]
    pub secrets: Vec<Secret>,
    /// S16: authored conversation graph, if this soul has one. Optional and
    /// skipped when absent so the S13 wire shape is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialogue: Option<crate::dialogue::DialogueGraph>,
    /// S16: authored deflection lines for the unscripted edge when no
    /// inference exists (offline / Classic). Never a hang, never lorem
    /// ipsum — the soul deflects in its own voice. Picked deterministically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deflections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub origin: String,
    pub faction_affiliation: String,
    pub role: String,
    /// What they'll tell you — the inspect panel shows exactly this.
    pub public_bio: String,
}

/// Traits/values are open strings, not enums: they are content vocabulary
/// (mutations add and remove them by name), and the schema — not the type
/// system — is the right place to lint them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Personality {
    #[serde(default)]
    pub traits: Vec<String>,
    #[serde(default)]
    pub values: Vec<String>,
    pub speaking_style: SpeakingStyle,
    #[serde(default)]
    pub quirks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmotionalState {
    pub dominant_mood: Mood,
    /// 0 (calm) ..= 1024 (overwhelming).
    pub intensity: i64,
    /// Conditions that shift mood, evaluated through the contract engine —
    /// highest priority match wins.
    #[serde(default)]
    pub triggers: Vec<Trigger>,
}

/// A mood trigger: when `condition` holds against the event context, mood
/// shifts to `mood` at `intensity`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub condition: Condition,
    pub mood: Mood,
    pub intensity: i64,
    #[serde(default)]
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    /// "conversation", "combat", "trade", "betrayal", "rescue", …
    pub event_type: String,
    #[serde(default)]
    pub player_involved: bool,
    /// 0 (forgettable) ..= 1024 (traumatic/formative). Drives eviction.
    pub emotional_weight: i64,
    /// Game tick of the event.
    pub timestamp: u64,
    /// For LLM context assembly (S16).
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    /// `"player"` or another soul id.
    pub target_id: String,
    /// -1024 (enemy) ..= 1024 (unquestioning).
    pub trust: i64,
    /// 0 (stranger) ..= 1024 (intimate).
    pub familiarity: i64,
    /// Key memory ids that shaped this relationship.
    #[serde(default)]
    pub history: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalPriority {
    /// Always in play.
    Constant,
    /// In play when the situation raises it.
    Situational,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub priority: GoalPriority,
    pub description: String,
}

/// What a soul does when a line is crossed. S13 delivers the *event*
/// ([`super::runtime::SoulOutput::SoulBreak`]); the game layer owns the
/// consequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakReaction {
    LeaveShip,
    RefuseOrders,
    Confront,
    Withdraw,
    Betray,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreakingPoint {
    /// Stable id — a breaking point fires at most once.
    pub id: String,
    pub trigger: Condition,
    pub reaction: BreakReaction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Secret {
    pub id: String,
    /// Evaluated against the bridge fields (mood, trust, event flags);
    /// once it holds the secret unlocks permanently.
    pub reveal_condition: Condition,
    /// Hidden from the player until unlocked.
    pub content: String,
}

/// An authored, fired-once soul mutation (spec §15) — the mechanism by
/// which written narrative permanently changes a person. Lives in
/// `content/storylines/*.ron` beside the faction arcs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoulMutation {
    pub id: String,
    pub soul_id: String,
    pub trigger: Condition,
    pub changes: Vec<SoulChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoulChange {
    AddTrait(String),
    RemoveTrait(String),
    SetRelationship {
        target: String,
        trust: i64,
        familiarity: i64,
    },
    UnlockSecret(String),
    AddGoal(Goal),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::types::Comparison;

    /// Minimal-but-complete soul used by the wire-shape test.
    pub(crate) fn sample_soul() -> SoulFile {
        SoulFile {
            id: "sample".into(),
            name: "Sample".into(),
            species: Species::Robot,
            portrait_id: String::new(),
            identity: Identity {
                origin: "test".into(),
                faction_affiliation: "crew".into(),
                role: "EVA".into(),
                public_bio: "A sample.".into(),
            },
            personality: Personality {
                traits: vec!["Consistent".into()],
                values: vec!["CrewSafety".into()],
                speaking_style: SpeakingStyle::Formal,
                quirks: vec![],
            },
            emotional_state: EmotionalState {
                dominant_mood: Mood::Stable,
                intensity: 307,
                triggers: vec![Trigger {
                    condition: Condition::Compare {
                        field: "ship.damage".into(),
                        op: Comparison::Gt,
                        value: 307,
                    },
                    mood: Mood::Anxious,
                    intensity: 640,
                    priority: 5,
                }],
            },
            memory_tree: vec![],
            relationship_graph: vec![Relationship {
                target_id: "player".into(),
                trust: 512,
                familiarity: 512,
                history: vec![],
            }],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![],
            dialogue: None,
            deflections: vec![],
        }
    }

    /// Iron rule #4: the soul's serialized form is a compatibility promise.
    /// If this test breaks, you are making a content-schema revision —
    /// update `content/schemas/soul.schema.json` and say so in the commit.
    #[test]
    fn soul_wire_shape_is_pinned() {
        let json = serde_json::to_value(sample_soul()).expect("serialize");
        let expected = serde_json::json!({
            "id": "sample",
            "name": "Sample",
            "species": "robot",
            "portrait_id": "",
            "identity": {
                "origin": "test",
                "faction_affiliation": "crew",
                "role": "EVA",
                "public_bio": "A sample."
            },
            "personality": {
                "traits": ["Consistent"],
                "values": ["CrewSafety"],
                "speaking_style": "formal",
                "quirks": []
            },
            "emotional_state": {
                "dominant_mood": "stable",
                "intensity": 307,
                "triggers": [{
                    "condition": {"compare": {"field": "ship.damage", "op": "gt", "value": 307}},
                    "mood": "anxious",
                    "intensity": 640,
                    "priority": 5
                }]
            },
            "memory_tree": [],
            "relationship_graph": [{
                "target_id": "player",
                "trust": 512,
                "familiarity": 512,
                "history": []
            }],
            "goals": [],
            "breaking_points": [],
            "contracts": [],
            "backstory": "",
            "secrets": []
        });
        assert_eq!(json, expected, "soul wire shape changed — schema revision");
    }

    #[test]
    fn soul_round_trips_through_ron() {
        let soul = sample_soul();
        let text = ron::to_string(&soul).expect("to ron");
        let back: SoulFile = ron::from_str(&text).expect("from ron");
        assert_eq!(soul, back);
    }
}
