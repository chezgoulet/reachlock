//! WebSocket message vocabulary (spec §8, extended S23). The `type` tags on
//! the wire are part of the protocol — tests below pin them.
//!
//! S23 adds presence, chat, and content-deployment message variants as well
//! as a `Hello` handshake for protocol version negotiation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::content::ContentFile;
use crate::contract::signature::SignedEvaluation;
use crate::contract::types::Contract;
use crate::combat::{HostileArchetype, HostileLocation};
use crate::galaxy::{ChartedSystem, GateNetwork};
use crate::seed::types::{Seed, SystemId};
use crate::universe::tier::UniverseTier;

/// S23/S29: bump when adding/removing message variants so mismatched clients get
/// a clear error instead of serde noise.
pub const PROTOCOL_VERSION: u32 = 4;

/// S26: wraps a `ServerMessage` with an optional `trace_id` field that old
/// clients (which don't know about this field) ignore via serde's default
/// unknown field behavior. Used for serialization only — the server never
/// deserializes `ServerMessage` from the client.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServerEnvelope<'a> {
    #[serde(flatten)]
    pub message: &'a ServerMessage,
    /// Trace-id for debugging. Old clients ignore unknown JSON fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system_prompt: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_tokens: Option<u32>,
    },
    #[serde(rename = "player.position")]
    PlayerPosition {
        system_id: SystemId,
        /// Fixed-point coordinates (spec §5 — gameplay values are integers).
        position: [i64; 3],
    },
    /// S23: send a chat message (system-scope for now; direct messages
    /// use player-scoped chat in a future revision).
    #[serde(rename = "chat.send")]
    ChatSend {
        /// The message body. Server enforces ≤ 256 bytes.
        text: String,
    },
    /// S29: voice signaling relay (offer/answer/ICE to a specific peer).
    #[serde(rename = "voice.signal")]
    VoiceSignal {
        target_player: String,
        signal: VoiceSignalPayload,
    },
    /// S29: request TURN server credentials from the server.
    #[serde(rename = "turn.request")]
    RequestTurnConfig,
    /// WASM content distribution: a wasm client has no filesystem, so it asks
    /// the server to push the authored content for a universe over the wire.
    #[serde(rename = "content.request")]
    RequestContent {
        universe: UniverseTier,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// S23: sent immediately on WS connect. Client must verify its own
    /// protocol_version matches what the server expects.
    #[serde(rename = "hello")]
    Hello {
        protocol_version: u32,
    },
    #[serde(rename = "seed.canonical")]
    SeedCanonical {
        system_id: SystemId,
        seed: Seed,
        diffs: Value,
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
    /// S23: a new player has joined this system — spawn their ship.
    #[serde(rename = "player.joined")]
    PlayerJoined {
        player_id: String,
        system_id: SystemId,
        universe: UniverseTier,
    },
    /// S23: a player has left the current system (disconnected or jumped).
    #[serde(rename = "player.left")]
    PlayerLeft {
        player_id: String,
        system_id: SystemId,
    },
    /// S23: a chat message from another player in the same system.
    #[serde(rename = "chat.message")]
    ChatMessage {
        from_player: String,
        text: String,
    },
    /// S23: content overrides have changed — re-fetch for the affected
    /// universe on next system entry.
    #[serde(rename = "content.update")]
    ContentUpdate {
        universe: UniverseTier,
    },
    #[serde(rename = "universe.event")]
    UniverseEvent { event: Value },
    #[serde(rename = "error")]
    Error { message: String },
    /// S29: voice signaling relay from another player (offer/answer/ICE).
    #[serde(rename = "voice.signal")]
    VoiceSignal {
        from_player: String,
        signal: VoiceSignalPayload,
    },
    /// S29: TURN server credentials (time-limited, HMAC-SHA1).
    #[serde(rename = "turn.config")]
    TurnConfig {
        url: String,
        username: String,
        password: String,
        ttl_secs: u32,
    },
    /// S28: system notice (subscription grace period, server messages).
    #[serde(rename = "system.notice")]
    SystemNotice { message: String },
    /// WASM content distribution: server pushes authored content for a
    /// universe to a client that has no local filesystem. The client merges
    /// this into its `ContentIndex` (spec §10, offline-first: the server adds,
    /// it never replaces the local-mode loader).
    #[serde(rename = "content.sync")]
    ContentSync {
        universe: UniverseTier,
        files: Vec<ContentFile>,
        hostile_archetypes: Vec<HostileArchetype>,
        hostile_locations: Vec<HostileLocation>,
        charted_systems: Vec<ChartedSystem>,
        gate_network: Option<GateNetwork>,
    },
}

/// S29: WebRTC signaling payload carried by `voice.signal` messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceSignalPayload {
    Offer { sdp: String },
    Answer { sdp: String },
    IceCandidate { candidate: String, sdp_mid: String, sdp_mline_index: u16 },
    Hangup,
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
            position: [1024, -2048, 0],
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
    fn protocol_version_is_four() {
        assert_eq!(PROTOCOL_VERSION, 4);
    }

    #[test]
    fn hello_wire_tag() {
        let msg = ServerMessage::Hello {
            protocol_version: 3,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "hello");
        assert_eq!(json["protocol_version"], 3);
    }

    #[test]
    fn chat_send_round_trips() {
        let msg = ClientMessage::ChatSend {
            text: "hello".into(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert!(s.contains("\"chat.send\""));
        assert!(s.contains("\"hello\""));
        assert_eq!(serde_json::from_str::<ClientMessage>(&s).unwrap(), msg);
    }

    #[test]
    fn chat_message_round_trips() {
        let msg = ServerMessage::ChatMessage {
            from_player: "pilot".into(),
            text: "hi".into(),
        };
        let s = serde_json::to_string(&msg).unwrap();
        assert_eq!(serde_json::from_str::<ServerMessage>(&s).unwrap(), msg);
    }

    #[test]
    fn player_left_wire_tag() {
        let msg = ServerMessage::PlayerLeft {
            player_id: "alice".into(),
            system_id: SystemId("s".into()),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "player.left");
        assert_eq!(json["player_id"], "alice");
    }

    #[test]
    fn player_joined_wire_tag() {
        let msg = ServerMessage::PlayerJoined {
            player_id: "bob".into(),
            system_id: SystemId("aethon".into()),
            universe: UniverseTier::Classic,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "player.joined");
        assert_eq!(json["player_id"], "bob");
    }

    #[test]
    fn voice_signal_offer_round_trips() {
        let offer = VoiceSignalPayload::Offer { sdp: "v=0\no=...".into() };
        let json = serde_json::to_string(&offer).unwrap();
        assert!(json.contains("\"offer\""));
        let back: VoiceSignalPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(offer, back);
    }

    #[test]
    fn voice_signal_ice_candidate() {
        let msg = ClientMessage::VoiceSignal {
            target_player: "tib".into(),
            signal: VoiceSignalPayload::IceCandidate {
                candidate: "candidate:1".into(),
                sdp_mid: "audio".into(),
                sdp_mline_index: 0,
            },
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "voice.signal");
        assert_eq!(json["signal"]["type"], "ice_candidate");
    }

    #[test]
    fn content_update_wire_tag() {
        let msg = ServerMessage::ContentUpdate {
            universe: UniverseTier::Classic,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "content.update");
        assert_eq!(json["universe"], "classic");
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
    fn request_turn_config_wire_tag() {
        let msg = ClientMessage::RequestTurnConfig;
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "turn.request");
    }

    #[test]
    fn turn_config_wire_tag() {
        let msg = ServerMessage::TurnConfig {
            url: "turn:relay.example.com:3478".into(),
            username: "1234567890:player1".into(),
            password: "base64hmac==".into(),
            ttl_secs: 86400,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "turn.config");
        assert_eq!(json["url"], "turn:relay.example.com:3478");
        assert_eq!(json["ttl_secs"], 86400);
    }

    #[test]
    fn system_notice_wire_tag() {
        let msg = ServerMessage::SystemNotice {
            message: "Payment past due.".into(),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "system.notice");
        assert_eq!(json["message"], "Payment past due.");
    }

    #[test]
    fn unknown_type_is_an_error_not_a_panic() {
        let result = serde_json::from_str::<ClientMessage>(r#"{"type":"warp.core.breach"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn request_content_wire_tag() {
        let msg = ClientMessage::RequestContent {
            universe: UniverseTier::Classic,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "content.request");
        assert_eq!(json["universe"], "classic");
    }

    #[test]
    fn content_sync_wire_tag_and_round_trip() {
        let msg = ServerMessage::ContentSync {
            universe: UniverseTier::Classic,
            files: vec![],
            hostile_archetypes: vec![],
            hostile_locations: vec![],
            charted_systems: vec![],
            gate_network: None,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(json["type"], "content.sync");
        assert_eq!(json["universe"], "classic");
        assert_eq!(json["files"], serde_json::json!([]));
        assert_eq!(
            serde_json::from_str::<ServerMessage>(&serde_json::to_string(&msg).unwrap()).unwrap(),
            msg
        );
    }
}
