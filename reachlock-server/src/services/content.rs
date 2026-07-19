//! WASM content distribution (spec §10, "offline is first-class").
//!
//! A wasm client has no filesystem, so it cannot read `mods/` at startup.
//! This service mirrors the client's `ContentIndex` loader but produces a
//! [`reachlock_core::network::ServerMessage::ContentSync`] payload for a
//! given universe, which the WS handler ships over the wire. The server
//! *adds* content; it never replaces a local-mode loader.

use std::collections::HashMap;
use std::path::Path;

use reachlock_core::combat::{HostileArchetype, HostileLocation};
use reachlock_core::content::ContentFile;
use reachlock_core::galaxy::{ChartedSystem, GateNetwork};
use reachlock_core::mod_manifest::{resolve_load_order, ModManifest};
use reachlock_core::network::ServerMessage;
use reachlock_core::universe::tier::UniverseTier;

/// Aggregate authored content for a universe, loaded from `mods/` on disk.
///
/// Content is read once at construction (the mod tree is static at runtime)
/// and cached; `sync_for` only filters the cached files by universe on each
/// request instead of re-walking the filesystem.
pub struct ContentService {
    files: Vec<ContentFile>,
    hostile_archetypes: Vec<HostileArchetype>,
    hostile_locations: Vec<HostileLocation>,
    charted_systems: Vec<ChartedSystem>,
    gate_network: Option<GateNetwork>,
}

/// All authored content loaded from `mods/`, before universe filtering.
struct LoadedContent {
    files: Vec<ContentFile>,
    hostile_archetypes: Vec<HostileArchetype>,
    hostile_locations: Vec<HostileLocation>,
    charted_systems: Vec<ChartedSystem>,
    gate_network: Option<GateNetwork>,
}

impl ContentService {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let loaded = load_all(&root);
        ContentService {
            files: loaded.files,
            hostile_archetypes: loaded.hostile_archetypes,
            hostile_locations: loaded.hostile_locations,
            charted_systems: loaded.charted_systems,
            gate_network: loaded.gate_network,
        }
    }

    /// Build the `content.sync` message for `universe`, filtering the cached
    /// `ContentFile` envelopes to those that apply to the tier (spec §10
    /// universe match). Typed collections are universe-agnostic, so they pass
    /// through unchanged.
    pub fn sync_for(&self, universe: UniverseTier) -> ServerMessage {
        let files: Vec<ContentFile> = self
            .files
            .iter()
            .filter(|f| f.matches_universe(universe))
            .cloned()
            .collect();
        ServerMessage::ContentSync {
            universe,
            files,
            hostile_archetypes: self.hostile_archetypes.clone(),
            hostile_locations: self.hostile_locations.clone(),
            charted_systems: self.charted_systems.clone(),
            gate_network: self.gate_network.clone(),
        }
    }
}

/// Load and cache all authored content from `root` (best effort — empty
/// collections on any missing piece).
fn load_all(root: &Path) -> LoadedContent {
    if !root.is_dir() {
        return LoadedContent {
            files: vec![],
            hostile_archetypes: vec![],
            hostile_locations: vec![],
            charted_systems: vec![],
            gate_network: None,
        };
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
                if let Ok(text) = std::fs::read_to_string(&manifest_path) {
                    if let Ok(m) = ron::from_str::<ModManifest>(&text) {
                        manifests.insert(m.id.clone(), m);
                    }
                }
            }
        }
    }

    // Phase 2: resolve load order (best effort — fall back to alphabetical).
    let all: Vec<ModManifest> = manifests.values().cloned().collect();
    let load_order = match resolve_load_order(&all) {
        Ok(order) => order,
        Err(_) => {
            let mut ids: Vec<String> = manifests.keys().cloned().collect();
            ids.sort();
            ids
        }
    };

    // Phase 3: load typed content in load order; last mod wins collisions.
    let mut hostile_archetypes: HashMap<String, HostileArchetype> = HashMap::new();
    let mut hostile_locations: HashMap<String, HostileLocation> = HashMap::new();
    let mut charted_systems: HashMap<String, ChartedSystem> = HashMap::new();
    let mut gate_network: Option<GateNetwork> = None;

    for mod_id in &load_order {
        let mod_dir = root.join(mod_id);
        load_typed_into(&mod_dir.join("combat"), &mut hostile_archetypes, |a: &HostileArchetype| {
            a.id.clone()
        });
        load_typed_into(&mod_dir.join("locations"), &mut hostile_locations, |l: &HostileLocation| {
            l.id.clone()
        });
        load_typed_into(&mod_dir.join("systems"), &mut charted_systems, |s: &ChartedSystem| {
            s.id.clone()
        });
        let gn_map: HashMap<String, GateNetwork> =
            load_typed(&mod_dir.join("gate_network"), |_| "core".to_string());
        if let Some((_, gn)) = gn_map.into_iter().next() {
            gate_network = Some(gn);
        }
    }

    // Phase 4: walk for ContentFile envelopes (universe filtering happens at
    // sync time in `sync_for`, so we keep them all here).
    let mut files = Vec::new();
    walk(root, &mut files);

    LoadedContent {
        files,
        hostile_archetypes: hostile_archetypes.into_values().collect(),
        hostile_locations: hostile_locations.into_values().collect(),
        charted_systems: charted_systems.into_values().collect(),
        gate_network,
    }
}

/// Parse every `.ron` in `dir` into a HashMap<T> keyed by a function.
fn load_typed<T, K>(dir: &Path, key: K) -> HashMap<String, T>
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
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(value) = ron::from_str::<T>(&text) {
                out.insert(key(&value), value);
            }
        }
    }
    out
}

/// Like `load_typed` but merges into an existing map (last-wins).
fn load_typed_into<T, K>(dir: &Path, out: &mut HashMap<String, T>, key: K)
where
    T: serde::de::DeserializeOwned,
    K: Fn(&T) -> String,
{
    let items = load_typed(dir, key);
    for (k, v) in items {
        out.insert(k, v);
    }
}

const FIXTURES_DIR: &str = "_fixtures";

/// Walk `mods/` collecting every `ContentFile` envelope (universe filtering
/// happens later in `sync_for`, so we keep them all here).
fn walk(dir: &Path, out: &mut Vec<ContentFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skip = [
                FIXTURES_DIR,
                "combat",
                "locations",
                "systems",
                "gate_network",
            ];
            if path
                .file_name()
                .is_some_and(|n| skip.contains(&n.to_str().unwrap_or("")))
            {
                continue;
            }
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "ron") {
            if path.file_name().is_some_and(|n| n == "mod.manifest.ron") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(file) = ron::from_str::<ContentFile>(&text) {
                    out.push(file);
                }
            }
        }
    }
}
