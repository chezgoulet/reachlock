//! Shared JSON-schema cache (handoff §Phase 2.5).
//!
//! Reads the authored JSON schemas from `mods/reachlock/schemas/` at editor
//! startup, compiles a `jsonschema` validator for each, and keeps the raw
//! schema text for inlining into AI prompts. Used by both the existing
//! `io::validate_content` path and the AI generation pipeline.

use std::collections::HashMap;

use crate::app::ContentType;

/// Resolve the directory holding the JSON schemas. The editor is run from
/// the workspace root (so `mods/reachlock/schemas/` is the primary path), but
/// unit tests execute from the crate directory; fall back to
/// `$CARGO_MANIFEST_DIR/../mods/reachlock/schemas` so they resolve too.
pub fn schemas_dir() -> std::path::PathBuf {
    let primary = std::path::PathBuf::from("mods/reachlock/schemas");
    if primary.exists() {
        return primary;
    }
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let fallback = std::path::Path::new(&manifest)
            .join("..")
            .join("mods")
            .join("reachlock")
            .join("schemas");
        if fallback.exists() {
            return fallback;
        }
    }
    primary
}

/// Schema id (filename stem) for each content type. Shared by the
/// validation path ([`crate::io::validate_content`]) and the AI
/// generation pipeline — this is the single source of truth.
pub fn schema_id(ct: &ContentType) -> Option<&'static str> {
    Some(match ct {
        ContentType::HullFrame => "hull_frame",
        ContentType::Station => "station",
        ContentType::Location => "location",
        ContentType::Soul => "soul",
        ContentType::Contract => "contract",
        ContentType::Faction => "faction",
        ContentType::EconomyGoods => "economy_goods",
        ContentType::Storyline => "storyline",
        ContentType::Item => "item",
        // `hostile.schema.json` describes the landed-combat `HostileArchetype`
        // type that this editor edits. (No separate enemy_archetype file.)
        ContentType::EnemyArchetype => "hostile",
        ContentType::ChartedSystem => "charted_system",
        // The Hull editor edits a `HullConfiguration`, not the raw
        // `GeneratedMesh` that `hull.schema.json` validates, so it uses the
        // dedicated hull_configuration schema instead.
        ContentType::HullMesh => "hull_configuration",
        ContentType::RoomTemplates => "room_template",
        ContentType::GateNetwork => "gate_network",
        // Previewers persist nothing; no schema applies.
        ContentType::ItemBrowser | ContentType::SpriteViewer => return None,
    })
}

/// A compiled schema: raw text (for prompts) + validator (for checks).
pub struct CompiledSchema {
    pub raw: String,
    validator: jsonschema::Validator,
}

impl CompiledSchema {
    /// Validate a JSON value, returning human-readable error strings.
    pub fn validate(&self, value: &serde_json::Value) -> Vec<String> {
        let mut errors = Vec::new();
        if let Err(validation_errors) = self.validator.validate(value) {
            for err in validation_errors {
                errors.push(format!("{}: {}", err.instance_path, err));
            }
        }
        errors
    }

    /// Compact, structural-only schema text for inlining into an LLM prompt
    /// (strips `$schema`/`title`/`description` meta fields).
    pub fn compact_prompt(&self) -> String {
        match serde_json::from_str::<serde_json::Value>(&self.raw) {
            Ok(mut v) => {
                if let Some(obj) = v.as_object_mut() {
                    obj.remove("$schema");
                    obj.remove("title");
                    obj.remove("description");
                    // Drop per-property descriptions to keep the prompt tight.
                    if let Some(props) = obj.get_mut("properties").and_then(|p| p.as_object_mut()) {
                        for prop in props.values_mut() {
                            if let Some(p) = prop.as_object_mut() {
                                p.remove("description");
                            }
                        }
                    }
                }
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| self.raw.clone())
            }
            Err(_) => self.raw.clone(),
        }
    }
}

/// Cache of compiled schemas keyed by content type.
#[derive(Default)]
pub struct SchemaCache {
    map: HashMap<ContentType, CompiledSchema>,
}

