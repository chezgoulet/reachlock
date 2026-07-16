//! LLM providers behind the proxy (S14, spec §7/§8). The `Provider` trait
//! is the frozen seam: `complete(InferenceRequest) -> InferenceResponse |
//! ProviderError`. Three implementations:
//!
//! - [`Stub`] — the deterministic dev/CI provider (previous behavior).
//! - [`OpenAiCompat`] — any OpenAI-compatible chat endpoint (OpenRouter,
//!   cloud providers, most BYOK targets).
//! - [`OllamaNative`] — Ollama's native `/api/chat` (v1 lesson: the
//!   OpenAI-compat endpoint ignored `think: false` on reasoning models, so
//!   we use the native API and strip reasoning traces regardless).
//!
//! Every path has a hard timeout and maps into the error taxonomy
//! (Timeout / RateLimited / Provider / BadResponse) that S15's failure
//! model consumes. `context_json` is never logged above debug.

use std::time::Duration;

use serde::Deserialize;

/// The server-side ceiling on any single inference call. A contract may ask
/// for less via `timeout_ms`; it can never ask for more.
pub const SERVER_TIMEOUT_CAP: Duration = Duration::from_secs(15);

/// What the proxy asks a provider for.
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub system_prompt: String,
    pub context_json: serde_json::Value,
    pub max_tokens: u32,
    /// Already clamped to [`SERVER_TIMEOUT_CAP`] by the router.
    pub timeout: Duration,
}

/// A shaped model answer: the action verb the contract engine executes and
/// the reasoning line the deliberation UI shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceResponse {
    pub action: String,
    pub reasoning: String,
}

/// The S14 error taxonomy. Everything a provider can do wrong becomes one
/// of these, and every one of these becomes `llm.failed { reason }` on the
/// wire — a clean failure, never a hang (spec §18: fail states are game
/// states).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    Timeout,
    RateLimited,
    /// Transport/HTTP/provider-side trouble. The string is for the server
    /// log, not the player.
    Provider(String),
    /// The model answered, but not in the shape asked for — spec §18's
    /// model-collapse failure mode. S15 turns this into fiction.
    BadResponse(String),
}

impl ProviderError {
    /// The `llm.failed { reason }` wire word.
    pub fn reason(&self) -> &'static str {
        match self {
            ProviderError::Timeout => "timeout",
            ProviderError::RateLimited => "rate_limited",
            ProviderError::Provider(_) => "provider_error",
            ProviderError::BadResponse(_) => "bad_response",
        }
    }
}

/// The frozen provider seam (S14 "freeze first").
pub trait Provider: Send + Sync {
    fn complete(
        &self,
        req: InferenceRequest,
    ) -> impl std::future::Future<Output = Result<InferenceResponse, ProviderError>> + Send;
}

/// Wrapper prompt: whatever the contract's own system prompt says, the
/// model must answer as `{ "action": ..., "reasoning": ... }` JSON so the
/// contract engine can execute the verb.
pub fn shaping_prompt(contract_prompt: &str) -> String {
    format!(
        "{contract_prompt}\n\n\
         Respond with ONLY a JSON object of the form \
         {{\"action\": \"<verb>\", \"reasoning\": \"<one sentence>\"}}. \
         No prose before or after the JSON. No markdown fences."
    )
}

/// Parse a raw model reply into the shaped response. Tolerates the common
/// model sins — reasoning traces (`<think>…</think>`), markdown fences,
/// prose around the JSON — and rejects the rest as [`ProviderError::BadResponse`].
pub fn shape_response(raw: &str) -> Result<InferenceResponse, ProviderError> {
    let cleaned = strip_reasoning_traces(raw);
    let candidate = extract_json_object(&cleaned)
        .ok_or_else(|| ProviderError::BadResponse("no JSON object in reply".into()))?;

    #[derive(Deserialize)]
    struct Shaped {
        action: String,
        #[serde(default)]
        reasoning: String,
    }
    let shaped: Shaped = serde_json::from_str(candidate)
        .map_err(|e| ProviderError::BadResponse(format!("unparseable JSON: {e}")))?;
    if shaped.action.trim().is_empty() {
        return Err(ProviderError::BadResponse("empty action".into()));
    }
    Ok(InferenceResponse {
        action: shaped.action,
        reasoning: shaped.reasoning,
    })
}

