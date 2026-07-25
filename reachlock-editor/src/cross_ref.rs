use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::SystemTime;

use reachlock_core::career::CareerPath;
use reachlock_core::combat::location::HostileLocation;
use reachlock_core::combat::HostileArchetype;
use reachlock_core::content::{ContentFile, ContentPayload};
use reachlock_core::faction::FactionCatalog;
use reachlock_core::galaxy::{ChartedSystem, GateNetwork};
use reachlock_core::item::ItemSeed;
use reachlock_core::soul::types::SoulFile;

use crate::app::ContentType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub source_id: String,
    pub source_type: ContentType,
    pub field_path: String,
    pub target_id: String,
}

#[derive(Debug, Clone)]
pub struct IdMeta {
    pub display_name: String,
    pub content_type: ContentType,
}

pub struct CrossReferenceIndex {
    pub incoming: HashMap<String, Vec<Reference>>,
    pub outgoing: HashMap<String, Vec<Reference>>,
    pub all_ids: HashSet<String>,
    pub id_metadata: HashMap<String, IdMeta>,
}

impl CrossReferenceIndex {
    pub fn new() -> Self {
        Self {
            incoming: HashMap::new(),
            outgoing: HashMap::new(),
            all_ids: HashSet::new(),
            id_metadata: HashMap::new(),
        }
    }

    pub fn build(snapshot: &ContentIndexSnapshot) -> Self {
        let mut index = Self::new();

        for file in &snapshot.files {
            index.all_ids.insert(file.id.clone());
            index
                .id_metadata
                .entry(file.id.clone())
                .or_insert_with(|| IdMeta {
                    display_name: file.display_name.clone(),
                    content_type: content_type_from_asset(&file.asset_type),
                });
        }
        for (ct, entries) in &snapshot.typed_ids {
            for (id, display_name) in entries {
                index.all_ids.insert(id.clone());
                index
                    .id_metadata
                    .entry(id.clone())
                    .or_insert_with(|| IdMeta {
                        display_name: display_name.clone(),
                        content_type: *ct,
                    });
            }
        }

        for file in &snapshot.files {
            let source_type = content_type_from_asset(&file.asset_type);
            let refs = extract_refs_from_content_file(file, &source_type);
            for r in &refs {
                index
                    .outgoing
                    .entry(r.source_id.clone())
                    .or_default()
                    .push(r.clone());
                index
                    .incoming
                    .entry(r.target_id.clone())
                    .or_default()
                    .push(r.clone());
            }
        }

        for refs in snapshot.typed_refs.values() {
            for r in refs {
                index
                    .outgoing
                    .entry(r.source_id.clone())
                    .or_default()
                    .push(r.clone());
                index
                    .incoming
                    .entry(r.target_id.clone())
                    .or_default()
                    .push(r.clone());
            }
        }

        index
    }

    pub fn usages_of(&self, id: &str) -> &[Reference] {
        static EMPTY: Vec<Reference> = Vec::new();
        self.incoming.get(id).map_or(&EMPTY, |v| v.as_slice())
    }

    pub fn references_from(&self, id: &str) -> &[Reference] {
        static EMPTY: Vec<Reference> = Vec::new();
        self.outgoing.get(id).map_or(&EMPTY, |v| v.as_slice())
    }

    pub fn is_known(&self, id: &str) -> bool {
        self.all_ids.contains(id)
    }

    pub fn autocomplete(&self, prefix: &str) -> Vec<IdMeta> {
        let lower = prefix.to_lowercase();
        let mut results: Vec<IdMeta> = self
            .id_metadata
            .iter()
            .filter(|(id, meta)| {
                id.to_lowercase().contains(&lower)
                    || meta.display_name.to_lowercase().contains(&lower)
            })
            .map(|(_, meta)| meta.clone())
            .collect();
        results.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        results.truncate(20);
        results
    }

    pub fn rename(&self, old_id: &str, _new_id: &str) -> Vec<Reference> {
        self.incoming.get(old_id).cloned().unwrap_or_default()
    }

    pub fn broken_references(&self) -> Vec<(String, String, String)> {
        let mut broken = Vec::new();
        for (source_id, refs) in &self.outgoing {
            for r in refs {
                if !self.all_ids.contains(&r.target_id) {
                    broken.push((source_id.clone(), r.target_id.clone(), r.field_path.clone()));
                }
            }
        }
        broken.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        broken
    }
}

