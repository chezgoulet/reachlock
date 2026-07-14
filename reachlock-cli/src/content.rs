//! `reachlock content …` — validate and preview authored `.ron` content
//! files (spec §10, Stage 2: CLI Validation). No Bevy window needed: the
//! structural integrity checks live in `reachlock-core::content::validate`,
//! schema validation checks the JSON projection against the content type's
//! schema, and previews reuse the SVG/PPM exporters from the `gen` module —
//! the same path a generated asset would take (spec §10: "the bridge doesn't
//! know the difference").

use clap::Subcommand;
use reachlock_core::content::{validate_content, AssetType, ContentFile, ContentPayload};
use reachlock_core::economy::GoodsCatalog;
use std::path::{Path, PathBuf};

use crate::gen;

// Load schemas at compile time
const HULL_SCHEMA: &str = include_str!("../../content/schemas/hull.schema.json");
const STATION_SCHEMA: &str = include_str!("../../content/schemas/station.schema.json");
const CONTRACT_SCHEMA: &str = include_str!("../../content/schemas/contract.schema.json");

#[derive(Subcommand)]
pub enum ContentCommand {
    /// Validate an authored content file's structural integrity (seed range,
    /// universe, no degenerate triangles, doors reference real rooms).
    /// Exit 0 if clean, 1 if any check fails — each failure is named.
    Validate {
        /// Path to a `.ron` content file.
        path: PathBuf,
    },
    /// Validate the authored economy catalogue (`content/economy/goods.ron`):
    /// every good has a positive base price and mass, contraband goods are
    /// tagged `Contraband`, and the version is sane. Exit 0 if clean, 1
    /// otherwise. (S10)
    ValidateGoods {
        /// Path to the `goods.ron` catalogue.
        path: PathBuf,
    },
    /// Render an authored content file to a dependency-free preview (SVG for
    /// hull/station geometry) so authors can eyeball it without the client.
    Preview {
        /// Path to a `.ron` content file.
        path: PathBuf,
        /// Write the preview here (default: alongside the input, `.svg`).
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

pub fn run(cmd: ContentCommand) -> Result<(), String> {
    match cmd {
        ContentCommand::ValidateGoods { path } => {
            let catalog = load_goods(&path)?;
            let errors = catalog.validate();
            if errors.is_empty() {
                println!(
                    "{}: valid goods catalogue — {} goods, version {}",
                    path.display(),
                    catalog.goods.len(),
                    catalog.version
                );
                Ok(())
            } else {
                for e in &errors {
                    eprintln!("  {e}");
                }
                Err(format!(
                    "{} validation error(s) in {}",
                    errors.len(),
                    path.display()
                ))
            }
        }
        ContentCommand::Validate { path } => {
            let content = load(&path)?;

            // Project to JSON and validate against schema
            let json_value =
                serde_json::to_value(&content).map_err(|e| format!("serializing to JSON: {e}"))?;
            let schema_errors = validate_schema(&content.asset_type, &json_value)?;

            // Perform structural checks
            let structural_errors = validate_content(&content);

            // Combine errors: schema errors first, then structural
            let mut all_errors = Vec::new();
            all_errors.extend(schema_errors);
            all_errors.extend(structural_errors.iter().map(|e| e.to_string()));

            if all_errors.is_empty() {
                println!(
                    "{}: valid — {:?} \"{}\" (id {}, seed {:#x})",
                    path.display(),
                    content.asset_type,
                    content.display_name,
                    content.id,
                    content.seed,
                );
                Ok(())
            } else {
                for e in &all_errors {
                    eprintln!("  {e}");
                }
                Err(format!(
                    "{} validation error(s) in {}",
                    all_errors.len(),
                    path.display()
                ))
            }
        }
        ContentCommand::Preview { path, out } => {
            let content = load(&path)?;
            let svg = match &content.payload {
                ContentPayload::Hull(mesh) => gen::mesh_svg(mesh),
                ContentPayload::Station { layout, .. } => gen::layout_svg(layout),
                ContentPayload::Contract(_) => {
                    // Contracts are text, not geometry — summarize instead.
                    println!(
                        "{}: contract \"{}\" (id {}) — no geometry to preview",
                        path.display(),
                        content.display_name,
                        content.id
                    );
                    return Ok(());
                }
            };
            let out = out.unwrap_or_else(|| path.with_extension("svg"));
            std::fs::write(&out, svg).map_err(|e| format!("writing {}: {e}", out.display()))?;
            println!("wrote {}", out.display());
            Ok(())
        }
    }
}

/// Validate a JSON value against the schema for the given asset type.
/// Returns a list of validation errors (empty if valid).
fn validate_schema(
    asset_type: &AssetType,
    json_value: &serde_json::Value,
) -> Result<Vec<String>, String> {
    let schema_text = match asset_type {
        AssetType::Hull => HULL_SCHEMA,
        AssetType::Station => STATION_SCHEMA,
        AssetType::Contract => CONTRACT_SCHEMA,
    };

    let schema = serde_json::from_str::<serde_json::Value>(schema_text)
        .map_err(|e| format!("loading schema: {e}"))?;

    let mut errors = Vec::new();

    // Check if the value is valid against the schema
    if !jsonschema::is_valid(&schema, json_value) {
        // If not valid, get the detailed error
        if let Err(err) = jsonschema::validate(&schema, json_value) {
            errors.push(format!("schema validation: {}", err));
        }
    }

    Ok(errors)
}

/// Read and deserialize a `.ron` content file into the shared envelope.
fn load(path: &Path) -> Result<ContentFile, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    ron::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
}

/// Read and deserialize a `goods.ron` economy catalogue.
fn load_goods(path: &Path) -> Result<GoodsCatalog, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    ron::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
}
