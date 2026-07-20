//! AI-assisted content generation (handoff §Phase 2.5).
//!
//! Calls any OpenAI-compatible `/v1/chat/completions` endpoint (Ollama by
//! default, local-first and offline). The editor assembles a system prompt
//! from a role instruction, a per-type context paragraph, and the compact
//! JSON schema, POSTs the user's natural-language description, then extracts
//! and schema-validates the returned JSON.

use serde_json::Value;

use reachlock_core::content::envelope::{ContentFile, ContentPayload};

use crate::app::ContentType;
use crate::schema::SchemaCache;

pub const DEFAULT_API_BASE_URL: &str = "http://localhost:11434/v1";
pub const DEFAULT_API_KEY: &str = "ollama";
pub const DEFAULT_MODEL: &str = "llama3.2:3b";
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// API configuration persisted to `save/editor-settings.ron`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiConfig {
    pub api_base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            api_base_url: DEFAULT_API_BASE_URL.into(),
            api_key: DEFAULT_API_KEY.into(),
            model: DEFAULT_MODEL.into(),
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }
}

/// Why generation failed.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum GenerationError {
    HttpError(String),
    NoJsonFound(String),
    SchemaValidationFailed(Vec<String>),
    DeserializationFailed(String),
}

impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerationError::HttpError(e) => write!(f, "Connection error: {e}"),
            GenerationError::NoJsonFound(_) => {
                write!(f, "Response contained no JSON — is the model replying?")
            }
            GenerationError::SchemaValidationFailed(errs) => {
                write!(f, "Schema validation failed:\n{}", errs.join("\n"))
            }
            GenerationError::DeserializationFailed(e) => {
                write!(f, "Could not map response to editor fields: {e}")
            }
        }
    }
}

pub struct GenerationResult {
    pub json_value: Value,
    pub warnings: Vec<String>,
}

/// Result posted back from the background generation thread.
pub enum AiGenOutcome {
    Ok {
        ct: ContentType,
        result: GenerationResult,
    },
    Err(GenerationError),
}

