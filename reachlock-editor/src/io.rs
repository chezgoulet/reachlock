use std::path::Path;

use crate::app::ContentType;

pub fn read_ron<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    ron::from_str(&String::from_utf8_lossy(&bytes))
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

pub fn write_ron<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = ron::to_string(value).map_err(|e| format!("failed to serialize: {e}"))?;
    std::fs::write(path, &text).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

#[allow(dead_code)]
pub fn validate_content(content_type: &ContentType, value: &serde_json::Value) -> Vec<String> {
    let schema_id = match content_type {
        ContentType::HullFrame => "hull_frame",
        ContentType::Station => "station",
        ContentType::Location => "location",
        ContentType::Soul => "soul",
        ContentType::Contract => "contract",
        ContentType::Faction => "faction",
        ContentType::EconomyGoods => "economy_goods",
        ContentType::Storyline => "storyline",
        ContentType::Item => "item",
        ContentType::EnemyArchetype => "enemy_archetype",
        ContentType::ChartedSystem => "charted_system",
        ContentType::HullMesh => "hull",
        ContentType::RoomTemplates => "room_template",
        ContentType::GateNetwork => "gate_network",
    };

    let schema_json = match std::fs::read_to_string(format!(
        "content/schemas/{}.schema.json",
        schema_id
    )) {
        Ok(s) => s,
        Err(_) => return vec!["no schema file found".into()],
    };

    let schema: serde_json::Value = match serde_json::from_str(&schema_json) {
        Ok(v) => v,
        Err(e) => return vec![format!("invalid schema: {e}")],
    };

    let validator = match jsonschema::options().build(&schema) {
        Ok(v) => v,
        Err(e) => return vec![format!("schema compilation failed: {e}")],
    };
    let mut errors = Vec::new();
    if let Err(validation_errors) = validator.validate(value) {
        for err in validation_errors {
            errors.push(format!("{}: {err}", err.instance_path));
        }
    }
    errors
}