fn content_type_from_asset(asset: &reachlock_core::content::AssetType) -> ContentType {
    use reachlock_core::content::AssetType;
    match asset {
        AssetType::Hull => ContentType::HullFrame,
        AssetType::Station => ContentType::Station,
        AssetType::Contract => ContentType::Contract,
        AssetType::Ecosystem => ContentType::Ecosystem,
        AssetType::Career => ContentType::Career,
        AssetType::PlanetCulture => ContentType::PlanetCulture,
        AssetType::Recipe => ContentType::Recipe,
        AssetType::Theme => ContentType::Theme,
        AssetType::Trope => ContentType::Trope,
        AssetType::ScriptedEncounter => ContentType::ScriptedEncounter,
        AssetType::Dialogue => ContentType::Dialogue,
        AssetType::Dungeon => ContentType::Dungeon,
        AssetType::Event => ContentType::Event,
        AssetType::Soul => ContentType::Soul,
        AssetType::HullFrame => ContentType::HullFrame,
        AssetType::RoomTemplates => ContentType::RoomTemplates,
    }
}

fn extract_refs_from_content_file(file: &ContentFile, ct: &ContentType) -> Vec<Reference> {
    let mut refs = Vec::new();
    let source_id = file.id.clone();
    let source_type = *ct;

    match &file.payload {
        ContentPayload::Soul(soul) => {
            if !soul.identity.faction_affiliation.is_empty() {
                refs.push(Reference {
                    source_id: source_id.clone(),
                    source_type,
                    field_path: "identity.faction_affiliation".into(),
                    target_id: soul.identity.faction_affiliation.clone(),
                });
            }
            for contract_id in &soul.contracts {
                refs.push(Reference {
                    source_id: source_id.clone(),
                    source_type,
                    field_path: "contracts[]".into(),
                    target_id: contract_id.clone(),
                });
            }
        }
        ContentPayload::Career(career) => {
            if let Some(fid) = &career.faction_id {
                if !fid.is_empty() {
                    refs.push(Reference {
                        source_id: source_id.clone(),
                        source_type,
                        field_path: "faction_id".into(),
                        target_id: fid.clone(),
                    });
                }
            }
            for cpath in &career.conflicting_paths {
                refs.push(Reference {
                    source_id: source_id.clone(),
                    source_type,
                    field_path: "conflicting_paths[]".into(),
                    target_id: cpath.clone(),
                });
            }
        }
        ContentPayload::Contract(_) => {}
        ContentPayload::Hull(_) => {}
        ContentPayload::HullFrame(_) => {}
        ContentPayload::RoomTemplates(_) => {}
        ContentPayload::Station { .. } => {}
        ContentPayload::Ecosystem(_) => {}
        ContentPayload::PlanetCulture(_) => {}
        ContentPayload::Theme(_) => {}
        ContentPayload::Trope(_) => {}
        ContentPayload::ScriptedEncounter(_) => {}
        ContentPayload::Dialogue(dialogue) => {
            for node in &dialogue.nodes {
                for choice in &node.choices {
                    refs.push(Reference {
                        source_id: source_id.clone(),
                        source_type,
                        field_path: format!("nodes[{}].choices[].next_node", node.id),
                        target_id: choice.next_node.clone(),
                    });
                }
            }
        }
        ContentPayload::Dungeon(_) => {}
        ContentPayload::Event(_) => {}
        ContentPayload::Recipe(_) => {}
    }

    refs
}

fn extract_refs_from_soul(
    soul: &SoulFile,
    source_id: &str,
    source_type: ContentType,
) -> Vec<Reference> {
    let mut refs = Vec::new();
    if !soul.identity.faction_affiliation.is_empty() {
        refs.push(Reference {
            source_id: source_id.to_string(),
            source_type,
            field_path: "identity.faction_affiliation".into(),
            target_id: soul.identity.faction_affiliation.clone(),
        });
    }
    for contract_id in &soul.contracts {
        refs.push(Reference {
            source_id: source_id.to_string(),
            source_type,
            field_path: "contracts[]".into(),
            target_id: contract_id.clone(),
        });
    }
    refs
}

