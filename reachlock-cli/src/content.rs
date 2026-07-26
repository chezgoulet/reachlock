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
use reachlock_core::faction::{load_faction_catalog, validate_storylines, FactionCatalog};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::gen;

// Load schemas at compile time
const HULL_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/hull.schema.json");
const HULL_FRAME_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/hull_frame.schema.json");
const STATION_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/station.schema.json");
const CONTRACT_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/contract.schema.json");
const SOUL_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/soul.schema.json");
const ROOM_TEMPLATE_SCHEMA: &str =
    include_str!("../../mods/reachlock/schemas/room_template.schema.json");
const ECOSYSTEM_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/ecosystem.schema.json");
const PLANT_CULTURE_SCHEMA: &str =
    include_str!("../../mods/reachlock/schemas/planet_culture.schema.json");
const CAREER_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/career_path.schema.json");
const THEME_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/theme.schema.json");
const TROPE_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/trope.schema.json");
const SCRIPTED_ENCOUNTER_SCHEMA: &str =
    include_str!("../../mods/reachlock/schemas/scripted_encounter.schema.json");
const DUNGEON_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/dungeon.schema.json");
const EVENT_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/event.schema.json");
const RECIPE_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/recipe.schema.json");
const ORIGIN_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/origin.schema.json");

const DIALOGUE_SCHEMA: &str = include_str!("../../mods/reachlock/schemas/dialogue.schema.json");

