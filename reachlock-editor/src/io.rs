use std::path::Path;

use crate::app::ContentType;

pub fn read_ron<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    ron::from_str(&String::from_utf8_lossy(&bytes))
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Pretty-printed RON — used for author-facing content files so they stay
/// readable and produce small, reviewable git diffs. Note: RON does not
/// preserve comments through a deserialize → serialize round-trip, so hand-
/// authored commented content should not be round-tripped through the editor.
pub fn write_ron<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("failed to serialize: {e}"))?;
    std::fs::write(path, &text).map_err(|e| format!("failed to write {}: {e}", path.display()))
}

#[allow(dead_code)]
pub fn validate_content(content_type: &ContentType, value: &serde_json::Value) -> Vec<String> {
    let Some(schema_id) = crate::schema::schema_id(content_type) else {
        // Previewers persist nothing; no schema applies.
        return Vec::new();
    };

    let schema_json = match std::fs::read_to_string(
        crate::schema::schemas_dir().join(format!("{schema_id}.schema.json")),
    ) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `write_ron` must produce pretty (multi-line) RON so author files stay
    /// readable and diff-friendly. Round-tripping the bytes back must yield
    /// the original value.
    #[test]
    fn write_ron_is_pretty_and_round_trips() {
        let value = reachlock_core::item::ItemSeed {
            seed: 12345,
            item_type: reachlock_core::item::ItemType::Equipment(
                reachlock_core::item::EquipmentKind::Armor,
            ),
            tier: 3,
            faction: "compact".into(),
            biome: "".into(),
        };
        let dir = std::env::temp_dir().join("reachlock_io_tests");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("seed.ron");
        write_ron(&path, &value).expect("write");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            text.contains('\n'),
            "write_ron should be pretty (multi-line)"
        );
        let back: reachlock_core::item::ItemSeed = read_ron(&path).expect("read back");
        assert_eq!(back, value);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
