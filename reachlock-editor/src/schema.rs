//! Shared JSON-schema cache (handoff §Phase 2.5).
//!
//! Reads the authored JSON schemas from `mods/reachlock/schemas/` at editor
//! startup, compiles a `jsonschema` validator for each, and keeps the raw
//! schema text for inlining into AI prompts. Used by both the existing
//! `io::validate_content` path and the AI generation pipeline.

use std::collections::HashMap;

use crate::app::ContentType;

/// Schema id (filename stem) for each content type.
fn schema_id(ct: &ContentType) -> Option<&'static str> {
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
        ContentType::EnemyArchetype => "enemy_archetype",
        ContentType::ChartedSystem => "charted_system",
        ContentType::HullMesh => "hull",
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
                    if let Some(props) = obj.get_mut("properties").and_then(|p| p.as_object_mut())
                    {
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
                let path = format!("mods/reachlock/schemas/{id}.schema.json");
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
