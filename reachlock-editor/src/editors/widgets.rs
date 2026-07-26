//! Shared editor widgets (handoff §11): the Condition tree used by the
//! Contract, Storyline, and Soul editors, plus the Action row editor,
//! and the character appearance editor (S76).

use reachlock_core::contract::types::{Action, Comparison, Condition};
use reachlock_core::generator::sprite::{CharacterLookConfig, HAIR_STYLE_COUNT};
use reachlock_core::soul::types::Species;

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
                        changed |= ui.selectable_value(op, o, comparison_symbol(o)).changed();
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

// ── S76: Character appearance editor ──

const SPECIES_LIST: [Species; 5] = [
    Species::Human,
    Species::Android,
    Species::Robot,
    Species::Voidborn,
    Species::Xenotype,
];

pub(crate) const HAIR_STYLES: [&str; HAIR_STYLE_COUNT as usize] =
    ["Bald", "Short", "Buzz", "Long", "Locs", "Bun", "Crest"];

/// A reusable widget for editing a `CharacterLookConfig`. Used by the editor
/// previewer and by the in-game character creator (S78).
///
/// Takes a `&mut CharacterLookConfig` and `&mut u64` (seed for "Reroll").
/// Returns `true` if any value changed.
pub fn character_appearance_editor(
    ui: &mut egui::Ui,
    config: &mut CharacterLookConfig,
    seed: &mut u64,
) -> bool {
    let mut changed = false;
    let is_robot = config.species == Species::Robot;

    ui.heading("Character Look");
    ui.separator();

    // Species dropdown
    let species_idx = SPECIES_LIST
        .iter()
        .position(|s| *s == config.species)
        .unwrap_or(0);
    let mut sel = species_idx;
    egui::ComboBox::from_label("Species")
        .selected_text(SPECIES_LIST[sel].to_string())
        .show_ui(ui, |ui| {
            for (i, sp) in SPECIES_LIST.iter().enumerate() {
                if ui.selectable_value(&mut sel, i, sp.to_string()).changed() {
                    config.species = *sp;
                    changed = true;
                }
            }
        });

    ui.separator();

    // Hair style selector
    {
        let idx = config.hair_style.unwrap_or(0) as usize % HAIR_STYLES.len();
        ui.horizontal(|ui| {
            ui.label("Hair:");
            if ui.button("◀").clicked() {
                let cur = config.hair_style.unwrap_or(0) as i32;
                let next = if cur <= 0 {
                    HAIR_STYLE_COUNT as i32 - 1
                } else {
                    cur - 1
                };
                config.hair_style = Some(next as u8);
                changed = true;
            }
            ui.label(HAIR_STYLES[idx]);
            if ui.button("▶").clicked() {
                let cur = config.hair_style.unwrap_or(0) as u32;
                let next = (cur + 1) % HAIR_STYLE_COUNT as u32;
                config.hair_style = Some(next as u8);
                changed = true;
            }
        });
    }

    if is_robot {
        ui.small("Robot: chassis + visor replace hair/skin tones.");
        ui.separator();
        changed |= color_control(ui, "Chassis", &mut config.chassis_color);
        changed |= color_control(ui, "Visor", &mut config.visor_color);
    } else {
        changed |= color_control(ui, "Hair Color", &mut config.hair_color);
        changed |= color_control(ui, "Skin Color", &mut config.skin_color);
    }

    ui.separator();
    changed |= color_control(ui, "Shirt", &mut config.shirt_color);
    changed |= color_control(ui, "Pants", &mut config.pants_color);

    ui.separator();
    {
        let enabled = config.jacket_enabled.unwrap_or(false);
        let mut new_enabled = enabled;
        if ui.checkbox(&mut new_enabled, "Jacket").changed() {
            config.jacket_enabled = Some(new_enabled);
            changed = true;
        }
        if new_enabled {
            changed |= color_control(ui, "Jacket Color", &mut config.jacket_color);
        }
    }

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Seed:");
        if ui
            .add(egui::DragValue::new(seed).range(0..=((1u64 << 53) - 1)))
            .changed()
        {
            changed = true;
        }
    });
    if ui.button("Reroll").clicked() {
        *seed = seed.wrapping_add(1);
        config.hair_style = None;
        config.hair_color = None;
        config.skin_color = None;
        config.shirt_color = None;
        config.pants_color = None;
        config.jacket_enabled = None;
        config.jacket_color = None;
        config.chassis_color = None;
        config.visor_color = None;
        changed = true;
    }

    changed
}

fn color_control(ui: &mut egui::Ui, label: &str, color: &mut Option<[u8; 3]>) -> bool {
    let mut changed = false;
    let mut auto = color.is_none();
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.checkbox(&mut auto, "(auto)").changed() {
            if auto {
                *color = None;
            } else {
                *color = Some([128, 128, 128]);
            }
            changed = true;
        }
        if !auto {
            let mut c = color.unwrap_or([128, 128, 128]);
            let mut srgba = [c[0], c[1], c[2], 255];
            if ui
                .color_edit_button_srgba_unmultiplied(&mut srgba)
                .changed()
            {
                *color = Some([srgba[0], srgba[1], srgba[2]]);
                changed = true;
            }
            ui.add(
                egui::DragValue::new(&mut c[0])
                    .range(0..=255)
                    .prefix("R")
                    .speed(1),
            );
            ui.add(
                egui::DragValue::new(&mut c[1])
                    .range(0..=255)
                    .prefix("G")
                    .speed(1),
            );
            ui.add(
                egui::DragValue::new(&mut c[2])
                    .range(0..=255)
                    .prefix("B")
                    .speed(1),
            );
            if c != color.unwrap_or([128, 128, 128]) {
                *color = Some(c);
                changed = true;
            }
        }
    });
    changed
}
