use std::collections::HashMap;

use bevy::prelude::*;
use reachlock_core::content::ContentFile;
use reachlock_core::galaxy::{ChartedSystem, GateNetwork};

/// The local override index (spec §10, "Loader reads `content/` from disk
/// at startup (local mode)"). Empty on wasm — there is no filesystem to
/// read; server distribution of overrides is S23's problem.
///
/// S20 adds typed sub-indices for landed-combat content; S21 adds galaxy
/// content. All loaded as plain structs from subdirectories of `content/`.
#[derive(Resource, Default)]
pub struct ContentIndex {
    pub files: Vec<ContentFile>,
    /// S20 enemy/companion archetypes, keyed by `HostileArchetype::id`.
    pub hostile_archetypes: HashMap<String, reachlock_core::combat::HostileArchetype>,
    /// S20 authored hostile interiors, keyed by `HostileLocation::id`.
    pub hostile_locations: HashMap<String, reachlock_core::combat::HostileLocation>,
    /// S21: authored charted systems, keyed by system id.
    pub charted_systems: HashMap<String, ChartedSystem>,
    /// S21: the authored gate network (single file: `core_region.ron`).
    pub gate_network: Option<GateNetwork>,
}

impl ContentIndex {
    /// Find an authored station payload by its pinned seed (S07: the docked
    /// station's `station_seed` is the authored file's `seed`). Returns the
    /// station file if present, so `interior::enter_interior` can use its
    /// authored layout + `npc_spawns` instead of regenerating.
    pub fn find_station_by_seed(&self, seed: u64) -> Option<&ContentFile> {
        self.files
            .iter()
            .find(|f| f.asset_type == reachlock_core::content::AssetType::Station && f.seed == seed)
    }
}

/// Directory under `content/` that holds test fixtures, not real authored
/// assets (spec §10 deliverable: "skip `content/_fixtures/`").
#[cfg(not(target_arch = "wasm32"))]
const FIXTURES_DIR: &str = "_fixtures";

/// Native loader: walks `content/` from the working directory, parsing each
/// `.ron` file into a `ContentFile`. Files that fail to parse are logged and
/// skipped rather than aborting startup — one bad authored file shouldn't
/// take down the whole index.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_content_index(mut commands: Commands) {
    let mut files = Vec::new();
    let root = std::path::Path::new("content");
    if root.is_dir() {
        walk(root, &mut files);
    } else {
        warn!("content index: no content/ directory found at {root:?}; index is empty");
    }
    // S20/S21: typed content loaded as plain structs (not ContentFile envelope).
    let hostile_archetypes = load_typed::<reachlock_core::combat::HostileArchetype, _>(
        root.join("combat"), "archetype", |a| a.id.clone());
    let hostile_locations = load_typed::<reachlock_core::combat::HostileLocation, _>(
        root.join("locations"), "location", |l| l.id.clone());
    let charted_systems = load_typed(root.join("systems"), "system", |s: &ChartedSystem| {
        s.id.clone()
    });
    let gate_network =
        load_typed::<GateNetwork, _>(root.join("gate_network"), "gate_network", |_| {
            "core_region".into()
        })
        .into_iter()
        .next()
        .map(|(_, n)| n);
    info!(
        "content index: loaded {} authored file(s), {} archetype(s), {} location(s), {} system(s)",
        files.len(),
        hostile_archetypes.len(),
        hostile_locations.len(),
        charted_systems.len(),
    );
    commands.insert_resource(ContentIndex {
        files,
        hostile_archetypes,
        hostile_locations,
        charted_systems,
        gate_network,
    });
}

/// Parse every `.ron` in `dir` into `T`, keyed by `key`. Missing dir → empty
/// map; a bad file is logged and skipped (one typo shouldn't blank the set).
#[cfg(not(target_arch = "wasm32"))]
fn load_typed<T, K>(dir: std::path::PathBuf, label: &str, key: K) -> HashMap<String, T>
where
    T: serde::de::DeserializeOwned,
    K: Fn(&T) -> String,
{
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out; // no such directory: nothing authored yet
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => match ron::from_str::<T>(&text) {
                Ok(value) => {
                    out.insert(key(&value), value);
                }
                Err(err) => warn!("content index: bad {label} {}: {err}", path.display()),
            },
            Err(err) => warn!("content index: failed to read {}: {err}", path.display()),
        }
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn walk(dir: &std::path::Path, out: &mut Vec<ContentFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        warn!("content index: failed to read directory {dir:?}");
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip typed-content dirs parsed by `load_typed` (not ContentFile).
            let skip = [FIXTURES_DIR, "combat", "locations", "systems", "gate_network"];
            if path.file_name().is_some_and(|n| skip.contains(&n.to_str().unwrap_or(""))) {
                continue;
            }
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "ron") {
            match std::fs::read_to_string(&path) {
                Ok(text) => match ron::from_str::<ContentFile>(&text) {
                    Ok(file) => out.push(file),
                    Err(err) => {
                        warn!("content index: failed to parse {}: {err}", path.display())
                    }
                },
                Err(err) => warn!("content index: failed to read {}: {err}", path.display()),
            }
        }
    }
}

/// Wasm loader: no filesystem access, so the index starts (and stays)
/// empty. Server distribution of overrides (S23) is what fills this in on
/// wasm eventually.
#[cfg(target_arch = "wasm32")]
pub fn load_content_index(mut commands: Commands) {
    commands.insert_resource(ContentIndex::default());
}