/// One-paragraph context describing what a content type means in-world.
/// Doubles as the per-editor documentation in the help window (F1).
pub fn type_context(ct: &ContentType) -> &'static str {
    match ct {
        ContentType::ChartedSystem => {
            "A star system in the ReachLock galaxy. Systems are connected by a gate \
             network. Each has a 3D position, a biome flavor, and a descriptive paragraph \
             visible in the galaxy map."
        }
        ContentType::Soul => {
            "An NPC character wrapped in a ContentFile envelope. The JSON must include the \
             envelope fields: id (string), display_name (string), asset_type (the string \
             \"soul\"), seed (integer), universe (\"all\"), priority (one of: procedural, \
             curated, event, authoritative), and payload.soul (the soul object). Species: Human \
             (cybernetically enhanced), Android (synthetic humanoid), Robot (non-humanoid \
             machine), Voidborn (space-dwelling, mystical, Predecessor-lore), Xenotype \
             (planet-bound ecosystem creature). Each has personality traits, an emotional state, \
             memories, relationships, secrets, goals, and optional branching dialogue."
        }
        ContentType::Faction => {
            "A political faction. Has a doctrine (Military/Economic/Diplomatic/Expansionist), \
             tariff policy, territory claims, internal divisions with agendas, relationships \
             with other factions, and goods it produces."
        }
        ContentType::HullFrame => {
            "A ship frame defining where hardpoints, armor zones, decals, and the engine mount \
             go. Classes: Shuttle (small, fast), Corvette (balanced), Freighter (large, slow), \
             Station (immobile), Rock (asteroid)."
        }
        ContentType::EnemyArchetype => {
            "A landed-combat enemy. Has HP, speed, light and heavy attack windows \
             (startup/active/recovery ticks, damage, range), block window, dodge window, \
             chase/disengage radii, and flee threshold."
        }
        ContentType::Station => {
            "A space station wrapped in a ContentFile envelope. The JSON must include the \
             envelope fields: id (string), display_name (string), asset_type (the string \
             \"station\"), seed (integer), universe (\"all\"), priority (one of: procedural, \
             curated, event, authoritative), and payload.station (with exterior, layout, and \
             npc_spawns). An exterior hull mesh, an interior layout of rooms connected by doors, \
             and NPC spawns with dialogue lines."
        }
        ContentType::Location => {
            "A hostile interior location (derelict ship, bunker, space station). Contains rooms \
             with enemy spawns, props, connections between rooms, and an optional keycard gate."
        }
        ContentType::EconomyGoods => {
            "A catalog of trade goods. Each good has a name, category (Consumable, Fuel, \
             Material, Manufactured, Medical, Luxury, Contraband), base price, mass, and \
             contraband flag."
        }
        ContentType::Contract => {
            "An automated contract wrapped in a ContentFile envelope. The JSON must include the \
             envelope fields: id (string), display_name (string), asset_type (the string \
             \"contract\"), seed (integer), universe (\"all\"), priority (one of: procedural, \
             curated, event, authoritative), and payload.contract (the contract object). A \
             contract has a trigger (Timer, Event, StateChange, or Manual), prioritized rules \
             with conditions (Always, Compare, All, Any, Not), actions, and optional LLM \
             fallback authority."
        }
        ContentType::Storyline => {
            "A faction's narrative arc. Contains chapters with triggers (TickAfter, \
             ChapterComplete, PlayerReputation, All, Any) and narration text that fires when \
             triggered."
        }
        ContentType::Item => {
            "A generated equipment item. Has a type hierarchy (Equipment->Weapon->Kinetic->\
             Cannon), tier (1-10), seed, faction/biome origin, and generates stats like Damage, \
             Range, FireRate, ShieldHp, etc."
        }
        ContentType::HullMesh => {
            "A hull configuration (how a hull is outfitted against a frame), NOT a raw mesh. \
             Fields: hull_id (string, references a frame), seed (integer), hardpoints (array of \
             {slot_id, item (ItemSeed), size_class (Small|Medium|Large)}), engine (ItemSeed), \
             plating (array of {zone_id, mass}), paint (primary|secondary|accent each one of \
             primary|accent|structure), decals (array of {slot_id, decal_id})."
        }
        ContentType::RoomTemplates => {
            "A set of room templates for ship interiors, wrapped in a ContentFile envelope. The \
             JSON must include the envelope fields: id (string), display_name (string), \
             asset_type (the string \"room_templates\"), seed (integer), universe (\"all\"), \
             priority (one of: procedural, curated, event, authoritative), and \
             payload.room_templates (an array of templates). Each template has a kind (Cockpit, \
             MedBay, Reactor, etc.), dimensions, required systems, furniture slots, and \
             adjacency bonus pairs."
        }
        ContentType::GateNetwork => {
            "A directed graph of star systems connected by gates. Each gate has a from/to \
             system, a status (Active, Blockaded, Restricted, Contested, Destroyed), and an \
             optional controlling faction."
        }
        ContentType::ItemBrowser | ContentType::SpriteViewer => {
            "A live preview with nothing persisted."
        }
    }
}

fn role_instruction() -> &'static str {
    "You are a content generation assistant for the ReachLock spacefaring game. Output ONLY \
     valid JSON matching the schema below. Do not include markdown code fences or explanatory \
     text. Output raw JSON only."
}

/// Assemble the full system prompt for a content type and its schema.
fn build_system_prompt(ct: &ContentType, schema: &crate::schema::CompiledSchema) -> String {
    format!(
        "{}\n\n{}\n\nSchema:\n{}",
        role_instruction(),
        type_context(ct),
        schema.compact_prompt()
    )
}

