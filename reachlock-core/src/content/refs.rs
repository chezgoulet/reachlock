//! Whole-tree reference checking for authored content.
//!
//! Per-file validation (`validate_content`) answers "is this file well
//! formed?". It cannot answer "does the id this file points at exist?",
//! because that is a property of the tree, not the file. So nothing caught
//! ten origins referring to eight ship templates that were never authored:
//! each origin file was individually perfect.
//!
//! This module builds the reference graph and reports the four failures a
//! per-file check structurally cannot see:
//!
//!   1. **Dangling** — a reference to an id nothing defines.
//!   2. **Duplicate** — one id defined twice within a kind, so which file
//!      wins depends on directory iteration order.
//!   3. **Unparseable** — a file in a content directory that is not any known
//!      payload. These are the dangerous ones: loaders skip what they cannot
//!      parse, so the content silently is not in the game.
//!   4. **Orphan** — defined, referenced by nothing. Not an error; a lead.
//!
//! # Why references are typed
//!
//! Every target carries a [`RefKind`]. An origin's `ship_template` must name
//! a *ship template*; it is not satisfied by a soul that happens to share the
//! id. Untyped id-set membership would call the tree healthy the moment any
//! file anywhere used the same string.
//!
//! # What is deliberately not a reference
//!
//! `soul.identity.faction_affiliation` is **prose**, not an id — real values
//! include `"Sorrow Station (independent, ISC-adjacent)"`. Treating it as a
//! reference (as the editor's cross-reference index does) reports every soul
//! in the tree as broken. `origin.starting_gear[].item_id` is likewise not
//! checked: items are generated from seeds and have no authored id space.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::combat::location::HostileLocation;
use crate::combat::HostileArchetype;
use crate::content::{AssetType, ContentFile, ContentPayload};
use crate::crew::ShipTemplate;
use crate::faction::FactionCatalog;
use crate::galaxy::ChartedSystem;

/// What kind of thing an id names. A reference is satisfied only by a
/// definition of the *same* kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefKind {
    Origin,
    Soul,
    Career,
    Faction,
    ShipTemplate,
    Crew,
    System,
    Archetype,
    Location,
    Station,
    Contract,
    Ecosystem,
    Culture,
    Theme,
    Trope,
    Encounter,
    Dialogue,
    Dungeon,
    Event,
    Recipe,
    Hull,
    HullFrame,
    RoomTemplates,
}

impl RefKind {
    pub fn label(self) -> &'static str {
        match self {
            RefKind::Origin => "origin",
            RefKind::Soul => "soul",
            RefKind::Career => "career",
            RefKind::Faction => "faction",
            RefKind::ShipTemplate => "ship template",
            RefKind::Crew => "crew package",
            RefKind::System => "system",
            RefKind::Archetype => "enemy archetype",
            RefKind::Location => "location",
            RefKind::Station => "station",
            RefKind::Contract => "contract",
            RefKind::Ecosystem => "ecosystem",
            RefKind::Culture => "planet culture",
            RefKind::Theme => "theme",
            RefKind::Trope => "trope",
            RefKind::Encounter => "scripted encounter",
            RefKind::Dialogue => "dialogue",
            RefKind::Dungeon => "dungeon",
            RefKind::Event => "event",
            RefKind::Recipe => "recipe",
            RefKind::Hull => "hull",
            RefKind::HullFrame => "hull frame",
            RefKind::RoomTemplates => "room templates",
        }
    }

    fn from_asset(asset: AssetType) -> Self {
        match asset {
            AssetType::Hull => RefKind::Hull,
            AssetType::Station => RefKind::Station,
            AssetType::Contract => RefKind::Contract,
            AssetType::Ecosystem => RefKind::Ecosystem,
            AssetType::Career => RefKind::Career,
            AssetType::PlanetCulture => RefKind::Culture,
            AssetType::Recipe => RefKind::Recipe,
            AssetType::Theme => RefKind::Theme,
            AssetType::Trope => RefKind::Trope,
            AssetType::ScriptedEncounter => RefKind::Encounter,
            AssetType::Dialogue => RefKind::Dialogue,
            AssetType::Dungeon => RefKind::Dungeon,
            AssetType::Event => RefKind::Event,
            AssetType::Soul => RefKind::Soul,
            AssetType::HullFrame => RefKind::HullFrame,
            AssetType::RoomTemplates => RefKind::RoomTemplates,
            AssetType::Origin => RefKind::Origin,
            AssetType::CrewPackage => RefKind::Crew,
            AssetType::SoulMutations => RefKind::Soul,
            AssetType::Storyline => RefKind::Encounter,
        }
    }
}

