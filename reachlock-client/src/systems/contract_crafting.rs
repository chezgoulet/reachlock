//! Contract crafting workshop (S34). Keyboard-driven rule builder: Tab cycles
//! tabs, W/S selects a row, A/D cycles choices, Enter confirms/edits, Esc
//! cancels or closes the panel.
//!
//! Follows the ship editor pattern (S17): one Resource owns the working draft,
//! a pure function builds the display string, a system handles input.

use bevy::prelude::*;

use reachlock_core::contract::engine::{evaluate, EvalContext, Outcome};
use reachlock_core::contract::meta_game::seasoned_bonus;
use reachlock_core::contract::metadata::CraftingWarning;
use reachlock_core::contract::types::{
    Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger,
};
use reachlock_core::contract::validate_contract;

use crate::settings::{InputAction, Settings};
use crate::systems::crew::{CrewMember, CrewRole, CrewRoster};
use crate::systems::interaction::ActivePanel;
use crate::systems::soul::SoulRegistry;

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorkshopTab {
    Rules,
    LlmConfig,
    Persona,
    Simulation,
}

const TABS: [WorkshopTab; 4] = [
    WorkshopTab::Rules,
    WorkshopTab::LlmConfig,
    WorkshopTab::Persona,
    WorkshopTab::Simulation,
];

fn tab_name(t: WorkshopTab) -> &'static str {
    match t {
        WorkshopTab::Rules => "RULES",
        WorkshopTab::LlmConfig => "LLM",
        WorkshopTab::Persona => "PERSONA",
        WorkshopTab::Simulation => "SIM",
    }
}

// ---------------------------------------------------------------------------
// Rule column indices (for the Rules tab sub-selection)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RuleCol {
    Condition,
    Action,
    Priority,
}

const RULE_COLS: [RuleCol; 3] = [RuleCol::Condition, RuleCol::Action, RuleCol::Priority];

// ---------------------------------------------------------------------------
// Action vocabulary (the player picks from known action verbs)
// ---------------------------------------------------------------------------

const ACTION_VERBS: &[&str] = &[
    "wake_crew",
    "maintain_course",
    "reinforce_shields",
    "repair_systems",
    "tend_medbay",
    "man_battle_stations",
    "plot_jump",
    "hold_course",
    "all_stop",
    "set_speed",
    "fire_weapons",
    "retreat",
    "broadcast",
];

// ---------------------------------------------------------------------------
// Preset simulation scenarios for the Simulation tab
// ---------------------------------------------------------------------------

struct Scenario {
    name: &'static str,
    ctx: EvalContext,
}