fn extract_refs_from_career(
    career: &CareerPath,
    source_id: &str,
    source_type: ContentType,
) -> Vec<Reference> {
    let mut refs = Vec::new();
    if let Some(fid) = &career.faction_id {
        if !fid.is_empty() {
            refs.push(Reference {
                source_id: source_id.to_string(),
                source_type,
                field_path: "faction_id".into(),
                target_id: fid.clone(),
            });
        }
    }
    for cpath in &career.conflicting_paths {
        refs.push(Reference {
            source_id: source_id.to_string(),
            source_type,
            field_path: "conflicting_paths[]".into(),
            target_id: cpath.clone(),
        });
    }
    refs
}

fn extract_refs_from_location(
    loc: &HostileLocation,
    source_id: &str,
    source_type: ContentType,
) -> Vec<Reference> {
    let mut refs = Vec::new();
    for room in &loc.rooms {
        for spawn in &room.spawns {
            if !spawn.archetype.is_empty() {
                refs.push(Reference {
                    source_id: source_id.to_string(),
                    source_type,
                    field_path: format!("rooms[{}].spawns[].archetype", room.id),
                    target_id: spawn.archetype.clone(),
                });
            }
        }
    }
    refs
}

#[derive(Debug, Clone)]
pub struct ContentIndexSnapshot {
    pub files: Vec<ContentFile>,
    pub typed_ids: HashMap<ContentType, Vec<(String, String)>>,
    pub typed_refs: HashMap<ContentType, Vec<Reference>>,
    pub built_at: SystemTime,
}

impl ContentIndexSnapshot {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            typed_ids: HashMap::new(),
            typed_refs: HashMap::new(),
            built_at: SystemTime::now(),
        }
    }

    pub fn from_content_root(root: &Path) -> Self {
        let mut snapshot = Self::new();
        snapshot.built_at = SystemTime::now();

        scan_content_files(root, &mut snapshot);
        scan_typed_souls(root, &mut snapshot);
        scan_typed_factions(root, &mut snapshot);
        scan_typed_careers(root, &mut snapshot);
        scan_typed_items(root, &mut snapshot);
        scan_typed_systems(root, &mut snapshot);
        scan_typed_archetypes(root, &mut snapshot);
        scan_typed_locations(root, &mut snapshot);
        scan_typed_gates(root, &mut snapshot);

        snapshot
    }
}

fn scan_content_files(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let dirs = [
        "hulls",
        "stations",
        "contracts",
        "ecosystems",
        "careers",
        "cultures",
        "themes",
        "tropes",
        "encounters",
        "dialogues",
        "dungeons",
        "events",
        "recipes",
    ];
    for dir in dirs {
        let path = root.join(dir);
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "ron") {
                if let Ok(file) = crate::io::read_ron::<ContentFile>(&p) {
                    snapshot.files.push(file);
                }
            }
        }
    }
}

fn scan_typed_souls(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("souls");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    let ct = ContentType::Soul;
    let refs = snapshot.typed_refs.entry(ct).or_default();
    let ids = snapshot.typed_ids.entry(ct).or_default();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(soul) = crate::io::read_ron::<SoulFile>(&p) {
                ids.push((soul.id.clone(), soul.name.clone()));
                let soul_refs = extract_refs_from_soul(&soul, &soul.id, ct);
                refs.extend(soul_refs);
            }
        }
    }
}

fn scan_typed_factions(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("factions");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(cat) = crate::io::read_ron::<FactionCatalog>(&p) {
                let ids = snapshot.typed_ids.entry(ContentType::Faction).or_default();
                for f in &cat.factions {
                    ids.push((f.id.0.clone(), f.name.clone()));
                }
            }
        }
    }
}

fn scan_typed_careers(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("careers");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    let ct = ContentType::Career;
    let refs = snapshot.typed_refs.entry(ct).or_default();
    let ids = snapshot.typed_ids.entry(ct).or_default();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(career) = crate::io::read_ron::<CareerPath>(&p) {
                ids.push((career.id.clone(), career.name.clone()));
                let career_refs = extract_refs_from_career(&career, &career.id, ct);
                refs.extend(career_refs);
            }
        }
    }
}

