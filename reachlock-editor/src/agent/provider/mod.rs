//! Provider abstraction (S101 P1).
//!
//! One normalized request/response shape, one trait, and an adapter per wire
//! format. A local model reached through Ollama and a cloud model reached
//! through its own API are the same kind of thing to everything above this
//! module — which is what lets the author switch provider per task without the
//! agent loop, the tool registry, or the UI knowing.
//!
//! The normalization is deliberate rather than cosmetic: tool calling and
//! image input are shaped differently by every vendor. OpenAI puts tool calls
//! in `choices[].message.tool_calls` and expects results back as messages with
//! `role: "tool"`; Anthropic puts them in `content` blocks of type `tool_use`
//! and expects `tool_result` blocks inside a *user* message. Papering over
//! that with one JSON body is how a provider-agnostic layer ends up supporting
//! exactly one provider properly.

// This module is the wire contract, not a feature: both adapters already
// *read* every variant below, but nothing *constructs* an image part, an
// assistant turn, or a tool-result message until the agent loop lands (S101
// P4–P6). Writing the contract once and filling it in is the point — the
// alternative is reshaping the trait mid-sprint and re-testing both adapters.
//
// Module-scoped and documented, per `scripts/check_dead_code.py`: the banned
// thing is a crate-root blanket that switches the compiler's own
// unreachability detection off for everything beneath it.
#![allow(dead_code)]

pub mod anthropic;
pub mod openai;

use std::fmt;

/// What a provider/model pair can actually do.
///
/// Read before building a request, not after failing one: a text-only local
/// model should be handed a text description instead of an image, and a model
/// with no tool support should not be sent tool definitions it will ignore
/// (some endpoints hard-error on an unknown field rather than dropping it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// Accepts image parts in user messages.
    pub vision: bool,
    /// Supports server-side tool calling.
    pub tools: bool,
}

/// One piece of a message. A message can mix text and images.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    /// A rendered preview. `media_type` is a MIME type ("image/png"); `data`
    /// is the raw bytes, base64-encoded by the adapter at send time rather
    /// than here, because the two APIs disagree about where the encoding goes.
    Image {
        media_type: String,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// A model's request to run one tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    /// Provider-assigned id. Echoed back with the result so the model can pair
    /// them up when several calls are in flight in one turn.
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The outcome of running one tool, on its way back to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    /// Rendered result. Tools that produce an image put it in `parts`.
    pub content: String,
    pub parts: Vec<Part>,
    pub is_error: bool,
}

/// One turn in the conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Author input, or a tool result being handed back.
    Turn {
        role: Role,
        parts: Vec<Part>,
        /// Tool calls the assistant asked for in this turn (assistant only).
        tool_calls: Vec<ToolCall>,
    },
    /// Results for the tool calls of the preceding assistant turn.
    ToolResults(Vec<ToolResult>),
}

impl Message {
    pub fn user(text: impl Into<String>) -> Message {
        Message::Turn {
            role: Role::User,
            parts: vec![Part::Text(text.into())],
            tool_calls: Vec::new(),
        }
    }
}

/// A tool as advertised to the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object.
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDef>,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    /// Why the model stopped. Used by the loop to decide whether to continue.
    pub stop: StopReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The model finished its turn with nothing outstanding.
    EndTurn,
    /// The model wants tools run before it continues.
    ToolUse,
    /// Hit the token ceiling. The loop surfaces this rather than looping on a
    /// truncated turn forever.
    MaxTokens,
    Other(&'static str),
}

#[derive(Debug, Clone)]
pub enum ProviderError {
    Http(String),
    /// The endpoint answered, but not in the shape this adapter expects.
    Protocol(String),
    /// The request asked for something this provider/model cannot do.
    Unsupported(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Http(e) => write!(f, "connection error: {e}"),
            ProviderError::Protocol(e) => write!(f, "unexpected response: {e}"),
            ProviderError::Unsupported(e) => write!(f, "not supported: {e}"),
        }
    }
}

/// A model endpoint the agent can talk to.
///
/// `Send + Sync` because the agent loop runs on its own thread — the UI thread
/// owns the editors, and `Box<dyn Editor>` cannot cross that boundary, so the
/// provider goes to the loop rather than the other way round.
pub trait Provider: Send + Sync {
    /// Display name for the transcript and the status bar.
    fn name(&self) -> &str;

    fn caps(&self) -> Caps;

    /// One completion round trip. Blocking: the caller is already on a
    /// dedicated thread, and an async signature here would force a runtime
    /// into the tool dispatcher for no benefit.
    fn complete(&self, req: &Request) -> Result<Response, ProviderError>;

    /// Probe reachability, returning a model name when the endpoint reports
    /// one. Used by the settings window's Test button.
    fn test_connection(&self) -> Result<Option<String>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_is_a_single_text_part() {
        let m = Message::user("hello");
        match m {
            Message::Turn {
                role,
                parts,
                tool_calls,
            } => {
                assert_eq!(role, Role::User);
                assert_eq!(parts, vec![Part::Text("hello".into())]);
                assert!(tool_calls.is_empty());
            }
            _ => panic!("expected a turn"),
        }
    }
}