/// Drop `<think>…</think>` (and unterminated `<think>…`) blocks — v1
/// lesson: reasoning models leak traces even when asked not to think.
fn strip_reasoning_traces(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Find the first balanced `{ … }` block (models love wrapping JSON in
/// prose or ``` fences).
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in text[start..].char_indices() {
        if in_string {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => in_string = false,
                _ => escaped = false,
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=start + i]);
                }
            }
            _ => {}
        }
    }
    None
}

// ───────────────────────────── Stub ─────────────────────────────

/// How long the stub "thinks" — nonzero so clients exercise their
/// deliberation UI (spec §6: latency is deliberation, not lag).
pub const STUB_DELIBERATION_MS: u64 = 400;

/// The deterministic dev/CI provider: conservative default action with a
/// reasoning line that names what it saw. This is the previous
/// `llm_proxy` stub behavior, kept as a first-class provider.
#[derive(Debug, Clone, Default)]
pub struct Stub;

impl Provider for Stub {
    async fn complete(&self, req: InferenceRequest) -> Result<InferenceResponse, ProviderError> {
        tokio::time::sleep(Duration::from_millis(STUB_DELIBERATION_MS)).await;
        let seen = req
            .context_json
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        Ok(InferenceResponse {
            action: "maintain_course".into(),
            reasoning: format!(
                "[stub] Rules didn't cover this. Observed: {seen}. \
                 Holding course until a real inference provider is wired in."
            ),
        })
    }
}

// ─────────────────────── OpenAI-compatible ───────────────────────

/// Any OpenAI-compatible `/v1/chat/completions` endpoint. Covers cloud
/// providers, OpenRouter, and most BYOK targets.
#[derive(Debug, Clone)]
pub struct OpenAiCompat {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

impl Provider for OpenAiCompat {
    async fn complete(&self, req: InferenceRequest) -> Result<InferenceResponse, ProviderError> {
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": [
                { "role": "system", "content": shaping_prompt(&req.system_prompt) },
                { "role": "user", "content": req.context_json.to_string() },
            ],
        });
        let client = http_client(req.timeout)?;
        let mut http = client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            http = http.bearer_auth(key);
        }
        let reply = send(http).await?;

        #[derive(Deserialize)]
        struct Choice {
            message: ChoiceMessage,
        }
        #[derive(Deserialize)]
        struct ChoiceMessage {
            content: String,
        }
        #[derive(Deserialize)]
        struct Completion {
            choices: Vec<Choice>,
        }
        let completion: Completion = serde_json::from_str(&reply)
            .map_err(|e| ProviderError::BadResponse(format!("completion shape: {e}")))?;
        let content = completion
            .choices
            .first()
            .map(|c| c.message.content.as_str())
            .ok_or_else(|| ProviderError::BadResponse("no choices".into()))?;
        shape_response(content)
    }
}

// ───────────────────────── Ollama native ─────────────────────────

/// Ollama's native `/api/chat` (non-streaming), with `think: false` — and
/// trace-stripping anyway, because v1 learned not to trust that flag.
#[derive(Debug, Clone)]
pub struct OllamaNative {
    pub base_url: String,
    pub model: String,
}

impl Provider for OllamaNative {
    async fn complete(&self, req: InferenceRequest) -> Result<InferenceResponse, ProviderError> {
        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "think": false,
            "options": { "num_predict": req.max_tokens },
            "messages": [
                { "role": "system", "content": shaping_prompt(&req.system_prompt) },
                { "role": "user", "content": req.context_json.to_string() },
            ],
        });
        let client = http_client(req.timeout)?;
        let reply = send(client.post(&url).json(&body)).await?;

        #[derive(Deserialize)]
        struct ChatMessage {
            content: String,
        }
        #[derive(Deserialize)]
        struct ChatReply {
            message: ChatMessage,
        }
        let chat: ChatReply = serde_json::from_str(&reply)
            .map_err(|e| ProviderError::BadResponse(format!("chat shape: {e}")))?;
        shape_response(&chat.message.content)
    }
}