fn preset_scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "Combat",
            ctx: {
                let mut c = EvalContext::default();
                c.set("weapons_damage", 512)
                    .set("shields", 0)
                    .set("crew_injured", 1)
                    .set("fuel", 800)
                    .set("hull", 1024);
                c
            },
        },
        Scenario {
            name: "Crisis",
            ctx: {
                let mut c = EvalContext::default();
                c.set("fuel", 100)
                    .set("hull", 200)
                    .set("fire_active", 1)
                    .set("crew_injured", 2)
                    .set("weapons_damage", 0);
                c
            },
        },
        Scenario {
            name: "Transit",
            ctx: {
                let mut c = EvalContext::default();
                c.set("fuel", 800)
                    .set("hull", 1024)
                    .set("distance_to_destination", 500)
                    .set("crew_injured", 0)
                    .set("fire_active", 0);
                c
            },
        },
        Scenario {
            name: "Social",
            ctx: {
                let mut c = EvalContext::default();
                c.set("station_contact", 1)
                    .set("faction_standing", 300)
                    .set("fuel", 1024)
                    .set("hull", 1024)
                    .set("crew_injured", 0);
                c
            },
        },
        Scenario {
            name: "Idle",
            ctx: {
                let mut c = EvalContext::default();
                c.set("fuel", 1024)
                    .set("hull", 1024)
                    .set("distance_to_destination", 0)
                    .set("crew_injured", 0)
                    .set("fire_active", 0);
                c
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Editor state
// ---------------------------------------------------------------------------

#[derive(Resource)]
pub struct ContractWorkshopState {
    pub draft: Option<Contract>,
    pub tab: WorkshopTab,
    pub sel: usize,
    pub col: RuleCol,
    pub dirty: bool,
    pub status: String,
    /// RON export text visible in the export tab area.
    #[allow(dead_code)]
    pub export_ron: String,
    /// Import text buffer (player pastes RON here).
    pub import_buffer: String,
    /// Import mode active.
    pub importing: bool,
    /// Simulation results cache.
    pub sim_results: Vec<(&'static str, String)>,
    /// Contract version (incremented on evolution).
    #[allow(dead_code)]
    pub version: u32,
    /// Evolution log for the current draft.
    #[allow(dead_code)]
    pub evolutions: Vec<String>,
    /// Metrics counters.
    pub metrics: WorkshopMetrics,
}

/// S34 metrics: counts of workshop events (no PII).
#[derive(Default)]
pub struct WorkshopMetrics {
    pub simulation_runs: u32,
    pub imports: u32,
    #[allow(dead_code)]
    pub exports: u32,
    #[allow(dead_code)]
    pub shares_shared: u32,
    #[allow(dead_code)]
    pub evolutions: u32,
}

impl Default for ContractWorkshopState {
    fn default() -> Self {
        ContractWorkshopState {
            draft: None,
            tab: WorkshopTab::Rules,
            sel: 0,
            col: RuleCol::Condition,
            dirty: false,
            status: String::new(),
            export_ron: String::new(),
            import_buffer: String::new(),
            importing: false,
            sim_results: Vec::new(),
            version: 1,
            evolutions: Vec::new(),
            metrics: WorkshopMetrics::default(),
        }
    }
}

/// Marker component for the contract workshop panel text node.
#[derive(Component)]
pub struct ContractWorkshopPanel;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_contract(crew: &CrewMember) -> Contract {
    Contract {
        id: format!("contract_{}", crew.id),
        label: format!("{}'s contract", crew.name),
        trigger: Trigger::Event {
            event_type: "situation".into(),
        },
        rules: vec![
            Rule {
                condition: Condition::Compare {
                    field: "hull".into(),
                    op: Comparison::Lt,
                    value: 512,
                },
                action: Action::verb("repair_systems"),
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
            system_prompt: String::new(),
            fallback_action: Some(Action::verb("maintain_course")),
        }),
    }
}

fn condition_summary(cond: &Condition) -> String {
    match cond {
        Condition::Always => "always".into(),
        Condition::Compare { field, op, value } => {
            let op_str = match op {
                Comparison::Lt => "<",
                Comparison::Le => "<=",
                Comparison::Eq => "==",
                Comparison::Ne => "!=",
                Comparison::Ge => ">=",
                Comparison::Gt => ">",
            };
            format!("{field} {op_str} {value}")
        }
        Condition::Not(c) => format!("not({})", condition_summary(c)),
        Condition::All(conds) => {
            let inner: Vec<String> = conds.iter().map(condition_summary).collect();
            format!("all({})", inner.join(", "))
        }
        Condition::Any(conds) => {
            let inner: Vec<String> = conds.iter().map(condition_summary).collect();
            format!("any({})", inner.join(", "))
        }
    }
}

fn cycle_op(op: Comparison, step: i64) -> Comparison {
    let ops = [
        Comparison::Lt,
        Comparison::Le,
        Comparison::Eq,
        Comparison::Ne,
        Comparison::Ge,
        Comparison::Gt,
    ];
    let i = ops.iter().position(|o| *o == op).unwrap_or(0);
    ops[(i as i64 + step).rem_euclid(ops.len() as i64) as usize]
}

fn cycle_verb(current: &str, step: i64) -> String {
    let i = ACTION_VERBS.iter().position(|v| *v == current).unwrap_or(0);
    ACTION_VERBS[(i as i64 + step).rem_euclid(ACTION_VERBS.len() as i64) as usize].to_string()
}

// ---------------------------------------------------------------------------
// Input handler system
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn workshop_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    panel: Res<ActivePanel>,
    mut state: ResMut<ContractWorkshopState>,
    roster: Res<CrewRoster>,
    souls: Res<SoulRegistry>,
) {
    if *panel != ActivePanel::ContractWorkshop {
        if state.draft.is_some() {
            state.draft = None;
            state.status.clear();
            state.sim_results.clear();
            state.importing = false;
        }
        return;
    }

    // Initialise draft from the first crew member's profile.
    if state.draft.is_none() {
        let member = roster.members.first().cloned().unwrap_or(CrewMember {
            id: "custom".into(),
            name: "Custom".into(),
            role: CrewRole::Engineer,
            duty_room: reachlock_core::generator::RoomKind::Reactor,
            current_room: reachlock_core::generator::RoomKind::Reactor,
            deck: 0,
            order: None,
            offscreen_eta: 0.0,
        });
        state.draft = Some(new_contract(&member));
        state.tab = WorkshopTab::Rules;
        state.sel = 0;
        state.col = RuleCol::Condition;
        state.dirty = true;
        state.status.clear();
        state.sim_results.clear();
        state.importing = false;
    }

    // ---- Tab switching (no borrow on draft needed) ----
    if keys.just_pressed(settings.key(InputAction::EditorTabNext)) {
        let i = TABS.iter().position(|t| *t == state.tab).unwrap_or(0);
        state.tab = TABS[(i + 1) % TABS.len()];
        state.sel = 0;
        state.col = RuleCol::Condition;
        state.importing = false;
        state.status.clear();
    }

    // ---- Row navigation (W/S) — compute row count without holding draft ----
    let prev_sel = state.sel;
    let row_count = match state.tab {
        WorkshopTab::Rules => state
            .draft
            .as_ref()
            .map(|d| d.rules.len().max(1))
            .unwrap_or(1),
        WorkshopTab::LlmConfig => 5,
        WorkshopTab::Persona => roster.members.len().max(1),
        WorkshopTab::Simulation => {
            if state.sim_results.is_empty() {
                1
            } else {
                state.sim_results.len() + 2
            }
        }
    };

    if keys.just_pressed(settings.key(InputAction::EditorCursorUp)) {
        state.sel = (prev_sel + row_count - 1) % row_count;
        state.status.clear();
    } else if keys.just_pressed(settings.key(InputAction::EditorCursorDown)) {
        state.sel = (prev_sel + 1) % row_count;
        state.status.clear();
    }

    // ---- Column cycling in Rules tab (A/D) ----
    let step = if keys.just_pressed(settings.key(InputAction::EditorCursorRight)) {
        1
    } else if keys.just_pressed(settings.key(InputAction::EditorCursorLeft)) {
        -1
    } else {
        0
    };

    // ---- Dispatch to tab handlers (each borrows draft from state as needed) ----
    match state.tab {
        WorkshopTab::Rules => {
            handle_rules_tab(&keys, &settings, &mut state, step);
        }
        WorkshopTab::LlmConfig => {
            handle_llm_tab(&keys, &mut state, step);
        }
        WorkshopTab::Persona => {
            handle_persona_tab(&mut state, &roster, &souls, step);
        }
        WorkshopTab::Simulation => {
            handle_sim_tab(&keys, &mut state);
        }
    }

    if step != 0 {
        state.dirty = true;
    }
}

fn handle_rules_tab(
    keys: &ButtonInput<KeyCode>,
    settings: &Settings,
    state: &mut ContractWorkshopState,
    step: i64,
) {
    let sel = state.sel;
    if step != 0 {
        let draft = state.draft.as_mut().unwrap();
        if sel < draft.rules.len() {
            let cols_len = RULE_COLS.len();
            let col_i = RULE_COLS.iter().position(|c| *c == state.col).unwrap_or(0);
            state.col = RULE_COLS[(col_i + 1) % cols_len];
        }
        return;
    }

    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        if state.importing {
            let trimmed = state.import_buffer.trim();
            if let Ok(imported) = ron::from_str::<Contract>(trimmed) {
                state.draft = Some(imported);
                state.metrics.imports += 1;
                state.status = "contract imported".into();
            } else {
                state.status = "invalid RON — import failed".into();
            }
            state.importing = false;
            return;
        }

        let draft = state.draft.as_mut().unwrap();
        if sel < draft.rules.len() {
            let rule = &mut draft.rules[sel];
            match state.col {
                RuleCol::Condition => {
                    if let Condition::Compare { op, .. } = &mut rule.condition {
                        *op = cycle_op(*op, 1);
                        state.status = "condition: cycled operator".into();
                    }
                }
                RuleCol::Action => {
                    rule.action.kind = cycle_verb(&rule.action.kind, 1);
                    state.status = format!("action: {}", rule.action.kind);
                }
                RuleCol::Priority => {
                    rule.priority = rule.priority.wrapping_add(1);
                    state.status = format!("priority: {}", rule.priority);
                }
            }
        }
    }

    if keys.just_pressed(settings.key(InputAction::EditorDelete)) {
        let draft = state.draft.as_mut().unwrap();
        if sel <= draft.rules.len() {
            draft.rules.push(Rule {
                condition: Condition::Compare {
                    field: "hull".into(),
                    op: Comparison::Lt,
                    value: 512,
                },
                action: Action::verb("maintain_course"),
                priority: 0,
            });
            state.status = format!("rule {} added", draft.rules.len() - 1);
        }
    }

    if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        let draft = state.draft.as_ref().unwrap();
        let warnings = validate_contract(draft);
        if warnings.is_empty() {
            state.status = "no craft warnings".into();
        } else {
            let ws: Vec<String> = warnings.iter().map(|w| format!("{w:?}")).collect();
            state.status = format!("warnings: {}", ws.join(", "));
        }
    }
}