/// An id the tree defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub id: String,
    pub kind: RefKind,
    pub file: PathBuf,
}

/// One id pointing at another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ref {
    pub source_id: String,
    pub source_file: PathBuf,
    /// Field that holds the reference, e.g. `ship_template`.
    pub field: String,
    pub target_id: String,
    pub target_kind: RefKind,
}

/// A file inside a content directory that is not any known payload.
///
/// Loaders skip these, so the content is absent from the game with no error
/// at the point of use — which is why this is a hard failure here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unparseable {
    pub file: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct ContentTree {
    pub definitions: Vec<Definition>,
    pub references: Vec<Ref>,
    pub unparseable: Vec<Unparseable>,
}

/// The verdict. Empty everywhere means the tree is internally consistent.
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    /// References with no definition of the required kind.
    pub dangling: Vec<Ref>,
    /// `(kind, id, files)` — one id defined more than once within a kind.
    pub duplicates: Vec<(RefKind, String, Vec<PathBuf>)>,
    pub unparseable: Vec<Unparseable>,
    /// Defined but referenced by nothing. Informational.
    pub orphans: Vec<Definition>,
}

impl CheckReport {
    /// Orphans are excluded: unreferenced content is normal (a system nothing
    /// points at is still a place you can fly to).
    pub fn is_clean(&self) -> bool {
        self.dangling.is_empty() && self.duplicates.is_empty() && self.unparseable.is_empty()
    }
}

