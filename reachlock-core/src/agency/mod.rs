//! LLM agency & failure model (S15, spec §18). "Who should decide?" is
//! mechanical here:
//!
//! S37 adds captain's log generation:
//! - [`log`] — key moment detection, session summarization
//! - [`log_generation`] — LLM-based log entry generation + template fallback

pub mod log;
pub mod log_generation;

// Every LLM call resolves through the spec's outcome table
// ([`LlmOutcome`], six rows) with seeded, modifier-shifted probabilities
// ([`OutcomeWeights`], fixed-point rows summing to 1024).
// - Rolls derive from `(contract_id, tick, chain position)` — deterministic
//   and replayable, never wall-clock or thread randomness.
// - The [`Dispatch`] routes orders: robots get [`RoutedOrder::Execute`]
//   (fallible mechanically, never deliberating), droids/androids get
//   [`RoutedOrder::Consider`] (may counter-propose; the counter is itself
//   a deliberation under this same outcome model).
// - Every deliberation produces a [`TraceEntry`] triple — start, outcome,
//   fallback-if-fired — so the player can always reconstruct "Boris timed
//   out during jump 47". Failure is gameplay, provably never silent.
//
// Classic tier never reaches this module: no LLM = no outcome table
// (rules-only ships fail only by having no rule, handled in the engine).

use serde::{Deserialize, Serialize};

use crate::soul::types::Species;

/// The six rows of the spec §18 outcome table, in table order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmOutcome {
    Success,
    Timeout,
    Misinterpretation,
    Confabulation,
    Collapse,
    Catastrophic,
}

impl LlmOutcome {
    pub const ALL: [LlmOutcome; 6] = [
        LlmOutcome::Success,
        LlmOutcome::Timeout,
        LlmOutcome::Misinterpretation,
        LlmOutcome::Confabulation,
        LlmOutcome::Collapse,
        LlmOutcome::Catastrophic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LlmOutcome::Success => "success",
            LlmOutcome::Timeout => "timeout",
            LlmOutcome::Misinterpretation => "misinterpretation",
            LlmOutcome::Confabulation => "confabulation",
            LlmOutcome::Collapse => "collapse",
            LlmOutcome::Catastrophic => "catastrophic",
        }
    }
}

/// Fixed-point weights per outcome row, indexed like [`LlmOutcome::ALL`].
/// Always sums to exactly 1024 after [`weights`] normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeWeights(pub [i64; 6]);

/// Spec §18 baseline: 70 / 10 / 10 / 5 / 3 / 2 percent, in 1024ths.
pub const BASELINE: OutcomeWeights = OutcomeWeights([717, 102, 102, 51, 31, 21]);

/// Where a probability shift comes from (spec §18's four player levers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModifierSource {
    Equipment,
    Crew,
    Model,
    ContractQuality,
}

/// Per-row weight deltas (1024ths), attributed to a source for the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modifier {
    pub source: ModifierSource,
    pub deltas: [i64; 6],
}

/// Apply modifiers to a base table: sum deltas per row, clamp at zero, then
/// renormalize to exactly 1024 (proportional, drift absorbed by the largest
/// row). Total collapse of every row degenerates to "always Success" —
/// a modifier stack can make failure impossible, never make outcome
/// resolution impossible.
pub fn weights(base: &OutcomeWeights, modifiers: &[Modifier]) -> OutcomeWeights {
    let mut rows = base.0;
    for m in modifiers {
        for (row, delta) in rows.iter_mut().zip(m.deltas.iter()) {
            *row += delta;
        }
    }
    for row in rows.iter_mut() {
        *row = (*row).max(0);
    }
    let sum: i64 = rows.iter().sum();
    if sum == 0 {
        return OutcomeWeights([1024, 0, 0, 0, 0, 0]);
    }
    let mut scaled = [0i64; 6];
    for (i, row) in rows.iter().enumerate() {
        scaled[i] = row * 1024 / sum;
    }
    let drift = 1024 - scaled.iter().sum::<i64>();
    // Absorb rounding drift into the largest row (deterministic: first max).
    let largest = (0..6).max_by_key(|&i| scaled[i]).expect("six rows");
    scaled[largest] += drift;
    OutcomeWeights(scaled)
}

