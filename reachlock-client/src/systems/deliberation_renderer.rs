use bevy::prelude::*;
use reachlock_core::contract::{
    evaluate_all,
    stage::{CostSnapshot, DeliberationStage, RuleSnapshot, StagePhase, VerdictSnapshot},
    EvalContext, RuleResult,
};

use crate::systems::contract::{ContractRuntime, DeliberationState, ShipLog};
use crate::systems::ship::ShipSystems;
use crate::theme;

#[derive(Resource)]
pub struct DeliberationPanel {
    pub stage: Option<DeliberationStage>,
    pub phase_timer: Timer,
    pub dismiss_timer: Option<Timer>,
    pub awaiting_dismiss: bool,
    pub interjection_active: bool,
    pub pending_interjection_action: Option<String>,
}

impl Default for DeliberationPanel {
    fn default() -> Self {
        DeliberationPanel {
            stage: None,
            phase_timer: Timer::from_seconds(1.5, TimerMode::Once),
            dismiss_timer: None,
            awaiting_dismiss: false,
            interjection_active: false,
            pending_interjection_action: None,
        }
    }
}

#[derive(Component)]
pub struct DeliberationPanelUi;

#[derive(Component)]
pub struct DeliberationPanelText;

// Marker for the uncovered-rule counter, which the deliberation panel does
// not render yet.
#[allow(dead_code)]
#[derive(Component)]
pub struct UncoveredCounterUi;

fn build_stage(
    deliberation: &DeliberationState,
    runtime: &ContractRuntime,
    systems: &ShipSystems,
) -> Option<DeliberationStage> {
    let active = deliberation.active.as_ref()?;
    let contract = runtime.contracts.get(&runtime.active_id)?;

    let mut ctx = EvalContext::default();
    ctx.set("fuel", systems.fuel.0)
        .set("unknown_signal", systems.unknown_signal as i64);

    let results = evaluate_all(contract, &ctx);
    let matched: Vec<RuleSnapshot> = results
        .iter()
        .filter(|r| r.matched)
        .map(snapshot_from_result)
        .collect();
    let unmatched: Vec<RuleSnapshot> = results
        .iter()
        .filter(|r| !r.matched)
        .map(snapshot_from_result)
        .collect();

    Some(DeliberationStage {
        phase: StagePhase::Examining,
        crew_member: active.crew_member.clone(),
        crew_portrait_id: format!("{}_default", active.crew_member.to_lowercase()),
        crew_mood: "ANXIOUS".into(),
        trust_with_player: 256,
        context_summary: active.context_summary.clone(),
        matched_rules: matched,
        unmatched_rules: unmatched,
        verdict: None,
        cost: None,
        recent_uncovered: runtime.recent_uncovered(),
        remaining_cs: (active.remaining.remaining().as_millis() as u32) / 10,
        total_cs: (active.remaining.duration().as_millis() as u32) / 10,
    })
}

fn snapshot_from_result(r: &RuleResult) -> RuleSnapshot {
    RuleSnapshot {
        index: r.index,
        label: r.label.clone(),
        action: r.action.clone(),
        condition_summary: r.condition_summary.clone(),
        matched: r.matched,
    }
}

fn phase_duration(phase: &StagePhase) -> f32 {
    match phase {
        StagePhase::Examining => 2.0,
        StagePhase::Weighing => 3.0,
        StagePhase::Deciding => 4.0,
        StagePhase::Verdict => 3.0,
        StagePhase::Complete => 5.0,
    }
}

pub fn advance_deliberation_stage(
    time: Res<Time>,
    mut panel: ResMut<DeliberationPanel>,
    deliberation: Res<DeliberationState>,
    runtime: Res<ContractRuntime>,
    systems: Res<ShipSystems>,
    mut log: ResMut<ShipLog>,
) {
    if panel.stage.is_none() {
        if deliberation.active.is_some() {
            panel.stage = build_stage(&deliberation, &runtime, &systems);
        }
        return;
    }

    if panel.awaiting_dismiss {
        if let Some(ref mut dt) = panel.dismiss_timer {
            if dt.tick(time.delta()).is_finished() {
                panel.stage = None;
                panel.awaiting_dismiss = false;
                panel.dismiss_timer = None;
            }
        }
        return;
    }

    if panel.interjection_active {
        return;
    }

    if panel.phase_timer.tick(time.delta()).is_finished() {
        let current_phase = panel
            .stage
            .as_ref()
            .map(|s| s.phase.clone())
            .unwrap_or(StagePhase::Complete);
        let next = match current_phase {
            StagePhase::Examining => StagePhase::Weighing,
            StagePhase::Weighing => StagePhase::Deciding,
            StagePhase::Deciding => StagePhase::Verdict,
            StagePhase::Verdict => StagePhase::Complete,
            StagePhase::Complete => StagePhase::Complete,
        };
        panel.phase_timer = Timer::from_seconds(phase_duration(&next), TimerMode::Once);
        if let Some(ref mut stage) = panel.stage {
            stage.phase = next;
        }

        if panel
            .stage
            .as_ref()
            .map(|s| s.phase == StagePhase::Complete)
            .unwrap_or(false)
        {
            panel.awaiting_dismiss = true;
            panel.dismiss_timer = Some(Timer::from_seconds(5.0, TimerMode::Once));
            if let Some(crew) = &deliberation.just_completed {
                log.log(format!("Deliberation with {crew} completed."));
            }
        }
    }

    if let Some(ref active) = deliberation.active {
        if let Some(ref mut stage) = panel.stage {
            stage.remaining_cs = (active.remaining.remaining().as_millis() as u32) / 10;
        }
    }
}