impl ContentTree {
    /// Walk a content root. Missing directories are skipped, so this works on
    /// a partial tree and on a mod that defines only one content type.
    pub fn scan(root: &Path) -> Self {
        let mut tree = Self::default();

        // Directories whose files are `ContentFile` envelopes. The envelope's
        // own `asset_type` decides the kind, so a file in the "wrong" folder
        // is still indexed correctly.
        //
        // The list comes from `super::dirs` rather than a copy kept here: this
        // checker and the client's loader disagreeing about a directory is
        // exactly how the authored theme went missing while the tree still
        // reported clean.
        for dir in super::dirs::envelope_dirs() {
            tree.scan_envelopes(&root.join(dir));
        }

        // `hulls/` is mixed: ship templates, hull frames, room templates, and
        // envelope-wrapped hulls all live there. Try the bare types first,
        // then fall back to the envelope.
        tree.scan_hulls(&root.join("hulls"));

        // Bare-typed directories: the file *is* the payload, no envelope.
        tree.scan_bare::<FactionCatalog>(&root.join("factions"), &mut |cat, path, t| {
            for f in &cat.factions {
                t.define(&f.id.0, RefKind::Faction, path);
            }
        });
        tree.scan_bare::<ChartedSystem>(&root.join("systems"), &mut |sys, path, t| {
            t.define(&sys.id, RefKind::System, path);
        });
        tree.scan_bare::<HostileArchetype>(&root.join("combat"), &mut |arch, path, t| {
            t.define(&arch.id, RefKind::Archetype, path);
        });
        tree.scan_bare::<HostileLocation>(&root.join("locations"), &mut |loc, path, t| {
            t.define(&loc.id, RefKind::Location, path);
            for room in &loc.rooms {
                for spawn in &room.spawns {
                    if spawn.archetype.is_empty() {
                        continue;
                    }
                    t.reference(
                        &loc.id,
                        path,
                        &format!("rooms[{}].spawns[].archetype", room.id),
                        &spawn.archetype,
                        RefKind::Archetype,
                    );
                }
            }
        });

        tree.definitions.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.file.cmp(&b.file))
        });
        tree.references.sort_by(|a, b| {
            a.source_id
                .cmp(&b.source_id)
                .then_with(|| a.field.cmp(&b.field))
                .then_with(|| a.target_id.cmp(&b.target_id))
        });
        tree.unparseable.sort_by(|a, b| a.file.cmp(&b.file));
        tree
    }

    pub fn check(&self) -> CheckReport {
        let defined: BTreeSet<(RefKind, &str)> = self
            .definitions
            .iter()
            .map(|d| (d.kind, d.id.as_str()))
            .collect();

        let dangling = self
            .references
            .iter()
            .filter(|r| !defined.contains(&(r.target_kind, r.target_id.as_str())))
            .cloned()
            .collect();

        let mut by_id: BTreeMap<(RefKind, &str), Vec<PathBuf>> = BTreeMap::new();
        for d in &self.definitions {
            by_id
                .entry((d.kind, d.id.as_str()))
                .or_default()
                .push(d.file.clone());
        }
        let duplicates = by_id
            .iter()
            .filter(|(_, files)| files.len() > 1)
            .map(|((kind, id), files)| (*kind, (*id).to_string(), files.clone()))
            .collect();

        let referenced: BTreeSet<(RefKind, &str)> = self
            .references
            .iter()
            .map(|r| (r.target_kind, r.target_id.as_str()))
            .collect();
        let orphans = self
            .definitions
            .iter()
            .filter(|d| !referenced.contains(&(d.kind, d.id.as_str())))
            .cloned()
            .collect();

        CheckReport {
            dangling,
            duplicates,
            unparseable: self.unparseable.clone(),
            orphans,
        }
    }

    fn define(&mut self, id: &str, kind: RefKind, file: &Path) {
        self.definitions.push(Definition {
            id: id.to_string(),
            kind,
            file: file.to_path_buf(),
        });
    }

    fn reference(
        &mut self,
        source_id: &str,
        source_file: &Path,
        field: &str,
        target_id: &str,
        target_kind: RefKind,
    ) {
        self.references.push(Ref {
            source_id: source_id.to_string(),
            source_file: source_file.to_path_buf(),
            field: field.to_string(),
            target_id: target_id.to_string(),
            target_kind,
        });
    }

    fn scan_envelopes(&mut self, dir: &Path) {
        for (path, text) in ron_files(dir) {
            match ron::from_str::<ContentFile>(&text) {
                Ok(file) => {
                    self.define(&file.id, RefKind::from_asset(file.asset_type), &path);
                    self.extract_payload_refs(&file, &path);
                }
                Err(e) => self.unparseable.push(Unparseable {
                    file: path,
                    reason: format!("not a ContentFile: {e}"),
                }),
            }
        }
    }

    fn scan_hulls(&mut self, dir: &Path) {
        for (path, text) in ron_files(dir) {
            if let Ok(t) = ron::from_str::<ShipTemplate>(&text) {
                self.define(&t.id, RefKind::ShipTemplate, &path);
                continue;
            }
            match ron::from_str::<ContentFile>(&text) {
                Ok(file) => self.define(&file.id, RefKind::from_asset(file.asset_type), &path),
                Err(e) => self.unparseable.push(Unparseable {
                    file: path,
                    reason: format!("neither a ShipTemplate nor a ContentFile: {e}"),
                }),
            }
        }
    }

    fn scan_bare<T>(&mut self, dir: &Path, f: &mut dyn FnMut(&T, &Path, &mut Self))
    where
        T: serde::de::DeserializeOwned,
    {
        for (path, text) in ron_files(dir) {
            match ron::from_str::<T>(&text) {
                Ok(value) => f(&value, &path, self),
                Err(e) => self.unparseable.push(Unparseable {
                    file: path,
                    reason: format!("does not parse as {}: {e}", std::any::type_name::<T>()),
                }),
            }
        }
    }

    fn extract_payload_refs(&mut self, file: &ContentFile, path: &Path) {
        let id = file.id.clone();
        match &file.payload {
            ContentPayload::Origin(origin) => {
                if !origin.starting_career.is_empty() {
                    self.reference(
                        &id,
                        path,
                        "starting_career",
                        &origin.starting_career,
                        RefKind::Career,
                    );
                }
                for delta in &origin.faction_deltas {
                    self.reference(
                        &id,
                        path,
                        "faction_deltas[].faction_id",
                        &delta.faction_id,
                        RefKind::Faction,
                    );
                }
                if let Some(ship) = &origin.ship_template {
                    self.reference(&id, path, "ship_template", ship, RefKind::ShipTemplate);
                }
                for member in &origin.starting_crew {
                    if let crate::content::CrewAssignment::Authored { soul_id, .. } = member {
                        self.reference(
                            &id,
                            path,
                            "starting_crew[].soul_id",
                            soul_id,
                            RefKind::Soul,
                        );
                    }
                }
            }
            ContentPayload::CrewPackage(pkg) => {
                for m in &pkg.members {
                    self.reference(&id, path, "members[].soul_id", &m.soul_id, RefKind::Soul);
                }
            }
            ContentPayload::Career(career) => {
                if let Some(fid) = career.faction_id.as_ref().filter(|f| !f.is_empty()) {
                    self.reference(&id, path, "faction_id", fid, RefKind::Faction);
                }
                for other in &career.conflicting_paths {
                    self.reference(&id, path, "conflicting_paths[]", other, RefKind::Career);
                }
            }
            // Dialogue `next_node` targets are node ids *within* the file, not
            // tree-wide ids, so they are checked here rather than added to the
            // graph — a graph edge would report every node as dangling.
            ContentPayload::Dialogue(dialogue) => {
                let nodes: BTreeSet<&str> = dialogue.nodes.iter().map(|n| n.id.as_str()).collect();
                for node in &dialogue.nodes {
                    for choice in &node.choices {
                        if choice.next_node.is_empty() || nodes.contains(choice.next_node.as_str())
                        {
                            continue;
                        }
                        self.unparseable.push(Unparseable {
                            file: path.to_path_buf(),
                            reason: format!(
                                "node \"{}\" has a choice pointing at \"{}\", which is not a node in this dialogue",
                                node.id, choice.next_node
                            ),
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

/// `.ron` files directly inside `dir`, sorted so output is stable. A missing
/// directory yields nothing: a mod need not define every content type.
fn ron_files(dir: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            out.push((path, text));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_with(defs: Vec<(RefKind, &str)>, refs: Vec<(RefKind, &str)>) -> ContentTree {
        ContentTree {
            definitions: defs
                .into_iter()
                .map(|(kind, id)| Definition {
                    id: id.into(),
                    kind,
                    file: PathBuf::from("t.ron"),
                })
                .collect(),
            references: refs
                .into_iter()
                .map(|(target_kind, target_id)| Ref {
                    source_id: "src".into(),
                    source_file: PathBuf::from("t.ron"),
                    field: "f".into(),
                    target_id: target_id.into(),
                    target_kind,
                })
                .collect(),
            unparseable: Vec::new(),
        }
    }

    #[test]
    fn resolved_reference_is_clean() {
        let tree = tree_with(
            vec![(RefKind::ShipTemplate, "corvette")],
            vec![(RefKind::ShipTemplate, "corvette")],
        );
        assert!(tree.check().is_clean());
    }

    #[test]
    fn dangling_reference_is_reported() {
        let tree = tree_with(vec![], vec![(RefKind::ShipTemplate, "corvette")]);
        let report = tree.check();
        assert!(!report.is_clean());
        assert_eq!(report.dangling.len(), 1);
        assert_eq!(report.dangling[0].target_id, "corvette");
    }

    /// The reason references carry a kind: an id defined as one thing must not
    /// satisfy a reference that needs another. Without this, authoring a soul
    /// named `corvette_mk2` would "fix" a missing ship template.
    #[test]
    fn same_id_of_the_wrong_kind_does_not_satisfy_a_reference() {
        let tree = tree_with(
            vec![(RefKind::Soul, "corvette")],
            vec![(RefKind::ShipTemplate, "corvette")],
        );
        let report = tree.check();
        assert_eq!(report.dangling.len(), 1, "soul must not satisfy a ship ref");
    }

    #[test]
    fn duplicate_id_within_a_kind_is_reported() {
        let mut tree = tree_with(vec![(RefKind::Soul, "tib"), (RefKind::Soul, "tib")], vec![]);
        tree.definitions[1].file = PathBuf::from("other.ron");
        let report = tree.check();
        assert_eq!(report.duplicates.len(), 1);
        assert_eq!(report.duplicates[0].2.len(), 2);
    }

    /// Same id in two different kinds is legal — a faction and a career may
    /// both be called `compact`.
    #[test]
    fn same_id_across_kinds_is_not_a_duplicate() {
        let tree = tree_with(
            vec![(RefKind::Faction, "compact"), (RefKind::Career, "compact")],
            vec![],
        );
        assert!(tree.check().duplicates.is_empty());
    }

    #[test]
    fn unreferenced_definition_is_an_orphan_but_still_clean() {
        let tree = tree_with(vec![(RefKind::System, "aethon")], vec![]);
        let report = tree.check();
        assert_eq!(report.orphans.len(), 1);
        assert!(report.is_clean(), "orphans must not fail the check");
    }

    #[test]
    fn unparseable_file_fails_the_check() {
        let tree = ContentTree {
            unparseable: vec![Unparseable {
                file: PathBuf::from("broken.ron"),
                reason: "boom".into(),
            }],
            ..Default::default()
        };
        assert!(!tree.check().is_clean());
    }

    #[test]
    fn missing_directory_scans_to_an_empty_tree() {
        let tree = ContentTree::scan(Path::new("/nonexistent/content/root"));
        assert!(tree.definitions.is_empty());
        assert!(tree.check().is_clean());
    }

    /// Every `AssetType` maps to a `RefKind`. A new asset type that forgets
    /// this fails to compile rather than silently going unchecked.
    #[test]
    fn every_asset_type_has_a_ref_kind() {
        for asset in AssetType::ALL {
            let kind = RefKind::from_asset(asset);
            assert!(!kind.label().is_empty());
        }
    }
}