fn scan_typed_items(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("items");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    let ct = ContentType::Item;
    let ids = snapshot.typed_ids.entry(ct).or_default();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(item) = crate::io::read_ron::<ItemSeed>(&p) {
                ids.push((
                    format!("item_{:x}", item.seed),
                    format!("{:?}", item.item_type),
                ));
            }
        }
    }
}

fn scan_typed_systems(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("systems");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    let ct = ContentType::ChartedSystem;
    let ids = snapshot.typed_ids.entry(ct).or_default();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(sys) = crate::io::read_ron::<ChartedSystem>(&p) {
                ids.push((sys.id, sys.display_name));
            }
        }
    }
}

fn scan_typed_archetypes(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("combat");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    let ct = ContentType::EnemyArchetype;
    let ids = snapshot.typed_ids.entry(ct).or_default();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(arch) = crate::io::read_ron::<HostileArchetype>(&p) {
                ids.push((arch.id, arch.display_name));
            }
        }
    }
}

fn scan_typed_locations(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("locations");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    let ct = ContentType::Location;
    let refs = snapshot.typed_refs.entry(ct).or_default();
    let ids = snapshot.typed_ids.entry(ct).or_default();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(loc) = crate::io::read_ron::<HostileLocation>(&p) {
                ids.push((loc.id.clone(), loc.display_name.clone()));
                let loc_refs = extract_refs_from_location(&loc, &loc.id, ct);
                refs.extend(loc_refs);
            }
        }
    }
}

fn scan_typed_gates(root: &Path, snapshot: &mut ContentIndexSnapshot) {
    let path = root.join("gate_network");
    let Ok(entries) = std::fs::read_dir(&path) else {
        return;
    };
    let ct = ContentType::GateNetwork;
    let ids = snapshot.typed_ids.entry(ct).or_default();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "ron") {
            if let Ok(gn) = crate::io::read_ron::<GateNetwork>(&p) {
                for gate in &gn.gates {
                    ids.push((
                        gate.from.0.clone(),
                        format!("gate {}→{}", gate.from.0, gate.to.0),
                    ));
                    ids.push((
                        gate.to.0.clone(),
                        format!("gate {}→{}", gate.from.0, gate.to.0),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_index_returns_no_usages() {
        let idx = CrossReferenceIndex::new();
        assert!(idx.usages_of("nonexistent").is_empty());
        assert!(!idx.is_known("nonexistent"));
    }

    #[test]
    fn autocomplete_finds_matching_ids() {
        let mut idx = CrossReferenceIndex::new();
        idx.all_ids.insert("compact".into());
        idx.all_ids.insert("compact_navy".into());
        idx.all_ids.insert("reach_pirates".into());
        idx.id_metadata.insert(
            "compact".into(),
            IdMeta {
                display_name: "Compact".into(),
                content_type: ContentType::Faction,
            },
        );
        idx.id_metadata.insert(
            "compact_navy".into(),
            IdMeta {
                display_name: "Compact Navy".into(),
                content_type: ContentType::Career,
            },
        );
        idx.id_metadata.insert(
            "reach_pirates".into(),
            IdMeta {
                display_name: "Reach Pirates".into(),
                content_type: ContentType::Career,
            },
        );

        let results = idx.autocomplete("comp");
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|m| m.display_name == "Compact"));
        assert!(results.iter().any(|m| m.display_name == "Compact Navy"));
    }

    #[test]
    fn broken_references_detects_missing_targets() {
        let mut idx = CrossReferenceIndex::new();
        idx.all_ids.insert("existing_faction".into());
        idx.outgoing.insert(
            "my_soul".into(),
            vec![Reference {
                source_id: "my_soul".into(),
                source_type: ContentType::Soul,
                field_path: "faction_affiliation".into(),
                target_id: "existing_faction".into(),
            }],
        );
        idx.outgoing.insert(
            "broken_soul".into(),
            vec![Reference {
                source_id: "broken_soul".into(),
                source_type: ContentType::Soul,
                field_path: "faction_affiliation".into(),
                target_id: "missing_faction".into(),
            }],
        );

        let broken = idx.broken_references();
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].0, "broken_soul");
        assert_eq!(broken[0].1, "missing_faction");
    }
}
