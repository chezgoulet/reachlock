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

    #[test]
    fn unknown_type_is_an_error_not_a_panic() {
        let result = serde_json::from_str::<ClientMessage>(r#"{"type":"warp.core.breach"}"#);
        assert!(result.is_err());
    }
}
