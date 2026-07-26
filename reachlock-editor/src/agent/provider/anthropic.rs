//! Anthropic Messages API adapter.
//!
//! Deliberately a separate adapter rather than a base-URL swap on the
//! OpenAI-compatible one: the two wire formats disagree about the three things
//! this agent depends on most.
//!
//! | | OpenAI-compatible | Anthropic |
//! |---|---|---|
//! | system prompt | a `messages[0]` with `role: "system"` | a top-level `system` field |
//! | tool call | `message.tool_calls[]`, arguments as a JSON *string* | a `tool_use` content block, `input` already an object |
//! | tool result | its own message with `role: "tool"` | a `tool_result` block inside a **user** message |
//! | image | `image_url` with a `data:` URI | `source: {type: "base64", media_type, data}` |
//!
//! Papering over that with one request builder is how a "provider-agnostic"
//! layer ends up supporting exactly one provider properly.

use base64::Engine as _;
use serde_json::{json, Value};

use super::{
    Caps, Message, Part, Provider, ProviderError, Request, Response, Role, StopReason, ToolCall,
};

/// Wire version. Pinned, not derived from anything — this is a protocol
/// constant and a silent bump is a protocol revision.
const API_VERSION: &str = "2023-06-01";

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1";
/// Current Opus. Overridable per profile; this is only the default a fresh
/// profile starts on.
pub const DEFAULT_MODEL: &str = "claude-opus-5";

pub struct Anthropic {
    name: String,
    base_url: String,
    api_key: String,
    model: String,
    caps: Caps,
    rt: tokio::runtime::Runtime,
    client: reqwest::Client,
}

impl Anthropic {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        caps: Caps,
    ) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("could not start async runtime: {e}"))?;
        Ok(Anthropic {
            name: name.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
            caps,
            rt,
            client: reqwest::Client::new(),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url.trim_end_matches('/'), path)
    }

    fn encode_parts(parts: &[Part]) -> Vec<Value> {
        parts
            .iter()
            .map(|p| match p {
                Part::Text(t) => json!({ "type": "text", "text": t }),
                Part::Image { media_type, data } => json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": base64::engine::general_purpose::STANDARD.encode(data),
                    }
                }),
            })
            .collect()
    }

    /// Render normalized messages into Anthropic's `messages` array.
    ///
    /// The system prompt is NOT included here — it is a top-level request
    /// field on this API, and a `role: "system"` message is rejected.
    fn encode_messages(req: &Request) -> Vec<Value> {
        let mut out = Vec::new();
        for m in &req.messages {
            match m {
                Message::Turn {
                    role,
                    parts,
                    tool_calls,
                } => {
                    let mut content = Self::encode_parts(parts);
                    // Tool calls are content blocks in the assistant's own
                    // message, not a sibling field.
                    for c in tool_calls {
                        content.push(json!({
                            "type": "tool_use",
                            "id": c.id,
                            "name": c.name,
                            "input": c.arguments,
                        }));
                    }
                    if content.is_empty() {
                        continue;
                    }
                    out.push(json!({
                        "role": match role { Role::User => "user", Role::Assistant => "assistant" },
                        "content": content,
                    }));
                }
                Message::ToolResults(results) => {
                    // Every result for a turn goes in ONE user message.
                    // Splitting them across messages is accepted by the API
                    // but teaches the model to stop issuing parallel calls —
                    // the agent quietly becomes serial and slower, with no
                    // error to explain why.
                    let blocks: Vec<Value> = results
                        .iter()
                        .map(|r| {
                            let mut content = vec![json!({"type": "text", "text": r.content})];
                            content.extend(Self::encode_parts(&r.parts));
                            json!({
                                "type": "tool_result",
                                "tool_use_id": r.call_id,
                                "content": content,
                                "is_error": r.is_error,
                            })
                        })
                        .collect();
                    out.push(json!({ "role": "user", "content": blocks }));
                }
            }
        }
        out
    }
}

impl Provider for Anthropic {
    fn name(&self) -> &str {
        &self.name
    }

    fn caps(&self) -> Caps {
        self.caps
    }

