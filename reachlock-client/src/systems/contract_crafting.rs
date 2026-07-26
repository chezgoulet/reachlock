//! Contract crafting workshop (S34). SelectablePanel-driven: row builders
//! convert state into SelectableRows; a shared system handles navigation/display.

use crate::settings::{InputAction, Settings};
use crate::systems::crew::{CrewMember, CrewRoster};
use crate::systems::interaction::ActivePanel;
use crate::systems::soul::SoulRegistry;
use crate::theme;
use crate::widget_kit::panel::{navigate_selectable_panel, SelectableRow};
use bevy::prelude::*;
use reachlock_core::contract::engine::{evaluate, EvalContext, Outcome};
use reachlock_core::contract::meta_game::seasoned_bonus;
use reachlock_core::contract::types::{
    Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger,
};
use reachlock_core::contract::validate_contract;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RuleCol {
    Condition,
    Action,
    Priority,
}
const RULE_COLS: [RuleCol; 3] = [RuleCol::Condition, RuleCol::Action, RuleCol::Priority];

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

struct Scenario {
    name: &'static str,
    ctx: EvalContext,
}
fn preset_scenarios() -> Vec<Scenario> {
    let s = |name: &'static str, vals: &[(&str, i64)]| -> Scenario {
        let mut ctx = EvalContext::default();
        for (k, v) in vals {
            ctx.set(*k, *v);
        }
        Scenario { name, ctx }
    };
    vec![
        s(
            "Combat",
            &[
                ("weapons_damage", 512),
                ("shields", 0),
                ("crew_injured", 1),
                ("fuel", 800),
                ("hull", 1024),
            ],
        ),
        s(
            "Crisis",
            &[
                ("fuel", 100),
                ("hull", 200),
                ("fire_active", 1),
                ("crew_injured", 2),
                ("weapons_damage", 0),
            ],
        ),
        s(
            "Transit",
            &[
                ("fuel", 800),
                ("hull", 1024),
                ("distance_to_destination", 500),
                ("crew_injured", 0),
                ("fire_active", 0),
            ],
        ),
        s(
            "Social",
            &[
                ("station_contact", 1),
                ("faction_standing", 300),
                ("fuel", 1024),
                ("hull", 1024),
                ("crew_injured", 0),
            ],
        ),
        s(
            "Idle",
            &[
                ("fuel", 1024),
                ("hull", 1024),
                ("distance_to_destination", 0),
                ("crew_injured", 0),
                ("fire_active", 0),
            ],
        ),
    ]
}

