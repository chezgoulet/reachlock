//! OpenAI-compatible adapter.
//!
//! Covers every endpoint speaking `/v1/chat/completions`: Ollama, llama.cpp's
//! server, vLLM, LM Studio, OpenRouter, and OpenAI itself. This is the local
//! model's path as much as the cloud's — the only difference is the base URL
//! and whether the key means anything.

use base64::Engine as _;
use serde_json::{json, Value};

use super::{
    Caps, Message, Part, Provider, ProviderError, Request, Response, Role, StopReason, ToolCall,
};

pub struct OpenAiCompat {
    name: String,
    base_url: String,
    api_key: String,
    model: String,
    caps: Caps,
    rt: tokio::runtime::Runtime,
    client: reqwest::Client,
}

impl OpenAiCompat {
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        caps: Caps,
    ) -> Result<Self, String> {
        // One runtime per provider, built once and reused. The pre-S101 code
        // built a fresh multi-thread runtime per generation; an agent loop
        // makes many calls per task, and standing up a thread pool for each
        // would dominate the latency of a small local model.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("could not start async runtime: {e}"))?;
        Ok(OpenAiCompat {
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

    /// Render our normalized messages into OpenAI's `messages` array.
    ///
    /// Tool results become their own `role: "tool"` entries keyed by
    /// `tool_call_id`, which is why `ToolResult` carries the call id: the
    /// model pairs them itself, and getting this wrong silently drops results
    /// on the floor when more than one tool runs in a turn.
    fn encode_messages(&self, req: &Request) -> Vec<Value> {
        let mut out = vec![json!({ "role": "system", "content": req.system })];
        for m in &req.messages {
            match m {
                Message::Turn {
                    role,
                    parts,
                    tool_calls,
                } => {
                    let role_str = match role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                    };
                    let mut msg = json!({ "role": role_str });
                    // A text-only message keeps the plain string form: some
                    // local servers reject the content-array form outright.
                    let only_text = parts.iter().all(|p| matches!(p, Part::Text(_)));
                    if only_text {
                        let text: String = parts
                            .iter()
                            .filter_map(|p| match p {
                                Part::Text(t) => Some(t.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        msg["content"] = json!(text);
                    } else {
                        msg["content"] = json!(parts
                            .iter()
                            .map(|p| match p {
                                Part::Text(t) => json!({ "type": "text", "text": t }),
                                Part::Image { media_type, data } => {
                                    let b64 =
                                        base64::engine::general_purpose::STANDARD.encode(data);
                                    json!({
                                        "type": "image_url",
                                        "image_url": {
                                            "url": format!("data:{media_type};base64,{b64}")
                                        }
                                    })
                                }
                            })
                            .collect::<Vec<_>>());
                    }
                    if !tool_calls.is_empty() {
                        msg["tool_calls"] = json!(tool_calls
                            .iter()
                            .map(|c| json!({
                                "id": c.id,
                                "type": "function",
                                "function": {
                                    "name": c.name,
                                    "arguments": c.arguments.to_string(),
                                }
                            }))
                            .collect::<Vec<_>>());
                    }
                    out.push(msg);
                }
                Message::ToolResults(results) => {
                    for r in results {
                        out.push(json!({
                            "role": "tool",
                            "tool_call_id": r.call_id,
                            "content": r.content,
                        }));
                    }
                }
            }
        }
        out
    }
}

impl Provider for OpenAiCompat {
    fn name(&self) -> &str {
        &self.name
    }

    fn caps(&self) -> Caps {
        self.caps
    }

    fn complete(&self, req: &Request) -> Result<Response, ProviderError> {
        let mut body = json!({
            "model": self.model,
            "messages": self.encode_messages(req),
            "max_tokens": req.max_tokens,
            "temperature": req.temperature,
        });
        // Only send `tools` when the model claims support. Several local
        // servers 400 on an unrecognised top-level field rather than ignoring
        // it, which would make every request fail instead of just degrading.
        if self.caps.tools && !req.tools.is_empty() {
            body["tools"] = json!(req
                .tools
                .iter()
                .map(|t| json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema,
                    }
                }))
                .collect::<Vec<_>>());
        }

        let url = self.url("chat/completions");
        let resp = self
            .rt
            .block_on(async {
                self.client
                    .post(&url)
                    .bearer_auth(&self.api_key)
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

        let choice = value
            .get("choices")
            .and_then(|c| c.get(0))
            .ok_or_else(|| ProviderError::Protocol(format!("no choices in {value}")))?;
        let msg = choice
            .get("message")
            .ok_or_else(|| ProviderError::Protocol("choice has no message".into()))?;

        let text = msg
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();

        let mut tool_calls = Vec::new();
        if let Some(calls) = msg.get("tool_calls").and_then(|c| c.as_array()) {
            for c in calls {
                let f = c.get("function");
                let name = f
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string();
                // Arguments arrive as a JSON *string*, not an object. A model
                // that emits an empty string means "no arguments"; treating
                // that as a parse failure would break zero-arg tools.
                let raw = f
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}");
                let arguments = if raw.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(raw).unwrap_or_else(|_| json!({ "_raw": raw }))
                };
                tool_calls.push(ToolCall {
                    id: c
                        .get("id")
                        .and_then(|i| i.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    name,
                    arguments,
                });
            }
        }

        let stop = match choice.get("finish_reason").and_then(|r| r.as_str()) {
            Some("tool_calls") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            Some("stop") => StopReason::EndTurn,
            _ if !tool_calls.is_empty() => StopReason::ToolUse,
            _ => StopReason::EndTurn,
        };

        Ok(Response {
            text,
            tool_calls,
            stop,
        })
    }

    fn test_connection(&self) -> Result<Option<String>, String> {
        let url = self.url("models");
        let resp = self
            .rt
            .block_on(async {
                self.client
                    .get(&url)
                    .bearer_auth(&self.api_key)
                    .send()
                    .await
            })
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("endpoint returned {}", resp.status()));
        }
        let body: Value = self
            .rt
            .block_on(async { resp.json().await })
            .map_err(|e| e.to_string())?;
        Ok(body
            .get("data")
            .and_then(|d| d.get(0))
            .and_then(|m| m.get("id"))
            .and_then(|id| id.as_str())
            .map(|s| s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::provider::{Part, ToolResult};

    fn provider() -> OpenAiCompat {
        OpenAiCompat::new(
            "test",
            "http://localhost:11434/v1",
            "ollama",
            "llama3.2:3b",
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
            temperature: 0.0,
        }
    }

    #[test]
    fn text_only_messages_keep_the_plain_string_form() {
        let p = provider();
        let out = p.encode_messages(&req(vec![Message::user("hi")]));
        assert_eq!(out[1]["content"], json!("hi"));
    }

    #[test]
    fn images_become_data_uri_parts() {
        let p = provider();
        let out = p.encode_messages(&req(vec![Message::Turn {
            role: Role::User,
            parts: vec![
                Part::Text("look".into()),
                Part::Image {
                    media_type: "image/png".into(),
                    data: vec![1, 2, 3],
                },
            ],
            tool_calls: Vec::new(),
        }]));
        let content = out[1]["content"].as_array().expect("array form");
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
        let url = content[1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "got {url}");
    }

    /// Each result becomes its own `role: "tool"` message keyed by call id.
    /// Collapsing several results into one message loses all but the first.
    #[test]
    fn tool_results_are_one_message_each() {
        let p = provider();
        let out = p.encode_messages(&req(vec![Message::ToolResults(vec![
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
                is_error: false,
            },
        ])]));
        assert_eq!(out.len(), 3, "system + two tool messages");
        assert_eq!(out[1]["tool_call_id"], "a");
        assert_eq!(out[2]["tool_call_id"], "b");
    }
}
