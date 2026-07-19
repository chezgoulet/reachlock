use serde::{Deserialize, Serialize};

/// A content addition — a new item that this mod provides. Two mods may
/// add different items with the same type+id; last-loaded wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentAdd {
    #[serde(rename = "type")]
    pub content_type: String,
    pub id: String,
}

/// An override of existing content. `load_order` within the manifest
/// controls whether this override wins over other mods' overrides.
/// Official ReachLock content uses load_order 0; community mods use 1-255.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentOverride {
    pub system_id: String,
    pub object_id: String,
    /// Optional load-order hint (0-255). Defaults to 128. Higher =
    /// loaded later = wins collisions. Only compared per mod; not a
    /// global ordering.
    #[serde(default = "default_load_order")]
    pub load_order: u8,
}

fn default_load_order() -> u8 {
    128
}

/// A mod's manifest — the contract between the mod packer and the loader.
/// Every mod directory contains exactly one `mod.manifest.ron` at its root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModManifest {
    pub id: String,
    pub name: String,
    /// Semver major.minor.patch.
    pub version: (u16, u16, u16),
    pub author: String,
    pub description: String,
    /// Mod ids this mod depends on. Loaded before this mod.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Mod ids that are incompatible with this mod. Refuse to co-load.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Load order hint (0-255). Official mod uses 0. Defaults to 128.
    #[serde(default = "default_load_order")]
    pub load_order: u8,
    /// Content items this mod adds.
    #[serde(default)]
    pub content_adds: Vec<ContentAdd>,
    /// Content overrides this mod applies.
    #[serde(default)]
    pub content_overrides: Vec<ContentOverride>,
}

/// Resolution of load order over a set of mods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOrderError {
    /// A dependency is missing from the mod set.
    MissingDependency { mod_id: String, dependency: String },
    /// A dependency cycle would prevent ordering.
    DependencyCycle { mod_id: String },
    /// A declared conflict is present in the mod set.
    ConflictingMod { mod_id: String, conflicting: String },
}

/// Resolve a flat mod list into a load order that respects dependencies,
/// conflicts, and load_order hints. Returns the sorted mod ids, or the
/// first error.
///
/// Rules:
/// 1. Official mod (id="reachlock") always loads first.
/// 2. Remaining mods are topologically sorted by dependency.
/// 3. Within the same dependency level, higher load_order = later.
/// 4. If any mod declares a conflict with another present mod, error.
pub fn resolve_load_order(manifests: &[ModManifest]) -> Result<Vec<String>, LoadOrderError> {
    let official_id = "reachlock";
    let mut remaining: Vec<&ModManifest> =
        manifests.iter().filter(|m| m.id != official_id).collect();

    // Check conflicts first.
    for m in manifests {
        for conflict_id in &m.conflicts {
            if manifests.iter().any(|other| &other.id == conflict_id) {
                return Err(LoadOrderError::ConflictingMod {
                    mod_id: m.id.clone(),
                    conflicting: conflict_id.clone(),
                });
            }
        }
    }

    // Topological sort (Kahn's algorithm).
    let mut sorted: Vec<String> = Vec::new();
    // First group: official mod if present.
    if manifests.iter().any(|m| m.id == official_id) {
        sorted.push(official_id.to_string());
    }

    // Iteratively pick mods whose deps are all already sorted.
    while !remaining.is_empty() {
        let ready: Vec<&ModManifest> = remaining
            .iter()
            .filter(|m| {
                m.dependencies
                    .iter()
                    .all(|dep| dep.as_str() == official_id || sorted.contains(dep))
            })
            .copied()
            .collect();

        if ready.is_empty() {
            // Deadlock — a remaining mod has an unsatisfied dep or cycle.
            if let Some(stuck) = remaining.first() {
                let missing = stuck
                    .dependencies
                    .iter()
                    .find(|dep| dep.as_str() != official_id && !sorted.contains(dep));
                return Err(LoadOrderError::MissingDependency {
                    mod_id: stuck.id.clone(),
                    dependency: missing.cloned().unwrap_or_default(),
                });
            }
        }

        // Sort ready mods by load_order ascending (lower = earlier).
        let mut ready_sorted = ready;
        ready_sorted.sort_by_key(|m| m.load_order);

        for m in ready_sorted {
            sorted.push(m.id.clone());
            remaining.retain(|r| r.id != m.id);
        }
    }

    Ok(sorted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, deps: &[&str]) -> ModManifest {
        ModManifest {
            id: id.into(),
            name: id.into(),
            version: (1, 0, 0),
            author: "test".into(),
            description: String::new(),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            conflicts: Vec::new(),
            load_order: 128,
            content_adds: Vec::new(),
            content_overrides: Vec::new(),
        }
    }

    #[test]
    fn official_is_first() {
        let ms = vec![manifest("duskway", &[]), manifest("reachlock", &[])];
        let order = resolve_load_order(&ms).unwrap();
        assert_eq!(order[0], "reachlock");
    }

    #[test]
    fn dependency_order() {
        let ms = vec![
            manifest("base", &[]),
            manifest("ext", &["base"]),
            manifest("reachlock", &[]),
        ];
        let order = resolve_load_order(&ms).unwrap();
        let base_idx = order.iter().position(|s| s == "base").unwrap();
        let ext_idx = order.iter().position(|s| s == "ext").unwrap();
        assert!(base_idx < ext_idx, "base must load before ext");
    }

    #[test]
    fn missing_dep_is_error() {
        let ms = vec![manifest("orphan", &["missing"])];
        assert!(matches!(
            resolve_load_order(&ms),
            Err(LoadOrderError::MissingDependency { .. })
        ));
    }

    #[test]
    fn conflict_blocks_load() {
        let mut a = manifest("a", &[]);
        a.conflicts = vec!["b".into()];
        let b = manifest("b", &[]);
        assert!(matches!(
            resolve_load_order(&[a, b]),
            Err(LoadOrderError::ConflictingMod { .. })
        ));
    }

    #[test]
    fn load_order_sorting() {
        let mut high = manifest("promoted", &[]);
        high.load_order = 200;
        let mut low = manifest("demoted", &[]);
        low.load_order = 50;
        let ms = vec![high, low, manifest("reachlock", &[])];
        let order = resolve_load_order(&ms).unwrap();
        let promoted = order.iter().position(|s| s == "promoted").unwrap();
        let demoted = order.iter().position(|s| s == "demoted").unwrap();
        assert!(demoted < promoted, "higher load_order must load later");
    }

    #[test]
    fn round_trips_through_ron() {
        let m = ModManifest {
            id: "test_mod".into(),
            name: "Test Mod".into(),
            version: (1, 2, 3),
            author: "Tester".into(),
            description: "A test.".into(),
            dependencies: vec!["base".into()],
            conflicts: vec!["broken".into()],
            load_order: 64,
            content_adds: vec![ContentAdd {
                content_type: "station".into(),
                id: "my_station".into(),
            }],
            content_overrides: vec![ContentOverride {
                system_id: "aethon".into(),
                object_id: "cargo_market".into(),
                load_order: 64,
            }],
        };
        let ron = ron::to_string(&m).unwrap();
        let back: ModManifest = ron::from_str(&ron).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn empty_deps_is_ok() {
        let ms = vec![manifest("standalone", &[])];
        let order = resolve_load_order(&ms).unwrap();
        assert_eq!(order, vec!["standalone"]);
    }
}
