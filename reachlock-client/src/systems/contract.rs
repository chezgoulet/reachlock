//! Contract engine integration (spec §6, §9): the core engine evaluates the
//! auto-helm contract against live ship state. Rule hits act instantly; the
//! unresolvable case enters deliberation. Offline mode (or online-but-not-
//! yet-connected) has no LLM, so the timeout path fires the fallback
//! action, exactly the "Boris couldn't decide" story. Online + connected
//! routes the same deliberation through the server instead (S02) — see
//! `systems::network`, which drives this module's `resolve_response` /
//! `resolve_timeout` from `llm.response` / `llm.failed` / `llm.deliberating`.

use bevy::prelude::*;
use reachlock_core::contract::{
    engine::{evaluate, EvalContext, Outcome},
    signature::SignatureChain,
    types::{Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger},
};
use reachlock_core::network::ClientMessage;

use crate::net::{ConnectionState, NetMode, NetOutbox};
use crate::systems::ship::ShipSystems;

/// The ship's log: every decision the automation makes, newest last.
#[derive(Resource, Default)]
pub struct ShipLog {
    pub entries: Vec<String>,
}

impl ShipLog {
    // S02: `systems::network` also logs directly (connection state,
    // seed sync, server errors) — pub(crate) rather than private.
    pub(crate) fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        info!("ship log: {line}");
        self.entries.push(line);
        if self.entries.len() > 200 {
            self.entries.remove(0);
        }
    }
}

/// Client-side deliberation state (spec §9): active while "Boris is
/// thinking". Offline: a timer that expires into the fallback action.
#[derive(Resource, Default)]
pub struct DeliberationState {
    pub active: Option<Deliberation>,
    /// Set to the crew member's name when deliberation resolves. The story
    /// submission system reads and clears this to detect that deliberation
    /// just completed.
    pub just_completed: Option<String>,
}

pub struct Deliberation {
    pub crew_member: String,
    pub context_summary: String,
    /// Offline: the LLM timeout. Online: a generous safety net — if the
    /// socket drops mid-call and no `llm.response`/`llm.failed` ever
    /// arrives, this still expires into the fallback action (spec: online
    /// adds, never replaces).
    pub remaining: Timer,
    pub fallback: Action,
    /// S02: `Some(call_id)` while an online LLM call is in flight;
    /// `systems::network` matches server responses against this. `None`
    /// offline, and cleared once resolved.
    pub call_id: Option<String>,
    /// S02: whether the deliberation overlay should render right now.
    /// Offline: true immediately. Online: stays false until `llm.deliberating`
    /// confirms the server is on it, so the overlay reads as deliberation
    /// (spec finding #5), not as a frozen HUD during ordinary latency.
    pub overlay_visible: bool,
}

#[derive(Resource)]
pub struct ContractRuntime {
    pub contract: Contract,
    pub eval_timer: Timer,
    /// Last action kind, to log only on change instead of every second.
    last_action: Option<String>,
    /// S02: signs every fired action for online submission (spec §6).
    /// Offline mode never touches this.
    chain: SignatureChain,
    /// Monotonic counter feeding `chain.sign_next` — `verify_chain` only
    /// requires strict monotonicity, not wall-clock ticks.
    next_tick: u64,
    /// S16B (S15 completion): rolling count of recent evaluations the rules
    /// did NOT cover. More uncovered edges = worse deliberation odds
    /// (`agency::contract_quality_modifier`) — write tighter rules.
    recent_uncovered: u8,
}

impl Default for ContractRuntime {
    fn default() -> Self {
        ContractRuntime {
            contract: auto_helm(),
            eval_timer: Timer::from_seconds(1.0, TimerMode::Repeating),
            last_action: None,
            chain: SignatureChain::default(),
            next_tick: 0,
            recent_uncovered: 0,
        }
    }
}

impl ContractRuntime {
    fn next_tick(&mut self) -> u64 {
        self.next_tick += 1;
        self.next_tick
    }

