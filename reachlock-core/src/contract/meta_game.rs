//! Contract meta-game (S34): seasoned trust bonus and contract evolution.
//! Pure functions — no I/O, no game state.

use serde::{Deserialize, Serialize};

/// Derived from a contract's service history. Higher values mean the crew
/// trusts this contract more, which translates to shorter LLM deliberation
/// latency and richer persona context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonedBonus {
    /// Additional trust points (0..256, fixed-point, 1024 = 1.0).
    pub trust_bonus: i64,
    /// How many extra context lines to inject into the LLM system prompt.
    pub personality_depth: usize,
}

/// Calculate the seasoned bonus for a contract based on its service history.
/// `service_sessions` = number of game sessions the contract has been active.
/// `deliberation_count` = number of times the contract has reached the LLM edge.
pub fn seasoned_bonus(service_sessions: u32, deliberation_count: u32) -> SeasonedBonus {
    let trust_bonus =
        (service_sessions.min(20) as i64 * 12) + (deliberation_count.min(50) as i64 * 5);
    let trust_bonus = trust_bonus.min(256);

    let depth = if service_sessions >= 10 && deliberation_count >= 20 {
        3
    } else if service_sessions >= 5 && deliberation_count >= 10 {
        2
    } else if service_sessions >= 2 && deliberation_count >= 5 {
        1
    } else {
        0
    };

    SeasonedBonus {
        trust_bonus,
        personality_depth: depth,
    }
}

/// A contract evolution record: one change to the contract rules, logged when
/// the player chooses to "evolve" after a major story event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractEvolution {
    pub contract_id: String,
    pub previous_version: u32,
    pub new_version: u32,
    pub reason: String, // narrative justification ("After the Veil incident…")
    pub rule_changes: Vec<String>, // human-readable summaries of what changed
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_contract_no_bonus() {
        let b = seasoned_bonus(0, 0);
        assert_eq!(b.trust_bonus, 0);
        assert_eq!(b.personality_depth, 0);
    }

    #[test]
    fn seasoned_contract_earns_bonus() {
        let b = seasoned_bonus(15, 30);
        assert!(b.trust_bonus > 200);
        assert_eq!(b.personality_depth, 3);
    }

    #[test]
    fn bonus_is_capped() {
        let b = seasoned_bonus(100, 500);
        assert_eq!(b.trust_bonus, 256);
        assert_eq!(b.personality_depth, 3);
    }

    #[test]
    fn mid_seasoning_earns_depth_2() {
        let b = seasoned_bonus(7, 12);
        assert_eq!(b.personality_depth, 2);
    }

    #[test]
    fn light_seasoning_earns_depth_1() {
        let b = seasoned_bonus(3, 5);
        assert_eq!(b.personality_depth, 1);
    }
}