#[derive(Resource)]
pub struct ContractWorkshopState {
    pub draft: Option<Contract>,
    pub tab: WorkshopTab,
    pub sel: usize,
    pub col: RuleCol,
    pub dirty: bool,
    pub status: String,
    #[allow(dead_code)]
    pub export_ron: String,
    pub import_buffer: String,
    pub importing: bool,
    pub sim_results: Vec<(&'static str, String)>,
    #[allow(dead_code)]
    pub version: u32,
    #[allow(dead_code)]
    pub evolutions: Vec<String>,
    pub metrics: WorkshopMetrics,
}
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
#[derive(Component)]
pub struct ContractWorkshopPanel;

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
        Condition::Compare { field, op, value } => format!(
            "{field} {} {value}",
            match op {
                Comparison::Lt => "<",
                Comparison::Le => "<=",
                Comparison::Eq => "==",
                Comparison::Ne => "!=",
                Comparison::Ge => ">=",
                Comparison::Gt => ">",
            }
        ),
        Condition::Not(c) => format!("not({})", condition_summary(c)),
        Condition::All(conds) => format!(
            "all({})",
            conds
                .iter()
                .map(condition_summary)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Condition::Any(conds) => format!(
            "any({})",
            conds
                .iter()
                .map(condition_summary)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
fn cycle_op(op: Comparison, step: i64) -> Comparison {
    const OPS: [Comparison; 6] = [
        Comparison::Lt,
        Comparison::Le,
        Comparison::Eq,
        Comparison::Ne,
        Comparison::Ge,
        Comparison::Gt,
    ];
    OPS[(OPS.iter().position(|o| *o == op).unwrap_or(0) as i64 + step).rem_euclid(OPS.len() as i64)
        as usize]
}
fn cycle_verb(current: &str, step: i64) -> String {
    let i = ACTION_VERBS.iter().position(|v| *v == current).unwrap_or(0);
    ACTION_VERBS[(i as i64 + step).rem_euclid(ACTION_VERBS.len() as i64) as usize].to_string()
}
fn llm_config() -> LlmConfig {
    LlmConfig {
        fallback_on_timeout: true,
        timeout_ms: 15000,
        max_tokens: 256,
        system_prompt: String::new(),
        fallback_action: Some(Action::verb("maintain_course")),
    }
}

// -- Pure row builders --
pub fn build_rules_rows(draft: &Contract) -> Vec<SelectableRow> {
    let mut rows = Vec::new();
    for (i, rule) in draft.rules.iter().enumerate() {
        let verbs: Vec<String> = ACTION_VERBS.iter().map(|v| v.to_string()).collect();
        let action_idx = ACTION_VERBS
            .iter()
            .position(|v| *v == rule.action.kind)
            .unwrap_or(0);
        rows.push(SelectableRow::Choice {
            label: format!("[{i}] cond"),
            choices: vec![condition_summary(&rule.condition)],
            selected: 0,
        });
        rows.push(SelectableRow::Choice {
            label: format!("[{i}] act"),
            choices: verbs,
            selected: action_idx,
        });
        rows.push(SelectableRow::Slider {
            label: format!("[{i}] pri"),
            value: rule.priority as f32,
            min: 0.0,
            max: 255.0,
        });
    }
    rows.push(SelectableRow::Action {
        label: "[+] add rule  (Delete)".into(),
    });
    rows
}
pub fn build_llm_rows(draft: &Contract) -> Vec<SelectableRow> {
    let llm = draft.llm_authority.as_ref();
    let verbs: Vec<String> = ACTION_VERBS.iter().map(|v| v.to_string()).collect();
    let fb_idx = llm
        .and_then(|l| l.fallback_action.as_ref())
        .and_then(|a| ACTION_VERBS.iter().position(|v| *v == a.kind))
        .unwrap_or(0);
    vec![
        SelectableRow::Toggle {
            label: "fallback on timeout".into(),
            value: llm.map(|l| l.fallback_on_timeout).unwrap_or(true),
        },
        SelectableRow::Slider {
            label: "timeout (ms)".into(),
            value: llm.map(|l| l.timeout_ms as f32).unwrap_or(15000.0),
            min: 1000.0,
            max: 120000.0,
        },
        SelectableRow::Slider {
            label: "max tokens".into(),
            value: llm.map(|l| l.max_tokens as f32).unwrap_or(256.0),
            min: 32.0,
            max: 4096.0,
        },
        SelectableRow::Choice {
            label: "fallback action".into(),
            choices: verbs,
            selected: fb_idx,
        },
        SelectableRow::Action {
            label: "edit system prompt  (Enter)".into(),
        },
    ]
}
pub fn build_persona_rows(
    roster: &CrewRoster,
    souls: &SoulRegistry,
    draft: &Contract,
) -> Vec<SelectableRow> {
    let mut rows: Vec<SelectableRow> = roster
        .members
        .iter()
        .map(|m| {
            let q = souls
                .files
                .get(&m.id)
                .map(|f| {
                    let qq: Vec<&str> = f.personality.quirks.iter().map(|s| s.as_str()).collect();
                    if qq.is_empty() {
                        "no quirks".into()
                    } else {
                        qq.join(", ")
                    }
                })
                .unwrap_or_else(|| "no soul file".into());
            SelectableRow::Action {
                label: format!("{}  {:12}  quirks: {}", m.name, m.role.name, q),
            }
        })
        .collect();
    let p = draft
        .llm_authority
        .as_ref()
        .map(|l| l.system_prompt.as_str())
        .unwrap_or("(no LLM config)");
    rows.push(SelectableRow::Action {
        label: format!("current persona: {p}"),
    });
    rows
}
pub fn build_simulation_rows(
    sim: &[(&'static str, String)],
    metrics: &WorkshopMetrics,
) -> Vec<SelectableRow> {
    if sim.is_empty() {
        return vec![SelectableRow::Action {
            label: "(not yet run — press Enter)".into(),
        }];
    }
    let mut rows: Vec<SelectableRow> = sim
        .iter()
        .map(|(n, s)| SelectableRow::Action {
            label: format!("  {n:12}  {s}"),
        })
        .collect();
    let rc = sim.iter().filter(|(_, s)| s.starts_with("rule")).count();
    let dc = sim.iter().filter(|(_, s)| s.starts_with("→ LLM")).count();
    let t = sim.len().max(1);
    rows.push(SelectableRow::Action {
        label: format!(
            "  ── {rc}/{} rules fired, {dc}/{} LLM calls ──",
            t - 1,
            t - 1
        ),
    });
    let b = seasoned_bonus(metrics.simulation_runs, metrics.simulation_runs * 2);
    rows.push(SelectableRow::Action {
        label: format!(
            "  seasoned bonus: trust +{}  depth: {}",
            b.trust_bonus, b.personality_depth
        ),
    });
    rows
}

// -- Workshop system --
#[allow(clippy::too_many_arguments)]
pub fn workshop_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    panel: Res<ActivePanel>,
    focus_stack: Res<crate::focus_stack::FocusStack>,
    mut state: ResMut<ContractWorkshopState>,
    roster: Res<CrewRoster>,
    souls: Res<SoulRegistry>,
    mut runtime: ResMut<crate::systems::contract::ContractRuntime>,
    mut log: ResMut<crate::systems::contract::ShipLog>,
    mut sel_panel: Query<
        &mut crate::widget_kit::panel::SelectablePanel,
        With<ContractWorkshopPanel>,
    >,
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
    if focus_stack.top_captures_input() {
        return;
    }
    if state.draft.is_none() {
        let member = roster.members.first().cloned().unwrap_or(CrewMember {
            id: "custom".into(),
            name: "Custom".into(),
            role: crate::systems::crew::CrewRole::new("engineer", "Engineer", ""),
            duty_room: reachlock_core::generator::RoomKind::Reactor,
            current_room: reachlock_core::generator::RoomKind::Reactor,
            deck: 0,
            order: None,
            offscreen_eta: 0.0,
            soul: None,
            salary: 0,
            unpaid_ticks: 0,
            health: crate::systems::crew::CrewHealth::Healthy,
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
    if keys.just_pressed(settings.key(InputAction::InstallContract)) {
        match state.draft.clone() {
            Some(d) => {
                let w = validate_contract(&d);
                let id = d.id.clone();
                let r = d.rules.len();
                runtime.install(d);
                state.dirty = false;
                state.status = if w.is_empty() {
                    format!("installed \"{id}\" ({r} rule(s)) — now live")
                } else {
                    format!(
                        "installed \"{id}\" ({r} rule(s)) — {} advisory warning(s)",
                        w.len()
                    )
                };
                log.log(format!("contract installed: {id}"));
            }
            None => state.status = "nothing to install".into(),
        }
    }
    if keys.just_pressed(settings.key(InputAction::EditorTabNext)) {
        let i = TABS.iter().position(|t| *t == state.tab).unwrap_or(0);
        state.tab = TABS[(i + 1) % TABS.len()];
        state.sel = 0;
        state.col = RuleCol::Condition;
        state.importing = false;
        state.status.clear();
        if let Ok(mut sp) = sel_panel.single_mut() {
            sp.selected_row = 0;
            sp.active_tab = TABS.iter().position(|t| *t == state.tab).unwrap_or(0);
        }
    }
    if let Ok(mut sp) = sel_panel.single_mut() {
        navigate_selectable_panel(&keys, &settings, &mut sp);
        state.sel = sp.selected_row;
    }
    let step = if keys.just_pressed(settings.key(InputAction::EditorCursorRight)) {
        1
    } else if keys.just_pressed(settings.key(InputAction::EditorCursorLeft)) {
        -1
    } else {
        0
    };
    match state.tab {
        WorkshopTab::Rules => handle_rules_tab(&keys, &settings, &mut state, step),
        WorkshopTab::LlmConfig => handle_llm_tab(&keys, &mut state, step),
        WorkshopTab::Persona => handle_persona_tab(&mut state, &roster, &souls, step),
        WorkshopTab::Simulation => handle_sim_tab(&keys, &mut state),
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
    let flat = state.sel;
    if step != 0 {
        let draft = state.draft.as_mut().unwrap();
        let total = draft.rules.len() * 3;
        if flat < total {
            let ri = flat / 3;
            let col = RULE_COLS[flat % 3];
            let rule = &mut draft.rules[ri];
            match col {
                RuleCol::Condition => {
                    if let Condition::Compare { op, .. } = &mut rule.condition {
                        *op = cycle_op(*op, step.signum());
                        state.status = "condition: cycled operator".into();
                    }
                }
                RuleCol::Action => {
                    rule.action.kind = cycle_verb(&rule.action.kind, step.signum());
                    state.status = format!("action: {}", rule.action.kind);
                }
                RuleCol::Priority => {
                    rule.priority = rule.priority.wrapping_add_signed(step.signum() as i8);
                    state.status = format!("priority: {}", rule.priority);
                }
            }
        }
        return;
    }
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        if state.importing {
            let t = state.import_buffer.trim();
            if let Ok(imported) = ron::from_str::<Contract>(t) {
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
        if flat == draft.rules.len() * 3 {
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
    if keys.just_pressed(settings.key(InputAction::EditorDelete)) {
        let draft = state.draft.as_mut().unwrap();
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
    if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        let w = validate_contract(state.draft.as_ref().unwrap());
        state.status = if w.is_empty() {
            "no craft warnings".into()
        } else {
            format!(
                "warnings: {}",
                w.iter()
                    .map(|w| format!("{w:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
    }
}

fn handle_llm_tab(keys: &ButtonInput<KeyCode>, state: &mut ContractWorkshopState, step: i64) {
    if step != 0 {
        let draft = state.draft.as_mut().unwrap();
        let llm = draft.llm_authority.get_or_insert_with(llm_config);
        match state.sel {
            0 => llm.fallback_on_timeout = !llm.fallback_on_timeout,
            1 => llm.timeout_ms = (llm.timeout_ms as i64 + step * 1000).clamp(1000, 120000) as u32,
            2 => llm.max_tokens = (llm.max_tokens as i64 + step * 32).clamp(32, 4096) as u32,
            3 => {
                let c = llm
                    .fallback_action
                    .as_ref()
                    .map(|a| a.kind.as_str())
                    .unwrap_or("maintain_course");
                llm.fallback_action = Some(Action::verb(cycle_verb(c, step.signum())));
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
    if step != 0 {
        let draft = state.draft.as_mut().unwrap();
        if let Some(member) = roster.members.get(state.sel) {
            let traits = souls
                .files
                .get(&member.id)
                .map(|f| {
                    f.personality
                        .traits
                        .iter()
                        .map(|t| t.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            let persona = format!(
                "You are {}. Role: {}. Traits: {}.",
                member.name, member.role.name, traits
            );
            if let Some(llm) = &mut draft.llm_authority {
                llm.system_prompt = persona;
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
            results.push((
                sc.name,
                match &outcome {
                    Outcome::Rule { action, .. } => format!("rule → {}", action.kind),
                    Outcome::Deliberate { .. } => "→ LLM deliberation".into(),
                    Outcome::NoDecision => "→ no decision".into(),
                },
            ));
        }
        let w = validate_contract(draft);
        let ws = if w.is_empty() {
            "no warnings".into()
        } else {
            w.iter()
                .map(|w| format!("{w:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        results.push(("Validation", ws));
        state.sim_results = results;
        state.metrics.simulation_runs += 1;
        state.status = "simulation complete".into();
    }
}

// -- Panel rendering --
pub fn render_workshop_panel(
    panel: Res<ActivePanel>,
    state: Res<ContractWorkshopState>,
    roster: Res<CrewRoster>,
    souls: Res<SoulRegistry>,
    settings: Res<Settings>,
    mut q: Query<
        (
            &mut crate::widget_kit::panel::SelectablePanel,
            &mut Text,
            &mut Visibility,
        ),
        With<ContractWorkshopPanel>,
    >,
) {
    let open = *panel == ActivePanel::ContractWorkshop && state.draft.is_some();
    if let Ok((mut sel, mut text, mut vis)) = q.single_mut() {
        if !open {
            **text = String::new();
            *vis = Visibility::Hidden;
            return;
        }
        *vis = Visibility::Visible;
        let draft = state.draft.as_ref().unwrap();
        if state.importing {
            sel.rows.clear();
            **text = format!("── CONTRACT WORKSHOP ──\n── IMPORT ──\nPaste RON contract below, then Enter to confirm:\n> {}\n(Esc cancel)", state.import_buffer);
            return;
        }
        sel.title = "CONTRACT WORKSHOP".into();
        sel.subtitle = format!(
            "Tab · W/S row · A/D change · Enter act · {} INSTALL",
            settings.key_display(InputAction::InstallContract)
        );
        sel.tabs = TABS.iter().map(|t| tab_name(*t).to_string()).collect();
        sel.active_tab = TABS.iter().position(|t| *t == state.tab).unwrap_or(0);
        sel.selected_row = state.sel;
        sel.status = state.status.clone();
        sel.rows = match state.tab {
            WorkshopTab::Rules => build_rules_rows(draft),
            WorkshopTab::LlmConfig => build_llm_rows(draft),
            WorkshopTab::Persona => build_persona_rows(&roster, &souls, draft),
            WorkshopTab::Simulation => build_simulation_rows(&state.sim_results, &state.metrics),
        };
        **text = crate::widget_kit::panel::format_selectable_panel_text(&sel);
    }
}

pub fn spawn_workshop_panel(mut commands: Commands) {
    use crate::widget_kit::panel::SelectablePanel;
    commands.spawn((
        ContractWorkshopPanel,
        SelectablePanel {
            title: String::new(),
            subtitle: String::new(),
            tabs: vec![],
            active_tab: 0,
            rows: vec![],
            selected_row: 0,
            status: String::new(),
        },
        Text::new(""),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        theme::fg("text"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(100.0),
            left: Val::Px(300.0),
            max_width: Val::Px(500.0),
            ..default()
        },
    ));
}
