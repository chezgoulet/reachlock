//! Contract editor (handoff §11): player/author automation contracts —
//! trigger, prioritized rules with condition trees and actions, and the
//! optional LLM fallback authority. Edits `Contract` under
//! `mods/reachlock/contracts/`.

use reachlock_core::contract::types::{
    Action, Comparison, Condition, Contract, LlmConfig, Rule, Trigger,
};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};
use super::widgets::{action_ui, comparison_symbol, condition_node_ui, COMPARISONS};

struct Entry {
    contract: Contract,
    path: Option<std::path::PathBuf>,
}

pub struct ContractEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_contract() -> Contract {
    Contract {
        id: "new_contract".into(),
        label: "New Contract".into(),
        trigger: Trigger::Manual,
        rules: vec![Rule {
            condition: Condition::Always,
            action: Action::verb("maintain_course"),
            priority: 0,
        }],
        llm_authority: None,
    }
}

/// Discriminant mirror for the trigger ComboBox.
#[derive(Clone, Copy, PartialEq)]
enum TriggerKind {
    Timer,
    Event,
    StateChange,
    Manual,
}

impl TriggerKind {
    fn of(t: &Trigger) -> Self {
        match t {
            Trigger::Timer { .. } => TriggerKind::Timer,
            Trigger::Event { .. } => TriggerKind::Event,
            Trigger::StateChange { .. } => TriggerKind::StateChange,
            Trigger::Manual => TriggerKind::Manual,
        }
    }

    fn name(self) -> &'static str {
        match self {
            TriggerKind::Timer => "Timer",
            TriggerKind::Event => "Event",
            TriggerKind::StateChange => "StateChange",
            TriggerKind::Manual => "Manual",
        }
    }

    fn default_trigger(self) -> Trigger {
        match self {
            TriggerKind::Timer => Trigger::Timer {
                interval_secs: 60,
                repeat: true,
            },
            TriggerKind::Event => Trigger::Event {
                event_type: String::new(),
            },
            TriggerKind::StateChange => Trigger::StateChange {
                field: String::new(),
                op: Comparison::Lt,
                value: 0,
            },
            TriggerKind::Manual => Trigger::Manual,
        }
    }
}

impl ContractEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir("mods/reachlock/contracts") {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(contract) = crate::io::read_ron::<Contract>(&path) {
                    entries.push(Entry {
                        contract,
                        path: Some(path),
                    });
                }
            }
        }
        if entries.is_empty() {
            entries.push(Entry {
                contract: blank_contract(),
                path: None,
            });
        }
        ContractEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

