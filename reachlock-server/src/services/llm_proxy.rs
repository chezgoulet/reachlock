//! LLM proxy routing (S14, spec §7/§8). Tier gating is unchanged; behind
//! it, `llm.call` now reaches real providers: FairPlay → a local
//! Ollama/llama.cpp endpoint, Spectrum → an OpenAI-compatible cloud
//! endpoint, BYOK → the player's own registered key. Unset env = the
//! deterministic [`providers::Stub`] (dev default, CI default — no network
//! in CI).
//!
//! Config (all optional):
//! - `REACHLOCK_FAIRPLAY_URL` / `REACHLOCK_FAIRPLAY_MODEL` — Ollama native.
//!   The FairPlay deployment promise is a small local model (≤8B); that cap
//!   is a deployment choice expressed here in config, not enforced in code.
//! - `REACHLOCK_SPECTRUM_URL` / `REACHLOCK_SPECTRUM_KEY` /
//!   `REACHLOCK_SPECTRUM_MODEL` — OpenAI-compatible.
//! - `REACHLOCK_BYOK_KEY` — 64 hex chars; enables `POST /byok`.
//!
//! Every failure becomes a clean `llm.failed { reason }` — the proxy never
//! hangs a session (hard cap [`providers::SERVER_TIMEOUT_CAP`]).

use reachlock_core::universe::rules::inference_grant;
use reachlock_core::universe::UniverseTier;

use super::byok::{ByokError, ByokService};
use super::limiter::{MemoryRateLimiter, RateLimiter};
use super::metrics::LatencyHistogram;
use super::providers::{
    AnyProvider, InferenceRequest, OllamaNative, OpenAiCompat, Provider, ProviderError, Stub,
};

/// Default per-call budget when the contract doesn't say otherwise
/// (`LlmConfig::timeout_ms` isn't on the `llm.call` wire shape yet — S15/16
/// may add it; until then the server default rules).
const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const DEFAULT_MAX_TOKENS: u32 = 256;

pub use super::providers::STUB_DELIBERATION_MS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmResponse {
    pub action: String,
    pub reasoning: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// Classic universe: no inference exists here, by design.
    NoInferenceTier,
    /// Token bucket empty: the crew is overwhelmed. A valid fiction.
    RateLimited,
    /// Provider-level failure (timeout / transport / bad shape). The reason
    /// string is the `llm.failed { reason }` wire word.
    Failed(&'static str),
}

impl LlmError {
    pub fn reason(&self) -> &'static str {
        match self {
            LlmError::NoInferenceTier => "no_inference_tier",
            LlmError::RateLimited => "rate_limited",
            LlmError::Failed(reason) => reason,
        }
    }
}

/// Everything `llm.call` routing needs, hung off `AppState`.
pub struct LlmService {
    fairplay: AnyProvider,
    spectrum: AnyProvider,
    pub byok: ByokService,
    limiter: Box<dyn RateLimiter>,
    pub metrics: LatencyHistogram,
}

impl Default for LlmService {
    fn default() -> Self {
        Self::from_env()
    }
}

impl LlmService {
    /// Build from environment; anything unset falls back to the stub.
    pub fn from_env() -> Self {
        let fairplay = match std::env::var("REACHLOCK_FAIRPLAY_URL") {
            Ok(url) if !url.is_empty() => AnyProvider::OllamaNative(OllamaNative {
                base_url: url,
                model: std::env::var("REACHLOCK_FAIRPLAY_MODEL")
                    .unwrap_or_else(|_| "llama3.2:3b".into()),
            }),
            _ => AnyProvider::Stub(Stub),
        };
        let spectrum = match std::env::var("REACHLOCK_SPECTRUM_URL") {
            Ok(url) if !url.is_empty() => AnyProvider::OpenAiCompat(OpenAiCompat {
                base_url: url,
                api_key: std::env::var("REACHLOCK_SPECTRUM_KEY")
                    .ok()
                    .filter(|k| !k.is_empty()),
                model: std::env::var("REACHLOCK_SPECTRUM_MODEL")
                    .unwrap_or_else(|_| "gpt-4o-mini".into()),
            }),
            _ => AnyProvider::Stub(Stub),
        };
        LlmService {
            fairplay,
            spectrum,
            byok: ByokService::default(),
            limiter: Box::new(MemoryRateLimiter::default()),
            metrics: LatencyHistogram::default(),
        }
    }

    /// Test constructor: explicit providers, tight limiter.
    #[cfg(test)]
    pub fn for_test(
        fairplay: AnyProvider,
        spectrum: AnyProvider,
        limiter: Box<dyn RateLimiter>,
    ) -> Self {
        LlmService {
            fairplay,
            spectrum,
            byok: ByokService {
                crypto: None,
                store: Box::new(super::byok::MemoryByokStore::default()),
            },
            limiter,
            metrics: LatencyHistogram::default(),
        }
    }

