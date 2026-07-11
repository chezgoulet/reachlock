//! LLM proxy routing (spec §7). Tier gating is real; the actual provider
//! calls are a stub for now — FairPlay/Spectrum/BYOK all answer through
//! `StubResponder`, which produces a deterministic canned deliberation so
//! the whole client UX path (deliberating → response) is exercisable
//! offline. Swapping in llama.cpp / OpenRouter / player-key HTTP calls is a
//! provider-impl change behind the same function.

use reachlock_core::universe::rules::inference_grant;
use reachlock_core::universe::UniverseTier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmResponse {
    pub action: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// Classic universe: no inference exists here, by design.
    NoInferenceTier,
}

/// How long the stub "thinks" — nonzero so clients exercise their
/// deliberation UI (spec §6: latency is deliberation, not lag).
pub const STUB_DELIBERATION_MS: u64 = 400;

pub async fn route_llm_call(
    tier: UniverseTier,
    contract_id: &str,
    context: &serde_json::Value,
) -> Result<LlmResponse, LlmError> {
    if !inference_grant(tier).llm_allowed {
        return Err(LlmError::NoInferenceTier);
    }
    tokio::time::sleep(std::time::Duration::from_millis(STUB_DELIBERATION_MS)).await;
    Ok(stub_response(contract_id, context))
}

/// Deterministic canned response: conservative default action with a
/// reasoning line that names what it saw. Enough for the client's crew-comm
/// display and for tests.
fn stub_response(contract_id: &str, context: &serde_json::Value) -> LlmResponse {
    let seen = context
        .as_object()
        .map(|o| o.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    LlmResponse {
        action: "maintain_course".into(),
        reasoning: format!(
            "[stub] Rules for '{contract_id}' didn't cover this. Observed: {seen}. \
             Holding course until a real inference provider is wired in."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn classic_gets_no_inference() {
        let r = route_llm_call(UniverseTier::Classic, "c", &serde_json::json!({})).await;
        assert_eq!(r, Err(LlmError::NoInferenceTier));
    }

    #[tokio::test]
    async fn fair_play_gets_a_response() {
        let r = route_llm_call(
            UniverseTier::FairPlay,
            "cryo-pilot",
            &serde_json::json!({"unknown_signal": 1}),
        )
        .await
        .unwrap();
        assert_eq!(r.action, "maintain_course");
        assert!(r.reasoning.contains("unknown_signal"));
    }
}
