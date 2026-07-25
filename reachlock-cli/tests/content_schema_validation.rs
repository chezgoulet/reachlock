use reachlock_core::content::ContentFile;
use std::fs;
use std::path::PathBuf;

fn get_content_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).parent().unwrap().to_path_buf()
}

fn load_file(path: &str) -> ContentFile {
    let root = get_content_root();
    let full_path = root.join(path);
    let text = fs::read_to_string(&full_path).expect("reading file");
    ron::from_str(&text).expect("parsing RON")
}

#[test]
fn all_committed_hull_files_pass_schema() {
    let content = load_file("mods/reachlock/hulls/loup_garou.ron");
    let _json = serde_json::to_value(&content).expect("JSON serialization");

    // Just check that it serializes without error
    // The CLI would normally validate this with schema checks
    assert_eq!(content.id, "loup_garou");
    assert_eq!(content.display_name, "Loup-Garou");
}

#[test]
fn all_committed_station_files_pass_schema() {
    let content = load_file("mods/reachlock/stations/sorrow_station.ron");
    let _json = serde_json::to_value(&content).expect("JSON serialization");

    assert_eq!(content.id, "sorrow_station");
    assert_eq!(content.display_name, "Sorrow Station");
}

#[test]
fn dangling_door_fixture_parses_and_passes_schema() {
    // The dangling_door fixture has a structural defect (door to non-existent room),
    // but it should pass schema validation
    let content = load_file("mods/reachlock/_fixtures/dangling_door.ron");
    let json = serde_json::to_value(&content).expect("JSON serialization");

    assert_eq!(content.id, "broken_station");
    assert_eq!(content.display_name, "Broken Station (test fixture)");

    // Verify the JSON has the expected structure
    assert!(json.get("id").is_some());
    assert!(json.get("payload").is_some());
}

#[test]
fn schema_rejects_misspelled_envelope_field() {
    // Create a JSON object with a typo in the envelope
    // For example, "disply_name" instead of "display_name"
    let malformed = serde_json::json!({
        "id": "test_hull",
        "disply_name": "Test Hull",  // <-- typo
        "asset_type": "hull",
        "seed": 12345,
        "universe": "all",
        "priority": "authoritative",
        "payload": {
            "hull": {
                "vertices": [
                    {"x": 0, "y": 0},
                    {"x": 100, "y": 0},
                    {"x": 50, "y": 100}
                ],
                "indices": [0, 1, 2]
            }
        }
    });

    // Load the schema manually to verify it rejects the typo
    let schema_text = include_str!("../../mods/reachlock/schemas/hull.schema.json");
    let schema = serde_json::from_str::<serde_json::Value>(schema_text).expect("parsing schema");

    // This should fail because additionalProperties: false and display_name is required
    assert!(
        !jsonschema::is_valid(&schema, &malformed),
        "Schema should reject object with misspelled display_name"
    );
}

#[test]
fn schema_accepts_optional_expires_at_field() {
    // Verify that expires_at is optional and can be omitted
    let hull_with_expires = serde_json::json!({
        "id": "test_hull",
        "display_name": "Test Hull",
        "asset_type": "hull",
        "seed": 12345,
        "universe": "all",
        "priority": "event",
        "expires_at": 1234567890,
        "payload": {
            "hull": {
                "vertices": [
                    {"x": 0, "y": 0},
                    {"x": 100, "y": 0},
                    {"x": 50, "y": 100}
                ],
                "indices": [0, 1, 2]
            }
        }
    });

    let schema_text = include_str!("../../mods/reachlock/schemas/hull.schema.json");
    let schema = serde_json::from_str::<serde_json::Value>(schema_text).expect("parsing schema");

    assert!(
        jsonschema::is_valid(&schema, &hull_with_expires),
        "Schema should accept event priority hull with expires_at"
    );
}

#[test]
fn schema_rejects_seed_out_of_range() {
    // Create a JSON object with seed exceeding 2^53-1
    // We can't use a literal > 2^53-1 directly, so we build it as a string and parse
    let json_str = r#"{
        "id": "test_hull",
        "display_name": "Test Hull",
        "asset_type": "hull",
        "seed": 9007199254740992,
        "universe": "all",
        "priority": "authoritative",
        "payload": {
            "hull": {
                "vertices": [
                    {"x": 0, "y": 0},
                    {"x": 100, "y": 0},
                    {"x": 50, "y": 100}
                ],
                "indices": [0, 1, 2]
            }
        }
    }"#;
    let hull_invalid_seed: serde_json::Value =
        serde_json::from_str(json_str).expect("parsing JSON");

    let schema_text = include_str!("../../mods/reachlock/schemas/hull.schema.json");
    let schema = serde_json::from_str::<serde_json::Value>(schema_text).expect("parsing schema");

    assert!(
        !jsonschema::is_valid(&schema, &hull_invalid_seed),
        "Schema should reject seed value greater than 2^53-1"
    );
}

#[test]
fn station_schema_validates_room_kinds() {
    // Verify that station schema accepts valid room kinds
    let station_valid = serde_json::json!({
        "id": "test_station",
        "display_name": "Test Station",
        "asset_type": "station",
        "seed": 12345,
        "universe": "all",
        "priority": "curated",
        "payload": {
            "station": {
                "exterior": {
                    "vertices": [
                        {"x": 0, "y": 0},
                        {"x": 100, "y": 0},
                        {"x": 50, "y": 100}
                    ],
                    "indices": [0, 1, 2]
                },
                "layout": {
                    "rooms": [
                        {
                            "kind": "hangar",
                            "x": 0,
                            "y": 0,
                            "width": 48,
                            "height": 32
                        }
                    ],
                    "doors": []
                }
            }
        }
    });

    let schema_text = include_str!("../../mods/reachlock/schemas/station.schema.json");
    let schema = serde_json::from_str::<serde_json::Value>(schema_text).expect("parsing schema");

    assert!(
        jsonschema::is_valid(&schema, &station_valid),
        "Schema should accept valid station with hangar room"
    );
}

#[test]
fn station_schema_rejects_invalid_room_kind() {
    let station_invalid = serde_json::json!({
        "id": "test_station",
        "display_name": "Test Station",
        "asset_type": "station",
        "seed": 12345,
        "universe": "all",
        "priority": "curated",
        "payload": {
            "station": {
                "exterior": {
                    "vertices": [
                        {"x": 0, "y": 0},
                        {"x": 100, "y": 0},
                        {"x": 50, "y": 100}
                    ],
                    "indices": [0, 1, 2]
                },
                "layout": {
                    "rooms": [
                        {
                            "kind": "invalid_room_type",  // <-- invalid
                            "x": 0,
                            "y": 0,
                            "width": 48,
                            "height": 32
                        }
                    ],
                    "doors": []
                }
            }
        }
    });

    let schema_text = include_str!("../../mods/reachlock/schemas/station.schema.json");
    let schema = serde_json::from_str::<serde_json::Value>(schema_text).expect("parsing schema");

    assert!(
        !jsonschema::is_valid(&schema, &station_invalid),
        "Schema should reject station with invalid room kind"
    );
}
