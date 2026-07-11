//! Rules evaluation engine (spec §6). Pure computation, no I/O: game state
//! in, decision out. LLM dispatch is the caller's problem — the engine only
//! reports that rules could not resolve the situation.

use std::collections::BTreeMap;

use super::types::{Condition, Contract};

/// Snapshot of game state visible to contracts. Fixed-point values keyed by
/// field name ("fuel", "distance_to_destination", "hostile_detected.range").
/// BTreeMap: deterministic iteration everywhere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvalContext {
    pub fields: BTreeMap<String, i64>,
}

impl EvalContext {
    pub fn set(&mut self, field: impl Into<String>, value: i64) -> &mut Self {
        self.fields.insert(field.into(), value);
        self
    }
}

/// The engine's verdict for one evaluation pass.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome<'c> {
    /// A rule matched: fire this action. `rule_index` is the position in the
    /// contract's authored rule list (for the captain's log).
    Rule {
        rule_index: usize,
        action: &'c super::types::Action,
    },
    /// No rule matched and the contract grants LLM authority: enter
    /// deliberation (the caller shows "Boris is thinking…" and dispatches).
    Deliberate { llm: &'c super::types::LlmConfig },
    /// No rule matched and there is no LLM authority: the contract stays
    /// silent. A valid outcome — fail states are stories (spec §1).
    NoDecision,
}

pub fn condition_holds(condition: &Condition, ctx: &EvalContext) -> bool {
    match condition {
        Condition::Always => true,
        Condition::Compare { field, op, value } => ctx
            .fields
            .get(field)
            .is_some_and(|lhs| op.apply(*lhs, *value)),
        Condition::Not(inner) => !condition_holds(inner, ctx),
        Condition::All(inner) => inner.iter().all(|c| condition_holds(c, ctx)),
        Condition::Any(inner) => inner.iter().any(|c| condition_holds(c, ctx)),
    }
}

/// Evaluate a contract against game state. Rules are checked in descending
/// priority; ties break by authored order. First match wins.
pub fn evaluate<'c>(contract: &'c Contract, ctx: &EvalContext) -> Outcome<'c> {
    let mut order: Vec<usize> = (0..contract.rules.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(contract.rules[i].priority));

    for i in order {
        let rule = &contract.rules[i];
        if condition_holds(&rule.condition, ctx) {
            return Outcome::Rule {
                rule_index: i,
                action: &rule.action,
            };
        }
    }

    match &contract.llm_authority {
        Some(llm) => Outcome::Deliberate { llm },
        None => Outcome::NoDecision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::types::{Action, Comparison, LlmConfig, Rule, Trigger};

    /// The spec's own example: Boris pilots during cryo (spec §6).
    fn cryo_pilot() -> Contract {
        Contract {
            id: "cryo-pilot".into(),
            label: "Boris takes helm during cryo transit".into(),
            trigger: Trigger::Event {
                event_type: "crew_cryo_activated".into(),
            },
            rules: vec![
                Rule {
                    condition: Condition::Compare {
                        field: "distance_to_destination".into(),
                        op: Comparison::Lt,
                        value: 500 * 1024,
                    },
                    action: Action::verb("wake_crew"),
                    priority: 1,
                },
                Rule {
                    condition: Condition::Compare {
                        field: "fuel".into(),
                        op: Comparison::Lt,
                        value: 154, // 0.15 in 1/1024 fixed point
                    },
                    action: Action::verb("wake_crew"),
                    priority: 1,
                },
                Rule {
                    condition: Condition::Compare {
                        field: "hostile_detected.range".into(),
                        op: Comparison::Lt,
                        value: 500 * 1024,
                    },
                    action: Action::verb("wake_crew"),
                    priority: 10,
                },
                Rule {
                    condition: Condition::Always,
                    action: Action::verb("maintain_course"),
                    priority: 0,
                },
            ],
            llm_authority: Some(LlmConfig {
                fallback_on_timeout: true,
                timeout_ms: 15000,
                max_tokens: 256,
                system_prompt: "You are Boris, a dependable engineer.".into(),
                fallback_action: Some(Action::verb("maintain_course")),
            }),
        }
    }

    fn ctx(fuel: i64, distance: i64) -> EvalContext {
        let mut c = EvalContext::default();
        c.set("fuel", fuel).set("distance_to_destination", distance);
        c
    }

    #[test]
    fn default_rule_maintains_course() {
        let contract = cryo_pilot();
        let outcome = evaluate(&contract, &ctx(1024, 9000 * 1024));
        assert!(
            matches!(outcome, Outcome::Rule { action, .. } if action.kind == "maintain_course")
        );
    }

    #[test]
    fn low_fuel_wakes_crew() {
        let contract = cryo_pilot();
        let outcome = evaluate(&contract, &ctx(100, 9000 * 1024));
        assert!(matches!(outcome, Outcome::Rule { action, .. } if action.kind == "wake_crew"));
    }

    #[test]
    fn hostile_beats_default_by_priority() {
        let contract = cryo_pilot();
        let mut c = ctx(1024, 9000 * 1024);
        c.set("hostile_detected.range", 300 * 1024);
        let outcome = evaluate(&contract, &c);
        assert!(
            matches!(outcome, Outcome::Rule { rule_index: 2, action } if action.kind == "wake_crew")
        );
    }

    #[test]
    fn missing_field_is_false_not_error() {
        let contract = cryo_pilot();
        // No hostile sensor field at all: the priority-10 rule silently
        // doesn't match, the default fires.
        let outcome = evaluate(&contract, &ctx(1024, 9000 * 1024));
        assert!(matches!(outcome, Outcome::Rule { rule_index: 3, .. }));
    }

    #[test]
    fn no_match_no_default_deliberates() {
        let mut contract = cryo_pilot();
        contract.rules.pop(); // remove the Always default
        let outcome = evaluate(&contract, &ctx(1024, 9000 * 1024));
        assert!(matches!(outcome, Outcome::Deliberate { .. }));
    }

    #[test]
    fn no_match_no_llm_is_silence() {
        let mut contract = cryo_pilot();
        contract.rules.pop();
        contract.llm_authority = None;
        assert_eq!(
            evaluate(&contract, &ctx(1024, 9000 * 1024)),
            Outcome::NoDecision
        );
    }

    #[test]
    fn boolean_composition() {
        let cond = Condition::All(vec![
            Condition::Compare {
                field: "fuel".into(),
                op: Comparison::Gt,
                value: 100,
            },
            Condition::Not(Box::new(Condition::Any(vec![Condition::Compare {
                field: "docked".into(),
                op: Comparison::Eq,
                value: 1,
            }]))),
        ]);
        let mut c = EvalContext::default();
        c.set("fuel", 500).set("docked", 0);
        assert!(condition_holds(&cond, &c));
        c.set("docked", 1);
        assert!(!condition_holds(&cond, &c));
    }
}