    /// Route one `llm.call`: tier gate → rate limit → provider → shaped
    /// response. Latency is recorded either way. This is the S14 version of
    /// the original `route_llm_call` — same shape, real guts.
    pub async fn route(
        &self,
        tier: UniverseTier,
        player_id: &str,
        contract_id: &str,
        context: &serde_json::Value,
    ) -> Result<LlmResponse, LlmError> {
        if !inference_grant(tier).llm_allowed {
            return Err(LlmError::NoInferenceTier);
        }
        if !self.limiter.try_acquire(player_id, tier) {
            tracing::info!(player = %player_id, ?tier, "llm call rate-limited");
            return Err(LlmError::RateLimited);
        }

        let request = InferenceRequest {
            system_prompt: format!(
                "You are the deliberation engine for ship contract '{contract_id}' \
                 in the game REACHLOCK. Decide the crew's next action from the \
                 context object."
            ),
            context_json: context.clone(),
            max_tokens: DEFAULT_MAX_TOKENS,
            timeout: DEFAULT_TIMEOUT,
        };

        let started = std::time::Instant::now();
        let result = match tier {
            UniverseTier::Classic => unreachable!("gated above"),
            UniverseTier::FairPlay => self.fairplay.complete(request).await,
            UniverseTier::Spectrum => self.spectrum.complete(request).await,
            UniverseTier::Byok => match self.byok.credentials(player_id) {
                Ok(creds) => {
                    let provider = OpenAiCompat {
                        base_url: creds.base_url,
                        api_key: Some(creds.api_key),
                        model: creds.model,
                    };
                    provider.complete(request).await
                }
                Err(ByokError::NoKeyRegistered) => {
                    Err(ProviderError::Provider("no BYOK key registered".into()))
                }
                Err(_) => Err(ProviderError::Provider("BYOK unavailable".into())),
            },
        };
        let latency = started.elapsed().as_millis() as u64;
        self.metrics.record(latency, result.is_err());

        match result {
            Ok(response) => {
                tracing::debug!(
                    player = %player_id,
                    contract = %contract_id,
                    latency_ms = latency,
                    "llm call completed"
                );
                Ok(LlmResponse {
                    action: response.action,
                    reasoning: response.reasoning,
                })
            }
            Err(e) => {
                // The taxonomy detail goes to the server log (never the
                // context payload); the wire gets the clean reason word.
                tracing::warn!(
                    player = %player_id,
                    contract = %contract_id,
                    latency_ms = latency,
                    error = ?e,
                    "llm call failed"
                );
                Err(match e {
                    ProviderError::RateLimited => LlmError::RateLimited,
                    other => LlmError::Failed(other.reason()),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::limiter::MemoryRateLimiter;

    fn stub_service() -> LlmService {
        LlmService::for_test(
            AnyProvider::Stub(Stub),
            AnyProvider::Stub(Stub),
            Box::new(MemoryRateLimiter::new(100.0, 0.001)),
        )
    }

    #[tokio::test]
    async fn classic_gets_no_inference() {
        let svc = stub_service();
        let r = svc
            .route(UniverseTier::Classic, "tib", "c", &serde_json::json!({}))
            .await;
        assert_eq!(r, Err(LlmError::NoInferenceTier));
    }

    #[tokio::test]
    async fn fair_play_gets_a_response() {
        let svc = stub_service();
        let r = svc
            .route(
                UniverseTier::FairPlay,
                "tib",
                "cryo-pilot",
                &serde_json::json!({"unknown_signal": 1}),
            )
            .await
            .unwrap();
        assert_eq!(r.action, "maintain_course");
        assert!(r.reasoning.contains("unknown_signal"));
    }

    #[tokio::test]
    async fn rate_limit_trips_as_rate_limited() {
        let svc = LlmService::for_test(
            AnyProvider::Stub(Stub),
            AnyProvider::Stub(Stub),
            Box::new(MemoryRateLimiter::new(1.0, 10_000.0)),
        );
        let ctx = serde_json::json!({});
        assert!(svc
            .route(UniverseTier::FairPlay, "tib", "c", &ctx)
            .await
            .is_ok());
        assert_eq!(
            svc.route(UniverseTier::FairPlay, "tib", "c", &ctx).await,
            Err(LlmError::RateLimited)
        );
    }

    #[tokio::test]
    async fn byok_without_key_fails_cleanly() {
        let svc = stub_service();
        let r = svc
            .route(UniverseTier::Byok, "tib", "c", &serde_json::json!({}))
            .await;
        assert_eq!(r, Err(LlmError::Failed("provider_error")));
    }
}