/// Pure, deterministic resolution: walk the cumulative table with the roll
/// reduced into 0..1024.
pub fn resolve_outcome(seed_roll: u64, weights: &OutcomeWeights) -> LlmOutcome {
    let mut point = (seed_roll % 1024) as i64;
    for (i, row) in weights.0.iter().enumerate() {
        if point < *row {
            return LlmOutcome::ALL[i];
        }
        point -= row;
    }
    // Only reachable if the table under-sums (defensive): the conservative
    // row.
    LlmOutcome::Success
}

/// The canonical roll derivation (the gotcha): `(contract_id, tick, chain
/// position)` through the seed-protocol hash primitives — deterministic and
/// replayable; the same deliberation replays the same fate.
pub fn deliberation_roll(contract_id: &str, tick: u64, chain_position: u32) -> u64 {
    use crate::seed::resolver::{finalize, fnv1a, FNV_OFFSET};
    let mut h = FNV_OFFSET;
    h = fnv1a(h, b"llm_outcome");
    h = fnv1a(h, contract_id.as_bytes());
    h = fnv1a(h, &tick.to_le_bytes());
    h = fnv1a(h, &chain_position.to_le_bytes());
    finalize(h)
}

// ───────────────────────── modifier builders ─────────────────────────

/// Model tier lever. FairPlay is the baseline; Spectrum's premium models
/// misread and collapse less; BYOK is declared-quality (neutral here — the
/// player chose it, the game doesn't grade it).
pub fn model_modifier(tier: crate::universe::UniverseTier) -> Modifier {
    use crate::universe::UniverseTier as T;
    let deltas = match tier {
        T::Classic | T::FairPlay | T::Byok => [0, 0, 0, 0, 0, 0],
        T::Spectrum => [66, -10, -31, -10, -13, -2],
    };
    Modifier {
        source: ModifierSource::Model,
        deltas,
    }
}

/// Crew lever (S13 bridge): a trusted crew member misreads the player's
/// intent less. `trust` is the soul's `trust.player` (-1024..=1024);
/// only trust above the midpoint helps, scaling to -40/1024ths off
/// misinterpretation (and the same onto success) at full trust.
pub fn crew_modifier(trust: i64) -> Modifier {
    let above = (trust - 512).clamp(0, 512);
    let shift = above * 40 / 512;
    Modifier {
        source: ModifierSource::Crew,
        deltas: [shift, 0, -shift, 0, 0, 0],
    }
}

/// Contract-quality lever: rules with recent uncovered evaluations deliberate
/// on thinner context. Every recent uncovered evaluation (capped at 8) adds
/// timeout + misinterpretation weight; a well-covered contract is neutral.
pub fn contract_quality_modifier(recent_uncovered: u32) -> Modifier {
    let n = recent_uncovered.min(8) as i64;
    Modifier {
        source: ModifierSource::ContractQuality,
        deltas: [-6 * n, 3 * n, 3 * n, 0, 0, 0],
    }
}

/// Equipment lever (S05 stat keys, zero until items are equippable — the
/// pipe exists). `deliberation_speed` cuts timeout; `failure_resistance`
/// cuts collapse + catastrophic. Both fixed-point 0..=1024.
pub fn equipment_modifier(deliberation_speed: i64, failure_resistance: i64) -> Modifier {
    let t = deliberation_speed.clamp(0, 1024) * 60 / 1024;
    let f = failure_resistance.clamp(0, 1024);
    let c = f * 20 / 1024;
    let cat = f * 10 / 1024;
    Modifier {
        source: ModifierSource::Equipment,
        deltas: [t + c + cat, -t, 0, 0, -c, -cat],
    }
}

// ───────────────────── outcome application ─────────────────────

/// What the provider actually did (S14's taxonomy, collapsed to what the
/// outcome model needs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderVerdict {
    /// The model answered with a shaped action + reasoning.
    Answered { action: String, reasoning: String },
    /// S14 `timeout` / `rate_limited` — maps straight to the Timeout row.
    TimedOut,
    /// S14 `bad_response` — maps straight to the Collapse row.
    Collapsed,
}

