use std::path::PathBuf;

use crate::app::ContentType;

pub struct TemplateManager {
    templates_dir: PathBuf,
    cache: Option<Vec<TemplateEntry>>,
}

#[derive(Clone)]
pub struct TemplateEntry {
    pub label: String,
    pub content_type: ContentType,
    pub path: PathBuf,
}

#[expect(dead_code)]
impl TemplateManager {
    pub fn new() -> Self {
        let templates_dir = find_templates_dir();
        Self {
            templates_dir,
            cache: None,
        }
    }

    pub fn reload(&mut self) {
        self.cache = None;
    }

    pub fn list_templates(&mut self) -> Vec<TemplateEntry> {
        if let Some(ref cached) = self.cache {
            return cached.clone();
        }

        let mut entries = Vec::new();
        if !self.templates_dir.is_dir() {
            self.cache = Some(entries.clone());
            return entries;
        }

        let Ok(dir) = std::fs::read_dir(&self.templates_dir) else {
            self.cache = Some(entries.clone());
            return entries;
        };

        for entry in dir.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "ron") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(ct) = content_type_from_template_stem(&stem) {
                    let label = format!("New {} from template", ct.name());
                    entries.push(TemplateEntry {
                        label,
                        content_type: ct,
                        path,
                    });
                }
            }
        }

        entries.sort_by(|a, b| a.content_type.name().cmp(b.content_type.name()));
        self.cache = Some(entries.clone());
        entries
    }

    pub fn load_template(&self, entry: &TemplateEntry) -> Result<String, String> {
        std::fs::read_to_string(&entry.path).map_err(|e| format!("failed to read template: {e}"))
    }

    pub fn templates_dir(&self) -> &PathBuf {
        &self.templates_dir
    }
}

fn content_type_from_template_stem(stem: &str) -> Option<ContentType> {
    match stem {
        "soul" => Some(ContentType::Soul),
        "contract" => Some(ContentType::Contract),
        "faction" => Some(ContentType::Faction),
        "career" => Some(ContentType::Career),
        "station" => Some(ContentType::Station),
        "hull_frame" => Some(ContentType::HullFrame),
        "hull_mesh" => Some(ContentType::HullMesh),
        "room_templates" => Some(ContentType::RoomTemplates),
        "dialogue" => Some(ContentType::Dialogue),
        "dungeon" => Some(ContentType::Dungeon),
        "event" => Some(ContentType::Event),
        "recipe" => Some(ContentType::Recipe),
        "item" => Some(ContentType::Item),
        "system" | "charted_system" => Some(ContentType::ChartedSystem),
        "gate_network" => Some(ContentType::GateNetwork),
        "ecosystem" => Some(ContentType::Ecosystem),
        "planet_culture" => Some(ContentType::PlanetCulture),
        "theme" => Some(ContentType::Theme),
        "trope" => Some(ContentType::Trope),
        "scripted_encounter" => Some(ContentType::ScriptedEncounter),
        "enemy_archetype" | "enemy" => Some(ContentType::EnemyArchetype),
        "location" => Some(ContentType::Location),
        "storyline" => Some(ContentType::Storyline),
        "economy_goods" | "economy" => Some(ContentType::EconomyGoods),
        _ => None,
    }
}

fn find_templates_dir() -> PathBuf {
    let bundled = if let Ok(exe) = std::env::current_exe() {
        exe.parent()
            .map(|p| p.join("templates"))
            .unwrap_or_else(|| PathBuf::from("templates"))
    } else {
        PathBuf::from("templates")
    };
    if bundled.is_dir() {
        return bundled;
    }

    let src = PathBuf::from("reachlock-editor/templates");
    if src.is_dir() {
        return src;
    }

    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let fallback = std::path::Path::new(&manifest).join("templates");
        if fallback.is_dir() {
            return fallback;
        }
    }

    bundled
}
