//! Structural integrity checks for authored content (spec §10, Stage 2:
//! CLI Validation — "no degenerate triangles", "door connectors reference
//! valid rooms", seed range). Pure functions over the deserialized
//! envelope, so they're testable in core and reusable from the CLI without
//! duplicating logic.
//!
//! JSON Schema validation (the RON→JSON projection against
//! `content/schemas/*.schema.json`) deliberately does NOT live here: that
//! needs a schema-validation dependency that has no business being
//! wasm-safe core, so it lives in `reachlock-cli`. These checks are the
//! part that's pure data-shape reasoning over structs core already owns.

use super::envelope::{ContentFile, ContentPayload};
use crate::generator::{GeneratedLayout, GeneratedMesh};
use crate::seed::types::Seed;
use crate::universe::tier::UniverseTier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    SeedOutOfRange {
        seed: u64,
    },
    DegenerateTriangle {
        asset: String,
        triangle_index: usize,
    },
    DanglingDoor {
        room_count: u32,
        door_from: u32,
        door_to: u32,
    },
    DanglingNpc {
        npc_index: usize,
        room_index: usize,
        room_count: usize,
    },
    UnknownUniverse {
        universe: String,
    },
    /// S13: a soul's fixed-point value sits outside its documented range
    /// (intensity/familiarity/weight 0..=1024, trust -1024..=1024).
    SoulValueOutOfRange {
        field: String,
        value: i64,
    },
    /// S13: a soul file whose envelope id and payload id disagree — the
    /// crew roster and save state key souls by id, so this must be one id.
    SoulIdMismatch {
        envelope_id: String,
        soul_id: String,
    },
    /// S16: the soul's authored dialogue graph is structurally broken
    /// (missing start node, dangling `next`, duplicate ids).
    DialogueGraphProblem {
        problem: String,
    },
    /// S17: a hull frame reuses an id within one namespace (slot / zone /
    /// decal slot) — configs reference these by id, so they must be unique.
    DuplicateFrameId {
        namespace: &'static str,
        id: String,
    },
    /// S18: two room templates share an id or a realized `RoomKind` —
    /// placements reference templates by id, and crew duty rooms map by
    /// kind, so both must be unique within the set.
    DuplicateRoomTemplate {
        namespace: &'static str,
        id: String,
    },
    /// S18: a room template with a zero-cell dimension can't be placed.
    ZeroSizeRoomTemplate {
        id: String,
    },
    /// S42: a career path has a structural problem (duplicate rank, etc.).
    CareerProblem {
        problem: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::SeedOutOfRange { seed } => write!(
                f,
                "seed {seed:#x} exceeds the 53-bit range (max {:#x})",
                Seed::MAX
            ),
            ValidationError::DegenerateTriangle {
                asset,
                triangle_index,
            } => write!(f, "{asset}: degenerate triangle at index {triangle_index}"),
            ValidationError::DanglingDoor {
                room_count,
                door_from,
                door_to,
            } => write!(
                f,
                "door {door_from} -> {door_to} references a room outside the layout \
                 (only {room_count} room(s) defined)"
            ),
            ValidationError::DanglingNpc {
                npc_index,
                room_index,
                room_count,
            } => write!(
                f,
                "npc #{npc_index} ({room_index}) references room {room_index} outside the \
                 layout (only {room_count} room(s) defined)"
            ),
            ValidationError::UnknownUniverse { universe } => write!(
                f,
                "unknown universe {universe:?} (expected \"all\" or a universe tier name)"
            ),
            ValidationError::SoulValueOutOfRange { field, value } => write!(
                f,
                "soul field {field} = {value} outside its fixed-point range"
            ),
            ValidationError::SoulIdMismatch {
                envelope_id,
                soul_id,
            } => write!(
                f,
                "envelope id {envelope_id:?} != soul id {soul_id:?} (souls are keyed by one id)"
            ),
            ValidationError::DialogueGraphProblem { problem } => {
                write!(f, "dialogue graph: {problem}")
            }
            ValidationError::DuplicateFrameId { namespace, id } => {
                write!(f, "hull frame: duplicate {namespace} id {id:?}")
            }
            ValidationError::DuplicateRoomTemplate { namespace, id } => {
                write!(f, "room templates: duplicate {namespace} {id:?}")
            }
            ValidationError::ZeroSizeRoomTemplate { id } => {
                write!(f, "room template {id:?} has a zero-cell dimension")
            }
            ValidationError::CareerProblem { problem } => {
                write!(f, "career path: {problem}")
            }
        }
    }
}

