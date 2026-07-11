//! Contract data model (spec §6).

use serde::{Deserialize, Serialize};

/// A contract: player-authored automation for a ship system or crew member.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    pub id: String,
    pub label: String,
    pub trigger: Trigger,
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_authority: Option<LlmConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Timer {
        interval_secs: u32,
        repeat: bool,
    },
    Event {
        event_type: String,
    },
    StateChange {
        field: String,
        op: Comparison,
        value: i64,
    },
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub condition: Condition,
    pub action: Action,
    /// Higher priority evaluates first; ties break by authored order.
    #[serde(default)]
    pub priority: u8,
}

/// Boolean expression over game state. All values are fixed-point integers
/// (spec §5) — floats never enter contract evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// Always true — the conventional default/fallback rule.
    Always,
    /// Compare a game-state field against a fixed-point constant.
    /// A missing field makes the comparison false, never an error:
    /// a contract must not crash because a sensor went offline.
    Compare {
        field: String,
        op: Comparison,
        value: i64,
    },
    Not(Box<Condition>),
    All(Vec<Condition>),
    Any(Vec<Condition>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
}

impl Comparison {
    pub fn apply(self, lhs: i64, rhs: i64) -> bool {
        match self {
            Comparison::Lt => lhs < rhs,
            Comparison::Le => lhs <= rhs,
            Comparison::Eq => lhs == rhs,
            Comparison::Ne => lhs != rhs,
            Comparison::Ge => lhs >= rhs,
            Comparison::Gt => lhs > rhs,
        }
    }
}

/// What a rule does when it fires. `kind` is the verb the game interprets
/// ("wake_crew", "maintain_course"); params are fixed-point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Action {
    pub kind: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub params: std::collections::BTreeMap<String, i64>,
}

impl Action {
    pub fn verb(kind: impl Into<String>) -> Self {
        Action {
            kind: kind.into(),
            params: Default::default(),
        }
    }
}

/// LLM fallback authority: fires only when no rule matches (spec §6 pillar —
/// "LLM at the edge, not the center").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmConfig {
    pub fallback_on_timeout: bool,
    pub timeout_ms: u32,
    pub max_tokens: u32,
    pub system_prompt: String,
    /// Action taken when the LLM times out or errors and
    /// `fallback_on_timeout` is set. The Boris story: "couldn't decide —
    /// fell back to maintenance routine."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_action: Option<Action>,
}