pub fn spawn_deliberation_panel(mut commands: Commands) {
    commands.spawn((
        DeliberationPanelUi,
        DeliberationPanelText,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        theme::fg("text.warn"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(40.0),
            left: Val::Percent(25.0),
            max_width: Val::Px(600.0),
            ..default()
        },
    ));
}

pub fn despawn_deliberation_panel(
    mut commands: Commands,
    query: Query<Entity, With<DeliberationPanelUi>>,
) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
}

fn panel_lines(panel: &DeliberationPanel) -> String {
    let Some(stage) = &panel.stage else {
        return String::new();
    };

    let mut lines = Vec::new();

    match stage.phase {
        StagePhase::Examining => {
            lines.push(format!(
                "── {} is examining the situation ──",
                stage.crew_member
            ));
            lines.push(String::new());
            lines.push(format!("  Mood: {}", stage.crew_mood));
            lines.push(format!("  \"{}\"", stage.context_summary));
        }
        StagePhase::Weighing => {
            lines.push(format!("── {} is weighing the rules ──", stage.crew_member));
            lines.push(String::new());
            if !stage.matched_rules.is_empty() {
                lines.push("  RULES THAT MATCHED (green):".into());
                for r in &stage.matched_rules {
                    lines.push(format!("    ✓ {} — {}", r.label, r.condition_summary));
                }
            }
            if !stage.unmatched_rules.is_empty() {
                lines.push("  GAPS (red):".into());
                for r in &stage.unmatched_rules {
                    lines.push(format!("    ✗ {} — {}", r.label, r.condition_summary));
                }
            }
            if stage.matched_rules.is_empty() && stage.unmatched_rules.is_empty() {
                lines.push("  (no rules in contract)".into());
            }
        }
        StagePhase::Deciding => {
            lines.push(format!("── {} is deciding… ──", stage.crew_member));
            lines.push(String::new());
            lines.push("  ⏳ thinking".into());
            let bar_width = 20;
            let progress = stage
                .total_cs
                .checked_sub(stage.remaining_cs)
                .and_then(|diff| diff.checked_mul(bar_width as u32))
                .and_then(|prod| prod.checked_div(stage.total_cs.max(1)))
                .unwrap_or(0) as usize;
            let bar = "█".repeat(progress.min(bar_width));
            let empty = "░".repeat(bar_width.saturating_sub(progress));
            lines.push(format!("  [{bar}{empty}]"));
            let remaining_s = stage.remaining_cs / 100;
            let remaining_cs_frac = stage.remaining_cs % 100;
            lines.push(format!(
                "  {}.{:02}s remaining",
                remaining_s, remaining_cs_frac
            ));
        }
        StagePhase::Verdict => {
            lines.push(format!("── {} reached a verdict ──", stage.crew_member));
            if let Some(ref v) = stage.verdict {
                lines.push(format!("  Outcome: {}", v.outcome_label));
                lines.push(format!("  Action: {}", v.action_taken));
                lines.push(format!("  \"{}\"", v.reasoning));
                if let Some(ref esc) = v.escalation {
                    lines.push(format!("  ⚠ {esc}"));
                }
            }
            if !panel.interjection_active {
                lines.push(String::new());
                lines.push("  [Tab] Interject — make the call yourself".into());
            }
        }
        StagePhase::Complete => {
            lines.push(format!("── {} deliberation complete ──", stage.crew_member));
            if let Some(ref cost) = stage.cost {
                for (who, delta) in &cost.relationship_deltas {
                    let sign = if *delta > 0 { "+" } else { "" };
                    lines.push(format!("  {who} ({sign}{delta} trust)"));
                }
                if let Some(dmg) = cost.hull_damage {
                    lines.push(format!("  ⚠ Hull damage: {dmg}"));
                }
            }
            if stage.recent_uncovered > 0 {
                lines.push(format!(
                    "  Your rules left {} gaps this cycle.",
                    stage.recent_uncovered
                ));
            }
            lines.push(String::new());
            lines.push("  [Enter/Space] Dismiss".into());
        }
    }

    if panel.interjection_active {
        lines.push(String::new());
        lines.push("── INTERJECT ──".into());
        lines.push("  Choose an action to override with:".into());
        lines.push("  [1] maintain_course  [2] all_stop  [3] fuel_warning".into());
        lines.push("  [Esc] Cancel".into());
    }

    lines.join("\n")
}

