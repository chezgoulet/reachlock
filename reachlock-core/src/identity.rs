//! Player identity (S75).
//!
//! PlayerCharacter is a wrapper, not a SoulFile variant, because
//! SoulFile's wire shape is frozen for NPC interop. The player's soul
//! is accessible at `pc.soul`.

use serde::{Deserialize, Serialize};

use crate::generator::sprite::CharacterLookConfig;
use crate::soul::SoulFile;

/// EntityId is a newtype over u64 with serialization that matches the
/// existing soul entity id scheme. Serializes as a JSON number ≤ 2^53
/// (JSON float survival — iron rule from the gotcha ledger).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EntityId(pub u64);

const SEED53_MAX: u64 = (1 << 53) - 1;

impl Serialize for EntityId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.0 > SEED53_MAX {
            return Err(serde::ser::Error::custom(format!(
                "EntityId {} exceeds JSON-safe integer limit 2^53",
                self.0
            )));
        }
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = u64::deserialize(d)?;
        if n > SEED53_MAX {
            return Err(serde::de::Error::custom(format!(
                "EntityId {} exceeds JSON-safe integer limit 2^53",
                n
            )));
        }
        Ok(EntityId(n))
    }
}

/// The player character's identity. Not a SoulFile variant — a wrapper
/// that pairs an identity record with a full soul, so SoulFile's wire
/// shape stays stable for NPC interop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerCharacter {
    pub id: EntityId,
    pub name: String,
    pub pronouns: String,
    pub species: String,
    pub look: CharacterLookConfig,
    pub origin_id: String,
    pub background_id: String,
    pub soul: SoulFile,
}

impl PlayerCharacter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: EntityId,
        name: String,
        pronouns: String,
        species: String,
        look: CharacterLookConfig,
        origin_id: String,
        background_id: String,
        soul: SoulFile,
    ) -> Self {
        Self {
            id,
            name,
            pronouns,
            species,
            look,
            origin_id,
            background_id,
            soul,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_soul(
        entity_id: EntityId,
        soul: SoulFile,
        name: String,
        pronouns: String,
        species: String,
        look: CharacterLookConfig,
        origin_id: String,
        background_id: String,
    ) -> Self {
        Self {
            id: entity_id,
            name,
            pronouns,
            species,
            look,
            origin_id,
            background_id,
            soul,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soul::types::*;

    fn sample_soul() -> SoulFile {
        SoulFile {
            id: "player_soul".into(),
            name: "Rook".into(),
            species: Species::Human,
            portrait_id: String::new(),
            identity: Identity {
                origin: String::new(),
                faction_affiliation: String::new(),
                role: "Captain".into(),
                public_bio: "A placeholder.".into(),
            },
            personality: Personality {
                traits: vec![],
                values: vec![],
                speaking_style: SpeakingStyle::Terse,
                quirks: vec![],
            },
            emotional_state: EmotionalState {
                dominant_mood: Mood::Stable,
                intensity: 512,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![Relationship {
                target_id: "player".into(),
                trust: 1024,
                familiarity: 1024,
                history: vec![],
            }],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![],
            dialogue: None,
            deflections: vec![],
            look: None,
        }
    }

    fn sample_player() -> PlayerCharacter {
        PlayerCharacter {
            id: EntityId(42),
            name: "Rook".into(),
            pronouns: "they/them".into(),
            species: "Human".into(),
            look: CharacterLookConfig::seed_derived("Human"),
            origin_id: "orphaned_colony".into(),
            background_id: "spacer".into(),
            soul: sample_soul(),
        }
    }

    #[test]
    fn entity_id_round_trips_through_json() {
        let id = EntityId(42);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "42");
        let back: EntityId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn entity_id_rejects_above_2_pow_53() {
        let id = EntityId(1 << 53);
        let result = serde_json::to_string(&id);
        assert!(result.is_err());
    }

    #[test]
    fn entity_id_rejects_deserialize_above_2_pow_53() {
        let result = serde_json::from_str::<EntityId>("9007199254740992");
        assert!(result.is_err());
    }

    #[test]
    fn player_character_round_trips_through_ron() {
        let pc = sample_player();
        let text = ron::to_string(&pc).unwrap();
        let back: PlayerCharacter = ron::from_str(&text).unwrap();
        assert_eq!(pc, back);
    }

    #[test]
    fn player_character_round_trips_through_json() {
        let pc = sample_player();
        let json = serde_json::to_string(&pc).unwrap();
        let back: PlayerCharacter = serde_json::from_str(&json).unwrap();
        assert_eq!(pc, back);
    }
}