/// Run every structural integrity check on a content file. Returns every
/// violation found (not just the first) so `content validate` can report
/// them all in one pass.
pub fn validate_content(content: &ContentFile) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    if content.seed > Seed::MAX {
        errors.push(ValidationError::SeedOutOfRange { seed: content.seed });
    }

    if content.universe != "all" && content.universe.parse::<UniverseTier>().is_err() {
        errors.push(ValidationError::UnknownUniverse {
            universe: content.universe.clone(),
        });
    }

    match &content.payload {
        ContentPayload::Hull(mesh) => errors.extend(check_mesh(&content.id, mesh)),
        ContentPayload::Station {
            exterior,
            layout,
            npc_spawns,
            ..
        } => {
            errors.extend(check_mesh(&content.id, exterior));
            errors.extend(check_doors(layout));
            for (i, npc) in npc_spawns.iter().enumerate() {
                if npc.room_index >= layout.rooms.len() {
                    errors.push(ValidationError::DanglingNpc {
                        npc_index: i,
                        room_index: npc.room_index,
                        room_count: layout.rooms.len(),
                    });
                }
            }
        }
        ContentPayload::Contract(_) => {}
        ContentPayload::PlanetCulture(_) => {}
        ContentPayload::Ecosystem(_) => {
            // Structural checks for ecosystems are schema-side for now.
        }
        ContentPayload::Career(career) => {
            let mut seen_ranks = std::collections::HashSet::new();
            for r in &career.ranks {
                if !seen_ranks.insert(r.rank) {
                    errors.push(ValidationError::CareerProblem {
                        problem: format!("duplicate rank {}", r.rank),
                    });
                }
            }
        }
        ContentPayload::Soul(soul) => {
            if soul.id != content.id {
                errors.push(ValidationError::SoulIdMismatch {
                    envelope_id: content.id.clone(),
                    soul_id: soul.id.clone(),
                });
            }
            let mut range = |field: &str, value: i64, lo: i64| {
                if value < lo || value > 1024 {
                    errors.push(ValidationError::SoulValueOutOfRange {
                        field: field.to_string(),
                        value,
                    });
                }
            };
            range(
                "emotional_state.intensity",
                soul.emotional_state.intensity,
                0,
            );
            for t in &soul.emotional_state.triggers {
                range("trigger.intensity", t.intensity, 0);
            }
            for m in &soul.memory_tree {
                range("memory.emotional_weight", m.emotional_weight, 0);
            }
            for r in &soul.relationship_graph {
                range(
                    &format!("relationship.{}.trust", r.target_id),
                    r.trust,
                    -1024,
                );
                range(
                    &format!("relationship.{}.familiarity", r.target_id),
                    r.familiarity,
                    0,
                );
            }
            if let Some(graph) = &soul.dialogue {
                for problem in graph.validate() {
                    errors.push(ValidationError::DialogueGraphProblem { problem });
                }
            }
        }
        ContentPayload::HullFrame(frame) => {
            let mut check_unique = |namespace: &'static str, ids: Vec<&String>| {
                let mut seen = std::collections::BTreeSet::new();
                for id in ids {
                    if !seen.insert(id.clone()) {
                        errors.push(ValidationError::DuplicateFrameId {
                            namespace,
                            id: id.clone(),
                        });
                    }
                }
            };
            check_unique("slot", frame.slots.iter().map(|s| &s.id).collect());
            check_unique("zone", frame.zones.iter().map(|z| &z.id).collect());
            check_unique("decal slot", frame.decal_slots.iter().collect());
        }
        ContentPayload::RoomTemplates(templates) => {
            let mut ids = std::collections::BTreeSet::new();
            let mut kinds = std::collections::BTreeSet::new();
            for tpl in templates {
                if !ids.insert(tpl.id.clone()) {
                    errors.push(ValidationError::DuplicateRoomTemplate {
                        namespace: "id",
                        id: tpl.id.clone(),
                    });
                }
                // RoomKind isn't Ord; the debug token is a stable stand-in.
                if !kinds.insert(format!("{:?}", tpl.kind)) {
                    errors.push(ValidationError::DuplicateRoomTemplate {
                        namespace: "kind",
                        id: tpl.id.clone(),
                    });
                }
                if tpl.width == 0 || tpl.height == 0 {
                    errors.push(ValidationError::ZeroSizeRoomTemplate { id: tpl.id.clone() });
                }
            }
        }
    }

    errors
}