/// Try progressively looser strategies to pull a JSON value out of the model
/// response text. Returns the parsed value or the raw text for debugging.
fn extract_json(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();

    // 1. Whole response is JSON.
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }

    // 2. Fenced ```json ... ``` block.
    if let Some(caps) = regex_extract(trimmed) {
        if let Ok(v) = serde_json::from_str::<Value>(&caps) {
            return Ok(v);
        }
    }

    // 3. First balanced {...} (object return types).
    if let Some(obj) = balanced(trimmed, '{', '}') {
        if let Ok(v) = serde_json::from_str::<Value>(&obj) {
            return Ok(v);
        }
    }

    // 4. First balanced [...] (array return types, e.g. RoomTemplates).
    if let Some(arr) = balanced(trimmed, '[', ']') {
        if let Ok(v) = serde_json::from_str::<Value>(&arr) {
            return Ok(v);
        }
    }

    Err(text.to_string())
}

/// Pull the contents of a ```json ... ``` fence (naive, no nesting).
fn regex_extract(text: &str) -> Option<String> {
    let start = text.find("```json")?;
    let after = &text[start + 7..];
    let end = after.find("```")?;
    Some(after[..end].trim().to_string())
}

/// When a schema describes a `ContentFile` envelope but the editor works with
/// the bare inner type, deserialize the envelope and extract the inner
/// payload as a JSON value. Returns `None` if `value` isn't a valid envelope
/// or the variant tag doesn't match.
pub fn extract_inner_from_envelope(value: &Value, tag: &str) -> Option<Value> {
    let cf: ContentFile = serde_json::from_value(value.clone()).ok()?;
    match (tag, cf.payload) {
        ("soul", ContentPayload::Soul(s)) => serde_json::to_value(*s).ok(),
        ("contract", ContentPayload::Contract(c)) => serde_json::to_value(c).ok(),
        _ => None,
    }
}

/// Character-level bracket matching for `{`/`}` or `[`/`]`.
fn balanced(text: &str, open: char, close: char) -> Option<String> {
    let mut depth = 0i32;
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c == open {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                end = Some(i + c.len_utf8());
                break;
            }
        }
    }
    match (start, end) {
        (Some(s), Some(e)) => Some(text[s..e].to_string()),
        _ => None,
    }
}

/// One chat-completion request + response extraction + schema validation.
pub async fn generate_content(
    config: &AiConfig,
    ct: ContentType,
    schemas: &SchemaCache,
    user_prompt: &str,
) -> Result<GenerationResult, GenerationError> {
    let schema = schemas.get(&ct).ok_or_else(|| {
        GenerationError::SchemaValidationFailed(vec![
            "No schema available for this content type — use procedural generation instead.".into(),
        ])
    })?;

    let system = build_system_prompt(&ct, schema);

    let payload = serde_json::json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user_prompt }
        ],
        "max_tokens": config.max_tokens,
        "temperature": 0.7
    });

    let url = format!(
        "{}/chat/completions",
        config.api_base_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| GenerationError::HttpError(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(GenerationError::HttpError(format!(
            "API error {} — is the key valid?",
            resp.status()
        )));
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| GenerationError::HttpError(e.to_string()))?;

    // OpenAI-compatible: choices[0].message.content
    let content = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| {
            GenerationError::NoJsonFound(serde_json::to_string(&body).unwrap_or_default())
        })?;

    let value = extract_json(content).map_err(GenerationError::NoJsonFound)?;

    let warnings = schema.validate(&value);
    // Soft: surface validation errors but still return the value so the
    // editor can decide. The editor maps/deserializes and may surface more.
    Ok(GenerationResult {
        json_value: value,
        warnings,
    })
}

/// Probe the endpoint's model list (Ollama-compatible `/models`).
/// Returns Ok(Some(model_name)) on a healthy response, Ok(None) if reachable
/// but no models reported, or Err with a connection error.
pub async fn test_connection(config: &AiConfig) -> Result<Option<String>, String> {
    let url = format!("{}/models", config.api_base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(&config.api_key)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("endpoint returned {}", resp.status()));
    }
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let first = body
        .get("data")
        .and_then(|d| d.get(0))
        .and_then(|m| m.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string());
    Ok(first)
}
