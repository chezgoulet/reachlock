//! Runtime loader for authored faction profiles and storylines.
//! Uses the Override API in `reachlock_core::faction` to replace embedded
//! defaults with content read from files at startup.

use std::path::Path;

use crate::faction::{
    set_faction_catalog, set_storylines, Faction, FactionCatalog, Storyline,
};

/// Load all faction JSON profiles from a directory and set the runtime catalog.
pub fn load_faction_profiles<P: AsRef<Path>>(dir: P) -> Result<(), String> {
    let mut factions = Vec::new();
    let dir = dir.as_ref();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read dir: {e}"))? {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("read {path:?}: {e}"))?;
        let faction: Faction =
            serde_json::from_str(&text).map_err(|e| format!("parse {path:?}: {e}"))?;
        factions.push(faction);
    }
    let catalog = FactionCatalog {
        factions,
        version: 1,
    };
    set_faction_catalog(catalog);
    Ok(())
}

/// Load all storyline RON files from a directory and set the runtime storylines.
pub fn load_storyline_files<P: AsRef<Path>>(dir: P) -> Result<(), String> {
    let mut storylines = Vec::new();
    let dir = dir.as_ref();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read dir: {e}"))? {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("ron") {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("read {path:?}: {e}"))?;
        let storyline: Storyline =
            ron::from_str(&text).map_err(|e| format!("parse {path:?}: {e}"))?;
        storylines.push(storyline);
    }
    if !storylines.is_empty() {
        set_storylines(storylines);
    }
    Ok(())
}