fn handle_llm_tab(keys: &ButtonInput<KeyCode>, state: &mut ContractWorkshopState, step: i64) {
    let sel = state.sel;
    if step != 0 {
        let draft = state.draft.as_mut().unwrap();
        let llm = draft.llm_authority.get_or_insert(LlmConfig {
            fallback_on_timeout: true,
            timeout_ms: 15000,
            max_tokens: 256,
            system_prompt: String::new(),
            fallback_action: Some(Action::verb("maintain_course")),
        });
        match sel {
            0 => llm.fallback_on_timeout = !llm.fallback_on_timeout,
            1 => {
                llm.timeout_ms = (llm.timeout_ms as i64 + step * 1000).clamp(1000, 120000) as u32;
            }
            2 => {
                llm.max_tokens = (llm.max_tokens as i64 + step * 32).clamp(32, 4096) as u32;
            }
            3 => {
                let current = llm
                    .fallback_action
                    .as_ref()
                    .map(|a| a.kind.as_str())
                    .unwrap_or("maintain_course");
                let next = cycle_verb(current, step);
                llm.fallback_action = Some(Action::verb(next));
            }
            4 => {}
            _ => {}
        }
        state.dirty = true;
        state.status.clear();
    }

    if keys.just_pressed(KeyCode::Enter) {
        state.status = "prompt preview shown below".into();
    }
}