/// Degenerate = zero-area triangle: a repeated vertex index, or three
/// distinct vertices that are collinear. Twice the signed area is integer
/// and exact under `Fixed`, so no epsilon comparison is needed.
fn check_mesh(asset: &str, mesh: &GeneratedMesh) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for (i, tri) in mesh.indices.chunks_exact(3).enumerate() {
        let (a, b, c) = (tri[0], tri[1], tri[2]);
        if a == b || b == c || a == c {
            errors.push(ValidationError::DegenerateTriangle {
                asset: asset.into(),
                triangle_index: i,
            });
            continue;
        }
        let verts = (
            mesh.vertices.get(a as usize),
            mesh.vertices.get(b as usize),
            mesh.vertices.get(c as usize),
        );
        if let (Some(va), Some(vb), Some(vc)) = verts {
            let area2 =
                (vb.x.0 - va.x.0) * (vc.y.0 - va.y.0) - (vc.x.0 - va.x.0) * (vb.y.0 - va.y.0);
            if area2 == 0 {
                errors.push(ValidationError::DegenerateTriangle {
                    asset: asset.into(),
                    triangle_index: i,
                });
            }
        }
    }
    errors
}

fn check_doors(layout: &GeneratedLayout) -> Vec<ValidationError> {
    let room_count = layout.rooms.len() as u32;
    layout
        .doors
        .iter()
        .filter(|d| d.from >= room_count || d.to >= room_count)
        .map(|d| ValidationError::DanglingDoor {
            room_count,
            door_from: d.from,
            door_to: d.to,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::envelope::AssetType;
    use crate::content::priority::Priority;
    use crate::generator::{Door, FixedVec2, Room, RoomKind};
    use crate::util::rng::Fixed;

    fn triangle_mesh() -> GeneratedMesh {
        GeneratedMesh {
            vertices: vec![
                FixedVec2 {
                    x: Fixed(0),
                    y: Fixed(0),
                },
                FixedVec2 {
                    x: Fixed(1024),
                    y: Fixed(0),
                },
                FixedVec2 {
                    x: Fixed(0),
                    y: Fixed(1024),
                },
            ],
            indices: vec![0, 1, 2],
        }
    }

    fn hull_file(seed: u64, mesh: GeneratedMesh) -> ContentFile {
        ContentFile {
            id: "test_hull".into(),
            display_name: "Test Hull".into(),
            asset_type: AssetType::Hull,
            seed,
            universe: "all".into(),
            priority: Priority::Curated,
            expires_at: None,
            payload: ContentPayload::Hull(mesh),
        }
    }

    #[test]
    fn well_formed_hull_passes() {
        assert!(validate_content(&hull_file(1, triangle_mesh())).is_empty());
    }

    #[test]
    fn seed_out_of_53_bit_range_is_rejected() {
        let errors = validate_content(&hull_file(Seed::MAX + 1, triangle_mesh()));
        assert!(matches!(errors[0], ValidationError::SeedOutOfRange { .. }));
    }

    #[test]
    fn repeated_index_is_degenerate() {
        let mut mesh = triangle_mesh();
        mesh.indices = vec![0, 0, 1];
        let errors = validate_content(&hull_file(1, mesh));
        assert!(matches!(
            errors[0],
            ValidationError::DegenerateTriangle { .. }
        ));
    }

    #[test]
    fn collinear_triangle_is_degenerate() {
        let mut mesh = triangle_mesh();
        // Three distinct, collinear vertices: zero area, no repeated index.
        mesh.vertices = vec![
            FixedVec2 {
                x: Fixed(0),
                y: Fixed(0),
            },
            FixedVec2 {
                x: Fixed(1024),
                y: Fixed(0),
            },
            FixedVec2 {
                x: Fixed(2048),
                y: Fixed(0),
            },
        ];
        let errors = validate_content(&hull_file(1, mesh));
        assert!(matches!(
            errors[0],
            ValidationError::DegenerateTriangle { .. }
        ));
    }

    #[test]
    fn dangling_door_names_the_door() {
        let station = ContentFile {
            id: "test_station".into(),
            display_name: "Test Station".into(),
            asset_type: AssetType::Station,
            seed: 1,
            universe: "all".into(),
            priority: Priority::Curated,
            expires_at: None,
            payload: ContentPayload::Station {
                exterior: triangle_mesh(),
                layout: GeneratedLayout {
                    rooms: vec![Room {
                        kind: RoomKind::Hangar,
                        x: 0,
                        y: 0,
                        width: 10,
                        height: 10,
                    }],
                    doors: vec![Door {
                        from: 0,
                        to: 7, // room 7 doesn't exist
                        x: 0,
                        y: 0,
                    }],
                },
                npc_spawns: vec![],
            },
        };
        let errors = validate_content(&station);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ValidationError::DanglingDoor {
                door_to,
                room_count,
                ..
            } => {
                assert_eq!(*door_to, 7);
                assert_eq!(*room_count, 1);
            }
            other => panic!("expected DanglingDoor, got {other:?}"),
        }
        // The error message names the offending door, per the acceptance
        // gate: "exit 1, names the door".
        assert!(errors[0].to_string().contains('7'));
    }

    #[test]
    fn duplicate_frame_slot_id_is_rejected() {
        use crate::editor::exterior::HullFrame;
        use crate::generator::hull::HullClass;

        let mut frame = HullFrame::reference(HullClass::Corvette);
        let dup = frame.slots[0].clone();
        frame.slots.push(dup);
        let file = ContentFile {
            id: "frame_corvette".into(),
            display_name: "Corvette Frame".into(),
            asset_type: AssetType::HullFrame,
            seed: 1,
            universe: "all".into(),
            priority: Priority::Curated,
            expires_at: None,
            payload: ContentPayload::HullFrame(frame),
        };
        let errors = validate_content(&file);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ValidationError::DuplicateFrameId { namespace, id } => {
                assert_eq!(*namespace, "slot");
                assert_eq!(id, "nose");
            }
            other => panic!("expected DuplicateFrameId, got {other:?}"),
        }
        // The message names the offending id (CLI rule: name the field).
        assert!(errors[0].to_string().contains("nose"));
    }

    #[test]
    fn well_formed_frame_passes() {
        use crate::editor::exterior::HullFrame;
        use crate::generator::hull::HullClass;

        for class in [
            HullClass::Shuttle,
            HullClass::Corvette,
            HullClass::Freighter,
        ] {
            let file = ContentFile {
                id: "frame".into(),
                display_name: "Frame".into(),
                asset_type: AssetType::HullFrame,
                seed: 1,
                universe: "all".into(),
                priority: Priority::Curated,
                expires_at: None,
                payload: ContentPayload::HullFrame(HullFrame::reference(class)),
            };
            assert!(validate_content(&file).is_empty(), "{class:?}");
        }
    }

    fn templates_file(templates: Vec<crate::editor::interior::RoomTemplate>) -> ContentFile {
        ContentFile {
            id: "room_templates".into(),
            display_name: "Room Templates".into(),
            asset_type: AssetType::RoomTemplates,
            seed: 1,
            universe: "all".into(),
            priority: Priority::Curated,
            expires_at: None,
            payload: ContentPayload::RoomTemplates(templates),
        }
    }

    #[test]
    fn reference_room_templates_pass() {
        use crate::editor::interior::RoomTemplate;
        assert!(validate_content(&templates_file(RoomTemplate::reference_set())).is_empty());
    }

    #[test]
    fn duplicate_template_id_and_kind_are_rejected() {
        use crate::editor::interior::RoomTemplate;
        let mut set = RoomTemplate::reference_set();
        let dup = set[0].clone();
        set.push(dup);
        let errors = validate_content(&templates_file(set));
        // The duplicate trips both namespaces: id and kind.
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::DuplicateRoomTemplate {
                namespace: "id",
                ..
            }
        )));
        assert!(errors.iter().any(|e| matches!(
            e,
            ValidationError::DuplicateRoomTemplate {
                namespace: "kind",
                ..
            }
        )));
        assert!(errors[0].to_string().contains("cockpit"));
    }

    #[test]
    fn zero_size_template_is_rejected() {
        use crate::editor::interior::RoomTemplate;
        let mut set = RoomTemplate::reference_set();
        set[0].width = 0;
        let errors = validate_content(&templates_file(set));
        assert!(matches!(
            errors[0],
            ValidationError::ZeroSizeRoomTemplate { .. }
        ));
    }

    #[test]
    fn unknown_universe_is_rejected() {
        let mut file = hull_file(1, triangle_mesh());
        file.universe = "nonexistent".into();
        let errors = validate_content(&file);
        assert!(matches!(errors[0], ValidationError::UnknownUniverse { .. }));
    }
}
