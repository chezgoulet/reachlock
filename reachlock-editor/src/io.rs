use std::path::{Path, PathBuf};

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

    pub(crate) fn from_file(file: &ContentFile) -> Self {
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

/// Filename stem for a new entry, derived from its content id.
///
/// Nine multi-entry editors each rolled their own version of this and
/// disagreed: some used the payload id, some the display name, one a loop
/// counter. Naming from the display name is how an agent-authored star system
/// with `id: "zola_swamp_system"` landed on disk as `Uncharted 0000.ron` —
/// a file whose name matches nothing anyone would search for, containing
/// content nothing references.
///
/// The id is the right source because it is what the rest of the tree refers
/// to. `display_name` is prose and changes; the id is the handle.
pub fn file_stem_for_id(id: &str, fallback: &str) -> String {
    let stem: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    // Collapse runs and trim, so "Uncharted 0000" cannot become
    // "uncharted_0000" with a trailing separator, and an id that is entirely
    // punctuation cannot produce a dotfile or an empty name.
    let mut out = String::with_capacity(stem.len());
    let mut last_underscore = false;
    for c in stem.chars() {
        if c == '_' {
            if !last_underscore && !out.is_empty() {
                out.push('_');
            }
            last_underscore = true;
        } else {
            out.push(c);
            last_underscore = false;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

/// A free path in `dir` for a new entry with this id.
///
/// Suffixes rather than overwriting. `write_ron` truncates, so without this a
/// second new entry that happened to share an id — two unnamed drafts, or a
/// re-run of the same generation — silently destroyed the first one's file.
pub fn new_entry_path(dir: &Path, id: &str, fallback: &str) -> PathBuf {
    let stem = file_stem_for_id(id, fallback);
    let first = dir.join(format!("{stem}.ron"));
    if !first.exists() {
        return first;
    }
    // Bounded: something is badly wrong long before 1000, and an unbounded
    // loop here would hang the save rather than fail it.
    for n in 2..1000 {
        let candidate = dir.join(format!("{stem}_{n}.ron"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // Give up distinctly rather than silently overwriting.
    dir.join(format!("{stem}_overflow.ron"))
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

/// Scan a content directory, parsing each `.ron` file as `T`.
///
/// Returns the successfully parsed files alongside a warning per file that
/// failed to parse. Multi-entry editors previously dropped unparseable files
/// with `if let Ok(..)`, so a malformed file vanished from the editor with no
/// error anywhere — the tab simply opened short an entry, or empty.
///
/// Callers that also reject files on a *payload variant* check must do that
/// filtering on the returned values, NOT here: a `hulls/` file that parses as
/// a `HullFrame` is correctly skipped by the HullMesh tab and must not warn.
pub fn scan_content_dir<T: serde::de::DeserializeOwned>(
    dir: &Path,
) -> (Vec<(PathBuf, T)>, Vec<String>) {
    let mut loaded = Vec::new();
    let mut warnings = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        // A missing directory is normal: a mod need not define every type.
        return (loaded, warnings);
    };
    let mut paths: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    paths.sort();
    for path in paths {
        match read_ron::<T>(&path) {
            Ok(value) => loaded.push((path, value)),
            Err(e) => warnings.push(e),
        }
    }
    (loaded, warnings)
}

/// Scan an envelope-wrapped content directory, preserving metadata.
/// Same shape as [`scan_content_dir`] but uses [`read_enveloped`] so the
/// author's `seed`, `universe` and `priority` survive the round trip.
pub fn scan_enveloped_dir<T: Enveloped>(
    dir: &Path,
) -> (Vec<(PathBuf, crate::io::EnvelopeMeta, T)>, Vec<String>) {
    let mut loaded = Vec::new();
    let mut warnings = Vec::new();
    let Ok(read) = std::fs::read_dir(dir) else {
        return (loaded, warnings);
    };
    let mut paths: Vec<PathBuf> = read
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    paths.sort();
    for path in paths {
        match read_enveloped::<T>(&path) {
            Ok((meta, value)) => loaded.push((path, meta, value)),
            Err(e) => warnings.push(e),
        }
    }
    (loaded, warnings)
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

    /// The case that started this: an agent-authored star system whose id was
    /// `zola_swamp_system` was saved as `Uncharted 0000.ron`, because the
    /// naming came from the display name rather than the id.
    #[test]
    fn a_stem_comes_from_the_id_not_a_display_name() {
        assert_eq!(
            file_stem_for_id("zola_swamp_system", "system_0"),
            "zola_swamp_system"
        );
    }

    #[test]
    fn a_stem_is_filename_safe() {
        // Spaces, capitals and punctuation all become one separator, and a
        // trailing separator is trimmed — otherwise "Uncharted 0000" yields
        // a name with a dangling underscore.
        assert_eq!(file_stem_for_id("Uncharted 0000", "x"), "uncharted_0000");
        assert_eq!(file_stem_for_id("a//b  c", "x"), "a_b_c");
        assert_eq!(file_stem_for_id("trailing---", "x"), "trailing");
        // An id that sanitizes to nothing must not produce "" or a dotfile.
        assert_eq!(file_stem_for_id("///", "fallback"), "fallback");
        assert_eq!(file_stem_for_id("", "fallback"), "fallback");
    }

    /// `write_ron` truncates, so without suffixing, a second new entry with
    /// the same id silently destroyed the first one's file.
    #[test]
    fn a_colliding_name_is_suffixed_not_overwritten() {
        let dir = std::env::temp_dir().join(format!(
            "rl_entry_path_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let first = new_entry_path(&dir, "zola", "x");
        assert_eq!(first.file_name().unwrap(), "zola.ron");
        std::fs::write(&first, "()").unwrap();

        let second = new_entry_path(&dir, "zola", "x");
        assert_eq!(second.file_name().unwrap(), "zola_2.ron");
        std::fs::write(&second, "()").unwrap();

        let third = new_entry_path(&dir, "zola", "x");
        assert_eq!(third.file_name().unwrap(), "zola_3.ron");

        // The original is intact — the whole point.
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "()");
        let _ = std::fs::remove_dir_all(&dir);
    }
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

    /// `scan_content_dir` parses parseable files and reports unparseable ones.
    /// A non-`.ron` file is silently ignored.
    #[test]
    fn scan_content_dir_reports_unparseable_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join("reachlock_scan_test");
        let _ = std::fs::create_dir_all(&dir);

        // Valid RON file.
        std::fs::write(dir.join("good.ron"), "42").unwrap();
        // Malformed RON file.
        std::fs::write(dir.join("bad.ron"), "not valid ron {{{").unwrap();
        // Non-RON file (should be silently ignored).
        std::fs::write(dir.join("README.md"), "just text").unwrap();

        let (loaded, warnings) = scan_content_dir::<i32>(&dir);
        assert_eq!(loaded.len(), 1, "should have parsed the good file");
        assert_eq!(loaded[0].0.file_name().unwrap(), "good.ron");
        assert_eq!(loaded[0].1, 42);
        assert_eq!(
            warnings.len(),
            1,
            "should have one warning for the bad file"
        );
        assert!(
            warnings[0].contains("bad.ron"),
            "warning should name the malformed file: {}",
            warnings[0]
        );

        let _ = std::fs::remove_file(dir.join("good.ron"));
        let _ = std::fs::remove_file(dir.join("bad.ron"));
        let _ = std::fs::remove_file(dir.join("README.md"));
        let _ = std::fs::remove_dir(&dir);
    }

    /// `scan_enveloped_dir` also reports unparseable files while keeping valid ones.
    #[test]
    fn scan_enveloped_dir_reports_unparseable_and_keeps_the_rest() {
        use reachlock_core::generator::music::Theme;

        let dir = std::env::temp_dir().join("reachlock_scan_env_test");
        let _ = std::fs::create_dir_all(&dir);

        // Copy a real envelope file as the valid one.
        let src = content_root().join("themes/calm_exploration.ron");
        let valid_text = std::fs::read_to_string(&src).unwrap();
        std::fs::write(dir.join("valid.ron"), &valid_text).unwrap();
        // Malformed file.
        std::fs::write(dir.join("bad.ron"), "not valid {{{").unwrap();

        let (loaded, warnings) = scan_enveloped_dir::<Theme>(&dir);
        assert_eq!(loaded.len(), 1, "should have parsed the valid envelope");
        assert_eq!(warnings.len(), 1, "should warn about the bad file");

        let _ = std::fs::remove_file(dir.join("valid.ron"));
        let _ = std::fs::remove_file(dir.join("bad.ron"));
        let _ = std::fs::remove_dir(&dir);
    }
}