    /// Signs `action` as the next link in this runtime's chain and returns
    /// the wire message ready for `NetOutbox`.
    fn sign_eval(&mut self, action: &Action) -> ClientMessage {
        let tick = self.next_tick();
        let eval = self.chain.sign_next(&self.contract.id, tick, action);
        ClientMessage::EvalSubmit { eval }
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

#[allow(clippy::too_many_arguments)]
pub fn evaluate_contracts(
    time: Res<Time>,
    mut runtime: ResMut<ContractRuntime>,
    systems: Res<ShipSystems>,
    mut log: ResMut<ShipLog>,
    mut deliberation: ResMut<DeliberationState>,
    mode: Res<NetMode>,
    conn: Res<ConnectionState>,
    mut outbox: ResMut<NetOutbox>,
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
        Act(Action),
        Deliberate { timeout_ms: u32, fallback: Action },
        Silence,
    }
    let decision = match evaluate(&runtime.contract, &ctx) {
        Outcome::Rule { action, .. } => Decision::Act(action.clone()),
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
        Decision::Act(action) => {
            let kind = action.kind.clone();
            if runtime.last_action.as_deref() != Some(kind.as_str()) {
                match kind.as_str() {
                    "fuel_warning" => log.log("Boris: fuel is low. We should dock soon."),
                    "maintain_course" => log.log("Boris: holding course."),
                    other => log.log(format!("Boris: {other}.")),
                }
                runtime.last_action = Some(kind);
            }
            runtime.recent_uncovered = runtime.recent_uncovered.saturating_sub(1);
            // S02: every fired contract action is signed and submitted in
            // online mode, whether or not it's a fresh log line.
            if mode.is_online() {
                outbox.push(runtime.sign_eval(&action));
            }
        }
        Decision::Deliberate {
            timeout_ms,
            fallback,
        } => {
            runtime.last_action = None;
            runtime.recent_uncovered = (runtime.recent_uncovered + 1).min(8);
            if mode.is_online() && matches!(*conn, ConnectionState::Connected) {
                let tick = runtime.next_tick();
                let call_id = format!("{}-{tick}", runtime.contract.id);
                log.log("Boris: my rules don't cover this. Radioing it in…");
                // S16B wire revision: the contract's own LLM budget rides
                // the call; the server clamps by its cap.
                let llm = runtime.contract.llm_authority.as_ref();
                outbox.push(ClientMessage::LlmCall {
                    call_id: call_id.clone(),
                    contract_id: runtime.contract.id.clone(),
                    context: serde_json::json!({ "unknown_signal": 1 }),
                    system_prompt: llm.map(|c| c.system_prompt.clone()),
                    timeout_ms: llm.map(|c| c.timeout_ms),
                    max_tokens: llm.map(|c| c.max_tokens),
                });
                deliberation.active = Some(Deliberation {
                    crew_member: "Boris".into(),
                    context_summary: "Unknown signal detected".into(),
                    remaining: Timer::from_seconds(timeout_ms as f32 / 1000.0, TimerMode::Once),
                    fallback,
                    call_id: Some(call_id),
                    overlay_visible: false,
                });
            } else {
                log.log("Boris: my rules don't cover this. Thinking…");
                deliberation.active = Some(Deliberation {
                    crew_member: "Boris".into(),
                    context_summary: "Unknown signal detected".into(),
                    remaining: Timer::from_seconds(timeout_ms as f32 / 1000.0, TimerMode::Once),
                    fallback,
                    call_id: None,
                    overlay_visible: true,
                });
            }
        }
        Decision::Silence => {
            log.log("The helm is silent. Nobody decides.");
        }
    }
}

/// Ticks the deliberation timer. Offline mode (or online with no response
/// ever arriving — a dropped socket mid-call): expiry = LLM timeout = the
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
    let note = if active.call_id.is_some() {
        "no response from the ship's mind — socket may have dropped"
    } else {
        "no inference in offline mode"
    };
    resolve_timeout(&mut deliberation, &mut systems, &mut log, note);
}

/// Fallback path: the deliberation expired (offline timeout, or the online
/// safety net after `llm.failed` / a dropped connection) without a usable
/// answer. Fires the contract's fallback action and clears the moment.
pub fn resolve_timeout(
    deliberation: &mut DeliberationState,
    systems: &mut ShipSystems,
    log: &mut ShipLog,
    note: &str,
) {
    let Some(active) = deliberation.active.take() else {
        return;
    };
    systems.unknown_signal = false; // the moment passes
    log.log(format!(
        "{} couldn't decide — fell back to {}. ({note}.)",
        active.crew_member, active.fallback.kind
    ));
}

