use reachlock_core::combat::{HostileArchetype, HostileLocation};
fn content(rel: &str) -> String {
    let p = format!("{}/../content/{}", env!("CARGO_MANIFEST_DIR"), rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p}: {e}"))
}
#[test]
fn archetypes_parse() {
    for f in ["raider_melee", "raider_gunner", "security_bot", "raider_boss"] {
        let text = content(&format!("combat/{f}.ron"));
        let a: HostileArchetype = ron::from_str(&text).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert_eq!(a.id, f);
    }
}
#[test]
fn location_parses() {
    let text = content("locations/derelict_hold.ron");
    let loc: HostileLocation = ron::from_str(&text).expect("derelict parses");
    assert_eq!(loc.rooms.len(), 5);
    assert!(loc.keycard.is_some());
    let known = ["raider_melee", "raider_gunner", "security_bot", "raider_boss"];
    for a in loc.referenced_archetypes() { assert!(known.contains(&a), "unknown archetype {a}"); }
}
