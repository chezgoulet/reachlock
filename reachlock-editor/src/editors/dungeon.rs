use reachlock_core::content::dungeon::{Dungeon, DungeonRoom};

use super::super::app::{ContentType, Editor};

pub struct DungeonEditor {
    path: Option<std::path::PathBuf>,
    dungeon: Dungeon,
    has_changes: bool,
}

impl DungeonEditor {
    fn load_or_new() -> Self {
        let dir = crate::app::content_root().join(ContentType::Dungeon.directory());
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "ron"))
            .map(|e| e.path())
            .collect();
        if let Some(path) = files.first() {
            if let Ok(d) = crate::io::read_ron::<Dungeon>(path) {
                return DungeonEditor { path: Some(path.clone()), dungeon: d, has_changes: false };
            }
        }
        DungeonEditor {
            path: None,
            dungeon: Dungeon {
                id: "new_dungeon".into(),
                rooms: vec![DungeonRoom {
                    id: "room_0".into(),
                    x: 0, y: 0, width: 8, height: 6,
                    connectors: vec![],
                    tags: vec!["entrance".into()],
                }],
                puzzles: vec![],
                enemies: vec![],
                reward_tables: vec![],
            },
            has_changes: false,
        }
    }
}

impl Editor for DungeonEditor {
    fn title(&self) -> &str { &self.dungeon.id }
    fn content_type(&self) -> ContentType { ContentType::Dungeon }
    fn has_unsaved_changes(&self) -> bool { self.has_changes }
    fn load(&mut self, path: &std::path::Path) -> Result<(), String> {
        let d: Dungeon = crate::io::read_ron(path).map_err(|e| format!("reading dungeon: {e}"))?;
        self.dungeon = d;
        self.path = Some(path.to_path_buf());
        self.has_changes = false;
        Ok(())
    }
    fn save(&self, path: &std::path::Path) -> Result<(), String> {
        crate::io::write_ron(path, &self.dungeon).map_err(|e| format!("saving dungeon: {e}"))
    }
    fn save_all(&mut self) -> Result<(), String> {
        let path = self.path.clone().unwrap_or_else(|| {
            crate::app::content_root().join(ContentType::Dungeon.directory()).join("generated_dungeon.ron")
        });
        self.save(&path)?;
        self.path = Some(path);
        self.has_changes = false;
        Ok(())
    }
    fn generate_from_seed(&mut self, seed: u64) {
        self.dungeon.id = format!("dungeon_{:#x}", seed);
        self.has_changes = true;
    }
    fn validate(&self) -> Vec<String> {
        let mut errors = vec![];
        if self.dungeon.id.is_empty() { errors.push("id is empty".into()); }
        errors
    }
    fn ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(format!("Dungeon: {}", self.dungeon.id));
            ui.label(format!("Rooms: {}", self.dungeon.rooms.len()));
            ui.label(format!("Puzzles: {}", self.dungeon.puzzles.len()));
            ui.label(format!("Enemy groups: {}", self.dungeon.enemies.len()));
        });
    }
    fn mark_saved(&mut self) { self.has_changes = false; }
}

pub fn create_editor() -> Box<dyn Editor> {
    Box::new(DungeonEditor::load_or_new())
}
