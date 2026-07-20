//! Soul / NPC editor (handoff §13): who an NPC is — identity, personality,
//! emotional baseline with mood triggers, dialogue graph, secrets, breaking
//! points, memories, relationships, goals, and contract references. Edits
//! `SoulFile` under `mods/reachlock/souls/`.
//!
//! Species-specific attributes the handoff sketches (chassis model, void
//! adaptation, …) have no fields on the frozen `SoulFile` contract — they
//! surface here as authoring hints, with the data itself living in the open
//! `personality.quirks` vocabulary.

use reachlock_core::dialogue::{DialogueChoice, DialogueGraph, DialogueNode};
use reachlock_core::soul::types::{
    BreakReaction, BreakingPoint, EmotionalState, Goal, GoalPriority, Identity, Memory, Mood,
    Personality, Relationship, Secret, SoulFile, SpeakingStyle, Species, Trigger,
};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};
use super::widgets::condition_node_ui;

const SPECIES: [Species; 5] = [
    Species::Human,
    Species::Android,
    Species::Robot,
    Species::Voidborn,
    Species::Xenotype,
];

const STYLES: [SpeakingStyle; 9] = [
    SpeakingStyle::Terse,
    SpeakingStyle::Elaborate,
    SpeakingStyle::Technical,
    SpeakingStyle::Lyrical,
    SpeakingStyle::Sarcastic,
    SpeakingStyle::Blunt,
    SpeakingStyle::Formal,
    SpeakingStyle::Wry,
    SpeakingStyle::Warm,
];

const MOODS: [Mood; 11] = [
    Mood::Stable,
    Mood::Happy,
    Mood::Tense,
    Mood::Grieving,
    Mood::Suspicious,
    Mood::Grateful,
    Mood::Anxious,
    Mood::Protective,
    Mood::Defensive,
    Mood::Focused,
    Mood::Withdrawn,
];

const REACTIONS: [BreakReaction; 5] = [
    BreakReaction::LeaveShip,
    BreakReaction::RefuseOrders,
    BreakReaction::Confront,
    BreakReaction::Withdraw,
    BreakReaction::Betray,
];

fn species_name(s: Species) -> &'static str {
    match s {
        Species::Human => "Human",
        Species::Android => "Android",
        Species::Robot => "Robot",
        Species::Voidborn => "Voidborn",
        Species::Xenotype => "Xenotype",
    }
}

/// Left-panel dot color per species (handoff §13).
fn species_color(s: Species) -> egui::Color32 {
    match s {
        Species::Human => egui::Color32::from_rgb(0xFF, 0xC8, 0x9E),
        Species::Android => egui::Color32::from_rgb(0x64, 0x95, 0xED),
        Species::Robot => egui::Color32::from_rgb(0x9E, 0x9E, 0x9E),
        Species::Voidborn => egui::Color32::from_rgb(0x94, 0x60, 0xD8),
        Species::Xenotype => egui::Color32::from_rgb(0x50, 0xC8, 0x50),
    }
}

fn species_hint(s: Species) -> &'static str {
    match s {
        Species::Human => {
            "Human — includes cybernetically enhanced humans. Cybernetic grade \
             is edited in the BodyMod editor; record notable augments as quirks."
        }
        Species::Android => {
            "Android — synthetic humanoid. Record chassis model and firmware \
             version as quirks (e.g. \"chassis: Meridian-3\")."
        }
        Species::Robot => {
            "Robot — industrial/non-humanoid synthetic. Record unit class \
             (Industrial/Service/Security/Exploration) and intelligence tier \
             as quirks."
        }
        Species::Voidborn => {
            "Voidborn — space-dwelling, Predecessor lore. Record void \
             adaptation (bioluminescent, pressure-resistant, …) and origin \
             region (Deep Space, Nebula Birth, Predecessor Ruin) as quirks."
        }
        Species::Xenotype => {
            "Xenotype — planet-bound ecosystem creature. Record planet of \
             origin, ecosystem role (Predator/Prey/Scavenger/Symbiont/Apex/\
             Decomposer) and environment (Aquatic/Arboreal/Subterranean/\
             Aerial/Plains) as quirks."
        }
    }
}

