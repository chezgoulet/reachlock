use std::collections::HashMap;

use bevy::prelude::*;
use reachlock_core::content::dirs::{classify, DirKind};
use reachlock_core::content::ContentFile;
use reachlock_core::galaxy::{ChartedSystem, GateNetwork};
use reachlock_core::mod_manifest::{resolve_load_order, ModManifest};

/// The local override index (spec §10, "Loader reads `mods/` from disk
/// at startup (local mode)"). Empty when there is no content root to
/// read; server distribution of overrides is S23's problem.
///
/// S22: the loader scans `mods/*/mod.manifest.ron`, resolves load order
/// (topological sort + load_order field), and aggregates typed content
/// from each mod directory. Last-loaded mod wins on collisions.
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
    pub fn find_station_by_seed(&self, seed: u64) -> Option<&ContentFile> {
        self.files
            .iter()
            .find(|f| f.asset_type == reachlock_core::content::AssetType::Station && f.seed == seed)
    }
}

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
        // `themes/` is deliberately absent here. Themes are envelopes; they
        // come in through the walk below and reach the audio engine via
        // `dispatch::consume_themes`. The bare-`Theme` loader that used to sit
        // here could never parse them, and the map it filled had no reader.
    }

    // Phase 4: walk for ContentFile envelopes. Directory classification lives
    // in core so this pass and the typed pass above cannot disagree.
    let mut files = Vec::new();
    walk(root, &mut files, false);

    info!(
        "content index: {} mod(s), {} file(s), {} archetype(s), {} location(s), {} system(s)",
        manifests.len(),
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

/// Walk a content root collecting `ContentFile` envelopes.
///
/// Which directories are envelope directories is
/// [`reachlock_core::content::dirs`]'s call, not this function's: the loader,
/// the content-tree checker and the editor all read the same table, so a
/// directory cannot be envelope-wrapped for one of them and bare for another.
///
/// `mixed` tracks whether we are inside a [`DirKind::Mixed`] directory, where a
/// failed envelope parse is the expected path for the bare files that share the
/// directory and must not warn.
fn walk(dir: &std::path::Path, out: &mut Vec<ContentFile>, mixed: bool) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        warn!("content index: failed to read directory {dir:?}");
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            match classify(name) {
                // Loaded by the typed pass above, embedded at compile time, or
                // not game content at all. Descending would only produce parse
                // failures for files this walk was never meant to read.
                DirKind::Typed | DirKind::External | DirKind::Fixtures => continue,
                DirKind::Mixed => walk(&path, out, true),
                DirKind::Envelope => walk(&path, out, mixed),
            }
        } else if path.extension().is_some_and(|e| e == "ron") {
            // Skip mod manifest files — they're parsed separately.
            if path.file_name().is_some_and(|n| n == "mod.manifest.ron") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => match ron::from_str::<ContentFile>(&text) {
                    Ok(file) => out.push(file),
                    Err(err) if mixed => {
                        debug!(
                            "content index: {} is not an envelope, leaving it to its own loader: {err}",
                            path.display()
                        )
                    }
                    Err(err) => {
                        warn!("content index: failed to parse {}: {err}", path.display())
                    }
                },
                Err(err) => warn!("content index: failed to read {}: {err}", path.display()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::content::AssetType;

    /// The reference mod, from the client crate's working directory.
    fn content_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("mods")
    }

    /// Every `.ron` the walk actually opens must parse as an envelope.
    ///
    /// This is the startup-warning gate. Twelve files used to fail here on
    /// every launch — `economy/` and `factions/` are compile-time embeds the
    /// walk had no business reading, and ten bare `ShipTemplate`s in `hulls/`
    /// belong to the ship catalog. A failure here is a line of `WARN` in every
    /// player's log, and the noise is what let the missing theme hide.
    #[test]
    fn every_file_the_walk_opens_is_an_envelope() {
        let mut offenders = Vec::new();
        visit(&content_root(), false, &mut offenders);

        fn visit(dir: &std::path::Path, mixed: bool, offenders: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    match classify(name) {
                        DirKind::Typed | DirKind::External | DirKind::Fixtures => continue,
                        DirKind::Mixed => visit(&path, true, offenders),
                        DirKind::Envelope => visit(&path, mixed, offenders),
                    }
                } else if path.extension().is_some_and(|e| e == "ron")
                    && path.file_name().is_some_and(|n| n != "mod.manifest.ron")
                    // Inside a Mixed directory a bare file is expected and the
                    // loader logs at debug, not warn.
                    && !mixed
                {
                    let text = std::fs::read_to_string(&path).expect("read");
                    if let Err(err) = ron::from_str::<ContentFile>(&text) {
                        offenders.push(format!("{}: {err}", path.display()));
                    }
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "these files sit in envelope directories but are not envelopes, so the loader \
             warns about each one at startup and skips the content:\n  {}",
            offenders.join("\n  ")
        );
    }

    /// `themes/` was in the walk's skip list, so the one authored theme never
    /// entered the index and never reached the audio engine. Pin it.
    #[test]
    fn authored_themes_reach_the_index() {
        let mut files = Vec::new();
        walk(&content_root(), &mut files, false);
        let themes: Vec<_> = files
            .iter()
            .filter(|f| f.asset_type == AssetType::Theme)
            .collect();
        assert!(
            !themes.is_empty(),
            "no theme reached the content index; the audio engine has nothing to riff on"
        );
        assert!(
            themes.iter().any(|t| t.id == "calm_exploration"),
            "expected the authored calm_exploration theme, got: {:?}",
            themes.iter().map(|t| &t.id).collect::<Vec<_>>()
        );
    }

    /// Souls, origins and crews all ride the envelope path. A regression here
    /// means crew members lose their personalities and fall back to raw ids.
    #[test]
    fn envelope_content_reaches_the_index() {
        let mut files = Vec::new();
        walk(&content_root(), &mut files, false);
        for (asset, least) in [
            (AssetType::Soul, 13),
            (AssetType::Origin, 9),
            (AssetType::Career, 10),
            (AssetType::CrewPackage, 1),
            // `storylines/` holds one storyline arc and one soul-mutation set;
            // the envelope's own asset_type is what sorts them, not the folder.
            (AssetType::Storyline, 1),
            (AssetType::SoulMutations, 1),
        ] {
            let count = files.iter().filter(|f| f.asset_type == asset).count();
            assert!(
                count >= least,
                "expected at least {least} {asset:?} file(s) in the index, found {count}"
            );
        }
    }
}