#[derive(Subcommand)]
pub enum ContentCommand {
    /// Validate an authored content file's structural integrity (seed range,
    /// universe, no degenerate triangles, doors reference real rooms).
    /// Exit 0 if clean, 1 if any check fails — each failure is named.
    Validate {
        /// Path to a `.ron` content file.
        path: PathBuf,
    },
    /// Check the whole content tree, not one file: every id a file references
    /// must be defined somewhere, by something of the right kind.
    ///
    /// `validate` cannot do this — it sees one file and a file that points at
    /// a nonexistent ship is perfectly well-formed. That is how ten origins
    /// came to name eight ship templates nobody had authored.
    ///
    /// Exit 0 if the tree is consistent, 1 on dangling references, duplicate
    /// ids, or files that parse as no known payload.
    Check {
        /// Content root to walk (the directory holding `origins/`, `souls/`, …).
        #[arg(default_value = "mods/reachlock")]
        root: PathBuf,
        /// Also list content that nothing references. Not a failure — a lead:
        /// an orphan is usually either unreachable or simply not wired up yet.
        #[arg(long)]
        orphans: bool,
    },
    /// Validate the authored economy catalogue (`content/economy/goods.ron`):
    /// every good has a positive base price and mass, contraband goods are
    /// tagged `Contraband`, and the version is sane. Exit 0 if clean, 1
    /// otherwise. (S10)
    ValidateGoods {
        /// Path to the `goods.ron` catalogue.
        path: PathBuf,
    },
    /// Validate the authored faction catalogue (`content/factions/canon.ron`):
    /// unique IDs, symmetric relationships, valid tariff params, territory
    /// control ≤ 100%. (S11)
    ValidateFactions {
        /// Path to the `factions.ron` catalogue (default: embedded canon).
        #[arg(default_value = "")]
        path: std::path::PathBuf,
    },
    /// Validate the authored storylines (`content/storylines/*.ron`):
    /// unique chapter IDs, `ChapterComplete` refs exist, `PlayerReputation`
    /// factions exist in the canon catalog. (S11)
    ValidateStorylines {
        /// Path to the storylines `.ron` file.
        path: std::path::PathBuf,
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
    /// Upload a content file to the reachlock-server and register it as an
    /// override. Prints the assigned content_override_id on success.
    Publish {
        /// Path to a `.ron` content file.
        path: PathBuf,
        /// Target server URL.
        #[arg(long, default_value = "http://127.0.0.1:40711")]
        server: String,
        /// Universe scope.
        #[arg(long, default_value = "all")]
        universe: String,
        /// Priority level.
        #[arg(long, default_value = "curated")]
        priority: String,
    },
}

pub fn run(cmd: ContentCommand) -> Result<(), String> {
    match cmd {
        ContentCommand::Check { root, orphans } => cmd_check(&root, orphans),
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
        ContentCommand::ValidateFactions { path } => {
            let embedded = path.as_os_str().is_empty();
            let catalog: FactionCatalog = if embedded {
                load_faction_catalog()
            } else {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("reading {}: {e}", path.display()))?;
                ron::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?
            };
            let errors = catalog.validate();
            if errors.is_empty() {
                println!(
                    "{}: valid faction catalogue — {} factions, version {}",
                    if embedded {
                        "canon.ron (embedded)".to_string()
                    } else {
                        path.display().to_string()
                    },
                    catalog.factions.len(),
                    catalog.version
                );
                Ok(())
            } else {
                for e in &errors {
                    eprintln!("  {e}");
                }
                Err(format!("{} validation error(s)", errors.len()))
            }
        }
        ContentCommand::ValidateStorylines { path } => {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            let stories: Vec<reachlock_core::faction::Storyline> =
                ron::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))?;
            let errors = validate_storylines(&stories);
            if errors.is_empty() {
                println!("{}: valid — {} storyline(s)", path.display(), stories.len(),);
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
        ContentCommand::Preview { path, out } => {
            let content = load(&path)?;
            let svg = match &content.payload {
                ContentPayload::Hull(mesh) => gen::mesh_svg(mesh),
                ContentPayload::Station { layout, .. } => gen::layout_svg(layout),
                ContentPayload::Soul(soul) => {
                    // Souls are people, not geometry — summarize instead.
                    println!(
                        "{}: soul \"{}\" ({:?}, {}) — {} trigger(s), {} secret(s)",
                        path.display(),
                        soul.name,
                        soul.species,
                        soul.identity.role,
                        soul.emotional_state.triggers.len(),
                        soul.secrets.len(),
                    );
                    return Ok(());
                }
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
                ContentPayload::HullFrame(frame) => {
                    // Frames are slot layouts over a generated silhouette —
                    // summarize; the composed hull is what the editor previews.
                    println!(
                        "{}: hull frame \"{}\" ({:?}) — {} slot(s), {} zone(s), {} decal slot(s)",
                        path.display(),
                        content.display_name,
                        frame.class,
                        frame.slots.len(),
                        frame.zones.len(),
                        frame.decal_slots.len(),
                    );
                    return Ok(());
                }
                ContentPayload::PlanetCulture(culture) => {
                    println!(
                        "{}: culture \"{}\" — language: {}, attitude: {:?}",
                        path.display(),
                        culture.cultural_id,
                        culture.language.base_language,
                        culture.attitude_toward_outsiders,
                    );
                    return Ok(());
                }
                ContentPayload::Ecosystem(eco) => {
                    println!(
                        "{}: ecosystem \"{}\" — {} biomes, {} species total",
                        path.display(),
                        content.display_name,
                        eco.biomes.len(),
                        eco.global_species_count,
                    );
                    return Ok(());
                }
                ContentPayload::Career(career) => {
                    println!(
                        "{}: career \"{}\" ({:?}) — {} rank(s), {} perk(s)",
                        path.display(),
                        career.name,
                        career.path_type,
                        career.ranks.len(),
                        career.perks.len(),
                    );
                    return Ok(());
                }
                ContentPayload::Theme(theme) => {
                    println!(
                        "{}: theme \"{}\" — {} notes, {} bpm range",
                        path.display(),
                        theme.id,
                        theme.notes.len(),
                        format_args!("{:?}", theme.bpm_range),
                    );
                    return Ok(());
                }
                ContentPayload::ScriptedEncounter(enc) => {
                    println!(
                        "{}: scripted encounter \"{}\" ({:?}) — {} scene(s), {} prereq(s)",
                        path.display(),
                        enc.id,
                        enc.encounter_type,
                        enc.scenes.len(),
                        enc.prerequisites.len(),
                    );
                    return Ok(());
                }
                ContentPayload::Trope(trope) => {
                    println!(
                        "{}: trope \"{}\" — {:?}, {} slot(s), {} branch(es)",
                        path.display(),
                        trope.id,
                        trope.trope_type,
                        trope.slots.len(),
                        trope.branches.len(),
                    );
                    return Ok(());
                }
                ContentPayload::Dialogue(dialogue) => {
                    println!(
                        "{}: dialogue — {} node(s), start: {}",
                        path.display(),
                        dialogue.nodes.len(),
                        dialogue.start_node,
                    );
                    return Ok(());
                }
                ContentPayload::Dungeon(dungeon) => {
                    println!(
                        "{}: dungeon \"{}\" — {} room(s), {} puzzle(s)",
                        path.display(),
                        dungeon.id,
                        dungeon.rooms.len(),
                        dungeon.puzzles.len(),
                    );
                    return Ok(());
                }
                ContentPayload::Event(event) => {
                    println!(
                        "{}: event \"{}\" — {} stage(s)",
                        path.display(),
                        event.id,
                        event.stages.len(),
                    );
                    return Ok(());
                }
                ContentPayload::Recipe(recipe) => {
                    println!(
                        "{}: recipe \"{}\" — {} ingredient(s), output: {} x{}",
                        path.display(),
                        recipe.id,
                        recipe.ingredients.len(),
                        recipe.output.item_id,
                        recipe.output.quantity,
                    );
                    return Ok(());
                }
                ContentPayload::RoomTemplates(templates) => {
                    // Templates are a palette, not geometry — summarize;
                    // the realized layout is what the editor previews.
                    println!(
                        "{}: room templates \"{}\" — {} template(s)",
                        path.display(),
                        content.display_name,
                        templates.len(),
                    );
                    for tpl in templates {
                        println!(
                            "  {} ({:?}) {}x{} cells, {} slot(s)",
                            tpl.id,
                            tpl.kind,
                            tpl.width,
                            tpl.height,
                            tpl.furniture_slots.len(),
                        );
                    }
                    return Ok(());
                }
                ContentPayload::Origin(origin) => {
                    println!(
                        "{}: origin \"{}\" (\"{}\") — career: {}, credits: {}",
                        path.display(),
                        origin.id,
                        origin.name,
                        origin.starting_career,
                        origin.starting_credits,
                    );
                    return Ok(());
                }
                ContentPayload::CrewPackage(pkg) => {
                    println!(
                        "{}: crew package \"{}\" — {} member(s)",
                        path.display(),
                        pkg.name,
                        pkg.members.len(),
                    );
                    return Ok(());
                }
                ContentPayload::SoulMutations(mutations) => {
                    println!(
                        "{}: soul mutations — {} arc(s)",
                        path.display(),
                        mutations.len(),
                    );
                    return Ok(());
                }
                ContentPayload::Storylines(stories) => {
                    println!(
                        "{}: faction storyline — {} story(s)",
                        path.display(),
                        stories.len(),
                    );
                    return Ok(());
                }
            };
            let out = out.unwrap_or_else(|| path.with_extension("svg"));
            std::fs::write(&out, svg).map_err(|e| format!("writing {}: {e}", out.display()))?;
            println!("wrote {}", out.display());
            Ok(())
        }
        ContentCommand::Publish { path, server, .. } => {
            let content = load(&path)?;
            let client = reqwest::blocking::Client::new();
            let url = format!("{}/content/publish", server.trim_end_matches('/'));
            let response = client
                .post(&url)
                .json(&content)
                .send()
                .map_err(|e| format!("request failed: {e}"))?;
            if response.status().is_success() {
                let resp: serde_json::Value = response
                    .json()
                    .map_err(|e| format!("parsing response: {e}"))?;
                let override_id = resp["content_override_id"].as_str().unwrap_or("unknown");
                println!("published: content_override_id = {override_id}");
                Ok(())
            } else {
                let status = response.status();
                let text = response
                    .text()
                    .unwrap_or_else(|e| format!("(response body unavailable: {e})"));
                Err(format!("failed (HTTP {status}): {text}"))
            }
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
        AssetType::HullFrame => HULL_FRAME_SCHEMA,
        AssetType::Station => STATION_SCHEMA,
        AssetType::Contract => CONTRACT_SCHEMA,
        AssetType::Career => CAREER_SCHEMA,
        AssetType::Soul => SOUL_SCHEMA,
        AssetType::Ecosystem => ECOSYSTEM_SCHEMA,
        AssetType::RoomTemplates => ROOM_TEMPLATE_SCHEMA,
        AssetType::PlanetCulture => PLANT_CULTURE_SCHEMA,
        AssetType::Theme => THEME_SCHEMA,
        AssetType::Trope => TROPE_SCHEMA,
        AssetType::ScriptedEncounter => SCRIPTED_ENCOUNTER_SCHEMA,
        AssetType::Dialogue => DIALOGUE_SCHEMA,
        AssetType::Dungeon => DUNGEON_SCHEMA,
        AssetType::Event => EVENT_SCHEMA,
        AssetType::Recipe => RECIPE_SCHEMA,
        AssetType::Origin => ORIGIN_SCHEMA,
        AssetType::CrewPackage => "",
        AssetType::SoulMutations => "",
        AssetType::Storyline => "",
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

/// `reachlock content check` — whole-tree reference integrity.
///
/// Output is grouped by failure kind and sorted, so a diff between two runs
/// shows what actually changed rather than reshuffled directory order.
fn cmd_check(root: &Path, show_orphans: bool) -> Result<(), String> {
    use reachlock_core::content::ContentTree;

    if !root.is_dir() {
        return Err(format!(
            "content root {} does not exist — pass the directory that holds origins/, souls/, …",
            root.display()
        ));
    }

    let tree = ContentTree::scan(root);
    let report = tree.check();

    println!(
        "scanned {}: {} ids defined, {} references",
        root.display(),
        tree.definitions.len(),
        tree.references.len()
    );

    if !report.unparseable.is_empty() {
        println!("\nUNPARSEABLE ({}):", report.unparseable.len());
        println!("  These files are skipped by every loader — the content is not in the game.");
        for u in &report.unparseable {
            println!("  {}\n      {}", u.file.display(), u.reason);
        }
    }

    if !report.duplicates.is_empty() {
        println!("\nDUPLICATE IDS ({}):", report.duplicates.len());
        println!("  Which file wins depends on directory order, so this is a coin flip.");
        for (kind, id, files) in &report.duplicates {
            println!("  {} \"{}\" defined in:", kind.label(), id);
            for f in files {
                println!("      {}", f.display());
            }
        }
    }

    if !report.dangling.is_empty() {
        // Group by what is missing rather than by who asked: eight origins
        // wanting eight different ships is eight problems, but eight origins
        // wanting one missing ship is one.
        let mut by_target: BTreeMap<(&str, &str), Vec<&reachlock_core::content::Ref>> =
            BTreeMap::new();
        for r in &report.dangling {
            by_target
                .entry((r.target_kind.label(), r.target_id.as_str()))
                .or_default()
                .push(r);
        }
        println!(
            "\nDANGLING REFERENCES ({} refs → {} missing ids):",
            report.dangling.len(),
            by_target.len()
        );
        for ((kind, id), refs) in &by_target {
            println!("  no {kind} with id \"{id}\" — referenced by:");
            for r in refs {
                println!("      {} ({})", r.source_id, r.field);
            }
        }
    }

    if show_orphans && !report.orphans.is_empty() {
        println!(
            "\nORPHANS ({}) — defined, referenced by nothing:",
            report.orphans.len()
        );
        for d in &report.orphans {
            println!("  {} \"{}\"  {}", d.kind.label(), d.id, d.file.display());
        }
    }

    if report.is_clean() {
        println!("\ncontent tree OK");
        Ok(())
    } else {
        Err(format!(
            "content tree has {} dangling reference(s), {} duplicate id(s), {} unparseable file(s)",
            report.dangling.len(),
            report.duplicates.len(),
            report.unparseable.len()
        ))
    }
}