fn string_list_ui(
    ui: &mut egui::Ui,
    items: &mut Vec<String>,
    add_label: &str,
) -> bool {
    let mut changed = false;
    let mut remove: Option<usize> = None;
    for (i, item) in items.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            changed |= ui.text_edit_singleline(item).changed();
            if ui.button("×").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        items.remove(i);
        changed = true;
    }
    if ui.button(add_label).clicked() {
        items.push(String::new());
        changed = true;
    }
    changed
}

struct Entry {
    soul: SoulFile,
    path: Option<std::path::PathBuf>,
}

pub struct SoulEditor {
    entries: Vec<Entry>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_soul() -> SoulFile {
    SoulFile {
        id: "new_soul".into(),
        name: "New Character".into(),
        species: Species::Human,
        portrait_id: String::new(),
        identity: Identity {
            origin: "unknown".into(),
            faction_affiliation: "independent".into(),
            role: "crew".into(),
            public_bio: String::new(),
        },
        personality: Personality {
            traits: vec![],
            values: vec![],
            speaking_style: SpeakingStyle::Elaborate,
            quirks: vec![],
        },
        emotional_state: EmotionalState {
            dominant_mood: Mood::Stable,
            intensity: 512,
            triggers: vec![],
        },
        memory_tree: vec![],
        relationship_graph: vec![],
        goals: vec![],
        breaking_points: vec![],
        contracts: vec![],
        backstory: String::new(),
        secrets: vec![],
        dialogue: None,
        deflections: vec![],
    }
}

impl SoulEditor {
    fn new() -> Self {
        let mut entries = Vec::new();
        if let Ok(dir) = std::fs::read_dir("mods/reachlock/souls") {
            let mut paths: Vec<_> = dir
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                if let Ok(soul) = crate::io::read_ron::<SoulFile>(&path) {
                    entries.push(Entry {
                        soul,
                        path: Some(path),
                    });
                }
            }
        }
        if entries.is_empty() {
            entries.push(Entry {
                soul: blank_soul(),
                path: None,
            });
        }
        SoulEditor {
            entries,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

/// The dialogue graph widget (handoff §13 tab 4). Nodes are collapsing
/// headers; each choice routes to a node id or ends the conversation.
fn dialogue_graph_ui(ui: &mut egui::Ui, graph: &mut DialogueGraph) -> bool {
    let mut changed = false;
    let node_ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let labels: std::collections::HashMap<String, String> = graph
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                format!("{}: {}", n.id, n.line.chars().take(20).collect::<String>()),
            )
        })
        .collect();
    let node_label =
        move |id: &str| -> String { labels.get(id).cloned().unwrap_or_else(|| id.to_string()) };

    ui.horizontal(|ui| {
        ui.label("Start node:");
        egui::ComboBox::from_id_salt("dialogue_start")
            .selected_text(node_label(&graph.start))
            .show_ui(ui, |ui| {
                for id in &node_ids {
                    changed |= ui
                        .selectable_value(&mut graph.start, id.clone(), node_label(id))
                        .changed();
                }
            });
    });

