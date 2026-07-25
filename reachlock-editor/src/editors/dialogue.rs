//! Dialogue tree editor (S55/S68).
//!
//! A branching conversation is a list of nodes plus a start node id. The
//! authoring risk is referential, not structural: `next_node` is a bare
//! string, so a typo produces a conversation that dead-ends at runtime with
//! nothing to tell you why. Every reference here is therefore a `ComboBox`
//! over the existing node ids, and `validate` reports dangling targets,
//! unreachable nodes, and dead ends.

use std::collections::HashSet;

use reachlock_core::content::dialogue::{Dialogue, DialogueChoice, DialogueNode, NodeType};

use super::super::app::{ContentType, Editor};

const NODE_TYPES: [NodeType; 5] = [
    NodeType::NarratorLine,
    NodeType::NpcLine,
    NodeType::PlayerChoice,
    NodeType::Branch,
    NodeType::End,
];

fn node_type_name(t: NodeType) -> &'static str {
    match t {
        NodeType::NarratorLine => "Narrator line",
        NodeType::NpcLine => "NPC line",
        NodeType::PlayerChoice => "Player choice",
        NodeType::Branch => "Branch",
        NodeType::End => "End",
    }
}

/// Colour-coded by role, always shown next to the type name so colour is
/// decoration rather than the only signal.
fn node_type_color(t: NodeType) -> egui::Color32 {
    match t {
        NodeType::NarratorLine => egui::Color32::from_rgb(0x9E, 0x9E, 0x9E),
        NodeType::NpcLine => egui::Color32::from_rgb(0x64, 0xB5, 0xF6),
        NodeType::PlayerChoice => egui::Color32::from_rgb(0x81, 0xC7, 0x84),
        NodeType::Branch => egui::Color32::from_rgb(0xFF, 0xB7, 0x4D),
        NodeType::End => egui::Color32::from_rgb(0xE5, 0x73, 0x73),
    }
}

pub struct DialogueEditor {
    path: Option<std::path::PathBuf>,
    dialogue: Dialogue,
    has_changes: bool,
    selected: usize,
}

/// A blank two-node conversation — something to edit, referentially valid.
fn blank_dialogue() -> Dialogue {
    Dialogue {
        nodes: vec![
            DialogueNode {
                id: "start".into(),
                node_type: NodeType::NpcLine,
                text: String::new(),
                choices: vec![DialogueChoice {
                    display_text: "…".into(),
                    condition: None,
                    consequence: None,
                    next_node: "end".into(),
                }],
                voice_clip: None,
            },
            DialogueNode {
                id: "end".into(),
                node_type: NodeType::End,
                text: String::new(),
                choices: vec![],
                voice_clip: None,
            },
        ],
        start_node: "start".into(),
    }
}

impl DialogueEditor {
    /// A genuinely new document.
    ///
    /// This used to scan the content directory and adopt the first `.ron` it
    /// found, so `File > New` silently bound to an existing conversation and
    /// the first save overwrote it. New means new.
    fn new() -> Self {
        DialogueEditor {
            path: None,
            dialogue: blank_dialogue(),
            has_changes: false,
            selected: 0,
        }
    }

    fn node_ids(&self) -> Vec<String> {
        self.dialogue.nodes.iter().map(|n| n.id.clone()).collect()
    }

    /// Node ids reachable from `start_node` by following choices.
    fn reachable(&self) -> HashSet<String> {
        let mut seen = HashSet::new();
        let mut stack = vec![self.dialogue.start_node.clone()];
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(node) = self.dialogue.nodes.iter().find(|n| n.id == id) {
                for c in &node.choices {
                    stack.push(c.next_node.clone());
                }
            }
        }
        seen
    }

    /// A fresh id that does not collide with an existing node.
    fn unique_node_id(&self, base: &str) -> String {
        let ids = self.node_ids();
        if !ids.iter().any(|i| i == base) {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}_{n}"))
            .find(|c| !ids.contains(c))
            .unwrap_or_default()
    }
}

impl Editor for DialogueEditor {
    fn title(&self) -> &str {
        &self.dialogue.start_node
    }

