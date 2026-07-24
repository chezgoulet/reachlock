use std::collections::HashMap;

use bevy::prelude::*;
use reachlock_core::content::ContentFile;
use reachlock_core::galaxy::{ChartedSystem, GateNetwork};
use reachlock_core::generator::music::Theme;
use reachlock_core::mod_manifest::{resolve_load_order, ModManifest};

/// The local override index (spec §10, "Loader reads `mods/` from disk
/// at startup (local mode)"). Empty on wasm — there is no filesystem to
/// read; server distribution of overrides is S23's problem.
///
/// S22: the loader scans `mods/*/mod.manifest.ron`, resolves load order
/// (topological sort + load_order field), and aggregates typed content
/// from each mod directory. Last-loaded mod wins on collisions.
#[derive(Resource, Default)]
pub struct ContentIndex {
    pub files: Vec<ContentFile>,
    /// All loaded mod manifests, keyed by mod id.
    #[allow(dead_code)]
    pub mod_manifests: HashMap<String, ModManifest>,
    /// Resolved load order: mod ids in the order they should be consulted.
    #[allow(dead_code)]
    pub load_order: Vec<String>,
    /// S20 enemy/companion archetypes, keyed by `HostileArchetype::id`.
    pub hostile_archetypes: HashMap<String, reachlock_core::combat::HostileArchetype>,
    /// S20 authored hostile interiors, keyed by `HostileLocation::id`.
    pub hostile_locations: HashMap<String, reachlock_core::combat::HostileLocation>,
    /// S21: authored charted systems, keyed by system id.
    pub charted_systems: HashMap<String, ChartedSystem>,
    /// S21: the authored gate network (single file: `core_region.ron`).
    pub gate_network: Option<GateNetwork>,
    /// S48: authored music themes, keyed by `Theme::id`.
    #[allow(dead_code)]
    pub themes: HashMap<String, Theme>,
}

impl ContentIndex {
    pub fn find_station_by_seed(&self, seed: u64) -> Option<&ContentFile> {
        self.files
            .iter()
            .find(|f| f.asset_type == reachlock_core::content::AssetType::Station && f.seed == seed)
    }
}

/// Directory that holds test fixtures, not real authored assets.
const FIXTURES_DIR: &str = "_fixtures";

pub fn load_content_index(mut commands: Commands) {
    let root = std::path::Path::new("mods");
    if !root.is_dir() {
        warn!("content index: no mods/ directory found at {root:?}; index is empty");
        commands.insert_resource(ContentIndex::default());
        return;
    }

    // Phase 1: discover mod manifests.
    let mut manifests: HashMap<String, ModManifest> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let mod_dir = entry.path();
            if !mod_dir.is_dir() {
                continue;
            }
            let manifest_path = mod_dir.join("mod.manifest.ron");
            if manifest_path.exists() {
                match std::fs::read_to_string(&manifest_path) {
                    Ok(text) => match ron::from_str::<ModManifest>(&text) {
                        Ok(m) => {
                            manifests.insert(m.id.clone(), m);
                        }
                        Err(err) => warn!(
                            "content index: bad manifest {}: {err}",
                            manifest_path.display()
                        ),
                    },
                    Err(err) => warn!(
                        "content index: failed to read manifest {}: {err}",
                        manifest_path.display()
                    ),
                }
            }
        }
    }

    // Phase 2: resolve load order.
    let all_manifests: Vec<ModManifest> = manifests.values().cloned().collect();
    let load_order = match resolve_load_order(&all_manifests) {
        Ok(order) => order,
        Err(err) => {
            warn!("content index: mod load order error: {err:?}");
            // Fall back to alphabetical.
            let mut ids: Vec<String> = manifests.keys().cloned().collect();
            ids.sort();
            ids
        }
    };

    // Phase 3: load typed content from each mod in load order.
    let mut hostile_archetypes: HashMap<String, reachlock_core::combat::HostileArchetype> =
        HashMap::new();
    let mut hostile_locations: HashMap<String, reachlock_core::combat::HostileLocation> =
        HashMap::new();
    let mut charted_systems: HashMap<String, ChartedSystem> = HashMap::new();
    let mut gate_network: Option<GateNetwork> = None;
    let mut themes: HashMap<String, Theme> = HashMap::new();

    for mod_id in &load_order {
        let mod_dir = root.join(mod_id);
        // load_typed inserts into the maps — last mod wins collisions.
        load_typed_into(
            &mod_dir.join("combat"),
            &mut hostile_archetypes,
            |a: &reachlock_core::combat::HostileArchetype| a.id.clone(),
        );
        load_typed_into(
            &mod_dir.join("locations"),
            &mut hostile_locations,
            |l: &reachlock_core::combat::HostileLocation| l.id.clone(),
        );
        load_typed_into(
            &mod_dir.join("systems"),
            &mut charted_systems,
            |s: &ChartedSystem| s.id.clone(),
        );
        // Gate network: only one file per mod, last mod loaded wins.
        let gn_map: HashMap<String, GateNetwork> =
            load_typed(&mod_dir.join("gate_network"), |_| "core".to_string());
        if let Some((_, gn)) = gn_map.into_iter().next() {
            gate_network = Some(gn);
        }
        // S48: authored music themes.
        load_typed_into(
            &mod_dir.join("themes"),
            &mut themes,
            |t: &Theme| t.id.clone(),
        );
    }

    // Phase 4: walk for ContentFile envelopes (skip typed dirs and manifest).
    let mut files = Vec::new();
    walk(root, &mut files);

    info!(
        "content index: {} mod(s), {} file(s), {} archetype(s), {} location(s), {} system(s), {} theme(s)",
        manifests.len(),
        files.len(),
        hostile_archetypes.len(),
        hostile_locations.len(),
        charted_systems.len(),
        themes.len(),
    );
    commands.insert_resource(ContentIndex {
        files,
        mod_manifests: manifests,
        load_order,
        hostile_archetypes,
        hostile_locations,
        charted_systems,
        gate_network,
        themes,
    });
}

/// Parse every `.ron` in `dir` into a HashMap<T> keyed by a function.
fn load_typed<T, K>(dir: &std::path::Path, key: K) -> HashMap<String, T>
where
    T: serde::de::DeserializeOwned,
    K: Fn(&T) -> String,
{
    let mut out = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
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
                Err(err) => warn!("content index: failed to parse {}: {err}", path.display()),
            },
            Err(err) => warn!("content index: failed to read {}: {err}", path.display()),
        }
    }
    out
}

/// Like `load_typed` but merges into an existing map (last-wins).
fn load_typed_into<T, K>(dir: &std::path::Path, out: &mut HashMap<String, T>, key: K)
where
    T: serde::de::DeserializeOwned,
    K: Fn(&T) -> String,
{
    let items = load_typed(dir, key);
    for (k, v) in items {
        out.insert(k, v);
    }
}

fn walk(dir: &std::path::Path, out: &mut Vec<ContentFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        warn!("content index: failed to read directory {dir:?}");
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip typed-content dirs and fixtures.
            let skip = [
                FIXTURES_DIR,
                "combat",
                "locations",
                "systems",
                "gate_network",
                "themes",
            ];
            if path
                .file_name()
                .is_some_and(|n| skip.contains(&n.to_str().unwrap_or("")))
            {
                continue;
            }
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "ron") {
            // Skip mod manifest files — they're parsed separately.
            if path.file_name().is_some_and(|n| n == "mod.manifest.ron") {
                continue;
            }
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


