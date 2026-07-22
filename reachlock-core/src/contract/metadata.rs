//! Contract metadata for sharing and discovery (S34). Pure types — no I/O,
//! no chrono (iron rule #1). Timestamps are Unix epoch seconds, matching the
//! codebase convention (S33's `timestamp: u64`).

use serde::{Deserialize, Serialize};

/// Crew role categories for the contract library browser filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrewRole {
    Pilot,
    Engineer,
    Navigator,
    Medic,
    Gunner,
    Tactical,
}

/// Metadata attached to a shareable contract. Stored alongside the contract
/// RON in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub author: String,
    pub created: u64,
    pub updated: u64,
    pub crew_member_name: String,
    pub crew_role: CrewRole,
    pub description: String,
    pub personality_tags: Vec<String>,
    pub story_tags: Vec<String>,
    pub usage_notes: String,
    pub shareable: bool,
}

/// A player-submitted anecdote about a contract producing an interesting
/// outcome. Attached to the contract's story feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractStory {
    pub contract_id: String,
    pub story: String,
    pub event_type: String,
    pub outcome_type: String,
    pub timestamp: u64,
}

/// Structural patterns detectable at compile time that produce uninteresting
/// gameplay behavior. Warnings, not errors — the player decides what's fun.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CraftingWarning {
    /// Every rule has an Always condition and there's no LLM config — the
    /// contract always resolves deterministically, never hitting the LLM edge.
    AlwaysResolvesWithoutLLM,
    /// No Always rule and no rule covers common situations — the LLM is called
    /// on nearly every input, defeating the purpose of authored rules.
    AlwaysRequiresLLM,
    /// Every rule has the same priority, so no conflict resolution emerges.
    AllSamePriority,
    /// LLM is configured but no fallback action is set — if the LLM times out,
    /// nothing happens.
    NoFallbackBehavior,
    /// A StateChange trigger uses exact equality, which will almost never fire.
    OverSpecificTrigger,
    /// One rule's action writes to a field another rule reads in its condition,
    /// creating a cycle that prevents stable evaluation.
    CircularRule,
}

/// A contract entry in the local library: paired metadata + serialized body.
/// The `contract_ron` field carries the full `Contract` as RON text so the
/// library can display rules without deserialising every entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractLibraryEntry {
    pub metadata: ContractMetadata,
    pub contract_ron: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The serialized form of ContractMetadata is a wire-shape contract.
    /// Changing a field name or adding a required field must update this test.
    #[test]
    fn wire_shape_is_stable() {
        let json = r##"{
  "author": "player42",
  "created": 1700000000,
  "updated": 1700000000,
  "crew_member_name": "Boris",
  "crew_role": "engineer",
  "description": "A cautious engineer who prioritizes hull integrity.",
  "personality_tags": ["cautious", "protective"],
  "story_tags": ["rescue", "engineering"],
  "usage_notes": "Best paired with a reckless pilot contract.",
  "shareable": true
}"##;
        let meta: ContractMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.author, "player42");
        assert_eq!(meta.crew_member_name, "Boris");
        assert_eq!(meta.crew_role, CrewRole::Engineer);
        assert_eq!(meta.personality_tags, vec!["cautious", "protective"]);
        assert!(meta.shareable);
    }

    #[test]
    fn ron_round_trip() {
        let meta = ContractMetadata::new(
            "test_author".into(),
            "Boris".into(),
            CrewRole::Engineer,
            "test description".into(),
        );
        let ron = ron::to_string(&meta).unwrap();
        let back: ContractMetadata = ron::from_str(&ron).unwrap();
        assert_eq!(meta.author, back.author);
        assert_eq!(meta.crew_member_name, back.crew_member_name);
        assert_eq!(meta.crew_role, back.crew_role);
    }

    #[test]
    fn touch_updates_timestamp() {
        let mut meta = ContractMetadata::new("a".into(), "b".into(), CrewRole::Pilot, "c".into());
        let before = meta.updated;
        meta.touch();
        // touch updates from system clock — just verify it doesn't crash
        // and the value is reasonable (within 2 seconds of creation).
        assert!(meta.updated >= before);
        assert!(meta.updated - before <= 2);
    }

    #[test]
    fn crew_role_serde_snake_case() {
        let json = r##""pilot""##;
        let role: CrewRole = serde_json::from_str(json).unwrap();
        assert_eq!(role, CrewRole::Pilot);
        let back = serde_json::to_string(&role).unwrap();
        // removed prefixes from json
        assert_eq!(back, r##""pilot""##);
    }

    #[test]
    fn contract_library_entry_round_trips() {
        let meta = ContractMetadata::new(
            "author".into(),
            "Boris".into(),
            CrewRole::Engineer,
            "desc".into(),
        );
        let entry = ContractLibraryEntry {
            metadata: meta.clone(),
            contract_ron: "(id:\"test\",label:\"x\",trigger:Manual,rules:[],llm_authority:None)"
                .into(),
        };
        let ron = ron::to_string(&entry).unwrap();
        let back: ContractLibraryEntry = ron::from_str(&ron).unwrap();
        assert_eq!(back.metadata.author, "author");
        assert_eq!(back.metadata.crew_role, CrewRole::Engineer);
        assert!(!back.contract_ron.is_empty());
    }
}

impl ContractMetadata {
    /// Build metadata for a new (unsaved) contract.
    pub fn new(
        author: String,
        crew_member_name: String,
        crew_role: CrewRole,
        description: String,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        ContractMetadata {
            author,
            created: now,
            updated: now,
            crew_member_name,
            crew_role,
            description,
            personality_tags: Vec::new(),
            story_tags: Vec::new(),
            usage_notes: String::new(),
            shareable: false,
        }
    }

    /// Mark metadata as updated (call before saving edits).
    pub fn touch(&mut self) {
        self.updated = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }
}