fn handle_persona_tab(
    state: &mut ContractWorkshopState,
    roster: &CrewRoster,
    souls: &SoulRegistry,
    step: i64,
) {
    let sel = state.sel;
    if step != 0 {
        let draft = state.draft.as_mut().unwrap();
        if let Some(member) = roster.members.get(sel) {
            let traits = souls
                .files
                .get(&member.id)
                .map(|file| {
                    file.personality
                        .traits
                        .iter()
                        .map(|t| t.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let persona = format!(
                "You are {}. Role: {:?}. Traits: {}.",
                member.name, member.role, traits
            );
            if let Some(llm) = &mut draft.llm_authority {
                llm.system_prompt = persona.clone();
                state.status = "persona auto-filled".into();
            } else {
                state.status = "no LLM config — enable one first".into();
            }
        }
    }
}

fn handle_sim_tab(keys: &ButtonInput<KeyCode>, state: &mut ContractWorkshopState) {
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::KeyR) {
        let draft = state.draft.as_ref().unwrap();
        let mut results: Vec<(&str, String)> = Vec::new();
        for sc in preset_scenarios() {
            let outcome = evaluate(draft, &sc.ctx);
            let summary = match &outcome {
                Outcome::Rule { action, .. } => format!("rule → {}", action.kind),
                Outcome::Deliberate { .. } => "→ LLM deliberation".into(),
                Outcome::NoDecision => "→ no decision".into(),
            };
            results.push((sc.name, summary));
        }
        let warnings = validate_contract(draft);
        let warn_summary = if warnings.is_empty() {
            "no warnings".to_string()
        } else {
            let ws: Vec<String> = warnings.iter().map(|w| format!("{w:?}")).collect();
            ws.join(", ")
        };
        results.push(("Validation", warn_summary));
        state.sim_results = results;
        state.metrics.simulation_runs += 1;
        state.status = "simulation complete".into();
    }
}

// ---------------------------------------------------------------------------
// Panel text rendering
// ---------------------------------------------------------------------------

pub fn workshop_panel_text(
    state: &ContractWorkshopState,
    roster: &CrewRoster,
    souls: &SoulRegistry,
) -> String {
    let Some(draft) = &state.draft else {
        return String::new();
    };

    let mut lines = vec![
        "── CONTRACT WORKSHOP ──  Tab · W/S row · A/D change · Enter act · Esc validate".into(),
        {
            let tabs: Vec<String> = TABS
                .iter()
                .map(|t| {
                    let name = tab_name(*t);
                    if *t == state.tab {
                        format!("[{name}]")
                    } else {
                        name.to_string()
                    }
                })
                .collect();
            tabs.join(" ")
        },
    ];

    let cursor = |i: usize| if i == state.sel { "> " } else { "  " };

    match state.tab {
        WorkshopTab::Rules => {
            lines.push(format!("contract: {}", draft.label));
            lines.push(format!("trigger: {}", trigger_summary(&draft.trigger)));

            if state.importing {
                lines.push("── IMPORT ──".into());
                lines.push("Paste RON contract below, then Enter to confirm:".into());
                lines.push(format!("> {}", state.import_buffer));
                lines.push("(Esc cancel)".into());
                return lines.join("\n");
            }

            for (i, rule) in draft.rules.iter().enumerate() {
                let _col_i = RULE_COLS.iter().position(|c| *c == state.col).unwrap_or(0);
                let col_mark = |col: RuleCol| {
                    if i == state.sel && state.col == col {
                        "▸"
                    } else {
                        " "
                    }
                };
                lines.push(format!(
                    "{}[{}] {}{:30} {}{:20} {}{}",
                    cursor(i),
                    i,
                    col_mark(RuleCol::Condition),
                    condition_summary(&rule.condition),
                    col_mark(RuleCol::Action),
                    rule.action.kind,
                    col_mark(RuleCol::Priority),
                    rule.priority,
                ));
            }
            // "Add rule" row (implicit — press Delete on last row to add, but
            // we display a hint).
            {
                let i = draft.rules.len();
                lines.push(format!(
                    "{}[+] — new rule —  (Delete key)",
                    cursor(if i == state.sel { i } else { draft.rules.len() })
                ));
            }

            let warnings = validate_contract(draft);
            if !warnings.is_empty() {
                lines.push("── CRAFT WARNINGS (Esc to re-check) ──".into());
                for w in &warnings {
                    let label: String = match w {
                        CraftingWarning::AlwaysResolvesWithoutLLM => {
                            "always resolves — no LLM edge".to_string()
                        }
                        CraftingWarning::AlwaysRequiresLLM => {
                            "always needs LLM — no Always rule".to_string()
                        }
                        CraftingWarning::AllSamePriority => "all rules same priority".to_string(),
                        CraftingWarning::NoFallbackBehavior => "no fallback action".to_string(),
                        CraftingWarning::OverSpecificTrigger => "over-specific trigger".to_string(),
                        CraftingWarning::CircularRule => "circular rule dependency".to_string(),
                    };
                    lines.push(format!("  ⚠ {label}"));
                }
            }
        }
        WorkshopTab::LlmConfig => {
            let default_llm = LlmConfig {
                fallback_on_timeout: true,
                timeout_ms: 15000,
                max_tokens: 256,
                system_prompt: String::new(),
                fallback_action: None,
            };
            let llm = draft.llm_authority.as_ref().unwrap_or(&default_llm);
            lines.push(format!(
                "{}fallback on timeout: {}",
                cursor(0),
                llm.fallback_on_timeout
            ));
            lines.push(format!("{}timeout (ms): {}", cursor(1), llm.timeout_ms));
            lines.push(format!("{}max tokens: {}", cursor(2), llm.max_tokens));
            let fallback_label = llm
                .fallback_action
                .as_ref()
                .map(|a| a.kind.as_str())
                .unwrap_or("— none —");
            lines.push(format!("{}fallback action: {fallback_label}", cursor(3)));
            lines.push(format!(
                "{}system prompt: (too long to edit here)",
                cursor(4)
            ));
            lines.push(String::new());
            lines.push("── PROMPT PREVIEW ──".into());
            let preview = if llm.system_prompt.is_empty() {
                "(empty — set from Persona tab)".into()
            } else {
                llm.system_prompt.clone()
            };
            lines.push(preview);
        }
        WorkshopTab::Persona => {
            lines.push("Select a crew member and press A/D to auto-fill persona:".into());
            for (i, member) in roster.members.iter().enumerate() {
                let soul_traits = souls
                    .files
                    .get(&member.id)
                    .map(|file| {
                        let moods: Vec<&str> =
                            file.personality.quirks.iter().map(|s| s.as_str()).collect();
                        if moods.is_empty() {
                            "no quirks".into()
                        } else {
                            moods.join(", ")
                        }
                    })
                    .unwrap_or_else(|| "no soul file".into());
                lines.push(format!(
                    "{}[{}] {:10}  {:12}  quirks: {}",
                    cursor(i),
                    i,
                    member.name,
                    format!("{:?}", member.role),
                    soul_traits,
                ));
            }
            lines.push(String::new());
            let current_prompt = draft
                .llm_authority
                .as_ref()
                .map(|l| l.system_prompt.as_str())
                .unwrap_or("(no LLM config)");
            lines.push(format!("current persona: {current_prompt}"));
        }
        WorkshopTab::Simulation => {
            lines.push("── SCENARIO  Simulation  ──".into());
            lines.push("  Enter = run all 5 scenarios  (R = rerun)".into());

            if state.sim_results.is_empty() {
                lines.push("  (not yet run)".into());
            } else {
                for (name, summary) in state.sim_results.iter() {
                    if name == &"Validation" {
                        lines.push(format!("── {} ──", name));
                        lines.push(format!("  {summary}"));
                    } else {
                        lines.push(format!("  {name:12}  {summary}"));
                    }
                }
                // Coverage summary.
                let rule_count = state
                    .sim_results
                    .iter()
                    .filter(|(_, s)| s.starts_with("rule"))
                    .count();
                let deliberate_count = state
                    .sim_results
                    .iter()
                    .filter(|(_, s)| s.starts_with("→ LLM"))
                    .count();
                let total = state.sim_results.len().max(1);
                lines.push(format!(
                    "  ── {}/{} rules fired, {}/{} LLM calls ──",
                    rule_count,
                    total - 1,
                    deliberate_count,
                    total - 1,
                ));
                // Seasoned bonus display (WO-6).
                let bonus = seasoned_bonus(
                    state.metrics.simulation_runs,
                    state.metrics.simulation_runs * 2,
                );
                lines.push(format!(
                    "  seasoned bonus: trust +{}  depth: {}",
                    bonus.trust_bonus, bonus.personality_depth
                ));
            }
        }
    }

    // Status line at the bottom.
    if !state.status.is_empty() {
        lines.push(format!("  · {}", state.status));
    }

    // Export/import actions.
    lines.push(String::new());
    lines.push("  [Ctrl+C] export RON  [Ctrl+V] import RON  (escapes only)".into());

    lines.join("\n")
}

fn trigger_summary(trigger: &Trigger) -> String {
    match trigger {
        Trigger::Timer {
            interval_secs,
            repeat,
        } => {
            format!(
                "every {interval_secs}s{}",
                if *repeat { "" } else { " (once)" }
            )
        }
        Trigger::Event { event_type } => format!("on event: {event_type}"),
        Trigger::StateChange { field, op, value } => {
            let op_str = match op {
                Comparison::Lt => "<",
                Comparison::Le => "<=",
                Comparison::Eq => "==",
                Comparison::Ne => "!=",
                Comparison::Ge => ">=",
                Comparison::Gt => ">",
            };
            format!("{field} {op_str} {value}")
        }
        Trigger::Manual => "manual".into(),
    }
}

// ---------------------------------------------------------------------------
// Spawn the panel text node
// ---------------------------------------------------------------------------

pub fn spawn_workshop_panel(mut commands: Commands) {
    commands.spawn((
        ContractWorkshopPanel,
        Text::new(""),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.95, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(100.0),
            left: Val::Px(300.0),
            max_width: Val::Px(500.0),
            ..default()
        },
    ));
}

// ---------------------------------------------------------------------------
// Update system — render the panel text each frame
// ---------------------------------------------------------------------------

pub fn render_workshop_panel(
    panel: Res<ActivePanel>,
    state: Res<ContractWorkshopState>,
    roster: Res<CrewRoster>,
    souls: Res<SoulRegistry>,
    mut texts: Query<&mut Text, With<ContractWorkshopPanel>>,
) {
    if let Ok(mut text) = texts.single_mut() {
        match &*panel {
            ActivePanel::ContractWorkshop => {
                **text = workshop_panel_text(&state, &roster, &souls);
            }
            _ => {
                **text = String::new();
            }
        }
    }
}
