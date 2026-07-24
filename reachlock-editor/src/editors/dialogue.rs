use reachlock_core::content::dialogue::{Dialogue, DialogueChoice, DialogueNode, NodeType};

use super::super::app::{ContentType, Editor};

pub struct DialogueEditor {
    path: Option<std::path::PathBuf>,
    dialogue: Dialogue,
    has_changes: bool,
}

impl DialogueEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Dialogue.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(d) = crate::io::read_ron::<Dialogue>(path) {
                return DialogueEditor { path: Some(path.clone()), dialogue: d, has_changes: false };
            }
        }
        DialogueEditor {
            path: None,
            dialogue: Dialogue {
                nodes: vec![
                    DialogueNode {
                        id: "start".into(),
                        node_type: NodeType::NpcLine,
                        text: "Hello, traveler.".into(),
                        choices: vec![DialogueChoice {
                            display_text: "Greetings.".into(),
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
            },
            has_changes: false,
        }
    }
}

impl Editor for DialogueEditor {
    fn title(&self) -> &str { &self.dialogue.start_node }
    fn content_type(&self) -> ContentType { ContentType::Dialogue }
    fn has_unsaved_changes(&self) -> bool { self.has_changes }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let d: Dialogue = crate::io::read_ron(path).map_err(|e| format!("reading dialogue: {e}"))?;
        self.dialogue = d;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.dialogue).map_err(|e| format!("saving dialogue: {e}"))
    }
    fn save_all(&mut self) -> Result<(), String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root().join(ContentType::Dialogue.directory()).join("generated_dialogue.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
        self.has_changes = false;
        Ok(())
    }
    fn generate_from_seed(&mut self, seed: u64) {
        self.dialogue.start_node = format!("node_{:#x}", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.dialogue.nodes.is_empty() { errors.push("no dialogue nodes".into()); }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("Dialogue nodes: {}", self.dialogue.nodes.len()));
            ui.label(format!("Start node: {}", self.dialogue.start_node));
        });
    }
    fn mark_saved(&mut self) { self.has_changes = false; }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(DialogueEditor::load_or_new())
}