    fn content_type(&self) -> ContentType {
        ContentType::Dialogue
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn touch(&mut self) {
        self.has_changes = true;
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let d: Dialogue =
            crate::io::read_ron(path).map_err(|e| format!("reading dialogue: {e}"))?;
        self.dialogue = d;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        self.selected = 0;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.dialogue).map_err(|e| format!("saving dialogue: {e}"))
    }

    fn save_all(&mut self) -> Result<bool, String> {
        // Only write when there is something to write. The previous version
        // saved unconditionally, which contradicted the autosave path (that
        // one *did* check) and rewrote the file on every Ctrl+S.
        if !self.has_changes {
            return Ok(self.path.is_some());
        }
        let Some(path) = self.path.clone() else {
            // No path yet: let the shell run Save As rather than inventing
            // one. A hardcoded fallback name meant two new dialogues
            // overwrote each other.
            return Ok(false);
        };
        self.save(&path)?;
        self.has_changes = false;
        Ok(true)
    }

    fn generate_from_seed(&mut self, seed: u64) {
        // Seeding names the conversation; it does not rewrite authored nodes.
        let id = format!("node_{seed:#x}");
        if let Some(node) = self.dialogue.nodes.first_mut() {
            node.id = id.clone();
            self.dialogue.start_node = id;
            self.has_changes = true;
        }
    }

