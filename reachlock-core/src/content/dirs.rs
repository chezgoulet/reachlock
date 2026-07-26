//! How a content directory is loaded — the single source of truth.
//!
//! Three consumers walk a content root and each used to carry its own list of
//! which directories hold [`ContentFile`](super::ContentFile) envelopes and
//! which hold bare payload structs: the client's `ContentIndex` loader, the
//! content-tree checker in [`super::refs`], and the editor's cross-reference
//! scanner. The three drifted, and the drift was invisible until content went
//! missing at runtime — `themes/` ended up skipped by the loader *and* parsed
//! as the wrong type by its fallback, so the one authored theme reached
//! nothing.
//!
//! Adding a directory to `mods/` now means classifying it here, once. The test
//! at the bottom fails if a directory ships without a classification, so "a
//! system nobody can reach" (iron rule #8) is a build error rather than a
//! silent skip.

/// How the files in a content directory are shaped, and therefore who loads
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirKind {
    /// Files are [`ContentFile`](super::ContentFile) envelopes. The envelope's
    /// own `asset_type` decides the kind, so a file in the "wrong" directory is
    /// still indexed correctly.
    Envelope,
    /// Files are bare payload structs with no envelope — reference data the
    /// player never picks from a menu. Loaded into typed maps, not into the
    /// envelope index.
    Typed,
    /// Never read from disk at runtime: these are `include_str!` embeds baked
    /// into the binary. Walking them yields nothing but parse warnings.
    External,
    /// Both shapes live here. Try the envelope first, fall back to the bare
    /// type, and don't warn when the envelope parse fails — that is the
    /// expected path for half the directory.
    Mixed,
    /// Test fixtures and JSON schemas: not authored game content.
    Fixtures,
}

/// Bare-typed reference data.
const TYPED: &[&str] = &["combat", "locations", "systems", "gate_network"];

/// Compile-time embeds. `economy/goods.ron` and `factions/canon.ron` are
/// `include_str!`-ed into core; the copies on disk exist for authoring and
/// validation, and the runtime must not try to parse them as envelopes.
const EXTERNAL: &[&str] = &["economy", "factions"];

/// `hulls/` carries envelope-wrapped hull meshes, hull frames and room
/// templates alongside bare `ShipTemplate`s, which the client reads through its
/// own catalog rather than the envelope index.
const MIXED: &[&str] = &["hulls"];

/// Not game content.
const FIXTURES: &[&str] = &["_fixtures", "schemas"];

/// Every directory name with an explicit classification. Used by the coverage
/// test; [`classify`] falls through to [`DirKind::Envelope`] for anything else
/// so a third-party mod can ship a directory core has never heard of.
pub const CLASSIFIED: &[&str] = &[
    "combat",
    "locations",
    "systems",
    "gate_network",
    "economy",
    "factions",
    "hulls",
    "_fixtures",
    "schemas",
    // Envelope directories are listed explicitly too: `classify` would return
    // `Envelope` for them anyway, but naming them here is what lets the
    // coverage test tell "deliberately an envelope directory" apart from
    // "nobody has looked at this directory yet".
    "origins",
    "souls",
    "careers",
    "stations",
    "cultures",
    "ecosystems",
    "themes",
    "crews",
    "storylines",
    "contracts",
    "tropes",
    "encounters",
    "dialogues",
    "dungeons",
    "events",
    "recipes",
];

/// Classify a directory by name (not path — pass the final component).
///
/// Unknown names are [`DirKind::Envelope`]: the envelope is the default
/// authoring format, and a mod that invents a directory should still load.
pub fn classify(dir_name: &str) -> DirKind {
    if TYPED.contains(&dir_name) {
        DirKind::Typed
    } else if EXTERNAL.contains(&dir_name) {
        DirKind::External
    } else if MIXED.contains(&dir_name) {
        DirKind::Mixed
    } else if FIXTURES.contains(&dir_name) {
        DirKind::Fixtures
    } else {
        DirKind::Envelope
    }
}

/// Every directory that holds envelopes, for consumers that need to enumerate
/// rather than classify. Ordered for stable output.
pub fn envelope_dirs() -> Vec<&'static str> {
    let mut dirs: Vec<&'static str> = CLASSIFIED
        .iter()
        .copied()
        .filter(|d| classify(d) == DirKind::Envelope)
        .collect();
    dirs.sort_unstable();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_directories_classify_as_intended() {
        assert_eq!(classify("combat"), DirKind::Typed);
        assert_eq!(classify("gate_network"), DirKind::Typed);
        assert_eq!(classify("economy"), DirKind::External);
        assert_eq!(classify("factions"), DirKind::External);
        assert_eq!(classify("hulls"), DirKind::Mixed);
        assert_eq!(classify("_fixtures"), DirKind::Fixtures);
        assert_eq!(classify("schemas"), DirKind::Fixtures);
        assert_eq!(classify("souls"), DirKind::Envelope);
        assert_eq!(classify("themes"), DirKind::Envelope);
        assert_eq!(classify("storylines"), DirKind::Envelope);
    }

    /// `themes/` was skipped by the client loader while its only authored file
    /// was a perfectly good envelope. Pin the classification so the skip cannot
    /// come back.
    #[test]
    fn themes_are_envelopes_not_typed() {
        assert_eq!(classify("themes"), DirKind::Envelope);
        assert!(!TYPED.contains(&"themes"));
    }

    /// `storylines/` holds working envelopes. A proposal to treat it as an
    /// `include_str!` embed would have silently dropped both files.
    #[test]
    fn storylines_are_not_external() {
        assert_eq!(classify("storylines"), DirKind::Envelope);
        assert!(!EXTERNAL.contains(&"storylines"));
    }

    /// Iron rule #8, as a build gate: a directory that ships in the reference
    /// mod without a classification is content nothing is guaranteed to reach.
    #[test]
    fn every_shipped_directory_is_classified() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("mods/reachlock");
        let entries =
            std::fs::read_dir(&root).unwrap_or_else(|e| panic!("reading {}: {e}", root.display()));
        let mut unclassified = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("directory name")
                .to_string();
            if !CLASSIFIED.contains(&name.as_str()) {
                unclassified.push(name);
            }
        }
        assert!(
            unclassified.is_empty(),
            "content directories with no entry in content::dirs::CLASSIFIED: {unclassified:?}. \
             Add each to the right category — an unclassified directory loads as an envelope \
             directory by default, which is silent if that guess is wrong."
        );
    }
}
