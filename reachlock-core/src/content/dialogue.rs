use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    NarratorLine,
    NpcLine,
    PlayerChoice,
    Branch,
    End,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueChoice {
    pub display_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consequence: Option<String>,
    pub next_node: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueNode {
    pub id: String,
    pub node_type: NodeType,
    pub text: String,
    #[serde(default)]
    pub choices: Vec<DialogueChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_clip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dialogue {
    pub nodes: Vec<DialogueNode>,
    pub start_node: String,
}