impl SchemaCache {
    /// Load every schema present under `mods/reachlock/schemas/`.
    /// Missing schemas are skipped (the AI bar reports "no schema" for those
    /// types rather than failing the whole cache).
    pub fn load_all() -> Self {
        let mut map = HashMap::new();
        for ct in ContentType::all() {
            if let Some(id) = schema_id(ct) {
                let path = schemas_dir().join(format!("{id}.schema.json"));
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(schema) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Ok(validator) = jsonschema::options().build(&schema) {
                            map.insert(
                                *ct,
                                CompiledSchema {
                                    raw: text,
                                    validator,
                                },
                            );
                        }
                    }
                }
            }
        }
        SchemaCache { map }
    }

    pub fn get(&self, ct: &ContentType) -> Option<&CompiledSchema> {
        self.map.get(ct)
    }

    pub fn has(&self, ct: &ContentType) -> bool {
        self.map.contains_key(ct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every content type that should have a schema must actually load one.
    #[test]
    fn every_mapped_type_has_a_schema() {
        let cache = SchemaCache::load_all();
        for ct in ContentType::all() {
            if let Some(id) = schema_id(ct) {
                assert!(
                    cache.has(ct),
                    "content type {:?} (schema id {id}) failed to load a schema",
                    ct
                );
            }
        }
    }

    /// A ContentFile envelope describing a station must validate against the
    /// station schema (the LLM is prompted to emit the full envelope).
    #[test]
    fn station_envelope_validates() {
        let cache = SchemaCache::load_all();
        let sample = serde_json::json!({
            "id": "sorrow_station",
            "display_name": "Sorrow Station",
            "asset_type": "station",
            "seed": 4218130448322139u64,
            "universe": "all",
            "priority": "curated",
            "payload": {
                "station": {
                    "exterior": { "vertices": [], "indices": [] },
                    "layout": { "rooms": [], "doors": [] }
                }
            }
        });
        let errors = cache.get(&ContentType::Station).unwrap().validate(&sample);
        assert!(
            errors.is_empty(),
            "unexpected validation errors: {errors:?}"
        );
    }

    /// A bare HostileArchetype (enemy) must validate against the hostile schema.
    #[test]
    fn enemy_archetype_validates() {
        let cache = SchemaCache::load_all();
        let sample = serde_json::json!({
            "id": "raider",
            "display_name": "Raider",
            "hp": 4000,
            "speed": 96,
            "light_attack": { "startup_ticks": 4, "active_ticks": 3, "recovery_ticks": 8, "damage": 512, "range": 1536 },
            "heavy_attack": { "startup_ticks": 8, "active_ticks": 3, "recovery_ticks": 16, "damage": 1024, "range": 1536 },
            "block": { "active_ticks": 12, "cooldown_ticks": 20, "parry_ticks": 3 },
            "dodge": { "i_frame_ticks": 6, "recovery_ticks": 8, "distance": 2048 },
            "chase_radius": 6144,
            "disengage_radius": 12288,
            "flee_hp_frac": 256
        });
        let errors = cache
            .get(&ContentType::EnemyArchetype)
            .unwrap()
            .validate(&sample);
        assert!(
            errors.is_empty(),
            "unexpected validation errors: {errors:?}"
        );
    }

    /// A FactionCatalog-shaped JSON must validate and deserialize into the
    /// editor's `FactionCatalog` (the editor stores the whole catalog).
    #[test]
    fn faction_catalog_round_trips() {
        use reachlock_core::faction::FactionCatalog;
        let cache = SchemaCache::load_all();
        let sample = serde_json::json!({
            "version": 1,
            "factions": [{
                "id": "compact",
                "name": "The Compact",
                "doctrine": "diplomatic",
                "goals": [{ "id": "hold_core", "description": "Hold the core worlds" }]
            }]
        });
        let errors = cache.get(&ContentType::Faction).unwrap().validate(&sample);
        assert!(
            errors.is_empty(),
            "unexpected validation errors: {errors:?}"
        );
        let catalog: FactionCatalog = serde_json::from_value(sample).expect("deserialize catalog");
        assert_eq!(catalog.factions.len(), 1);
    }

    /// A HullConfiguration-shaped JSON must validate and deserialize into the
    /// editor's `HullConfiguration`.
    #[test]
    fn hull_configuration_round_trips() {
        use reachlock_core::editor::exterior::HullConfiguration;
        let cache = SchemaCache::load_all();
        let sample = serde_json::json!({
            "hull_id": "frame_corvette",
            "seed": 12345,
            "hardpoints": [{
                "slot_id": "hp_0",
                "item": { "seed": 1, "item_type": { "equipment": { "weapon": { "kinetic": "cannon" } } }, "tier": 3, "faction": "compact", "biome": "" },
                "size_class": "medium"
            }],
            "engine": { "seed": 2, "item_type": { "component": "power_plant" }, "tier": 2, "faction": "", "biome": "" },
            "plating": [{ "zone_id": "hull", "mass": 100 }],
            "paint": { "primary": "primary", "secondary": "accent", "accent": "structure" },
            "decals": []
        });
        let errors = cache.get(&ContentType::HullMesh).unwrap().validate(&sample);
        assert!(
            errors.is_empty(),
            "unexpected validation errors: {errors:?}"
        );
        let _cfg: HullConfiguration =
            serde_json::from_value(sample).expect("deserialize hull configuration");
    }
}
