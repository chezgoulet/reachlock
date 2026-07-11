//! Per-tier rule enforcement (spec §7). What each universe permits — the
//! server's LLM proxy consults this; the client uses it to explain why a
//! deliberation did or didn't happen.

use super::tier::UniverseTier;

/// Inference capability granted by a universe tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceGrant {
    /// Whether LLM deliberation exists at all in this universe.
    pub llm_allowed: bool,
    /// Server-enforced model parameter cap, in billions. `None` = uncapped.
    pub model_param_cap_billions: Option<u32>,
    /// Whether the player supplies their own provider key.
    pub player_key: bool,
}

pub fn inference_grant(tier: UniverseTier) -> InferenceGrant {
    match tier {
        UniverseTier::Classic => InferenceGrant {
            llm_allowed: false,
            model_param_cap_billions: None,
            player_key: false,
        },
        UniverseTier::FairPlay => InferenceGrant {
            llm_allowed: true,
            model_param_cap_billions: Some(8),
            player_key: false,
        },
        UniverseTier::Spectrum => InferenceGrant {
            llm_allowed: true,
            model_param_cap_billions: None,
            player_key: false,
        },
        UniverseTier::Byok => InferenceGrant {
            llm_allowed: true,
            model_param_cap_billions: None,
            player_key: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_never_calls_an_llm() {
        assert!(!inference_grant(UniverseTier::Classic).llm_allowed);
    }

    #[test]
    fn fair_play_is_capped() {
        let grant = inference_grant(UniverseTier::FairPlay);
        assert!(grant.llm_allowed);
        assert_eq!(grant.model_param_cap_billions, Some(8));
    }
}