    let mut remove_node: Option<usize> = None;
    for (i, node) in graph.nodes.iter_mut().enumerate() {
        let header = format!(
            "Node {}: {}",
            node.id,
            node.line.chars().take(30).collect::<String>()
        );
        egui::CollapsingHeader::new(header)
            .id_salt(("dialogue_node", i))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("ID:");
                    changed |= ui.text_edit_singleline(&mut node.id).changed();
                    if ui.button("Remove Node").clicked() {
                        remove_node = Some(i);
                    }
                });
                ui.label("NPC line:");
                changed |= ui
                    .add(
                        egui::TextEdit::multiline(&mut node.line)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY),
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut node.llm_edge, "LLM edge (\"say something else\")")
                    .changed();

                ui.label("Player responses:");
                let mut remove_choice: Option<usize> = None;
                for (j, choice) in node.choices.iter_mut().enumerate() {
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(">");
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(&mut choice.label)
                                        .desired_width(240.0),
                                )
                                .changed();
                            ui.label("→");
                            let current = choice
                                .next
                                .clone()
                                .unwrap_or_else(|| "(end)".into());
                            egui::ComboBox::from_id_salt(("dialogue_next", i, j))
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    if ui
                                        .selectable_label(choice.next.is_none(), "(end)")
                                        .clicked()
                                    {
                                        choice.next = None;
                                        changed = true;
                                    }
                                    for id in &node_ids {
                                        if ui
                                            .selectable_label(
                                                choice.next.as_deref() == Some(id),
                                                node_label(id),
                                            )
                                            .clicked()
                                        {
                                            choice.next = Some(id.clone());
                                            changed = true;
                                        }
                                    }
                                });
                            if ui.button("×").clicked() {
                                remove_choice = Some(j);
                            }
                        });
                        let mut has_condition = choice.condition.is_some();
                        if ui
                            .checkbox(&mut has_condition, "Condition gate")
                            .changed()
                        {
                            choice.condition = has_condition
                                .then_some(reachlock_core::contract::types::Condition::Always);
                            changed = true;
                        }
                        if let Some(cond) = &mut choice.condition {
                            let (c, _) = condition_node_ui(
                                ui,
                                cond,
                                egui::Id::new(("dialogue_cond", i, j)),
                                0,
                                false,
                            );
                            changed |= c;
                        }
                    });
                }
                if let Some(j) = remove_choice {
                    node.choices.remove(j);
                    changed = true;
                }
                if ui.button("+ Add Response").clicked() {
                    node.choices.push(DialogueChoice {
                        label: String::new(),
                        condition: None,
                        effects: vec![],
                        next: None,
                    });
                    changed = true;
                }
            });
    }
    if let Some(i) = remove_node {
        graph.nodes.remove(i);
        changed = true;
    }
    if ui.button("+ Add Node").clicked() {
        let id = format!("node_{}", graph.nodes.len() + 1);
        if graph.nodes.is_empty() {
            graph.start = id.clone();
        }
        graph.nodes.push(DialogueNode {
            id,
            line: String::new(),
            choices: vec![],
            llm_edge: false,
        });
        changed = true;
    }
    changed
}

impl Editor for SoulEditor {
    fn title(&self) -> &str {
        "Soul Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Soul
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let soul: SoulFile = crate::io::read_ron(path)?;
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| e.path.as_deref() == Some(path))
        {
            self.entries[i].soul = soul;
            self.selected = i;
        } else {
            self.entries.push(Entry {
                soul,
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
            .ok_or_else(|| "no soul selected".to_string())?;
        crate::io::write_ron(path, &entry.soul)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(entry) = self.entries.get(self.selected) else {
            return errors;
        };
        let soul = &entry.soul;
        if soul.id.is_empty() {
            errors.push("id must not be empty".into());
        }
        if soul.name.is_empty() {
            errors.push("name must not be empty".into());
        }
        if !(0..=1024).contains(&soul.emotional_state.intensity) {
            errors.push("emotional intensity must be within 0..=1024".into());
        }
        for (i, rel) in soul.relationship_graph.iter().enumerate() {
            if rel.target_id.is_empty() {
                errors.push(format!("relationship {i}: target_id must not be empty"));
            }
            if !(-1024..=1024).contains(&rel.trust) {
                errors.push(format!("relationship {i}: trust must be within -1024..=1024"));
            }
            if !(0..=1024).contains(&rel.familiarity) {
                errors.push(format!(
                    "relationship {i}: familiarity must be within 0..=1024"
                ));
            }
        }
        if let Some(graph) = &soul.dialogue {
            errors.extend(graph.validate());
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("soul_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self.selected as u64 + 42;
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.entries.push(Entry {
                        soul: blank_soul(),
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

        egui::SidePanel::left("soul_list")
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
                        let soul = &self.entries[i].soul;
                        if !needle.is_empty() && !soul.name.to_lowercase().contains(&needle) {
                            continue;
                        }
                        let color = species_color(soul.species);
                        let name = soul.name.clone();
                        ui.horizontal(|ui| {
                            ui.colored_label(color, "●");
                            if ui.selectable_label(self.selected == i, &name).clicked() {
                                self.selected = i;
                            }
                        });
                    }
                });
            });

        let validation = self.validate();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(entry) = self.entries.get_mut(self.selected) else {
                ui.label("No soul selected.");
                return;
            };
            let soul = &mut entry.soul;
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Identity — who they are")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("soul_identity").show(ui, |ui| {
                            ui.label("ID:");
                            changed |= ui.text_edit_singleline(&mut soul.id).changed();
                            ui.end_row();
                            ui.label("Name:");
                            changed |= ui.text_edit_singleline(&mut soul.name).changed();
                            ui.end_row();
                            ui.label("Species:");
                            egui::ComboBox::from_id_salt("soul_species")
                                .selected_text(species_name(soul.species))
                                .show_ui(ui, |ui| {
                                    for s in SPECIES {
                                        changed |= ui
                                            .selectable_value(
                                                &mut soul.species,
                                                s,
                                                species_name(s),
                                            )
                                            .changed();
                                    }
                                });
                            ui.end_row();
                            ui.label("Portrait ID:");
                            changed |=
                                ui.text_edit_singleline(&mut soul.portrait_id).changed();
                            ui.end_row();
                            ui.label("Origin:");
                            changed |=
                                ui.text_edit_singleline(&mut soul.identity.origin).changed();
                            ui.end_row();
                            ui.label("Faction:");
                            changed |= ui
                                .text_edit_singleline(&mut soul.identity.faction_affiliation)
                                .changed();
                            ui.end_row();
                            ui.label("Role:");
                            changed |=
                                ui.text_edit_singleline(&mut soul.identity.role).changed();
                            ui.end_row();
                        });
                        ui.small(species_hint(soul.species));
                        ui.label("Public bio:");
                        changed |= ui
                            .add(
                                egui::TextEdit::multiline(&mut soul.identity.public_bio)
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed();
                        ui.label("Backstory (author reference, never a prompt):");
                        changed |= ui
                            .add(
                                egui::TextEdit::multiline(&mut soul.backstory)
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed();
                    });

