//! Storyline editor (handoff §12): faction narrative arcs — chapters with
//! trigger trees, narration, and released events. A storyline `.ron` file
//! holds a `Vec<Storyline>`; the left panel lists the storylines inside the
//! loaded file by faction.

use reachlock_core::faction::{Chapter, ChapterTrigger, FactionId, Storyline};
use reachlock_core::util::rng::SeededRng;

use super::super::app::{ContentType, Editor};

fn trigger_variant_name(t: &ChapterTrigger) -> &'static str {
    match t {
        ChapterTrigger::TickAfter(_) => "TickAfter",
        ChapterTrigger::ChapterComplete(_) => "ChapterComplete",
        ChapterTrigger::PlayerReputation { .. } => "PlayerReputation",
        ChapterTrigger::All(_) => "All",
        ChapterTrigger::Any(_) => "Any",
    }
}

fn default_trigger(name: &str) -> ChapterTrigger {
    match name {
        "ChapterComplete" => ChapterTrigger::ChapterComplete(String::new()),
        "PlayerReputation" => ChapterTrigger::PlayerReputation {
            faction: FactionId(String::new()),
            trust: 50,
        },
        "All" => ChapterTrigger::All(vec![ChapterTrigger::TickAfter(0)]),
        "Any" => ChapterTrigger::Any(vec![ChapterTrigger::TickAfter(0)]),
        _ => ChapterTrigger::TickAfter(0),
    }
}

/// Recursive ChapterTrigger tree node (handoff §12 widget). Returns
/// `(changed, remove_requested)`.
fn trigger_node_ui(
    ui: &mut egui::Ui,
    trigger: &mut ChapterTrigger,
    id: egui::Id,
    depth: usize,
    removable: bool,
) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    ui.horizontal(|ui| {
        ui.add_space(20.0 * depth as f32);
        let current = trigger_variant_name(trigger);
        egui::ComboBox::from_id_salt(id.with("variant"))
            .selected_text(current)
            .width(140.0)
            .show_ui(ui, |ui| {
                for name in [
                    "TickAfter",
                    "ChapterComplete",
                    "PlayerReputation",
                    "All",
                    "Any",
                ] {
                    if ui.selectable_label(current == name, name).clicked() && current != name {
                        *trigger = default_trigger(name);
                        changed = true;
                    }
                }
            });
        match trigger {
            ChapterTrigger::TickAfter(tick) => {
                changed |= ui.add(egui::DragValue::new(tick)).changed();
            }
            ChapterTrigger::ChapterComplete(chapter) => {
                changed |= ui
                    .add(egui::TextEdit::singleline(chapter).desired_width(160.0))
                    .changed();
            }
            ChapterTrigger::PlayerReputation { faction, trust } => {
                ui.label("faction:");
                changed |= ui
                    .add(egui::TextEdit::singleline(&mut faction.0).desired_width(100.0))
                    .changed();
                ui.label("trust:");
                changed |= ui
                    .add(egui::DragValue::new(trust).range(-100..=100))
                    .changed();
            }
            _ => {}
        }
        if removable && ui.button("×").clicked() {
            remove = true;
        }
    });

    if let ChapterTrigger::All(children) | ChapterTrigger::Any(children) = trigger {
        let mut remove_child: Option<usize> = None;
        for (i, child) in children.iter_mut().enumerate() {
            let (c, r) = trigger_node_ui(ui, child, id.with(i), depth + 1, true);
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
                children.push(ChapterTrigger::TickAfter(0));
                changed = true;
            }
        });
    }
    (changed, remove)
}

pub struct StorylineEditor {
    storylines: Vec<Storyline>,
    path: Option<std::path::PathBuf>,
    selected: usize,
    search: String,
    has_changes: bool,
}

fn blank_storyline() -> Storyline {
    Storyline {
        faction: FactionId("compact".into()),
        chapters: vec![Chapter {
            id: "chapter_1".into(),
            trigger: Some(ChapterTrigger::TickAfter(5)),
            narration: String::new(),
            events: Vec::new(),
        }],
    }
}