pub fn render_deliberation_panel(
    panel: Res<DeliberationPanel>,
    mut query: Query<&mut Text, With<DeliberationPanelText>>,
) {
    if let Ok(mut text) = query.single_mut() {
        **text = panel_lines(&panel);
    }
}

pub fn handle_interjection_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut panel: ResMut<DeliberationPanel>,
    mut deliberation: ResMut<DeliberationState>,
    mut systems: ResMut<ShipSystems>,
    mut log: ResMut<ShipLog>,
) {
    let stage_is_some = panel.stage.is_some();
    if !stage_is_some {
        return;
    }

    if panel.interjection_active {
        if keys.just_pressed(KeyCode::Escape) {
            panel.interjection_active = false;
            panel.pending_interjection_action = None;
            return;
        }
        if keys.just_pressed(KeyCode::Digit1) {
            panel.pending_interjection_action = Some("maintain_course".into());
        }
        if keys.just_pressed(KeyCode::Digit2) {
            panel.pending_interjection_action = Some("all_stop".into());
        }
        if keys.just_pressed(KeyCode::Digit3) {
            panel.pending_interjection_action = Some("fuel_warning".into());
        }
        if let Some(action) = panel.pending_interjection_action.take() {
            panel.interjection_active = false;
            let crew = panel
                .stage
                .as_ref()
                .map(|s| s.crew_member.clone())
                .unwrap_or_default();
            let _active = deliberation.active.take();
            systems.unknown_signal = false;
            log.log(format!(
                "Captain overrode {crew}: ordered {action}. Trust may shift."
            ));
            let cost_snapshot = CostSnapshot {
                relationship_deltas: vec![(crew.clone(), -40)],
                hull_damage: None,
                cargo_loss: None,
            };
            if let Some(ref mut stage) = panel.stage {
                stage.verdict = Some(VerdictSnapshot {
                    outcome_label: "player_override".into(),
                    action_taken: action,
                    reasoning: "Captain made the call.".into(),
                    escalation: None,
                });
                stage.cost = Some(cost_snapshot);
                stage.phase = StagePhase::Complete;
            }
            panel.awaiting_dismiss = true;
            panel.dismiss_timer = Some(Timer::from_seconds(5.0, TimerMode::Once));
        }
        return;
    }

    let can_interject = panel
        .stage
        .as_ref()
        .map(|s| s.phase == StagePhase::Verdict || s.phase == StagePhase::Deciding)
        .unwrap_or(false);
    if can_interject && keys.just_pressed(KeyCode::Tab) {
        panel.interjection_active = true;
        return;
    }

    if panel.awaiting_dismiss
        && (keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space))
    {
        panel.stage = None;
        panel.awaiting_dismiss = false;
        panel.dismiss_timer = None;
    }
}

pub fn update_verdict_from_resolution(
    mut panel: ResMut<DeliberationPanel>,
    deliberation: Res<DeliberationState>,
    runtime: Res<ContractRuntime>,
) {
    let Some(ref mut stage) = panel.stage else {
        return;
    };
    if stage.verdict.is_some() {
        return;
    }
    if deliberation.just_completed.is_some() && stage.phase == StagePhase::Deciding {
        stage.verdict = Some(VerdictSnapshot {
            outcome_label: "offline_fallback".into(),
            action_taken: "maintain_course".into(),
            reasoning: "No rule matched; fell back to standard course.".into(),
            escalation: None,
        });
        stage.cost = Some(CostSnapshot {
            relationship_deltas: vec![(stage.crew_member.clone(), 10)],
            hull_damage: None,
            cargo_loss: None,
        });
        stage.recent_uncovered = runtime.recent_uncovered();
    }
}