/// A deliberation fully resolved through the outcome table: what the crew
/// did, what they *believed*, and how it should read in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedDeliberation {
    pub outcome: LlmOutcome,
    /// The action that actually fires. `None` = fall back (timeout/collapse).
    pub action: Option<String>,
    /// The reasoning to show — for confabulation this includes the invented
    /// detail the crew believed.
    pub reasoning: String,
    /// Catastrophic only: the escalated consequence the game layer must
    /// apply (recoverable by design — damage, strand, cargo loss — never
    /// save-corrupting).
    pub escalation: Option<String>,
}

/// Invented details a confabulating crew member believes (deterministic
/// pick by roll). Flavor, but load-bearing flavor: the log must show what
/// the crew THOUGHT was true.
const CONFABULATIONS: [&str; 4] = [
    "a phantom contact reading 0.3 on the aft scope",
    "a fuel-margin figure that appears in no tank telemetry",
    "a docking clearance nobody transmitted",
    "a debris field the sensors never recorded",
];

/// Escalations for the catastrophic row — recoverable consequences only.
const ESCALATIONS: [&str; 3] = ["hull_stress", "cargo_loss", "stranded_drift"];

/// Classify one provider verdict through the outcome table and produce the
/// final resolved deliberation. `action_set` is the contract's verb set
/// (rule actions + fallback) — misinterpretation swaps within it, so the
/// wrong action is always a *plausible* wrong action.
pub fn classify(
    verdict: ProviderVerdict,
    roll: u64,
    weights: &OutcomeWeights,
    action_set: &[String],
) -> ResolvedDeliberation {
    match verdict {
        // Hard provider failures don't roll: they are their row.
        ProviderVerdict::TimedOut => ResolvedDeliberation {
            outcome: LlmOutcome::Timeout,
            action: None,
            reasoning: "no answer came back before the window closed".into(),
            escalation: None,
        },
        ProviderVerdict::Collapsed => ResolvedDeliberation {
            outcome: LlmOutcome::Collapse,
            action: None,
            reasoning: "the answer degraded into noise — safe mode until manual override".into(),
            escalation: None,
        },
        ProviderVerdict::Answered { action, reasoning } => {
            let outcome = resolve_outcome(roll, weights);
            apply_outcome(outcome, action, reasoning, roll, action_set)
        }
    }
}

fn apply_outcome(
    outcome: LlmOutcome,
    action: String,
    reasoning: String,
    roll: u64,
    action_set: &[String],
) -> ResolvedDeliberation {
    match outcome {
        LlmOutcome::Success => ResolvedDeliberation {
            outcome,
            action: Some(action),
            reasoning,
            escalation: None,
        },
        // The model answered, but the answer arrived after the moment
        // passed / degraded on the way: the fiction of these rows stands
        // even on a provider success.
        LlmOutcome::Timeout => ResolvedDeliberation {
            outcome,
            action: None,
            reasoning: "the answer came too late — the moment had passed".into(),
            escalation: None,
        },
        LlmOutcome::Collapse => ResolvedDeliberation {
            outcome,
            action: None,
            reasoning: "the reply degraded into noise — safe mode until manual override".into(),
            escalation: None,
        },
        LlmOutcome::Misinterpretation => {
            let wrong = perturb_action(&action, roll, action_set);
            ResolvedDeliberation {
                outcome,
                reasoning: format!("{reasoning} (misread the situation)"),
                action: Some(wrong),
                escalation: None,
            }
        }
        LlmOutcome::Confabulation => {
            let invented = CONFABULATIONS[(roll / 7 % CONFABULATIONS.len() as u64) as usize];
            ResolvedDeliberation {
                outcome,
                action: Some(action),
                reasoning: format!("{reasoning} — citing {invented}"),
                escalation: None,
            }
        }
        LlmOutcome::Catastrophic => {
            // Wraps another row's action with an escalated consequence: the
            // wrong call, at the worst time.
            let wrong = perturb_action(&action, roll, action_set);
            let escalation = ESCALATIONS[(roll / 11 % ESCALATIONS.len() as u64) as usize];
            ResolvedDeliberation {
                outcome,
                action: Some(wrong),
                reasoning: format!("{reasoning} (a rare coincidence of failure modes)"),
                escalation: Some(escalation.to_string()),
            }
        }
    }
}