impl StorylineEditor {
    fn new() -> Self {
        let default_path = crate::app::content_root()
            .join(ContentType::Storyline.directory())
            .join("compact_arc.ron");
        let (storylines, path) = match crate::io::read_ron::<Vec<Storyline>>(&default_path) {
            Ok(s) if !s.is_empty() => (s, Some(default_path.to_path_buf())),
            _ => (vec![blank_storyline()], None),
        };
        StorylineEditor {
            storylines,
            path,
            selected: 0,
            search: String::new(),
            has_changes: false,
        }
    }
}

impl Editor for StorylineEditor {
    fn title(&self) -> &str {
        "Storyline Editor"
    }

    fn content_type(&self) -> ContentType {
        ContentType::Storyline
    }

    fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }

    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.storylines = crate::io::read_ron(path)?;
        if self.storylines.is_empty() {
            self.storylines.push(blank_storyline());
        }
        self.path = Some(path.to_path_buf());
        self.selected = 0;
        self.has_changes = false;
        Ok(())
    }

    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.storylines)
    }

    fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let Some(story) = self.storylines.get(self.selected) else {
            return errors;
        };
        if story.faction.0.is_empty() {
            errors.push("faction must not be empty".into());
        }
        let mut seen = std::collections::HashSet::new();
        for (i, chapter) in story.chapters.iter().enumerate() {
            if chapter.id.is_empty() {
                errors.push(format!("chapter {i}: id must not be empty"));
            }
            if !seen.insert(&chapter.id) {
                errors.push(format!("duplicate chapter id: {}", chapter.id));
            }
            if chapter.narration.is_empty() {
                errors.push(format!("chapter {i}: narration must not be empty"));
            }
        }
        errors
    }

    #[allow(clippy::too_many_lines)]
    fn ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("storyline_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Generate from Seed").clicked() {
                    let seed = self.selected as u64 + 42;
                    self.generate_from_seed(seed);
                }
                if ui.button("New").clicked() {
                    self.storylines.push(blank_storyline());
                    self.selected = self.storylines.len() - 1;
                    self.has_changes = true;
                }
                if ui.button("Remove").clicked()
                    && self.storylines.len() > 1
                    && self.selected < self.storylines.len()
                {
                    self.storylines.remove(self.selected);
                    if self.selected >= self.storylines.len() {
                        self.selected = self.storylines.len() - 1;
                    }
                    self.has_changes = true;
                }
                let name = self
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(unsaved)".into());
                ui.label(name);
                if self.has_changes {
                    ui.label("*");
                }
            });
        });

        egui::SidePanel::left("storyline_list")
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
                    for i in 0..self.storylines.len() {
                        let s = &self.storylines[i];
                        let label = format!("{} ({} chapters)", s.faction.0, s.chapters.len());
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
            let Some(story) = self.storylines.get_mut(self.selected) else {
                ui.label("No storyline selected.");
                return;
            };
            let mut changed = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Faction:");
                    changed |= ui.text_edit_singleline(&mut story.faction.0).changed();
                });

                let mut remove_chapter: Option<usize> = None;
                for (i, chapter) in story.chapters.iter_mut().enumerate() {
                    egui::CollapsingHeader::new(format!("Chapter {i}: {}", chapter.id))
                        .id_salt(("storyline_chapter", i))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("ID:");
                                changed |= ui.text_edit_singleline(&mut chapter.id).changed();
                                if ui.button("Remove Chapter").clicked() {
                                    remove_chapter = Some(i);
                                }
                            });

                            let mut has_trigger = chapter.trigger.is_some();
                            if ui.checkbox(&mut has_trigger, "Has Trigger").changed() {
                                chapter.trigger =
                                    has_trigger.then_some(ChapterTrigger::TickAfter(0));
                                changed = true;
                            }
                            if let Some(trigger) = &mut chapter.trigger {
                                let (c, r) = trigger_node_ui(
                                    ui,
                                    trigger,
                                    egui::Id::new(("storyline_trigger", i)),
                                    0,
                                    true,
                                );
                                changed |= c;
                                if r {
                                    chapter.trigger = None;
                                    changed = true;
                                }
                            }

                            ui.label("Narration:");
                            changed |= ui
                                .add(
                                    egui::TextEdit::multiline(&mut chapter.narration)
                                        .desired_rows(3)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed();

                            ui.label("Released events:");
                            let mut remove_event: Option<usize> = None;
                            for (j, event) in chapter.events.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    changed |= ui.text_edit_singleline(event).changed();
                                    if ui.button("×").clicked() {
                                        remove_event = Some(j);
                                    }
                                });
                            }
                            if let Some(j) = remove_event {
                                chapter.events.remove(j);
                                changed = true;
                            }
                            if ui.button("Add Event").clicked() {
                                chapter.events.push(String::new());
                                changed = true;
                            }
                        });
                }
                if let Some(i) = remove_chapter {
                    story.chapters.remove(i);
                    changed = true;
                }
                if ui.button("Add Chapter").clicked() {
                    story.chapters.push(Chapter {
                        id: format!("chapter_{}", story.chapters.len() + 1),
                        trigger: None,
                        narration: String::new(),
                        events: Vec::new(),
                    });
                    changed = true;
                }

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
        let mut rng = SeededRng::new(seed ^ 0x5709_C00C);
        let factions = ["compact", "isc", "corp_charter", "the_reach"];
        let faction = factions[rng.next_below(factions.len() as u64) as usize];
        let templates = [
            "Word spreads through {f} space: something is stirring beyond the gates.",
            "A {f} convoy goes silent. Patrols double at every checkpoint.",
            "The {f} leadership calls an emergency session behind closed doors.",
        ];
        let chapters = [5u64, 15, 30]
            .iter()
            .enumerate()
            .map(|(i, &tick)| Chapter {
                id: format!("chapter_{}", i + 1),
                trigger: Some(ChapterTrigger::TickAfter(tick)),
                narration: templates[rng.next_below(templates.len() as u64) as usize]
                    .replace("{f}", faction),
                events: Vec::new(),
            })
            .collect();
        let storyline = Storyline {
            faction: FactionId(faction.into()),
            chapters,
        };
        if let Some(s) = self.storylines.get_mut(self.selected) {
            *s = storyline;
        }
        self.has_changes = true;
    }

    fn apply_ai_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let storyline: Storyline =
            serde_json::from_value(value.clone()).map_err(|e| format!("storyline: {e}"))?;
        if self.storylines.is_empty() {
            self.storylines.push(storyline);
        } else {
            self.storylines[self.selected] = storyline;
        }
        self.has_changes = true;
        Ok(())
    }

    fn snapshot(&self) -> Option<String> {
        ron::to_string(&(&self.storylines, &self.path, self.selected)).ok()
    }

    fn restore_snapshot(&mut self, ron: &str) -> Result<(), String> {
        let (storylines, path, selected): (Vec<Storyline>, Option<std::path::PathBuf>, usize) =
            ron::from_str(ron).map_err(|e| e.to_string())?;
        self.storylines = storylines;
        self.path = path;
        self.selected = selected.min(self.storylines.len().saturating_sub(1));
        self.has_changes = true;
        Ok(())
    }

    fn mark_saved(&mut self) {
        self.has_changes = false;
    }

    fn selected_entry_name(&self) -> Option<String> {
        if self.storylines.len() <= 1 {
            return None;
        }
        self.storylines
            .get(self.selected)
            .map(|s| format!("{} storyline", s.faction.0))
    }

    fn delete_selected(&mut self) -> bool {
        if self.storylines.len() <= 1 || self.selected >= self.storylines.len() {
            return false;
        }
        self.storylines.remove(self.selected);
        if self.selected >= self.storylines.len() {
            self.selected = self.storylines.len() - 1;
        }
        self.has_changes = true;
        true
    }

    fn preview_ui(&self, ui: &mut egui::Ui) {
        let Some(s) = self.storylines.get(self.selected) else {
            return;
        };
        ui.strong(format!("Faction: {}", s.faction.0));
        ui.label(format!("{} chapter(s)", s.chapters.len()));
        for chapter in s.chapters.iter().take(5) {
            ui.weak(format!(
                "· {}{}",
                chapter.id,
                if chapter.trigger.is_some() {
                    ""
                } else {
                    " (no trigger)"
                }
            ));
        }
        ui.label(format!("{} storyline(s) in file", self.storylines.len()));
    }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(StorylineEditor::new())
}