// ──────────────────────── shared HTTP plumbing ────────────────────────

fn http_client(timeout: Duration) -> Result<reqwest::Client, ProviderError> {
    reqwest::Client::builder()
        .timeout(timeout.min(SERVER_TIMEOUT_CAP))
        .build()
        .map_err(|e| ProviderError::Provider(format!("client build: {e}")))
}

async fn send(req: reqwest::RequestBuilder) -> Result<String, ProviderError> {
    let response = req.send().await.map_err(|e| {
        if e.is_timeout() {
            ProviderError::Timeout
        } else {
            ProviderError::Provider(redact(&e.to_string()))
        }
    })?;
    let status = response.status();
    if status.as_u16() == 429 {
        return Err(ProviderError::RateLimited);
    }
    if !status.is_success() {
        return Err(ProviderError::Provider(format!("http {status}")));
    }
    response
        .text()
        .await
        .map_err(|e| ProviderError::Provider(redact(&e.to_string())))
}

/// Keys ride in headers, but belt-and-braces: never let anything that looks
/// like a bearer token into an error string.
fn redact(s: &str) -> String {
    if s.to_ascii_lowercase().contains("bearer") {
        "provider transport error (detail redacted)".into()
    } else {
        s.to_string()
    }
}

/// Storage for the per-tier provider selection: the trait keeps the seam,
/// the enum keeps it object-safe-free and configurable.
#[derive(Debug, Clone)]
pub enum AnyProvider {
    Stub(Stub),
    OpenAiCompat(OpenAiCompat),
    OllamaNative(OllamaNative),
}

impl Provider for AnyProvider {
    async fn complete(&self, req: InferenceRequest) -> Result<InferenceResponse, ProviderError> {
        match self {
            AnyProvider::Stub(p) => p.complete(req).await,
            AnyProvider::OpenAiCompat(p) => p.complete(req).await,
            AnyProvider::OllamaNative(p) => p.complete(req).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_accepts_clean_json() {
        let r = shape_response(r#"{"action":"wake_crew","reasoning":"anomaly"}"#).unwrap();
        assert_eq!(r.action, "wake_crew");
        assert_eq!(r.reasoning, "anomaly");
    }

    #[test]
    fn shape_tolerates_fences_prose_and_think_traces() {
        let raw = "<think>hmm, what would Boris do…</think>\
                   Sure! Here's my answer:\n```json\n\
                   {\"action\":\"maintain_course\",\"reasoning\":\"nothing new\"}\n```";
        let r = shape_response(raw).unwrap();
        assert_eq!(r.action, "maintain_course");
    }

    #[test]
    fn shape_rejects_garbage_as_bad_response() {
        for garbage in ["", "I think we should go left", "{\"action\": }", "{}"] {
            match shape_response(garbage) {
                Err(ProviderError::BadResponse(_)) => {}
                other => panic!("{garbage:?} should be BadResponse, got {other:?}"),
            }
        }
    }

    #[test]
    fn shape_survives_braces_inside_strings() {
        let raw = r#"{"action":"say","reasoning":"the sign read {closed}"}"#;
        let r = shape_response(raw).unwrap();
        assert_eq!(r.reasoning, "the sign read {closed}");
    }

    #[test]
    fn unterminated_think_trace_is_dropped() {
        assert!(shape_response("<think>forever…").is_err());
        let ok = shape_response("{\"action\":\"a\",\"reasoning\":\"b\"}<think>tail");
        assert!(ok.is_ok());
    }

    #[tokio::test]
    async fn stub_is_deterministic_and_names_what_it_saw() {
        let stub = Stub;
        let req = InferenceRequest {
            system_prompt: "x".into(),
            context_json: serde_json::json!({"unknown_signal": 1}),
            max_tokens: 64,
            timeout: Duration::from_secs(1),
        };
        let r = stub.complete(req).await.unwrap();
        assert_eq!(r.action, "maintain_course");
        assert!(r.reasoning.contains("unknown_signal"));
    }
}