/// Deterministically swap an action for a plausible-but-wrong sibling from
/// the contract's own verb set. A one-verb contract keeps its verb (there
/// is nothing plausible to be wrong with).
fn perturb_action(action: &str, roll: u64, action_set: &[String]) -> String {
    let others: Vec<&String> = action_set.iter().filter(|a| *a != action).collect();
    if others.is_empty() {
        return action.to_string();
    }
    others[(roll / 13 % others.len() as u64) as usize].clone()
}

// ───────────────────────── the dispatch ─────────────────────────

/// How the dispatch routes an order, by body kind (spec §18): robots
/// execute — fallible mechanically, never deliberating; droids/androids
/// consider — they may counter-propose. Humans aren't under dispatch
/// authority at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutedOrder {
    Execute { order: String },
    Consider { order: String },
}

/// A droid's answer to a considered order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Consideration {
    Accept,
    /// "Understood. But…" — the counter is itself a deliberation, resolved
    /// through the same outcome table by the caller.
    Counter {
        action: String,
        rationale: String,
    },
}

/// The ship's dispatch: the central automation system (NOT a soul) — a
/// contract set plus routing rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dispatch {
    /// Contract ids this dispatch owns (the ship's automation set).
    pub contract_ids: Vec<String>,
}

impl Dispatch {
    /// Route one order to one crew body. `None` = the dispatch has no
    /// authority over this body (humans decide for themselves).
    pub fn route(&self, species: Species, order: &str) -> Option<RoutedOrder> {
        match species {
            Species::Robot => Some(RoutedOrder::Execute {
                order: order.to_string(),
            }),
            Species::Android => Some(RoutedOrder::Consider {
                order: order.to_string(),
            }),
            Species::Human => None,
            Species::Voidborn => None,
            Species::Xenotype => None,
        }
    }
}

/// A considering droid checks the order against what its own sensors say:
/// any `hazard.*` field above the fixed-point midpoint that the order does
/// not name earns a counter-proposal (the spec §18 dispatch/droid exchange:
/// "Understood. But sensors show a debris field on that route. Recommend
/// 3% deviation to avoid.").
pub fn consider_order(order: &str, ctx: &crate::contract::engine::EvalContext) -> Consideration {
    let hazard = ctx
        .fields
        .iter()
        .find(|(k, v)| k.starts_with("hazard.") && **v > 512 && !order.contains(&k[7..]));
    match hazard {
        Some((k, _)) => Consideration::Counter {
            action: "adjust_course".into(),
            rationale: format!(
                "Understood. But sensors show {} on that route. \
                 Recommend deviation to avoid.",
                &k[7..]
            ),
        },
        None => Consideration::Accept,
    }
}

// ─────────────────────── traceability ───────────────────────

/// One line of the deliberation audit trail. The battery test proves every
/// deliberation can be reconstructed from these alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceEntry {
    Start {
        contract_id: String,
        tick: u64,
        context_summary: String,
    },
    Outcome {
        contract_id: String,
        tick: u64,
        outcome: LlmOutcome,
        reasoning: String,
    },
    Fallback {
        contract_id: String,
        tick: u64,
        action: String,
    },
}

impl TraceEntry {
    /// The player-facing log line. Categories read distinctly — a timeout
    /// and a misinterpretation are different stories (spec §18: "that's a
    /// story, not a bug").
    pub fn render(&self, crew_member: &str) -> String {
        match self {
            TraceEntry::Start {
                contract_id,
                tick,
                context_summary,
            } => format!(
                "[tick {tick}] {crew_member} is considering ({contract_id}): {context_summary}"
            ),
            TraceEntry::Outcome {
                contract_id,
                tick,
                outcome,
                reasoning,
            } => {
                let mark = match outcome {
                    LlmOutcome::Success => "·",
                    LlmOutcome::Timeout => "⌛",
                    LlmOutcome::Misinterpretation => "≈",
                    LlmOutcome::Confabulation => "✎",
                    LlmOutcome::Collapse => "▢",
                    LlmOutcome::Catastrophic => "‼",
                };
                format!(
                    "[tick {tick}] {mark} {crew_member} ({contract_id}, {}): {reasoning}",
                    outcome.as_str()
                )
            }
            TraceEntry::Fallback {
                contract_id,
                tick,
                action,
            } => format!("[tick {tick}] {crew_member} fell back to {action} ({contract_id})."),
        }
    }
}