                egui::CollapsingHeader::new("Personality — voice and vocabulary")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Speaking style:");
                            egui::ComboBox::from_id_salt("soul_style")
                                .selected_text(format!(
                                    "{:?}",
                                    soul.personality.speaking_style
                                ))
                                .show_ui(ui, |ui| {
                                    for s in STYLES {
                                        changed |= ui
                                            .selectable_value(
                                                &mut soul.personality.speaking_style,
                                                s,
                                                format!("{s:?}"),
                                            )
                                            .changed();
                                    }
                                });
                        });
                        ui.label("Traits:");
                        changed |=
                            string_list_ui(ui, &mut soul.personality.traits, "Add Trait");
                        ui.label("Values:");
                        changed |=
                            string_list_ui(ui, &mut soul.personality.values, "Add Value");
                        ui.label("Quirks (species attributes live here too):");
                        changed |=
                            string_list_ui(ui, &mut soul.personality.quirks, "Add Quirk");
                    });

                egui::CollapsingHeader::new("Emotional state — baseline and triggers")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("soul_emotion").show(ui, |ui| {
                            ui.label("Dominant mood:");
                            egui::ComboBox::from_id_salt("soul_mood")
                                .selected_text(format!(
                                    "{:?}",
                                    soul.emotional_state.dominant_mood
                                ))
                                .show_ui(ui, |ui| {
                                    for m in MOODS {
                                        changed |= ui
                                            .selectable_value(
                                                &mut soul.emotional_state.dominant_mood,
                                                m,
                                                format!("{m:?}"),
                                            )
                                            .changed();
                                    }
                                });
                            ui.end_row();
                            ui.label("Intensity (0..=1024):");
                            changed |= ui
                                .add(
                                    egui::DragValue::new(
                                        &mut soul.emotional_state.intensity,
                                    )
                                    .range(0..=1024),
                                )
                                .changed();
                            ui.end_row();
                        });
                        ui.label("Mood triggers:");
                        let mut remove: Option<usize> = None;
                        for (i, trigger) in
                            soul.emotional_state.triggers.iter_mut().enumerate()
                        {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("→ mood:");
                                    egui::ComboBox::from_id_salt(("soul_trigger_mood", i))
                                        .selected_text(format!("{:?}", trigger.mood))
                                        .show_ui(ui, |ui| {
                                            for m in MOODS {
                                                changed |= ui
                                                    .selectable_value(
                                                        &mut trigger.mood,
                                                        m,
                                                        format!("{m:?}"),
                                                    )
                                                    .changed();
                                            }
                                        });
                                    ui.label("intensity:");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut trigger.intensity)
                                                .range(0..=1024),
                                        )
                                        .changed();
                                    ui.label("priority:");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(&mut trigger.priority)
                                                .range(0..=255),
                                        )
                                        .changed();
                                    if ui.button("×").clicked() {
                                        remove = Some(i);
                                    }
                                });
                                let (c, _) = condition_node_ui(
                                    ui,
                                    &mut trigger.condition,
                                    egui::Id::new(("soul_trigger_cond", i)),
                                    0,
                                    false,
                                );
                                changed |= c;
                            });
                        }
                        if let Some(i) = remove {
                            soul.emotional_state.triggers.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Trigger").clicked() {
                            soul.emotional_state.triggers.push(Trigger {
                                condition:
                                    reachlock_core::contract::types::Condition::Always,
                                mood: Mood::Tense,
                                intensity: 512,
                                priority: 0,
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Dialogue graph — authored conversation")
                    .default_open(false)
                    .show(ui, |ui| {
                        let mut has_graph = soul.dialogue.is_some();
                        if ui.checkbox(&mut has_graph, "Has Dialogue Graph").changed() {
                            soul.dialogue = has_graph.then(|| DialogueGraph {
                                start: "node_1".into(),
                                nodes: vec![DialogueNode {
                                    id: "node_1".into(),
                                    line: "Hello, traveler.".into(),
                                    choices: vec![],
                                    llm_edge: false,
                                }],
                            });
                            changed = true;
                        }
                        if let Some(graph) = &mut soul.dialogue {
                            changed |= dialogue_graph_ui(ui, graph);
                        }
                    });

                egui::CollapsingHeader::new("Secrets & breaking points")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label("Secrets:");
                        let mut remove_secret: Option<usize> = None;
                        for (i, secret) in soul.secrets.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("ID:");
                                    changed |=
                                        ui.text_edit_singleline(&mut secret.id).changed();
                                    if ui.button("×").clicked() {
                                        remove_secret = Some(i);
                                    }
                                });
                                ui.label("Reveal condition:");
                                let (c, _) = condition_node_ui(
                                    ui,
                                    &mut secret.reveal_condition,
                                    egui::Id::new(("soul_secret_cond", i)),
                                    0,
                                    false,
                                );
                                changed |= c;
                                ui.label("Content (hidden until revealed):");
                                changed |= ui
                                    .add(
                                        egui::TextEdit::multiline(&mut secret.content)
                                            .desired_rows(2)
                                            .desired_width(f32::INFINITY),
                                    )
                                    .changed();
                            });
                        }
                        if let Some(i) = remove_secret {
                            soul.secrets.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Secret").clicked() {
                            soul.secrets.push(Secret {
                                id: format!("secret_{}", soul.secrets.len()),
                                reveal_condition:
                                    reachlock_core::contract::types::Condition::Always,
                                content: String::new(),
                            });
                            changed = true;
                        }

                        ui.separator();
                        ui.label("Breaking points:");
                        let mut remove_bp: Option<usize> = None;
                        for (i, bp) in soul.breaking_points.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label("ID:");
                                    changed |= ui.text_edit_singleline(&mut bp.id).changed();
                                    ui.label("Reaction:");
                                    egui::ComboBox::from_id_salt(("soul_bp_reaction", i))
                                        .selected_text(format!("{:?}", bp.reaction))
                                        .show_ui(ui, |ui| {
                                            for r in REACTIONS {
                                                changed |= ui
                                                    .selectable_value(
                                                        &mut bp.reaction,
                                                        r,
                                                        format!("{r:?}"),
                                                    )
                                                    .changed();
                                            }
                                        });
                                    if ui.button("×").clicked() {
                                        remove_bp = Some(i);
                                    }
                                });
                                ui.label("Trigger:");
                                let (c, _) = condition_node_ui(
                                    ui,
                                    &mut bp.trigger,
                                    egui::Id::new(("soul_bp_cond", i)),
                                    0,
                                    false,
                                );
                                changed |= c;
                            });
                        }
                        if let Some(i) = remove_bp {
                            soul.breaking_points.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Breaking Point").clicked() {
                            soul.breaking_points.push(BreakingPoint {
                                id: format!("break_{}", soul.breaking_points.len()),
                                trigger:
                                    reachlock_core::contract::types::Condition::Always,
                                reaction: BreakReaction::Withdraw,
                            });
                            changed = true;
                        }

                        ui.separator();
                        ui.label("Deflections (offline fallback lines, in-voice):");
                        changed |=
                            string_list_ui(ui, &mut soul.deflections, "Add Deflection");
                    });

                egui::CollapsingHeader::new("Memory & relationships")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.label("Formative memories:");
                        let mut remove_mem: Option<usize> = None;
                        for (i, memory) in soul.memory_tree.iter_mut().enumerate() {
                            ui.group(|ui| {
                                egui::Grid::new(("soul_memory", i)).show(ui, |ui| {
                                    ui.label("ID:");
                                    changed |=
                                        ui.text_edit_singleline(&mut memory.id).changed();
                                    ui.end_row();
                                    ui.label("Event type:");
                                    changed |= ui
                                        .text_edit_singleline(&mut memory.event_type)
                                        .changed();
                                    ui.end_row();
                                    ui.label("Timestamp (tick):");
                                    changed |= ui
                                        .add(egui::DragValue::new(&mut memory.timestamp))
                                        .changed();
                                    ui.end_row();
                                    ui.label("Emotional weight (0..=1024):");
                                    changed |= ui
                                        .add(
                                            egui::DragValue::new(
                                                &mut memory.emotional_weight,
                                            )
                                            .range(0..=1024),
                                        )
                                        .changed();
                                    ui.end_row();
                                    ui.label("Player involved:");
                                    changed |= ui
                                        .checkbox(&mut memory.player_involved, "")
                                        .changed();
                                    ui.end_row();
                                });
                                ui.label("Summary:");
                                changed |= ui
                                    .add(
                                        egui::TextEdit::multiline(&mut memory.summary)
                                            .desired_rows(2)
                                            .desired_width(f32::INFINITY),
                                    )
                                    .changed();
                                if ui.button("Remove Memory").clicked() {
                                    remove_mem = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_mem {
                            soul.memory_tree.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Memory").clicked() {
                            soul.memory_tree.push(Memory {
                                id: format!("memory_{}", soul.memory_tree.len()),
                                event_type: "conversation".into(),
                                player_involved: false,
                                emotional_weight: 256,
                                timestamp: 0,
                                summary: String::new(),
                            });
                            changed = true;
                        }

                        ui.separator();
                        ui.label("Relationships:");
                        let mut remove_rel: Option<usize> = None;
                        for (i, rel) in soul.relationship_graph.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |=
                                    ui.text_edit_singleline(&mut rel.target_id).changed();
                                ui.label("trust:");
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut rel.trust)
                                            .range(-1024..=1024),
                                    )
                                    .changed();
                                ui.label("familiarity:");
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut rel.familiarity)
                                            .range(0..=1024),
                                    )
                                    .changed();
                                if !rel.history.is_empty() {
                                    ui.label(format!("({} history)", rel.history.len()));
                                }
                                if ui.button("×").clicked() {
                                    remove_rel = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_rel {
                            soul.relationship_graph.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Relationship").clicked() {
                            soul.relationship_graph.push(Relationship {
                                target_id: "player".into(),
                                trust: 0,
                                familiarity: 0,
                                history: vec![],
                            });
                            changed = true;
                        }

                        ui.separator();
                        ui.label("Goals:");
                        let mut remove_goal: Option<usize> = None;
                        for (i, goal) in soul.goals.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                changed |= ui.text_edit_singleline(&mut goal.id).changed();
                                egui::ComboBox::from_id_salt(("soul_goal_priority", i))
                                    .selected_text(format!("{:?}", goal.priority))
                                    .show_ui(ui, |ui| {
                                        for p in
                                            [GoalPriority::Constant, GoalPriority::Situational]
                                        {
                                            changed |= ui
                                                .selectable_value(
                                                    &mut goal.priority,
                                                    p,
                                                    format!("{p:?}"),
                                                )
                                                .changed();
                                        }
                                    });
                                changed |=
                                    ui.text_edit_singleline(&mut goal.description).changed();
                                if ui.button("×").clicked() {
                                    remove_goal = Some(i);
                                }
                            });
                        }
                        if let Some(i) = remove_goal {
                            soul.goals.remove(i);
                            changed = true;
                        }
                        if ui.button("Add Goal").clicked() {
                            soul.goals.push(Goal {
                                id: format!("goal_{}", soul.goals.len()),
                                priority: GoalPriority::Situational,
                                description: String::new(),
                            });
                            changed = true;
                        }
                    });

                egui::CollapsingHeader::new("Contracts — automation this soul can run")
                    .default_open(false)
                    .show(ui, |ui| {
                        changed |= string_list_ui(
                            ui,
                            &mut soul.contracts,
                            "Add Contract Reference",
                        );
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
        let mut rng = SeededRng::new(seed ^ 0x5001_D00D);
        // Weighted species roll (handoff §13): 40% Human, 20% Android,
        // 15% Robot, 15% Voidborn, 10% Xenotype.
        let species = match rng.next_below(100) {
            0..=39 => Species::Human,
            40..=59 => Species::Android,
            60..=74 => Species::Robot,
            75..=89 => Species::Voidborn,
            _ => Species::Xenotype,
        };
        let names = [
            "Kaelen", "Zeryn", "Mira", "Torben", "Saris", "Lyra", "Dax", "Vella",
        ];
        let traits = [
            "adaptable", "stubborn", "curious", "cautious", "loyal", "restless",
        ];
        let values = ["survival", "honesty", "profit", "freedom", "kinship"];
        let mut soul = blank_soul();
        soul.id = format!("soul_{seed:x}");
        soul.name = names[rng.next_below(names.len() as u64) as usize].into();
        soul.species = species;
        soul.portrait_id = format!("portrait_{seed:x}");
        soul.identity.public_bio = format!(
            "A {} making their way through charted space.",
            species_name(species).to_lowercase()
        );
        soul.personality.traits = (0..2)
            .map(|_| traits[rng.next_below(traits.len() as u64) as usize].to_string())
            .collect();
        soul.personality.values =
            vec![values[rng.next_below(values.len() as u64) as usize].to_string()];
        soul.personality.speaking_style = STYLES[rng.next_below(9) as usize];
        soul.emotional_state.dominant_mood = MOODS[rng.next_below(11) as usize];
        soul.emotional_state.intensity = 256 + rng.next_below(768) as i64;
        soul.relationship_graph = (0..1 + rng.next_below(2))
            .map(|i| Relationship {
                target_id: if i == 0 { "player".into() } else { "boris".into() },
                trust: rng.next_below(1024) as i64 - 256,
                familiarity: rng.next_below(512) as i64,
                history: vec![],
            })
            .collect();
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.soul = soul;
        }
        self.has_changes = true;
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        // The soul schema describes a ContentFile envelope; some models emit
        // the envelope, others the bare SoulFile. Accept both.
        let inner = match serde_json::from_value::<SoulFile>(value.clone()) {
            Ok(soul) => soul,
            Err(_) => {
                let extracted = super::super::ai::extract_inner_from_envelope(value, "soul").ok_or_else(|| {
                    "response was neither a bare soul nor a ContentFile envelope".to_string()
                })?;
                serde_json::from_value::<SoulFile>(extracted)
                    .map_err(|e| format!("soul file: {e}"))?
            }
        };
        if let Some(entry) = self.entries.get_mut(self.selected) {
            entry.soul = inner;
        } else {
            self.entries.push(Entry { soul: inner, path: None });
            self.selected = self.entries.len() - 1;
        }
        self.has_changes = true;
        Ok(())
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(SoulEditor::new())
}