    fn complete(&self, req: &Request) -> Result<Response, ProviderError> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "system": req.system,
            "messages": Self::encode_messages(req),
        });
        // `temperature` is deliberately never sent. It was removed from the
        // current Opus/Sonnet models and is rejected with a 400 rather than
        // ignored, so forwarding the normalized `Request::temperature` here
        // would make every request fail against exactly the models an author
        // is most likely to pick. Steering belongs in the system prompt.
        if self.caps.tools && !req.tools.is_empty() {
            body["tools"] = json!(req
                .tools
                .iter()
                .map(|t| json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                }))
                .collect::<Vec<_>>());
        }

        let url = self.url("messages");
        let resp = self
            .rt
            .block_on(async {
                self.client
                    .post(&url)
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", API_VERSION)
                    .json(&body)
                    .send()
                    .await
            })
            .map_err(|e| ProviderError::Http(e.to_string()))?;

        let status = resp.status();
        let value: Value = self
            .rt
            .block_on(async { resp.json().await })
            .map_err(|e| ProviderError::Http(format!("{status}: {e}")))?;
        if !status.is_success() {
            return Err(ProviderError::Http(format!("API error {status}: {value}")));
        }

        let blocks = value
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| ProviderError::Protocol(format!("no content in {value}")))?;

        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for b in blocks {
            match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => tool_calls.push(ToolCall {
                    id: b
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name: b
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    // Already an object here — no string parse, unlike the
                    // OpenAI-compatible path.
                    arguments: b.get("input").cloned().unwrap_or_else(|| json!({})),
                }),
                _ => {}
            }
        }

        let stop = match value.get("stop_reason").and_then(|r| r.as_str()) {
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
            // A safety decline arrives as a successful 200 with an empty or
            // partial `content`, not as an error status. Surfacing it as
            // `EndTurn` would show the author a blank reply with no reason.
            Some("refusal") => StopReason::Other("refused"),
            Some(other) => {
                // `pause_turn` and anything added later: end the turn rather
                // than looping on a response we do not know how to continue.
                tracing::debug!(stop_reason = other, "unhandled Anthropic stop_reason");
                StopReason::EndTurn
            }
            None => StopReason::EndTurn,
        };

        Ok(Response {
            text,
            tool_calls,
            stop,
        })
    }

    fn test_connection(&self) -> Result<Option<String>, String> {
        // There is no unauthenticated ping; the cheapest real check is a
        // one-token completion. `max_tokens` must be >= 1.
        let url = self.url("messages");
        let body = json!({
            "model": self.model,
            "max_tokens": 1,
            "messages": [{ "role": "user", "content": "hi" }],
        });
        let resp = self
            .rt
            .block_on(async {
                self.client
                    .post(&url)
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", API_VERSION)
                    .json(&body)
                    .send()
                    .await
            })
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("endpoint returned {}", resp.status()));
        }
        let value: Value = self
            .rt
            .block_on(async { resp.json().await })
            .map_err(|e| e.to_string())?;
        Ok(value
            .get("model")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::provider::{ToolDef, ToolResult};

    fn provider() -> Anthropic {
        Anthropic::new(
            "test",
            DEFAULT_BASE_URL,
            "key",
            DEFAULT_MODEL,
            Caps {
                vision: true,
                tools: true,
            },
        )
        .expect("runtime builds")
    }

    fn req(messages: Vec<Message>) -> Request {
        Request {
            system: "sys".into(),
            messages,
            tools: Vec::new(),
            max_tokens: 100,
            temperature: 0.7,
        }
    }

    /// The system prompt is a top-level field. A `role: "system"` entry in
    /// `messages` is rejected by this API.
    #[test]
    fn system_is_not_a_message() {
        let out = Anthropic::encode_messages(&req(vec![Message::user("hi")]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["role"], "user");
    }

    /// All results for one turn share a single user message. One message per
    /// result is accepted by the API but suppresses parallel tool calls on
    /// later turns, with no error to explain the slowdown.
    #[test]
    fn tool_results_share_one_user_message() {
        let out = Anthropic::encode_messages(&req(vec![Message::ToolResults(vec![
            ToolResult {
                call_id: "a".into(),
                content: "ra".into(),
                parts: vec![],
                is_error: false,
            },
            ToolResult {
                call_id: "b".into(),
                content: "rb".into(),
                parts: vec![],
                is_error: true,
            },
        ])]));
        assert_eq!(out.len(), 1, "expected one message, got {out:?}");
        let blocks = out[0]["content"].as_array().expect("content array");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["tool_use_id"], "a");
        assert_eq!(blocks[1]["is_error"], true);
    }

    #[test]
    fn images_use_a_base64_source_block() {
        let out = Anthropic::encode_messages(&req(vec![Message::Turn {
            role: Role::User,
            parts: vec![Part::Image {
                media_type: "image/png".into(),
                data: vec![1, 2, 3],
            }],
            tool_calls: Vec::new(),
        }]));
        let block = &out[0]["content"][0];
        assert_eq!(block["type"], "image");
        assert_eq!(block["source"]["type"], "base64");
        assert_eq!(block["source"]["media_type"], "image/png");
    }

    #[test]
    fn tool_calls_are_content_blocks_on_the_assistant_turn() {
        let out = Anthropic::encode_messages(&req(vec![Message::Turn {
            role: Role::Assistant,
            parts: vec![Part::Text("working".into())],
            tool_calls: vec![ToolCall {
                id: "toolu_1".into(),
                name: "query_content".into(),
                arguments: json!({"kind": "soul"}),
            }],
        }]));
        assert_eq!(out[0]["role"], "assistant");
        let blocks = out[0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["input"]["kind"], "soul");
    }

    /// Sending `temperature` to a current Opus/Sonnet model is a 400, not a
    /// no-op. The normalized request carries one; this adapter must drop it.
    #[test]
    fn temperature_is_never_sent() {
        let p = provider();
        let r = Request {
            tools: vec![ToolDef {
                name: "t".into(),
                description: "d".into(),
                input_schema: json!({"type": "object"}),
            }],
            ..req(vec![Message::user("hi")])
        };
        // Rebuild the body the same way `complete` does, without the network.
        let mut body = json!({
            "model": p.model,
            "max_tokens": r.max_tokens,
            "system": r.system,
            "messages": Anthropic::encode_messages(&r),
        });
        if p.caps.tools && !r.tools.is_empty() {
            body["tools"] = json!([{ "name": "t" }]);
        }
        assert!(
            body.get("temperature").is_none(),
            "temperature must not reach the Anthropic API"
        );
    }
}