/// One deliberation's identity and rules: which contract, when, under what
/// odds, with what verbs to be wrong with.
#[derive(Debug, Clone)]
pub struct DeliberationParams<'a> {
    pub contract_id: &'a str,
    pub tick: u64,
    pub chain_position: u32,
    pub context_summary: &'a str,
    pub weights: &'a OutcomeWeights,
    /// The contract's verb set — misinterpretation swaps within it.
    pub action_set: &'a [String],
    /// The contract's default verb, fired when no action resolves.
    pub fallback_action: &'a str,
}

/// Run one full deliberation: classify, then emit the trace triple —
/// start, outcome, and fallback if one fired.
pub fn deliberate(
    params: &DeliberationParams<'_>,
    verdict: ProviderVerdict,
) -> (ResolvedDeliberation, Vec<TraceEntry>) {
    let roll = deliberation_roll(params.contract_id, params.tick, params.chain_position);
    let resolved = classify(verdict, roll, params.weights, params.action_set);
    let mut trace = vec![
        TraceEntry::Start {
            contract_id: params.contract_id.to_string(),
            tick: params.tick,
            context_summary: params.context_summary.to_string(),
        },
        TraceEntry::Outcome {
            contract_id: params.contract_id.to_string(),
            tick: params.tick,
            outcome: resolved.outcome,
            reasoning: resolved.reasoning.clone(),
        },
    ];
    if resolved.action.is_none() {
        trace.push(TraceEntry::Fallback {
            contract_id: params.contract_id.to_string(),
            tick: params.tick,
            action: params.fallback_action.to_string(),
        });
    }
    (resolved, trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::engine::EvalContext;

    #[test]
    fn baseline_sums_to_1024_and_matches_the_spec_table() {
        assert_eq!(BASELINE.0.iter().sum::<i64>(), 1024);
        // 70/10/10/5/3/2 percent within integer rounding.
        assert_eq!(BASELINE.0[0], 717);
        assert_eq!(BASELINE.0[5], 21);
    }

    /// Property battery: for a spread of delta stacks, weights always
    /// normalize to exactly 1024 with no negative row.
    #[test]
    fn weights_always_normalize_and_never_go_negative() {
        for a in [-2000i64, -300, -21, 0, 17, 300, 2000] {
            for b in [-500i64, 0, 250] {
                let m = [
                    Modifier {
                        source: ModifierSource::Model,
                        deltas: [a, b, -b, a / 2, -a, b],
                    },
                    Modifier {
                        source: ModifierSource::Crew,
                        deltas: [b, a, b, -a, a, -b],
                    },
                ];
                let w = weights(&BASELINE, &m);
                assert_eq!(w.0.iter().sum::<i64>(), 1024, "sum for a={a} b={b}");
                assert!(w.0.iter().all(|r| *r >= 0), "negative row for a={a} b={b}");
            }
        }
    }

    #[test]
    fn resolve_is_deterministic_and_covers_every_row() {
        // Every row is reachable and the same roll always lands the same row.
        let mut seen = std::collections::BTreeSet::new();
        for roll in 0..1024u64 {
            let one = resolve_outcome(roll, &BASELINE);
            let two = resolve_outcome(roll, &BASELINE);
            assert_eq!(one, two);
            seen.insert(one.as_str());
        }
        assert_eq!(seen.len(), 6, "all six rows reachable: {seen:?}");
    }

    #[test]
    fn distribution_matches_weights_exactly_over_the_roll_space() {
        let mut counts = [0i64; 6];
        for roll in 0..1024u64 {
            let o = resolve_outcome(roll, &BASELINE);
            counts[LlmOutcome::ALL.iter().position(|x| *x == o).unwrap()] += 1;
        }
        assert_eq!(counts.to_vec(), BASELINE.0.to_vec());
    }

    #[test]
    fn rolls_derive_from_contract_tick_chain() {
        let a = deliberation_roll("cryo-pilot", 47, 0);
        assert_eq!(a, deliberation_roll("cryo-pilot", 47, 0), "replayable");
        assert_ne!(a, deliberation_roll("cryo-pilot", 48, 0));
        assert_ne!(a, deliberation_roll("cryo-pilot", 47, 1));
        assert_ne!(a, deliberation_roll("engine_maintenance", 47, 0));
    }

    #[test]
    fn provider_failures_map_straight_to_their_rows() {
        let acts = vec!["wake_crew".to_string(), "maintain_course".to_string()];
        let t = classify(ProviderVerdict::TimedOut, 0, &BASELINE, &acts);
        assert_eq!(t.outcome, LlmOutcome::Timeout);
        assert_eq!(t.action, None);
        let c = classify(ProviderVerdict::Collapsed, 0, &BASELINE, &acts);
        assert_eq!(c.outcome, LlmOutcome::Collapse);
        assert_eq!(c.action, None);
    }

    #[test]
    fn misinterpretation_swaps_to_a_plausible_wrong_verb() {
        let acts = vec!["wake_crew".to_string(), "maintain_course".to_string()];
        // Find a roll that lands the misinterpretation row.
        let roll = (0..1024u64)
            .find(|r| resolve_outcome(*r, &BASELINE) == LlmOutcome::Misinterpretation)
            .unwrap();
        let r = classify(
            ProviderVerdict::Answered {
                action: "wake_crew".into(),
                reasoning: "anomaly".into(),
            },
            roll,
            &BASELINE,
            &acts,
        );
        assert_eq!(r.outcome, LlmOutcome::Misinterpretation);
        assert_eq!(
            r.action.as_deref(),
            Some("maintain_course"),
            "the wrong verb comes from the contract's own set"
        );
        // One-verb contracts have nothing plausible to be wrong with.
        let solo = classify(
            ProviderVerdict::Answered {
                action: "wake_crew".into(),
                reasoning: "anomaly".into(),
            },
            roll,
            &BASELINE,
            &["wake_crew".to_string()],
        );
        assert_eq!(solo.action.as_deref(), Some("wake_crew"));
    }

    #[test]
    fn confabulation_cites_invented_data_and_catastrophic_escalates() {
        let acts = vec!["wake_crew".to_string(), "maintain_course".to_string()];
        let conf_roll = (0..1024u64)
            .find(|r| resolve_outcome(*r, &BASELINE) == LlmOutcome::Confabulation)
            .unwrap();
        let r = classify(
            ProviderVerdict::Answered {
                action: "maintain_course".into(),
                reasoning: "steady".into(),
            },
            conf_roll,
            &BASELINE,
            &acts,
        );
        assert!(r.reasoning.contains("citing"), "the belief is visible");
        assert!(r.escalation.is_none());

        let cat_roll = (0..1024u64)
            .find(|r| resolve_outcome(*r, &BASELINE) == LlmOutcome::Catastrophic)
            .unwrap();
        let r = classify(
            ProviderVerdict::Answered {
                action: "maintain_course".into(),
                reasoning: "steady".into(),
            },
            cat_roll,
            &BASELINE,
            &acts,
        );
        assert_eq!(r.outcome, LlmOutcome::Catastrophic);
        let esc = r.escalation.expect("catastrophic escalates");
        assert!(
            ESCALATIONS.contains(&esc.as_str()),
            "recoverable consequences only"
        );
    }

    #[test]
    fn modifiers_move_the_right_rows() {
        // Spectrum cuts misinterpretation vs FairPlay.
        let fair = weights(
            &BASELINE,
            &[model_modifier(crate::universe::UniverseTier::FairPlay)],
        );
        let spectrum = weights(
            &BASELINE,
            &[model_modifier(crate::universe::UniverseTier::Spectrum)],
        );
        assert!(spectrum.0[2] < fair.0[2]);
        assert!(spectrum.0[0] > fair.0[0]);
        // Trust cuts misinterpretation; low trust is neutral.
        assert_eq!(crew_modifier(0).deltas, [0, 0, 0, 0, 0, 0]);
        assert!(crew_modifier(1024).deltas[2] < 0);
        // Bad coverage raises timeout + misinterpretation.
        let m = contract_quality_modifier(5);
        assert!(m.deltas[1] > 0 && m.deltas[2] > 0);
        // Equipment pipes: speed cuts timeout, resistance cuts collapse/cat.
        let e = equipment_modifier(1024, 1024);
        assert!(e.deltas[1] < 0 && e.deltas[4] < 0 && e.deltas[5] < 0);
        assert_eq!(equipment_modifier(0, 0).deltas, [0, 0, 0, 0, 0, 0]);
    }

    /// The spec §18 dispatch/droid exchange, ported: the dispatch routes a
    /// course order; the robot executes; the droid counters the same order
    /// because its sensors show a debris field the order ignores.
    #[test]
    fn dispatch_routes_by_body_and_droids_counter_hazards() {
        let dispatch = Dispatch {
            contract_ids: vec!["cryo-pilot".into()],
        };
        assert_eq!(
            dispatch.route(Species::Robot, "hold course through sector 7"),
            Some(RoutedOrder::Execute {
                order: "hold course through sector 7".into()
            })
        );
        let considered = dispatch.route(Species::Android, "hold course through sector 7");
        assert_eq!(
            considered,
            Some(RoutedOrder::Consider {
                order: "hold course through sector 7".into()
            })
        );
        assert_eq!(dispatch.route(Species::Human, "any"), None);
        assert_eq!(dispatch.route(Species::Voidborn, "any"), None);
        assert_eq!(dispatch.route(Species::Xenotype, "any"), None);

        // The droid's sensors know something the order doesn't.
        let mut ctx = EvalContext::default();
        ctx.set("hazard.debris_field", 800);
        match consider_order("hold course through sector 7", &ctx) {
            Consideration::Counter { action, rationale } => {
                assert_eq!(action, "adjust_course");
                assert!(rationale.contains("debris_field"));
                assert!(rationale.starts_with("Understood. But"));
            }
            Consideration::Accept => panic!("the droid should counter"),
        }
        // No hazard (or one the order already names): accept.
        let calm = EvalContext::default();
        assert_eq!(consider_order("hold course", &calm), Consideration::Accept);
        let mut named = EvalContext::default();
        named.set("hazard.debris_field", 800);
        assert_eq!(
            consider_order("deviate around the debris_field", &named),
            Consideration::Accept
        );
    }

    /// The traceability battery: 100 seeded deliberations, every one
    /// reconstructable from its trace alone — contract, tick, outcome,
    /// and the fallback when one fired. Failure is provably never silent.
    #[test]
    fn every_deliberation_is_reconstructable_from_the_log() {
        let acts = vec!["wake_crew".to_string(), "maintain_course".to_string()];
        for tick in 0..100u64 {
            let verdict = if tick % 9 == 0 {
                ProviderVerdict::TimedOut
            } else if tick % 17 == 0 {
                ProviderVerdict::Collapsed
            } else {
                ProviderVerdict::Answered {
                    action: "maintain_course".into(),
                    reasoning: "steady as she goes".into(),
                }
            };
            let (resolved, trace) = deliberate(
                &DeliberationParams {
                    contract_id: "cryo-pilot",
                    tick,
                    chain_position: 0,
                    context_summary: "anomalous reading, rules don't cover it",
                    weights: &BASELINE,
                    action_set: &acts,
                    fallback_action: "maintain_course",
                },
                verdict,
            );
            // (1) start entry with context summary…
            assert!(matches!(
                &trace[0],
                TraceEntry::Start { contract_id, tick: t, context_summary }
                    if contract_id == "cryo-pilot" && *t == tick && !context_summary.is_empty()
            ));
            // (2) …outcome entry with reasoning that reconstructs the row…
            let TraceEntry::Outcome {
                outcome, reasoning, ..
            } = &trace[1]
            else {
                panic!("second entry is the outcome");
            };
            assert_eq!(*outcome, resolved.outcome);
            assert!(!reasoning.is_empty());
            // …and its render names crew, tick, and category.
            let line = trace[1].render("Boris");
            assert!(line.contains("Boris") && line.contains(&format!("tick {tick}")));
            assert!(line.contains(resolved.outcome.as_str()));
            // (3) fallback entry exactly when no action fired.
            match resolved.action {
                None => assert!(matches!(&trace[2], TraceEntry::Fallback { action, .. }
                    if action == "maintain_course")),
                Some(_) => assert_eq!(trace.len(), 2),
            }
        }
    }
}