    /// Renaming ids on reroll would break every `next_node` pointing at them.
    fn accept_seed_reroll(&self) -> bool {
        false
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.dialogue.nodes.is_empty() {
            errors.push("no dialogue nodes".into());
            return errors;
        }
        let ids = self.node_ids();

        for (i, id) in ids.iter().enumerate() {
            if id.trim().is_empty() {
                errors.push(format!("node {i} has an empty id"));
            }
            if ids.iter().filter(|o| *o == id).count() > 1 {
                errors.push(format!("duplicate node id \"{id}\""));
            }
        }
        if !ids.contains(&self.dialogue.start_node) {
            errors.push(format!(
                "start node \"{}\" does not exist",
                self.dialogue.start_node
            ));
        }
        for node in &self.dialogue.nodes {
            for c in &node.choices {
                if !ids.contains(&c.next_node) {
                    errors.push(format!(
                        "node \"{}\" choice \"{}\" points at missing node \"{}\"",
                        node.id, c.display_text, c.next_node
                    ));
                }
            }
            if node.choices.is_empty() && node.node_type != NodeType::End {
                errors.push(format!(
                    "node \"{}\" is a dead end (no choices, not an End node)",
                    node.id
                ));
            }
        }
        let reachable = self.reachable();
        for node in &self.dialogue.nodes {
            if !reachable.contains(&node.id) {
                errors.push(format!(
                    "node \"{}\" is unreachable from the start node",
                    node.id
                ));
            }
        }
        errors
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let ids = self.node_ids();
        let reachable = self.reachable();
        let mut changed = false;
        let mut add_node = false;
        let mut duplicate_node: Option<usize> = None;
        let mut remove_node: Option<usize> = None;

        egui::TopBottomPanel::top("dialogue_toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Start node:");
                egui::ComboBox::from_id_salt("dialogue_start")
                    .selected_text(&self.dialogue.start_node)
                    .show_ui(ui, |ui| {
                        for id in &ids {
                            if ui
                                .selectable_label(self.dialogue.start_node == *id, id)
                                .clicked()
                                && self.dialogue.start_node != *id
                            {
                                self.dialogue.start_node = id.clone();
                                changed = true;
                            }
                        }
                    });
                ui.separator();
                ui.label(format!("{} node(s)", self.dialogue.nodes.len()));
                let unreachable = self
                    .dialogue
                    .nodes
                    .iter()
                    .filter(|n| !reachable.contains(&n.id))
                    .count();
                if unreachable > 0 {
                    ui.colored_label(
                        egui::Color32::from_rgb(0xFF, 0xB3, 0x00),
                        format!("⚠ {unreachable} unreachable"),
                    );
                }
            });
        });

        egui::SidePanel::left("dialogue_nodes")
            .resizable(true)
            .default_width(190.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("➕ Node").clicked() {
                        add_node = true;
                    }
                    if ui.button("⧉ Duplicate").clicked() {
                        duplicate_node = Some(self.selected);
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, node) in self.dialogue.nodes.iter().enumerate() {
                        let is_start = node.id == self.dialogue.start_node;
                        let orphan = !reachable.contains(&node.id);
                        ui.horizontal(|ui| {
                            ui.colored_label(node_type_color(node.node_type), "●")
                                .on_hover_text(node_type_name(node.node_type));
                            let label = format!(
                                "{}{}{}",
                                if is_start { "▶ " } else { "" },
                                node.id,
                                if orphan { "  ⚠" } else { "" }
                            );
                            if ui.selectable_label(self.selected == i, label).clicked() {
                                self.selected = i;
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let Some(node) = self.dialogue.nodes.get_mut(self.selected) else {
                ui.weak("No node selected.");
                return;
            };
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("dialogue_node_form")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Node id:");
                        changed |= ui.text_edit_singleline(&mut node.id).changed();
                        ui.end_row();

                        ui.label("Type:");
                        egui::ComboBox::from_id_salt("dialogue_node_type")
                            .selected_text(node_type_name(node.node_type))
                            .show_ui(ui, |ui| {
                                for t in NODE_TYPES {
                                    changed |= ui
                                        .selectable_value(&mut node.node_type, t, node_type_name(t))
                                        .changed();
                                }
                            });
                        ui.end_row();

                        ui.label("Voice clip:");
                        let mut clip = node.voice_clip.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut clip).changed() {
                            node.voice_clip = (!clip.trim().is_empty()).then_some(clip);
                            changed = true;
                        }
                        ui.end_row();
                    });

                ui.label("Text:");
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(&mut node.text)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();

                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong("Choices");
                    if ui.small_button("➕ Add").clicked() {
                        node.choices.push(DialogueChoice {
                            display_text: "New choice".into(),
                            condition: None,
                            consequence: None,
                            next_node: ids.first().cloned().unwrap_or_default(),
                        });
                        changed = true;
                    }
                });

                let mut remove_choice: Option<usize> = None;
                for (ci, choice) in node.choices.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Text:");
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut choice.display_text)
                                        .desired_width(220.0),
                                )
                                .changed();
                            if ui.button("×").clicked() {
                                remove_choice = Some(ci);
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("→ goes to:");
                            // A ComboBox, not a text field: a typo here is a
                            // conversation that dead-ends with no explanation.
                            let dangling = !ids.contains(&choice.next_node);
                            egui::ComboBox::from_id_salt(("dialogue_next", ci))
                                .selected_text(if dangling {
                                    format!("⚠ {}", choice.next_node)
                                } else {
                                    choice.next_node.clone()
                                })
                                .show_ui(ui, |ui| {
                                    for id in &ids {
                                        if ui
                                            .selectable_label(choice.next_node == *id, id)
                                            .clicked()
                                            && choice.next_node != *id
                                        {
                                            choice.next_node = id.clone();
                                            changed = true;
                                        }
                                    }
                                });
                            if dangling {
                                ui.colored_label(
                                    egui::Color32::from_rgb(0xE5, 0x73, 0x73),
                                    "missing node",
                                );
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Condition:");
                            let mut cond = choice.condition.clone().unwrap_or_default();
                            if ui
                                .add(egui::TextEdit::singleline(&mut cond).desired_width(150.0))
                                .changed()
                            {
                                choice.condition = (!cond.trim().is_empty()).then_some(cond);
                                changed = true;
                            }
                            ui.label("Consequence:");
                            let mut cons = choice.consequence.clone().unwrap_or_default();
                            if ui
                                .add(egui::TextEdit::singleline(&mut cons).desired_width(150.0))
                                .changed()
                            {
                                choice.consequence = (!cons.trim().is_empty()).then_some(cons);
                                changed = true;
                            }
                        });
                    });
                }
                if let Some(ci) = remove_choice {
                    node.choices.remove(ci);
                    changed = true;
                }

                ui.add_space(8.0);
                if ui.button("🗑 Delete this node").clicked() {
                    remove_node = Some(self.selected);
                }
            });
        });

        if add_node {
            let id = self.unique_node_id("node");
            self.dialogue.nodes.push(DialogueNode {
                id,
                node_type: NodeType::NpcLine,
                text: String::new(),
                choices: vec![],
                voice_clip: None,
            });
            self.selected = self.dialogue.nodes.len() - 1;
            changed = true;
        }
        if let Some(i) = duplicate_node {
            if let Some(src) = self.dialogue.nodes.get(i).cloned() {
                let id = self.unique_node_id(&src.id);
                self.dialogue.nodes.push(DialogueNode { id, ..src });
                self.selected = self.dialogue.nodes.len() - 1;
                changed = true;
            }
        }
        if let Some(i) = remove_node {
            if self.dialogue.nodes.len() > 1 {
                self.dialogue.nodes.remove(i);
                self.selected = self.selected.min(self.dialogue.nodes.len() - 1);
                changed = true;
            }
        }
        if changed {
            self.has_changes = true;
        }
    }

    fn snapshot(&self) -> Option<String> {
        ron::ser::to_string(&self.dialogue).ok()
    }

    fn restore_snapshot(&mut self, ron_text: &str) -> Result<(), String> {
        let d: Dialogue = ron::from_str(ron_text).map_err(|e| e.to_string())?;
        self.dialogue = d;
        self.selected = self
            .selected
            .min(self.dialogue.nodes.len().saturating_sub(1));
        self.has_changes = true;
        Ok(())
    }

    fn selected_entry_name(&self) -> Option<String> {
        self.dialogue.nodes.get(self.selected).map(|n| n.id.clone())
    }

    fn delete_selected(&mut self) -> bool {
        if self.dialogue.nodes.len() <= 1 {
            return false;
        }
        self.dialogue.nodes.remove(self.selected);
        self.selected = self.selected.min(self.dialogue.nodes.len() - 1);
        self.has_changes = true;
        true
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        ui.strong("Dialogue Tree");
        ui.label(format!("{} node(s)", self.dialogue.nodes.len()));
        ui.label(format!("start: {}", self.dialogue.start_node));
        let issues = self.validate();
        if issues.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(0x4C, 0xAF, 0x50), "✔ clean");
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(0xE5, 0x73, 0x73),
                format!("✘ {} issue(s)", issues.len()),
            );
            for issue in issues.iter().take(5) {
                ui.weak(issue);
            }
        }
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(DialogueEditor::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `File > New` must produce an empty, unbound document — not adopt an
    /// existing file from the content directory.
    #[test]
    fn new_is_unbound_and_clean() {
        let e = DialogueEditor::new();
        assert!(e.path.is_none(), "a new editor must not bind to a file");
        assert!(!e.has_unsaved_changes());
    }

    /// A clean editor must not write. The old `save_all` wrote every time.
    #[test]
    fn save_all_on_a_clean_editor_writes_nothing() {
        let mut e = DialogueEditor::new();
        assert_eq!(e.save_all(), Ok(false), "nothing to save, no path invented");
    }

    #[test]
    fn mutation_marks_dirty() {
        let mut e = DialogueEditor::new();
        e.touch();
        assert!(e.has_unsaved_changes());
    }

    #[test]
    fn validate_catches_dangling_and_unreachable_nodes() {
        let mut e = DialogueEditor::new();
        e.dialogue.nodes[0].choices[0].next_node = "nowhere".into();
        let issues = e.validate();
        assert!(
            issues
                .iter()
                .any(|i| i.contains("missing node \"nowhere\"")),
            "{issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.contains("unreachable")),
            "the end node is now orphaned: {issues:?}"
        );
    }

    #[test]
    fn validate_catches_a_bad_start_node() {
        let mut e = DialogueEditor::new();
        e.dialogue.start_node = "ghost".into();
        assert!(e.validate().iter().any(|i| i.contains("start node")));
    }

    #[test]
    fn unique_node_id_avoids_collisions() {
        let e = DialogueEditor::new();
        assert_eq!(e.unique_node_id("fresh"), "fresh");
        assert_eq!(e.unique_node_id("start"), "start_2");
    }

    #[test]
    fn snapshot_round_trips_for_undo() {
        let mut e = DialogueEditor::new();
        let snap = e.snapshot().expect("snapshot");
        e.dialogue.nodes.clear();
        e.restore_snapshot(&snap).expect("restore");
        assert_eq!(e.dialogue.nodes.len(), 2);
    }

    /// Rerolling would rename ids that `next_node` references point at.
    #[test]
    fn seed_reroll_is_declined() {
        assert!(!DialogueEditor::new().accept_seed_reroll());
    }
}
