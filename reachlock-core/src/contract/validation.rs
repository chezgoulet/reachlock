//! Structural contract validation (S34). Pure functions that detect crafting
//! issues — design patterns producing uninteresting behavior — without
//! running the contract engine. All warnings are advisory.

use super::types::{
    Action, Comparison, Condition, Condition::Always, Condition::Compare, Contract, Trigger,
};
use crate::contract::metadata::CraftingWarning;

/// Analyze a contract for structural crafting warnings. Every warning is a
/// detectable pattern the player may want to correct or keep — the system
/// never treats them as errors.
pub fn validate_contract(contract: &Contract) -> Vec<CraftingWarning> {
    let mut warnings = Vec::new();

    let has_always = contract.rules.iter().any(|r| r.condition == Always);
    let has_llm = contract.llm_authority.is_some();

    // --- AlwaysResolvesWithoutLLM ---
    // Every situation has a deterministic rule, so the LLM edge is never hit.
    if has_always && !has_llm {
        warnings.push(CraftingWarning::AlwaysResolvesWithoutLLM);
    }

    // --- AlwaysRequiresLLM ---
    // No Always-cover rule means any unmatched situation hits the LLM.
    // If every rule has a specific condition, most inputs fall through.
    if !has_always && has_llm && !contract.rules.is_empty() {
        let all_specific = contract
            .rules
            .iter()
            .all(|r| !matches!(r.condition, Always));
        if all_specific {
            warnings.push(CraftingWarning::AlwaysRequiresLLM);
        }
    }

    // --- AllSamePriority ---
    if contract.rules.len() > 1 {
        let first = contract.rules[0].priority;
        if contract.rules.iter().all(|r| r.priority == first) {
            warnings.push(CraftingWarning::AllSamePriority);
        }
    }

    // --- NoFallbackBehavior ---
    if let Some(llm) = &contract.llm_authority {
        if llm.fallback_action.is_none() {
            warnings.push(CraftingWarning::NoFallbackBehavior);
        }
    }

    // --- OverSpecificTrigger ---
    if let Trigger::StateChange { op, .. } = &contract.trigger {
        if *op == Comparison::Eq {
            warnings.push(CraftingWarning::OverSpecificTrigger);
        }
    }

    // --- CircularRule ---
    // Shallow detection: one rule's action writes a field another rule's
    // condition reads. This catches the common case where Rule A fires and
    // creates the exact state Rule B needs, which fires and writes back to
    // A's condition field.
    if contract.rules.len() > 1 {
        let written_fields: Vec<&str> = contract
            .rules
            .iter()
            .flat_map(|r| action_written_fields(&r.action))
            .collect();
        for rule in &contract.rules {
            let reads = condition_read_fields(&rule.condition);
            if written_fields.iter().any(|w| reads.contains(w)) {
                warnings.push(CraftingWarning::CircularRule);
                break;
            }
        }
    }

    warnings.dedup();
    warnings
}

/// Check if contract evaluation would make sense against a provided scenario
/// context. Pure — no side effects.
pub fn validate_against_scenario(
    contract: &Contract,
    ctx: &super::engine::EvalContext,
) -> Option<&'static str> {
    match super::engine::evaluate(contract, ctx) {
        super::engine::Outcome::Rule { .. } => Some("rule_fired"),
        super::engine::Outcome::Deliberate { .. } => Some("deliberates"),
        super::engine::Outcome::NoDecision => Some("no_decision"),
    }
    // Return the label — the caller decides what to do with it.
}

/// Fields that an action writes (from its `params` keys).
fn action_written_fields(action: &Action) -> Vec<&str> {
    action.params.keys().map(|s| s.as_str()).collect()
}

/// Field names that a condition reads (from `Compare` nodes).
fn condition_read_fields(condition: &Condition) -> Vec<&str> {
    match condition {
        Compare { field, .. } => vec![field.as_str()],
        Condition::Not(c) => condition_read_fields(c),
        Condition::All(conds) | Condition::Any(conds) => {
            let mut fields = Vec::new();
            for c in conds {
                fields.extend(condition_read_fields(c));
            }
            fields
        }
        Always => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::types::{
        Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger,
    };

    fn well_formed_contract() -> Contract {
        Contract {
            id: "test".into(),
            label: "Test contract".into(),
            trigger: Trigger::Event {
                event_type: "combat_start".into(),
            },
            rules: vec![
                Rule {
                    condition: Condition::Compare {
                        field: "shields".into(),
                        op: Comparison::Lt,
                        value: 256,
                    },
                    action: Action::verb("reinforce_shields"),
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
                system_prompt: "You are Boris.".into(),
                fallback_action: Some(Action::verb("maintain_course")),
            }),
        }
    }

    #[test]
    fn no_warnings_for_well_formed_contract() {
        let c = well_formed_contract();
        let warnings = validate_contract(&c);
        assert!(
            warnings.is_empty(),
            "well-formed contract should have no warnings: {warnings:?}"
        );
    }

    #[test]
    fn no_llm_triggers_always_resolves_without_llm() {
        let mut c = well_formed_contract();
        c.llm_authority = None;
        let warnings = validate_contract(&c);
        assert!(warnings.contains(&CraftingWarning::AlwaysResolvesWithoutLLM));
    }

    #[test]
    fn no_always_triggers_requires_llm() {
        let mut c = well_formed_contract();
        c.rules
            .retain(|r| !matches!(r.condition, Condition::Always));
        let warnings = validate_contract(&c);
        assert!(warnings.contains(&CraftingWarning::AlwaysRequiresLLM));
    }

    #[test]
    fn same_priority_triggers_warning() {
        let mut c = well_formed_contract();
        for r in c.rules.iter_mut() {
            r.priority = 0;
        }
        let warnings = validate_contract(&c);
        assert!(warnings.contains(&CraftingWarning::AllSamePriority));
    }

    #[test]
    fn no_fallback_triggers_warning() {
        let mut c = well_formed_contract();
        if let Some(ref mut llm) = c.llm_authority {
            llm.fallback_action = None;
        }
        let warnings = validate_contract(&c);
        assert!(warnings.contains(&CraftingWarning::NoFallbackBehavior));
    }

    #[test]
    fn over_specific_trigger_triggers_warning() {
        let mut c = well_formed_contract();
        c.trigger = Trigger::StateChange {
            field: "enemy_count".into(),
            op: Comparison::Eq,
            value: 7,
        };
        let warnings = validate_contract(&c);
        assert!(warnings.contains(&CraftingWarning::OverSpecificTrigger));
    }

    #[test]
    fn circular_rule_triggers_warning() {
        let mut c = well_formed_contract();
        // Add a rule that reads a field written by another rule's action.
        let mut action_with_param = Action::verb("set_flag");
        action_with_param.params.insert("shields".into(), 512);
        c.rules.push(Rule {
            condition: Condition::Compare {
                field: "shields".into(),
                op: Comparison::Ge,
                value: 512,
            },
            action: action_with_param,
            priority: 5,
        });
        let warnings = validate_contract(&c);
        assert!(
            warnings.contains(&CraftingWarning::CircularRule),
            "circular rule should trigger warning: {warnings:?}"
        );
    }

    #[test]
    fn validate_against_scenario_returns_outcome() {
        let c = well_formed_contract();
        let mut ctx = super::super::engine::EvalContext::default();
        ctx.set("shields", 100);
        let note = validate_against_scenario(&c, &ctx);
        assert!(note.is_some());
    }
}
