use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorialHint {
    pub hint_id: String,
    pub trigger: HintTrigger,
    pub title: String,
    pub body: String,
    pub position: HintPosition,
    pub dismiss_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HintTrigger {
    FirstRun,
    FirstDeliberation,
    FirstDocking,
    FirstUndocking,
    FirstContractEdit,
    FirstUncoveredGap { threshold: u8 },
    GameModeEntered(GameModeData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameModeData {
    pub mode: String,
    pub transition_animation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HintPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    AboveElement,
    BelowElement,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_trigger_variant_has_at_least_one_hint() {
        let triggers = vec![
            HintTrigger::FirstRun,
            HintTrigger::FirstDeliberation,
            HintTrigger::FirstDocking,
            HintTrigger::FirstUndocking,
            HintTrigger::FirstContractEdit,
            HintTrigger::FirstUncoveredGap { threshold: 3 },
            HintTrigger::GameModeEntered(GameModeData {
                mode: "SpaceFlight".into(),
                transition_animation: "none".into(),
            }),
        ];
        for t in &triggers {
            let hint = make_demo_hint(t.clone());
            assert!(!hint.hint_id.is_empty());
            assert!(!hint.title.is_empty());
            assert!(!hint.body.is_empty());
        }
    }

    fn make_demo_hint(trigger: HintTrigger) -> TutorialHint {
        TutorialHint {
            hint_id: format!("{:?}", trigger),
            trigger,
            title: "Test hint".into(),
            body: "This is a test hint body.".into(),
            position: HintPosition::BottomRight,
            dismiss_action: Some("Enter".into()),
        }
    }
}
