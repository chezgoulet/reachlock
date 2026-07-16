//! WebSocket message vocabulary (spec §8). The `type` tags on the wire are
//! part of the protocol — tests below pin them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::contract::signature::SignedEvaluation;
use crate::contract::types::Contract;
use crate::seed::types::{Seed, SystemId};
use crate::universe::tier::UniverseTier;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "seed.discover")]
    SeedDiscover {
        universe: UniverseTier,
        system_id: SystemId,
        seed: Seed,
    },
    #[serde(rename = "seed.modify")]
    SeedModify {
        universe: UniverseTier,
        system_id: SystemId,
        diffs: Value,
    },
    #[serde(rename = "contract.sync")]
    ContractSync { contracts: Vec<Contract> },
    #[serde(rename = "eval.submit")]
    EvalSubmit { eval: SignedEvaluation },
    #[serde(rename = "llm.call")]
    LlmCall {
        call_id: String,
        contract_id: String,
        context: Value,
        /// S16B protocol revision (additive; absent = previous behavior):
        /// the caller's system prompt — dialogue sends the soul's voice
        /// prompt so the model speaks in character as the TRUE system
        /// message, not a payload hint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system_prompt: Option<String>,
        /// Contract `LlmConfig` budget: per-call timeout, clamped by the
        /// server cap. Absent = server default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u32>,
        /// Contract `LlmConfig` budget: max tokens. Absent = server default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_tokens: Option<u32>,
    },
    #[serde(rename = "player.position")]
    PlayerPosition {
        system_id: SystemId,
        /// Fixed-point coordinates (spec §5 — gameplay values are integers).
        position: [i64; 2],
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "seed.canonical")]
    SeedCanonical {
        system_id: SystemId,
        seed: Seed,
        diffs: Value,
        /// True when the submitting client was first — its tentative seed
        /// became canonical.
        you_discovered: bool,
    },
    #[serde(rename = "eval.verified")]
    EvalVerified { eval_id: String, accepted: bool },
    #[serde(rename = "eval.rejected")]
    EvalRejected { eval_id: String, reason: String },
    #[serde(rename = "llm.deliberating")]
    LlmDeliberating { call_id: String },
    #[serde(rename = "llm.response")]
    LlmResponse {
        call_id: String,
        action: String,
        reasoning: String,
    },
    #[serde(rename = "llm.failed")]
    LlmFailed { call_id: String, reason: String },
    #[serde(rename = "player.entered")]
    PlayerEntered {
        player_id: String,
        system_id: SystemId,
        universe: UniverseTier,
    },
    #[serde(rename = "universe.event")]
    UniverseEvent { event: Value },
    #[serde(rename = "error")]
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_tags_match_spec() {
        let msg = ClientMessage::SeedDiscover {
            universe: UniverseTier::FairPlay,
            system_id: SystemId("duskway-0417".into()),
            seed: Seed::new(12345),
        };
        let json: Value = serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "seed.discover");
        assert_eq!(json["seed"], 12345);
        assert_eq!(json["universe"], "fair_play");
    }

    #[test]
    fn round_trip_both_directions() {
        let client = ClientMessage::PlayerPosition {
            system_id: SystemId("s".into()),
            position: [1024, -2048],
        };
        let s = serde_json::to_string(&client).unwrap();
        assert_eq!(serde_json::from_str::<ClientMessage>(&s).unwrap(), client);

        let server = ServerMessage::SeedCanonical {
            system_id: SystemId("s".into()),
            seed: Seed::new(7),
            diffs: serde_json::json!({}),
            you_discovered: true,
        };
        let s = serde_json::to_string(&server).unwrap();
        assert_eq!(serde_json::from_str::<ServerMessage>(&s).unwrap(), server);
    }

    /// S16B protocol revision: `llm.call` gained optional `system_prompt`,
    /// `timeout_ms`, `max_tokens`. Old-shape messages must still parse
    /// (absent = None), and absent fields must not serialize — this test IS
    /// the revision record (iron rule #4).
    #[test]
    fn llm_call_revision_is_backward_compatible() {
        let old_shape =
            r#"{"type":"llm.call","call_id":"c1","contract_id":"cryo-pilot","context":{}}"#;
        let parsed: ClientMessage = serde_json::from_str(old_shape).unwrap();
        let ClientMessage::LlmCall {
            system_prompt,
            timeout_ms,
            max_tokens,
            ..
        } = &parsed
        else {
            panic!("parsed as the wrong variant");
        };
        assert!(system_prompt.is_none() && timeout_ms.is_none() && max_tokens.is_none());
        // Absent options round-trip invisibly (old servers stay compatible).
        let json = serde_json::to_string(&parsed).unwrap();
        assert!(!json.contains("system_prompt"));
        // Present options survive the round trip.
        let full = ClientMessage::LlmCall {
            call_id: "c2".into(),
            contract_id: "dialogue:boris".into(),
            context: serde_json::json!({}),
            system_prompt: Some("You are Boris.".into()),
            timeout_ms: Some(4000),
            max_tokens: Some(128),
        };
        let s = serde_json::to_string(&full).unwrap();
        assert_eq!(serde_json::from_str::<ClientMessage>(&s).unwrap(), full);
    }

    #[test]
    fn unknown_type_is_an_error_not_a_panic() {
        let result = serde_json::from_str::<ClientMessage>(r#"{"type":"warp.core.breach"}"#);
        assert!(result.is_err());
    }
}
