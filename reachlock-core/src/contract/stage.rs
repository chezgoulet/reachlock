use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StagePhase {
    Examining,
    Weighing,
    Deciding,
    Verdict,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSnapshot {
    pub index: usize,
    pub label: String,
    pub action: String,
    pub condition_summary: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictSnapshot {
    pub outcome_label: String,
    pub action_taken: String,
    pub reasoning: String,
    pub escalation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSnapshot {
    pub relationship_deltas: Vec<(String, i64)>,
    pub hull_damage: Option<i64>,
    pub cargo_loss: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliberationStage {
    pub phase: StagePhase,
    pub crew_member: String,
    pub crew_portrait_id: String,
    pub crew_mood: String,
    pub trust_with_player: i64,
    pub context_summary: String,
    pub matched_rules: Vec<RuleSnapshot>,
    pub unmatched_rules: Vec<RuleSnapshot>,
    pub verdict: Option<VerdictSnapshot>,
    pub cost: Option<CostSnapshot>,
    pub recent_uncovered: u8,
    pub remaining_cs: u32,
    pub total_cs: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_round_trip_json() {
        let stage = DeliberationStage {
            phase: StagePhase::Examining,
            crew_member: "Boris".into(),
            crew_portrait_id: "boris_default".into(),
            crew_mood: "ANXIOUS".into(),
            trust_with_player: 256,
            context_summary: "Unknown signal detected".into(),
            matched_rules: vec![],
            unmatched_rules: vec![],
            verdict: None,
            cost: None,
            recent_uncovered: 2,
            remaining_cs: 400,
            total_cs: 400,
        };
        let json = serde_json::to_string(&stage).unwrap();
        let back: DeliberationStage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phase, StagePhase::Examining);
        assert_eq!(back.crew_member, "Boris");
        assert_eq!(back.recent_uncovered, 2);
    }

    #[test]
    fn phase_ordering_never_skips() {
        let order = [
            StagePhase::Examining,
            StagePhase::Weighing,
            StagePhase::Deciding,
            StagePhase::Verdict,
            StagePhase::Complete,
        ];
        for w in order.windows(2) {
            assert_ne!(w[0], w[1]);
        }
    }

    #[test]
    fn full_stage_serializes() {
        let stage = DeliberationStage {
            phase: StagePhase::Verdict,
            crew_member: "Tove".into(),
            crew_portrait_id: "tove_medic".into(),
            crew_mood: "RESOLUTE".into(),
            trust_with_player: 512,
            context_summary: "Hull breach detected".into(),
            matched_rules: vec![RuleSnapshot {
                index: 0,
                label: "emergency_seal".into(),
                action: "seal_breach".into(),
                condition_summary: "hull_hp < 30%".into(),
                matched: true,
            }],
            unmatched_rules: vec![RuleSnapshot {
                index: 1,
                label: "evacuate".into(),
                action: "abandon_ship".into(),
                condition_summary: "hull_hp < 10%".into(),
                matched: false,
            }],
            verdict: Some(VerdictSnapshot {
                outcome_label: "success".into(),
                action_taken: "seal_breach".into(),
                reasoning: "Contained the breach quickly.".into(),
                escalation: None,
            }),
            cost: Some(CostSnapshot {
                relationship_deltas: vec![("Boris".into(), 30)],
                hull_damage: Some(64),
                cargo_loss: None,
            }),
            recent_uncovered: 1,
            remaining_cs: 0,
            total_cs: 400,
        };
        let json = serde_json::to_string(&stage).unwrap();
        let back: DeliberationStage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.verdict.unwrap().outcome_label, "success");
        assert_eq!(back.matched_rules.len(), 1);
        assert_eq!(back.unmatched_rules.len(), 1);
    }
}