impl Editor for ContractEditor {
    fn title(&self) -> &str {
        "Contract Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Contract
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let contract: Contract = crate::io::read_ron(path)?;
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.path.as_deref() == Some(path))
        {
            self.entries[i].contract = contract;
            self.selected = i;
        } else {
            self.entries.push(Entry {
                contract,
                path: Some(path.to_path_buf()),
            });
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let entry = self
            .entries
            .get(self.selected)
            .ok_or_else(|| "no contract selected".to_string())?;
        crate::io::write_ron(path, &entry.contract)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(entry) = self.entries.get(self.selected) else {
            return errors;
        };
        let c = &entry.contract;
        if c.id.is_empty() {
            errors.push("id must not be empty".into());
        }
        if c.label.is_empty() {
            errors.push("label must not be empty".into());
        }
        if c.rules.is_empty() {
            errors.push("at least one rule is required".into());
        }
        for (i, rule) in c.rules.iter().enumerate() {
            if rule.action.kind.is_empty() {
                errors.push(format!("rule {i}: action kind must not be empty"));
            }
        }
        if let Trigger::Event { event_type } = &c.trigger {
            if event_type.is_empty() {
                errors.push("event trigger: event_type must not be empty".into());
            }
        }
        if let Some(llm) = &c.llm_authority {
            if llm.timeout_ms < 100 {
                errors.push("llm timeout_ms must be at least 100".into());
            }
            if llm.max_tokens == 0 {
                errors.push("llm max_tokens must be at least 1".into());
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("contract_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self.selected as u64 + 42;
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.entries.push(Entry {
                        contract: blank_contract(),
                        path: None,
                    });
                    self.selected = self.entries.len() - 1;
                    self.has_changes = true;
                }
                if let Some(entry) = self.entries.get(self.selected) {
                    let name = entry
                        .path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "(unsaved)".into());
                    ui.label(name);
                    if self.has_changes {
                        ui.label("*");
                    }
                }
            });
        });

        egui::SidePanel::left("contract_list")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.search);
                });
                ui.separator();
                let needle = self.search.to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for i in 0..self.entries.len() {
                        let label = self.entries[i].contract.label.clone();
                        if !needle.is_empty() && !label.to_lowercase().contains(&needle) {
                            continue;
                        }
                        if ui.selectable_label(self.selected == i, &label).clicked() {
                            self.selected = i;
                        }
                    }
                });
            });

        let validation = self.validate();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(entry) = self.entries.get_mut(self.selected) else {
                ui.label("No contract selected.");
                return;
            };
            let c = &mut entry.contract;
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("contract_identity").show(ui, |ui| {
                    ui.label("ID:");
                    changed |= ui.text_edit_singleline(&mut c.id).changed();
                    ui.end_row();
                    ui.label("Label:");
                    changed |= ui.text_edit_singleline(&mut c.label).changed();
                    ui.end_row();
                });

                egui::CollapsingHeader::new("Trigger — when the contract evaluates")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut kind = TriggerKind::of(&c.trigger);
                        egui::ComboBox::from_id_salt("contract_trigger_kind")
                            .selected_text(kind.name())
                            .show_ui(ui, |ui| {
                                for k in [
                                    TriggerKind::Timer,
                                    TriggerKind::Event,
                                    TriggerKind::StateChange,
                                    TriggerKind::Manual,
                                ] {
                                    if ui.selectable_value(&mut kind, k, k.name()).changed() {
                                        c.trigger = k.default_trigger();
                                        changed = true;
                                    }
                                }
                            });
                        match &mut c.trigger {
                            Trigger::Timer {
                                interval_secs,
                                repeat,
                            } => {
                                ui.horizontal(|ui| {
                                    ui.label("Interval (secs):");
                                    changed |= ui
                                        .add(egui::DragValue::new(interval_secs).range(1..=86_400))
                                        .changed();
                                    changed |= ui.checkbox(repeat, "repeat").changed();
                                });
                            }
                            Trigger::Event { event_type } => {
                                ui.horizontal(|ui| {
                                    ui.label("Event type:");
                                    changed |= ui.text_edit_singleline(event_type).changed();
                                });
                            }
                            Trigger::StateChange { field, op, value } => {
                                ui.horizontal(|ui| {
                                    ui.label("Field:");
                                    changed |= ui.text_edit_singleline(field).changed();
                                    egui::ComboBox::from_id_salt("contract_trigger_op")
                                        .selected_text(comparison_symbol(*op))
                                        .show_ui(ui, |ui| {
                                            for o in COMPARISONS {
                                                changed |= ui
                                                    .selectable_value(op, o, comparison_symbol(o))
                                                    .changed();
                                            }
                                        });
                                    ui.label("Value:");
                                    changed |= ui.add(egui::DragValue::new(value)).changed();
                                });
                            }
                            Trigger::Manual => {
                                ui.label("(fires when triggered explicitly)");
                            }
                        }
                    });

                egui::CollapsingHeader::new("Rules — prioritized condition → action")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut remove_rule: Option<usize> = None;
                        for (i, rule) in c.rules.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(format!("Rule {i} — priority:"));
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut rule.priority).range(0..=255),
                                        )
                                        .changed();
                                    if ui.button("Remove Rule").clicked() {
                                        remove_rule = Some(i);
                                    }
                                });
                                ui.label("Condition:");
                                let (cc, _) = condition_node_ui(
                                    ui,
                                    &mut rule.condition,
                                    egui::Id::new(("contract_rule_cond", i)),
                                    0,
                                    false,
                                );
                                changed |= cc;
                                changed |= action_ui(
                                    ui,
                                    &mut rule.action,
                                    egui::Id::new(("contract_rule_action", i)),
                                );
                            });
                        }
                        if let Some(i) = remove_rule {
                            c.rules.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Rule").clicked() {
                            c.rules.push(Rule {
                                condition: Condition::Always,
                                action: Action::verb("maintain_course"),
                                priority: 0,
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("LLM authority — optional fallback brain")
                    .default_open(true)
                    .show(ui, |ui| {
                        let mut enabled = c.llm_authority.is_some();
                        if ui.checkbox(&mut enabled, "Enable LLM authority").changed() {
                            c.llm_authority = enabled.then(|| LlmConfig {
                                fallback_on_timeout: true,
                                timeout_ms: 5000,
                                max_tokens: 256,
                                system_prompt: String::new(),
                                fallback_action: None,
                            });
                            changed = true;
                        }
                        if let Some(llm) = &mut c.llm_authority {
                            egui::Grid::new("contract_llm").show(ui, |ui| {
                                ui.label("Fallback on timeout:");
                                changed |= ui.checkbox(&mut llm.fallback_on_timeout, "").changed();
                                ui.end_row();
                                ui.label("Timeout (ms):");
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut llm.timeout_ms)
                                            .range(100..=120_000),
                                    )
                                    .changed();
                                ui.end_row();
                                ui.label("Max tokens:");
                                changed |= ui
                                    .add(egui::DragValue::new(&mut llm.max_tokens).range(1..=8192))
                                    .changed();
                                ui.end_row();
                            });
                            ui.label("System prompt:");
                            changed |= ui
                                .add(
                                    egui::TextEdit::multiline(&mut llm.system_prompt)
                                        .desired_rows(3)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed();
                            let mut has_fallback = llm.fallback_action.is_some();
                            if ui.checkbox(&mut has_fallback, "Fallback action").changed() {
                                llm.fallback_action =
                                    has_fallback.then(|| Action::verb("maintain_course"));
                                changed = true;
                            }
                            if let Some(action) = &mut llm.fallback_action {
                                changed |=
                                    action_ui(ui, action, egui::Id::new("contract_llm_fallback"));
                            }
                        }
                    });

                if !validation.is_empty() {
                    ui.separator();
                    for err in &validation {
                        ui.colored_label(egui::Color32::RED, err);
                    }
                }
            });
            if changed {
                self.has_changes = true;
            }
        });
    }

    fn generate_from_seed(&mut self, seed: u64) {
        let mut rng = SeededRng::new(seed ^ 0xC0B7_B00B);
        let trigger = if rng.next_below(2) == 0 {
            Trigger::Timer {
                interval_secs: 30 + rng.next_below(270) as u32,
                repeat: true,
            }
        } else {
            Trigger::Manual
        };
        let fields = ["hull_hp", "fuel", "shield_hp", "crew_awake", "cargo_free"];
        let verbs = ["wake_crew", "maintain_course", "dock_nearest", "vent_cargo"];
        let rules = (0..1 + rng.next_below(2))
            .map(|i| Rule {
                condition: Condition::Compare {
                    field: fields[rng.next_below(fields.len() as u64) as usize].into(),
                    op: COMPARISONS[rng.next_below(6) as usize],
                    value: (rng.next_below(100) as i64) * 1024,
                },
                action: Action::verb(verbs[rng.next_below(verbs.len() as u64) as usize]),
                priority: (i * 10) as u8,
            })
            .collect();
        let contract = Contract {
            id: format!("contract_{seed:x}"),
            label: format!("Contract {seed:04}"),
            trigger,
            rules,
            llm_authority: None,
        };
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.contract = contract;
        }
        self.has_changes = true;
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        // The contract schema describes a ContentFile envelope; some models
        // emit the envelope, others the bare Contract. Accept both.
        let contract = match serde_json::from_value::<Contract>(value.clone()) {
            Ok(c) => c,
            Err(_) => {
                let extracted = super::super::ai::extract_inner_from_envelope(value, "contract")
                    .ok_or_else(|| {
                        "response was neither a bare contract nor a ContentFile envelope"
                            .to_string()
                    })?;
                serde_json::from_value::<Contract>(extracted)
                    .map_err(|e| format!("contract: {e}"))?
            }
        };
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.contract = contract;
        } else {
            self.entries.push(Entry {
                contract,
                path: None,
            });
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = true;
        Ok(())
    }

    fn snapshot(&self) -> Option<String> {
        let state: Vec<(&Contract, &Option<std::path::PathBuf>)> = self
            .entries
            .iter()
            .map(|e| (&e.contract, &e.path))
            .collect();
        ron::to_string(&(state, self.selected)).ok()
    }

    fn restore_snapshot(&mut self, ron: &str) -> Result<(), String> {
        let (state, selected): (Vec<(Contract, Option<std::path::PathBuf>)>, usize) =
            ron::from_str(ron).map_err(|e| e.to_string())?;
        self.entries = state
            .into_iter()
            .map(|(contract, path)| Entry { contract, path })
            .collect();
        self.selected = selected.min(self.entries.len().saturating_sub(1));
        self.has_changes = true;
        Ok(())
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }

    fn selected_entry_name(&self) -> Option<String> {
        if self.entries.len() <= 1 {
            return None;
        }
        self.entries
            .get(self.selected)
            .map(|e| e.contract.label.clone())
    }

    fn delete_selected(&mut self) -> bool {
        if self.entries.len() <= 1 || self.selected >= self.entries.len() {
            return false;
        }
        self.entries.remove(self.selected);
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = true;
        true
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        let c = &entry.contract;
        ui.strong(&c.label);
        let trigger = match &c.trigger {
            Trigger::Timer {
                interval_secs,
                repeat,
            } => format!(
                "Timer: every {interval_secs}s{}",
                if *repeat { " (repeating)" } else { "" }
            ),
            Trigger::Event { event_type } => format!("Event: {event_type}"),
            Trigger::StateChange { field, .. } => format!("StateChange: {field}"),
            Trigger::Manual => "Manual trigger".into(),
        };
        ui.label(trigger);
        ui.label(format!("{} rule(s)", c.rules.len()));
        for rule in c.rules.iter().take(4) {
            ui.weak(format!("→ {}", rule.action.kind));
        }
        if c.llm_authority.is_some() {
            ui.weak("LLM authority enabled");
        }
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(ContractEditor::new())
}
