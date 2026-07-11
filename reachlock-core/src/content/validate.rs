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
    UnknownUniverse {
        universe: String,
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
            ValidationError::UnknownUniverse { universe } => write!(
                f,
                "unknown universe {universe:?} (expected \"all\" or a universe tier name)"
            ),
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
        ContentPayload::Station { exterior, layout } => {
            errors.extend(check_mesh(&content.id, exterior));
            errors.extend(check_doors(layout));
        }
        ContentPayload::Contract(_) => {}
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
            },
        };
        let errors = validate_content(&station);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ValidationError::DanglingDoor {
                door_to, room_count, ..
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
    fn unknown_universe_is_rejected() {
        let mut file = hull_file(1, triangle_mesh());
        file.universe = "nonexistent".into();
        let errors = validate_content(&file);
        assert!(matches!(
            errors[0],
            ValidationError::UnknownUniverse { .. }
        ));
    }
}
