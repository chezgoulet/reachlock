//! Contract engine integration (spec §6, §9): the core engine evaluates the
//! auto-helm contract against live ship state. Rule hits act instantly; the
//! unresolvable case enters deliberation — offline mode has no LLM, so the
//! timeout path fires the fallback action, exactly the "Boris couldn't
//! decide" story.

use bevy::prelude::*;
use reachlock_core::contract::{
    engine::{evaluate, EvalContext, Outcome},
    types::{Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger},
};

use crate::systems::ship::ShipSystems;

/// The ship's log: every decision the automation makes, newest last.
#[derive(Resource, Default)]
pub struct ShipLog {
    pub entries: Vec<String>,
}

impl ShipLog {
    fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        info!("ship log: {line}");
        self.entries.push(line);
        if self.entries.len() > 6 {
            self.entries.remove(0);
        }
    }
}

/// Client-side deliberation state (spec §9): active while "Boris is
/// thinking". Offline: a timer that expires into the fallback action.
#[derive(Resource, Default)]
pub struct DeliberationState {
    pub active: Option<Deliberation>,
}

pub struct Deliberation {
    pub crew_member: String,
    pub context_summary: String,
    pub remaining: Timer,
    pub fallback: Action,
}

#[derive(Resource)]
pub struct ContractRuntime {
    pub contract: Contract,
    pub eval_timer: Timer,
    /// Last action kind, to log only on change instead of every second.
    last_action: Option<String>,
}

impl Default for ContractRuntime {
    fn default() -> Self {
        ContractRuntime {
            contract: auto_helm(),
            eval_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            last_action: None,
        }
    }
}

/// The starter contract: Boris keeps the helm. Covers low fuel and normal
/// cruising; does NOT cover unknown signals — that is the deliberation
/// demo, and the point.
fn auto_helm() -> Contract {
    Contract {
        id: "auto-helm".into(),
        label: "Boris holds the helm".into(),
        trigger: Trigger::Timer {
            interval_secs: 1,
            repeat: true,
        },
        rules: vec![
            Rule {
                condition: Condition::Compare {
                    field: "fuel".into(),
                    op: Comparison::Lt,
                    value: 154, // 0.15 tank
                },
                action: Action::verb("fuel_warning"),
                priority: 5,
            },
            Rule {
                condition: Condition::Compare {
                    field: "unknown_signal".into(),
                    op: Comparison::Eq,
                    value: 0,
                },
                action: Action::verb("maintain_course"),
                priority: 0,
            },
        ],
        llm_authority: Some(LlmConfig {
            fallback_on_timeout: true,
            timeout_ms: 4000,
            max_tokens: 128,
            system_prompt: "You are Boris. Crew safety > ship integrity > mission.".into(),
            fallback_action: Some(Action::verb("all_stop")),
        }),
    }
}

pub fn evaluate_contracts(
    time: Res<Time>,
    mut runtime: ResMut<ContractRuntime>,
    systems: Res<ShipSystems>,
    mut log: ResMut<ShipLog>,
    mut deliberation: ResMut<DeliberationState>,
) {
    if !runtime.eval_timer.tick(time.delta()).just_finished() {
        return;
    }
    if deliberation.active.is_some() {
        return; // Boris is already thinking; don't pile on
    }

    let mut ctx = EvalContext::default();
    ctx.set("fuel", systems.fuel.0)
        .set("unknown_signal", systems.unknown_signal as i64);

    // Own the verdict before touching `runtime` again — Outcome borrows the
    // contract.
    enum Decision {
        Act(String),
        Deliberate { timeout_ms: u32, fallback: Action },
        Silence,
    }
    let decision = match evaluate(&runtime.contract, &ctx) {
        Outcome::Rule { action, .. } => Decision::Act(action.kind.clone()),
        Outcome::Deliberate { llm } => Decision::Deliberate {
            timeout_ms: llm.timeout_ms,
            fallback: llm
                .fallback_action
                .clone()
                .unwrap_or_else(|| Action::verb("maintain_course")),
        },
        Outcome::NoDecision => Decision::Silence,
    };

    match decision {
        Decision::Act(kind) => {
            if runtime.last_action.as_deref() != Some(kind.as_str()) {
                match kind.as_str() {
                    "fuel_warning" => log.log("Boris: fuel is low. We should dock soon."),
                    "maintain_course" => log.log("Boris: holding course."),
                    other => log.log(format!("Boris: {other}.")),
                }
                runtime.last_action = Some(kind);
            }
        }
        Decision::Deliberate {
            timeout_ms,
            fallback,
        } => {
            log.log("Boris: my rules don't cover this. Thinking…");
            runtime.last_action = None;
            deliberation.active = Some(Deliberation {
                crew_member: "Boris".into(),
                context_summary: "Unknown signal detected".into(),
                remaining: Timer::from_seconds(timeout_ms as f32 / 1000.0, TimerMode::Once),
                fallback,
            });
        }
        Decision::Silence => {
            log.log("The helm is silent. Nobody decides.");
        }
    }
}

/// Ticks the deliberation timer. Offline mode: expiry = LLM timeout = the
/// fallback action fires and the log tells the story.
pub fn tick_deliberation(
    time: Res<Time>,
    mut deliberation: ResMut<DeliberationState>,
    mut systems: ResMut<ShipSystems>,
    mut log: ResMut<ShipLog>,
) {
    let Some(active) = deliberation.active.as_mut() else {
        return;
    };
    if !active.remaining.tick(time.delta()).is_finished() {
        return;
    }
    let fallback = active.fallback.kind.clone();
    let who = active.crew_member.clone();
    deliberation.active = None;
    systems.unknown_signal = false; // the moment passes
    log.log(format!(
        "{who} couldn't decide — fell back to {fallback}. (No inference in offline mode.)"
    ));
}