/// Success path: an online deliberation resolved via `llm.response` — now
/// classified through the S15 outcome table (spec §18): the model answered,
/// and the fiction decides how well. Rolls derive from `(contract_id,
/// tick, chain position)`, so a replay replays the same fate. `trust` is
/// the deliberating crew member's `trust.player` (S13), 0 when unknown.
#[allow(clippy::too_many_arguments)]
pub fn resolve_response(
    deliberation: &mut DeliberationState,
    systems: &mut ShipSystems,
    log: &mut ShipLog,
    runtime: &mut ContractRuntime,
    outbox: &mut NetOutbox,
    feed: &mut crate::systems::comms::CommFeed,
    action_kind: &str,
    reasoning: &str,
    tier: reachlock_core::universe::UniverseTier,
    trust: i64,
) {
    use reachlock_core::agency;

    let Some(active) = deliberation.active.take() else {
        return;
    };
    deliberation.just_completed = Some(active.crew_member.clone());
    systems.unknown_signal = false;

    // The contract's own verb set: a wrong call is always a plausible one.
    let mut action_set: Vec<String> = runtime
        .contract
        .rules
        .iter()
        .map(|r| r.action.kind.clone())
        .collect();
    action_set.dedup();

    let weights = agency::weights(
        &agency::BASELINE,
        &[
            agency::model_modifier(tier),
            agency::crew_modifier(trust),
            // S16B: contract quality is live — recent uncovered evaluations
            // worsen the odds (the "write tighter rules" lever).
            agency::contract_quality_modifier(runtime.recent_uncovered as u32),
            // Equipment pipe: stat keys read as zero until S05 items equip.
            agency::equipment_modifier(0, 0),
        ],
    );
    let (resolved, trace) = agency::deliberate(
        &agency::DeliberationParams {
            contract_id: &runtime.contract.id,
            tick: runtime.next_tick,
            chain_position: 0,
            context_summary: &active.context_summary,
            weights: &weights,
            action_set: &action_set,
            fallback_action: &active.fallback.kind,
        },
        agency::ProviderVerdict::Answered {
            action: action_kind.to_string(),
            // S16: crew comms speak through the same voice pipeline as
            // dialogue — meta stripped, length capped, in-character.
            reasoning: reachlock_core::dialogue::shape_line(reasoning, &active.crew_member),
        },
    );
    // The start-of-deliberation line already hit the log when the moment
    // opened; the outcome (and the fallback, if one fired) land now — the
    // §18 traceability triple.
    for entry in trace.iter().skip(1) {
        log.log(entry.render(&active.crew_member));
    }
    // S16B: the crew says it out loud too (HUD comm line / speech bubble).
    feed.say(active.crew_member.clone(), resolved.reasoning.clone());
    let fired = resolved
        .action
        .clone()
        .unwrap_or_else(|| active.fallback.kind.clone());
    outbox.push(runtime.sign_eval(&Action::verb(fired)));

    // Catastrophic escalation: recoverable by design (hull stress, never a
    // corrupted save) — the death/respawn loop already handles a breach.
    if let Some(escalation) = resolved.escalation {
        systems.hull_hp = reachlock_core::util::rng::Fixed((systems.hull_hp.0 - 300).max(64));
        log.log(format!(
            "{} escalated: {escalation} — hull stress rising. Recoverable. Barely.",
            active.crew_member
        ));
    }
}

/// Failure path with S15 categories: `llm.failed { reason }` maps onto the
/// outcome table's rows (timeout/rate_limited → Timeout, bad_response →
/// Collapse), so a timeout and a collapse read as different stories in the
/// log before the fallback fires.
pub fn resolve_failed(
    deliberation: &mut DeliberationState,
    systems: &mut ShipSystems,
    log: &mut ShipLog,
    runtime: &ContractRuntime,
    feed: &mut crate::systems::comms::CommFeed,
    reason: &str,
) {
    use reachlock_core::agency;

    let Some(active) = deliberation.active.take() else {
        return;
    };
    deliberation.just_completed = Some(active.crew_member.clone());
    systems.unknown_signal = false;
    let verdict = match reason {
        "bad_response" => agency::ProviderVerdict::Collapsed,
        _ => agency::ProviderVerdict::TimedOut,
    };
    let (resolved, trace) = agency::deliberate(
        &agency::DeliberationParams {
            contract_id: &runtime.contract.id,
            tick: runtime.next_tick,
            chain_position: 0,
            context_summary: &active.context_summary,
            weights: &agency::BASELINE,
            action_set: &[],
            fallback_action: &active.fallback.kind,
        },
        verdict,
    );
    for entry in trace.iter().skip(1) {
        log.log(entry.render(&active.crew_member));
    }
    feed.say(active.crew_member.clone(), resolved.reasoning.clone());
}
