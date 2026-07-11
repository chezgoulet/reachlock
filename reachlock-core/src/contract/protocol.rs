//! Contract serialization (spec §6). JSON is the wire and storage format;
//! authored contracts may be written in YAML but are converted to this JSON
//! shape by the content pipeline before they reach the engine.

use super::types::Contract;

pub fn to_json(contract: &Contract) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(contract)
}

pub fn from_json(json: &str) -> Result<Contract, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::types::{Action, Comparison, Condition, LlmConfig, Rule, Trigger};

    #[test]
    fn round_trip() {
        let contract = Contract {
            id: "cryo-pilot".into(),
            label: "Boris takes helm".into(),
            trigger: Trigger::Event {
                event_type: "crew_cryo_activated".into(),
            },
            rules: vec![Rule {
                condition: Condition::Compare {
                    field: "fuel".into(),
                    op: Comparison::Lt,
                    value: 154,
                },
                action: Action::verb("wake_crew"),
                priority: 1,
            }],
            llm_authority: Some(LlmConfig {
                fallback_on_timeout: true,
                timeout_ms: 15000,
                max_tokens: 256,
                system_prompt: "You are Boris.".into(),
                fallback_action: Some(Action::verb("maintain_course")),
            }),
        };
        let json = to_json(&contract).unwrap();
        assert_eq!(from_json(&json).unwrap(), contract);
    }

    #[test]
    fn wire_shape_is_stable() {
        // Guard the serialized names: they are the storage format. Renaming
        // a field in types.rs silently breaks every stored contract; this
        // test makes that loud.
        let json = r#"{
            "id": "c1",
            "label": "test",
            "trigger": { "event": { "event_type": "fuel_low" } },
            "rules": [
                {
                    "condition": { "compare": { "field": "fuel", "op": "lt", "value": 154 } },
                    "action": { "kind": "wake_crew" },
                    "priority": 1
                },
                { "condition": "always", "action": { "kind": "maintain_course" } }
            ]
        }"#;
        let contract = from_json(json).expect("documented wire shape must parse");
        assert_eq!(contract.rules.len(), 2);
        assert_eq!(contract.rules[1].priority, 0, "priority defaults to 0");
        assert!(contract.llm_authority.is_none());
    }
}
