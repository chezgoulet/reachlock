//! Shared editor widgets (handoff §11): the Condition tree used by the
//! Contract, Storyline, and Soul editors, plus the Action row editor.

use reachlock_core::contract::types::{Action, Comparison, Condition};

pub const COMPARISONS: [Comparison; 6] = [
    Comparison::Lt,
    Comparison::Le,
    Comparison::Eq,
    Comparison::Ne,
    Comparison::Ge,
    Comparison::Gt,
];

pub fn comparison_symbol(op: Comparison) -> &'static str {
    match op {
        Comparison::Lt => "<",
        Comparison::Le => "<=",
        Comparison::Eq => "==",
        Comparison::Ne => "!=",
        Comparison::Ge => ">=",
        Comparison::Gt => ">",
    }
}

fn condition_variant_name(c: &Condition) -> &'static str {
    match c {
        Condition::Always => "Always",
        Condition::Compare { .. } => "Compare",
        Condition::Not(_) => "Not",
        Condition::All(_) => "All",
        Condition::Any(_) => "Any",
    }
}

fn default_condition(name: &str) -> Condition {
    match name {
        "Compare" => Condition::Compare {
            field: String::new(),
            op: Comparison::Ge,
            value: 0,
        },
        "Not" => Condition::Not(Box::new(Condition::Always)),
        "All" => Condition::All(vec![Condition::Always]),
        "Any" => Condition::Any(vec![Condition::Always]),
        _ => Condition::Always,
    }
}

/// Recursive condition node editor. Returns `(changed, remove_requested)` —
/// the parent owns removal because a node can't delete itself.
pub fn condition_node_ui(
    ui: &mut egui::Ui,
    cond: &mut Condition,
    id: egui::Id,
    depth: usize,
    removable: bool,
) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.add_space(20.0 * depth as f32);
        let current = condition_variant_name(cond);
        egui::ComboBox::from_id_salt(id.with("variant"))
            .selected_text(current)
            .width(90.0)
            .show_ui(ui, |ui| {
                for name in ["Always", "Compare", "All", "Any", "Not"] {
                    if ui.selectable_label(current == name, name).clicked() && current != name {
                        *cond = default_condition(name);
                        changed = true;
                    }
                }
            });
        if let Condition::Compare { field, op, value } = cond {
            ui.label("field:");
            changed |= ui
                .add(egui::TextEdit::singleline(field).desired_width(120.0))
                .changed();
            egui::ComboBox::from_id_salt(id.with("op"))
                .selected_text(comparison_symbol(*op))
                .width(50.0)
                .show_ui(ui, |ui| {
                    for o in COMPARISONS {
                        changed |= ui
                            .selectable_value(op, o, comparison_symbol(o))
                            .changed();
                    }
                });
            ui.label("value:");
            changed |= ui.add(egui::DragValue::new(value)).changed();
        }
        if removable && ui.button("×").clicked() {
            remove = true;
        }
    });

    match cond {
        Condition::Not(child) => {
            let (c, _) = condition_node_ui(ui, child, id.with("not"), depth + 1, false);
            changed |= c;
        }
        Condition::All(children) | Condition::Any(children) => {
            let mut remove_child: Option<usize> = None;
            for (i, child) in children.iter_mut().enumerate() {
                let (c, r) = condition_node_ui(ui, child, id.with(i), depth + 1, true);
                changed |= c;
                if r {
                    remove_child = Some(i);
                }
            }
            if let Some(i) = remove_child {
                children.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                ui.add_space(20.0 * (depth + 1) as f32);
                if ui.button("+ Add Child").clicked() {
                    children.push(Condition::Always);
                    changed = true;
                }
            });
        }
        _ => {}
    }
    (changed, remove)
}

/// Action editor: verb kind plus a key→i64 params table.
pub fn action_ui(ui: &mut egui::Ui, action: &mut Action, id: egui::Id) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Action kind:");
        changed |= ui
            .add(egui::TextEdit::singleline(&mut action.kind).desired_width(160.0))
            .changed();
    });
    let mut rename: Option<(String, String)> = None;
    let mut remove_key: Option<String> = None;
    for (key, value) in action.params.iter_mut() {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            let mut key_edit = key.clone();
            if ui
                .add(egui::TextEdit::singleline(&mut key_edit).desired_width(120.0))
                .changed()
            {
                rename = Some((key.clone(), key_edit));
            }
            changed |= ui.add(egui::DragValue::new(value)).changed();
            if ui.button("×").clicked() {
                remove_key = Some(key.clone());
            }
        });
    }
    if let Some((old, new)) = rename {
        if let Some(v) = action.params.remove(&old) {
            action.params.insert(new, v);
            changed = true;
        }
    }
    if let Some(key) = remove_key {
        action.params.remove(&key);
        changed = true;
    }
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        if ui.button("Add Param").clicked() {
            let mut n = 0;
            let key = loop {
                let candidate = if n == 0 {
                    "param".to_string()
                } else {
                    format!("param_{n}")
                };
                if !action.params.contains_key(&candidate) {
                    break candidate;
                }
                n += 1;
            };
            action.params.insert(key, 0);
            changed = true;
        }
    });
    let _ = id;
    changed
}
