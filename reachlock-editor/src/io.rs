use std::path::Path;

use reachlock_core::content::{content_seed, ContentFile, Enveloped, Priority};

use crate::app::ContentType;

/// The envelope fields around an authored payload.
///
/// Most editor tabs edit one payload type and have no UI for the envelope, so
/// they keep working with the bare `Theme`/`SoulFile`/`Origin` and carry this
/// alongside. It exists so a save puts the metadata back: `seed`, `universe`
/// and `priority` are author decisions, and a tab that silently reset them on
/// every save would be corrupting content rather than editing it.
/// `Serialize`/`Deserialize` so tabs that snapshot their state for undo carry
/// the envelope through the round trip — an undo that dropped the metadata
/// would silently reset the author's seed and priority on the next save.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnvelopeMeta {
    pub id: String,
    pub display_name: String,
    pub seed: u64,
    pub universe: String,
    pub priority: Priority,
    pub expires_at: Option<u64>,
}

impl EnvelopeMeta {
    /// Defaults for a document the author just created. The seed is derived
    /// the canonical way (spec §10, Seed Integration) rather than left at zero,
    /// so a new file is diffable against a recomputation like every other
    /// authored file.
    pub fn new_for(id: &str) -> Self {
        EnvelopeMeta {
            id: id.to_string(),
            display_name: id.to_string(),
            seed: content_seed("content_override", id),
            universe: "all".into(),
            priority: Priority::Authoritative,
            expires_at: None,
        }
    }

    fn from_file(file: &ContentFile) -> Self {
        EnvelopeMeta {
            id: file.id.clone(),
            display_name: file.display_name.clone(),
            seed: file.seed,
            universe: file.universe.clone(),
            priority: file.priority,
            expires_at: file.expires_at,
        }
    }
}

/// Read an envelope-wrapped content file and hand back the metadata and the
/// bare payload.
///
/// Fails when the file is not an envelope, or is an envelope carrying a
/// different payload type — pointing a tab at the wrong file must be visible,
/// not a silently empty document.
pub fn read_enveloped<T: Enveloped>(path: &Path) -> Result<(EnvelopeMeta, T), String> {
    let file: ContentFile = read_ron(path)?;
    let meta = EnvelopeMeta::from_file(&file);
    let asset_type = file.asset_type;
    let inner = file.into_inner::<T>().ok_or_else(|| {
        format!(
            "{} is a {:?} file, but this editor edits {:?}",
            path.display(),
            asset_type,
            T::ASSET_TYPE
        )
    })?;
    Ok((meta, inner))
}

/// Write a bare payload back inside its envelope, preserving the metadata the
/// file was loaded with.
pub fn write_enveloped<T: Enveloped>(
    path: &Path,
    meta: &EnvelopeMeta,
    inner: T,
) -> Result<(), String> {
    let mut file = ContentFile::wrap(
        meta.id.clone(),
        meta.display_name.clone(),
        meta.seed,
        meta.universe.clone(),
        meta.priority,
        inner,
    );
    file.expires_at = meta.expires_at;
    write_ron(path, &file)
}

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

// Schema validation has no caller: `Editor::validate` checks structure
// per tab, and the AI path validates the model's JSON before applying it,
// but nothing validates a hand-edited document against
// `mods/reachlock/schemas/*.json`. Wiring it needs a JSON view of an open
// document; editors expose RON (`snapshot`) and their own typed state.
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
    validator
        .iter_errors(value)
        .map(|err| format!("{}: {err}", err.instance_path()))
        .collect()
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

    fn content_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("mods/reachlock")
    }

    /// Authored files pin `seed`, `universe` and `priority`. A save that reset
    /// them would quietly rewrite the author's decisions — the failure mode
    /// this whole layer exists to prevent.
    #[test]
    fn envelope_metadata_survives_a_save() {
        use reachlock_core::generator::music::Theme;

        let src = content_root().join("themes/calm_exploration.ron");
        let (meta, theme) = read_enveloped::<Theme>(&src).expect("load authored theme");
        assert_ne!(meta.seed, 0, "the authored theme pins a real seed");

        let dir = std::env::temp_dir().join("reachlock_envelope_round_trip");
        let _ = std::fs::create_dir_all(&dir);
        let dst = dir.join("theme.ron");
        write_enveloped(&dst, &meta, theme.clone()).expect("write");

        let (back_meta, back_theme) = read_enveloped::<Theme>(&dst).expect("reload");
        assert_eq!(back_meta, meta);
        assert_eq!(back_theme, theme);

        let _ = std::fs::remove_file(&dst);
        let _ = std::fs::remove_dir(&dir);
    }

    /// Opening a soul file in the theme editor must say so, not hand back an
    /// empty document the author then saves over the real content.
    #[test]
    fn reading_the_wrong_payload_type_is_an_error() {
        use reachlock_core::generator::music::Theme;

        let soul = content_root().join("souls/tib.ron");
        let err = read_enveloped::<Theme>(&soul).expect_err("must not load a soul as a theme");
        assert!(
            err.contains("Soul") && err.contains("Theme"),
            "error should name both types, got: {err}"
        );
    }

    /// A brand-new document gets a real derived seed, not zero.
    #[test]
    fn new_documents_get_a_canonical_seed() {
        let meta = EnvelopeMeta::new_for("brand_new_theme");
        assert_ne!(meta.seed, 0);
        assert_eq!(meta.id, "brand_new_theme");
        assert_eq!(meta.universe, "all");
        assert_eq!(
            meta.seed,
            EnvelopeMeta::new_for("brand_new_theme").seed,
            "seed derivation must be deterministic"
        );
    }
}
