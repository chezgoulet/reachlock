//! Content-tree tools (S101 P2).
//!
//! Disk only — no live editor session, so these run unchanged in the headless
//! `--mcp-stdio` frontend. They are the tools that stop the model inventing
//! ids: `query_content` lists what actually exists, `find_references` says
//! what points at an id, `check_tree` reports what is already broken.
//!
//! Everything here goes through `reachlock_core::content::refs::ContentTree`,
//! the same scanner `reachlock content check` and the editor's startup warning
//! panel use. A second implementation would be a second answer to "what is in
//! the tree", and the first thing to drift.

use std::collections::BTreeMap;

use reachlock_core::content::refs::{ContentTree, RefKind};
use serde_json::{json, Value};

use super::{Mutability, Tool, ToolCtx, ToolOutcome};

/// Where the content tree lives.
///
/// `app::content_root()` is relative to the process cwd. That is correct for
/// the GUI (launched from the workspace root) but wrong for a headless MCP
/// server, which an external client may spawn from anywhere — so the resolved
/// root is overridable and reported in every tool result, rather than being an
/// invisible assumption that yields a confidently empty answer.
fn root() -> std::path::PathBuf {
    if let Ok(explicit) = std::env::var("REACHLOCK_CONTENT_ROOT") {
        return std::path::PathBuf::from(explicit);
    }
    crate::app::content_root()
}

fn scan() -> ContentTree {
    ContentTree::scan(&root())
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

/// Reduce a label to letters only, so spacing and punctuation stop mattering.
fn normalize(label: &str) -> String {
    label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Parse a `kind` argument against the labels `RefKind` already publishes, so
/// the vocabulary the model sees is the vocabulary the checker uses.
///
/// Deliberately tolerant. The editor's own content browser calls these
/// "Music Theme" and "Dialogue Tree" while `RefKind` calls them "theme" and
/// "dialogue", so a model reading the UI — or a person describing what they
/// want — lands on a name the strict form rejects. Matching is: exact, then
/// letters-only, then a containment match in either direction, so "music
/// theme", "Planet Culture" and "systems" all resolve.
fn parse_kind(label: &str) -> Option<RefKind> {
    if let Some(k) = ALL_KINDS.iter().copied().find(|k| k.label() == label) {
        return Some(k);
    }
    let want = normalize(label);
    if want.is_empty() {
        return None;
    }
    if let Some(k) = ALL_KINDS
        .iter()
        .copied()
        .find(|k| normalize(k.label()) == want)
    {
        return Some(k);
    }
    // Containment, but only when exactly one kind matches — "hull" would
    // otherwise silently pick between `hull` and `hull frame`.
    let mut hits = ALL_KINDS.iter().copied().filter(|k| {
        let have = normalize(k.label());
        have.contains(&want) || want.contains(&have)
    });
    let first = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    Some(first)
}

const ALL_KINDS: &[RefKind] = &[
    RefKind::Origin,
    RefKind::Soul,
    RefKind::Career,
    RefKind::Faction,
    RefKind::ShipTemplate,
    RefKind::Crew,
    RefKind::System,
    RefKind::Archetype,
    RefKind::Location,
    RefKind::Station,
    RefKind::Contract,
    RefKind::Ecosystem,
    RefKind::Culture,
    RefKind::Theme,
    RefKind::Trope,
    RefKind::Encounter,
    RefKind::Dialogue,
    RefKind::Dungeon,
    RefKind::Event,
    RefKind::Recipe,
    RefKind::Hull,
    RefKind::HullFrame,
    RefKind::RoomTemplates,
];

/// Exhaustive over [`RefKind`], so adding a variant fails to compile and
/// points the author at `ALL_KINDS` directly above.
///
/// That list was hand-maintained and held twelve of the twenty-three kinds,
/// so `kind: planet culture` — and ten others — came back "unknown kind" with
/// a valid-list that silently omitted them. A model cannot route around a
/// vocabulary that lies about itself; it just retries until it runs out of
/// turns, which is exactly what happened.
///
/// `cfg(test)` because its only job is to make the compiler check the match:
/// nothing calls it at runtime, and `make check` runs clippy over all targets,
/// so a new variant still breaks the gate.
#[cfg(test)]
fn is_known_kind(k: RefKind) -> bool {
    match k {
        RefKind::Origin
        | RefKind::Soul
        | RefKind::Career
        | RefKind::Faction
        | RefKind::ShipTemplate
        | RefKind::Crew
        | RefKind::System
        | RefKind::Archetype
        | RefKind::Location
        | RefKind::Station
        | RefKind::Contract
        | RefKind::Ecosystem
        | RefKind::Culture
        | RefKind::Theme
        | RefKind::Trope
        | RefKind::Encounter
        | RefKind::Dialogue
        | RefKind::Dungeon
        | RefKind::Event
        | RefKind::Recipe
        | RefKind::Hull
        | RefKind::HullFrame
        | RefKind::RoomTemplates => ALL_KINDS.contains(&k),
    }
}

fn kind_labels() -> Vec<&'static str> {
    ALL_KINDS.iter().map(|k| k.label()).collect()
}

/// Rejection that helps rather than restates the question.
///
/// An ambiguous label ("hull" matching both `hull` and `hull frame`) gets the
/// candidates; anything else gets the full list. Returning a bare "unknown"
/// is what produced a retry loop in a real session.
fn unknown_kind_message(label: &str) -> String {
    let want = normalize(label);
    let near: Vec<&str> = ALL_KINDS
        .iter()
        .map(|k| k.label())
        .filter(|l| {
            let have = normalize(l);
            !want.is_empty() && (have.contains(&want) || want.contains(&have))
        })
        .collect();
    if near.len() > 1 {
        format!(
            "`{label}` is ambiguous — it could mean {}. Say which.",
            near.join(" or ")
        )
    } else {
        format!(
            "Unknown kind `{label}`. Valid kinds: {}",
            kind_labels().join(", ")
        )
    }
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "query_content",
            description:
                "List the content that exists in the tree — each id with the file it lives in \
                 — optionally filtered by kind or by a substring of the id. Use this before \
                 referring to any id, and to get the path for read_file rather than guessing \
                 one: ids do not map predictably to filenames. An id that is not in this list \
                 does not exist, and writing it into a document creates a dangling reference \
                 that fails `make check`.",
            input_schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "description": "Restrict to one kind of content.",
                            "enum": kind_labels(),
                        },
                        "contains": {
                            "type": "string",
                            "description": "Case-insensitive substring of the id.",
                        },
                    },
                    "additionalProperties": false,
                })
            },
            mutability: Mutability::ReadOnly,
            needs_session: false,
            run: run_query_content,
        },
        Tool {
            name: "describe_content_type",
            description:
                "Everything needed to author one kind of content, in a single call: which \
                 directory it lives in, how many exist, the JSON schema its documents are \
                 validated against, and the path of a real authored example to read. Call this \
                 first when working with a type you have not touched yet — it replaces the \
                 several rounds of listing and path-guessing that would otherwise be needed to \
                 learn the same thing.",
            input_schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "description": "A kind name as reported by query_content.",
                            "enum": kind_labels(),
                        },
                    },
                    "required": ["kind"],
                    "additionalProperties": false,
                })
            },
            mutability: Mutability::ReadOnly,
            needs_session: false,
            run: run_describe_content_type,
        },
        Tool {
            name: "find_references",
            description: "Show every place an id is referenced from, and every id a given source \
                 references. Use before renaming or deleting content to see what would break.",
            input_schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "The content id to look up.",
                        },
                    },
                    "required": ["id"],
                    "additionalProperties": false,
                })
            },
            mutability: Mutability::ReadOnly,
            needs_session: false,
            run: run_find_references,
        },
        Tool {
            name: "check_tree",
            description:
                "Report the current integrity of the whole content tree: references pointing at \
                 ids nothing defines, ids defined more than once, and files that parse as no \
                 known payload. This is what `make check-content` runs. Call it after writing \
                 content to confirm the tree is still clean.",
            input_schema: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
            mutability: Mutability::ReadOnly,
            needs_session: false,
            run: run_check_tree,
        },
        Tool {
            name: "read_file",
            description:
                "Read one content file verbatim, as RON. Take the path from query_content or \
                 describe_content_type — do not construct one, because a directory name is not \
                 the kind name and an id is not always the filename. Read an existing file of a \
                 type before authoring more of it: RON has traps a schema does not show (fixed- \
                 size arrays are tuples, newtypes need their parens, enum variants are \
                 snake_case, and most payloads need a ContentFile envelope).",
            input_schema: || {
                json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "A path exactly as query_content or describe_content_type reported it.",
                        },
                    },
                    "required": ["path"],
                    "additionalProperties": false,
                })
            },
            mutability: Mutability::ReadOnly,
            needs_session: false,
            run: run_read_file,
        },
    ]
}

fn run_query_content(args: &Value, _ctx: &ToolCtx) -> ToolOutcome {
    let kind_filter = match arg_str(args, "kind") {
        Some(label) => match parse_kind(label) {
            Some(k) => Some(k),
            None => return ToolOutcome::error(unknown_kind_message(label)),
        },
        None => None,
    };
    let contains = arg_str(args, "contains").map(|s| s.to_lowercase());

    let tree = scan();
    let root = root();
    // `(id, path)`, not just id. Ids do not map predictably to filenames or
    // to directories, so returning ids alone forced a guess-and-retry loop:
    // one real session spent ten of its twenty-four turns trying
    // `ecosystem/…`, `ecosystems/…`, `planet_culture/…` against a tool that
    // already knew the answer.
    let mut by_kind: BTreeMap<&'static str, Vec<(String, String)>> = BTreeMap::new();
    for d in &tree.definitions {
        if kind_filter.is_some_and(|k| k != d.kind) {
            continue;
        }
        if contains
            .as_ref()
            .is_some_and(|c| !d.id.to_lowercase().contains(c.as_str()))
        {
            continue;
        }
        let rel = d
            .file
            .strip_prefix(&root)
            .unwrap_or(&d.file)
            .display()
            .to_string();
        by_kind
            .entry(d.kind.label())
            .or_default()
            .push((d.id.clone(), rel));
    }

    if by_kind.is_empty() {
        // Say where we looked. A confidently empty answer from the wrong
        // directory is worse than an error.
        return ToolOutcome::ok(format!("No matching content under {}.", root.display()));
    }

    let mut out = String::new();
    for (kind, mut entries) in by_kind {
        entries.sort();
        out.push_str(&format!("{kind} ({}):\n", entries.len()));
        for (id, path) in entries {
            out.push_str(&format!("  {id}  —  {path}\n"));
        }
    }
    out.push_str("\nPaths are ready to pass to read_file as-is.");
    ToolOutcome::ok(out)
}

/// Map a `RefKind` to the editor `ContentType` that edits it, so the schema
/// lookup goes through the same table the editor tabs use.
fn content_type_for(kind: RefKind) -> Option<crate::app::ContentType> {
    use crate::app::ContentType as C;
    Some(match kind {
        RefKind::Origin => C::Origin,
        RefKind::Soul => C::Soul,
        RefKind::Career => C::Career,
        RefKind::Faction => C::Faction,
        RefKind::Crew => C::CrewPackage,
        RefKind::System => C::ChartedSystem,
        RefKind::Archetype => C::EnemyArchetype,
        RefKind::Location => C::Location,
        RefKind::Station => C::Station,
        RefKind::Contract => C::Contract,
        RefKind::Ecosystem => C::Ecosystem,
        RefKind::Culture => C::PlanetCulture,
        RefKind::Theme => C::Theme,
        RefKind::Trope => C::Trope,
        RefKind::Encounter => C::ScriptedEncounter,
        RefKind::Dialogue => C::Dialogue,
        RefKind::Dungeon => C::Dungeon,
        RefKind::Event => C::Event,
        RefKind::Recipe => C::Recipe,
        RefKind::HullFrame => C::HullFrame,
        RefKind::RoomTemplates => C::RoomTemplates,
        // A ship template is catalog data the client reads directly, and a
        // `Hull` envelope carries a raw mesh that no tab edits as such.
        // Saying so beats handing back a schema for a different type.
        RefKind::ShipTemplate | RefKind::Hull => return None,
    })
}

fn run_describe_content_type(args: &Value, _ctx: &ToolCtx) -> ToolOutcome {
    let Some(label) = arg_str(args, "kind") else {
        return ToolOutcome::error(format!(
            "`kind` is required. One of: {}",
            kind_labels().join(", ")
        ));
    };
    let Some(kind) = parse_kind(label) else {
        return ToolOutcome::error(unknown_kind_message(label));
    };

    let tree = scan();
    let root = root();
    let mut defs: Vec<&reachlock_core::content::refs::Definition> =
        tree.definitions.iter().filter(|d| d.kind == kind).collect();
    defs.sort_by(|a, b| a.id.cmp(&b.id));

    let mut out = format!("{label}\n");

    // Directory, taken from where the content actually is rather than from a
    // second table that could disagree with the tree.
    let dirs: std::collections::BTreeSet<String> = defs
        .iter()
        .filter_map(|d| {
            d.file
                .strip_prefix(&root)
                .ok()?
                .parent()
                .map(|p| p.display().to_string())
        })
        .filter(|p| !p.is_empty())
        .collect();
    if dirs.is_empty() {
        out.push_str("directory: nothing of this kind is authored yet.\n");
    } else {
        out.push_str(&format!(
            "directory: {}\n",
            dirs.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    out.push_str(&format!("authored: {}\n", defs.len()));

    match content_type_for(kind) {
        Some(ct) => {
            let cache = crate::schema::SchemaCache::load_all();
            match cache.get(&ct) {
                Some(schema) => {
                    out.push_str("\nschema:\n");
                    out.push_str(&schema.compact_prompt());
                    out.push('\n');
                }
                None => out.push_str(
                    "\nschema: none is authored for this type; follow the example below.\n",
                ),
            }
        }
        None => out.push_str(
            "\nschema: this kind is reference data with no editor tab; follow the example.\n",
        ),
    }

    if let Some(example) = defs.first() {
        let rel = example
            .file
            .strip_prefix(&root)
            .unwrap_or(&example.file)
            .display();
        out.push_str(&format!(
            "\nexample: read_file path `{rel}` (id `{}`) to see how this is actually \
             authored. The schema does not show RON's shape — read one before writing one.\n",
            example.id
        ));
    }

    ToolOutcome::ok(out)
}

fn run_find_references(args: &Value, _ctx: &ToolCtx) -> ToolOutcome {
    let Some(id) = arg_str(args, "id") else {
        return ToolOutcome::error("`id` is required and must be a non-empty string.");
    };
    let tree = scan();

    let defined: Vec<String> = tree
        .definitions
        .iter()
        .filter(|d| d.id == id)
        .map(|d| format!("  defined as {} in {}", d.kind.label(), d.file.display()))
        .collect();

    let incoming: Vec<String> = tree
        .references
        .iter()
        .filter(|r| r.target_id == id)
        .map(|r| {
            format!(
                "  {} references it via {} ({})",
                r.source_id,
                r.field,
                r.source_file.display()
            )
        })
        .collect();

    let outgoing: Vec<String> = tree
        .references
        .iter()
        .filter(|r| r.source_id == id)
        .map(|r| {
            format!(
                "  references {} ({}) via {}",
                r.target_id,
                r.target_kind.label(),
                r.field
            )
        })
        .collect();

    if defined.is_empty() && incoming.is_empty() && outgoing.is_empty() {
        return ToolOutcome::ok(format!(
            "`{id}` is not defined and nothing references it (searched {}).",
            root().display()
        ));
    }

    let mut out = format!("{id}\n");
    for section in [
        ("definition", defined),
        ("referenced by", incoming),
        ("references", outgoing),
    ] {
        if !section.1.is_empty() {
            out.push_str(&format!("{}:\n{}\n", section.0, section.1.join("\n")));
        }
    }
    ToolOutcome::ok(out)
}

fn run_check_tree(_args: &Value, _ctx: &ToolCtx) -> ToolOutcome {
    let tree = scan();
    let report = tree.check();

    let mut out = format!(
        "scanned {}: {} ids defined, {} references\n",
        root().display(),
        tree.definitions.len(),
        tree.references.len()
    );

    if report.is_clean() {
        out.push_str("content tree OK");
        return ToolOutcome::ok(out);
    }

    for r in &report.dangling {
        out.push_str(&format!(
            "DANGLING: {} field {} points at `{}` ({}), which nothing defines\n",
            r.source_id,
            r.field,
            r.target_id,
            r.target_kind.label()
        ));
    }
    for (kind, id, files) in &report.duplicates {
        out.push_str(&format!(
            "DUPLICATE: {} `{}` defined in {}\n",
            kind.label(),
            id,
            files
                .iter()
                .map(|f| f.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for u in &report.unparseable {
        // Worth spelling out: an unparseable file is skipped by every loader,
        // so the content is absent from the game with no error at the point
        // of use.
        out.push_str(&format!(
            "UNPARSEABLE: {} — {} (every loader skips this file)\n",
            u.file.display(),
            u.reason
        ));
    }
    // Not an error outcome: a broken tree is a true answer to the question,
    // and the model needs to read it to fix it.
    ToolOutcome::ok(out)
}

fn run_read_file(args: &Value, _ctx: &ToolCtx) -> ToolOutcome {
    let Some(rel) = arg_str(args, "path") else {
        return ToolOutcome::error("`path` is required and must be a non-empty string.");
    };

    let root = root();
    let joined = root.join(rel);
    // Confine reads to the content root. `path` is model-supplied, and an
    // absolute path or a `..` chain would otherwise read anything the editor
    // process can — including the settings file, which holds API keys.
    let Ok(canonical) = joined.canonicalize() else {
        return ToolOutcome::error(format!("No such file under the content root: {rel}"));
    };
    let Ok(canonical_root) = root.canonicalize() else {
        return ToolOutcome::error(format!("Content root {} is unreadable.", root.display()));
    };
    if !canonical.starts_with(&canonical_root) {
        return ToolOutcome::error(format!(
            "`{rel}` resolves outside the content root. Paths must stay inside {}.",
            canonical_root.display()
        ));
    }

    match std::fs::read_to_string(&canonical) {
        Ok(text) => ToolOutcome::ok(text),
        Err(e) => ToolOutcome::error(format!("Could not read {rel}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::mode::Mode;
    use crate::agent::tools::ToolRegistry;

    /// Point the tools at the real authored tree regardless of the cwd the
    /// test runner happens to use. `schema::schemas_dir()` already carries the
    /// `CARGO_MANIFEST_DIR` fallback, and its parent is the content root.
    ///
    /// Set once and never cleared. The first version set the variable, ran the
    /// closure, then removed it — but the environment is process-global and
    /// the test harness runs threads in parallel, so one test cleared the root
    /// out from under another mid-call and the failure looked like a missing
    /// file rather than a racing test.
    fn with_real_root<T>(f: impl FnOnce() -> T) -> T {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let dir = crate::schema::schemas_dir();
            let root = dir.parent().expect("schemas dir has a parent");
            std::env::set_var("REACHLOCK_CONTENT_ROOT", root);
        });
        f()
    }

    fn content_root_for_tests() -> std::path::PathBuf {
        with_real_root(root)
    }

    /// The vocabulary the tool advertises must be the whole vocabulary the
    /// checker uses. It once held twelve of twenty-three, so a model asking
    /// for a real kind was told it did not exist.
    #[test]
    fn every_kind_is_in_the_advertised_vocabulary() {
        for k in ALL_KINDS {
            assert!(is_known_kind(*k), "{k:?} missing from ALL_KINDS");
        }
        // Kinds that a real session asked for and was refused.
        for label in [
            "planet culture",
            "music theme",
            "dialogue tree",
            "room templates",
        ] {
            assert!(
                parse_kind(label).is_some(),
                "`{label}` should be a valid kind; valid list is {:?}",
                kind_labels()
            );
        }
    }

    /// Ids do not map to filenames. The whole point of returning paths is
    /// that this pair cannot be guessed.
    #[test]
    fn query_content_reports_the_file_for_each_id() {
        let out = with_real_root(|| {
            ToolRegistry::new().dispatch(
                "query_content",
                &json!({"kind": "ecosystem"}),
                Mode::Plan,
                &ToolCtx::headless(),
            )
        });
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains(".ron"), "no path in:\n{}", out.content);
        assert!(
            out.content.contains("read_file"),
            "the result should say the paths are usable directly:\n{}",
            out.content
        );
    }

    #[test]
    fn describe_content_type_gives_directory_schema_and_an_example() {
        let out = with_real_root(|| {
            ToolRegistry::new().dispatch(
                "describe_content_type",
                &json!({"kind": "soul"}),
                Mode::Plan,
                &ToolCtx::headless(),
            )
        });
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("directory: souls"), "{}", out.content);
        assert!(out.content.contains("schema:"), "{}", out.content);
        assert!(
            out.content.contains("example: read_file path `souls/"),
            "{}",
            out.content
        );
    }

    /// A kind with no authored content must say so rather than claim a
    /// directory it inferred from nothing.
    #[test]
    fn describe_content_type_is_honest_about_an_empty_kind() {
        let out = with_real_root(|| {
            ToolRegistry::new().dispatch(
                "describe_content_type",
                &json!({"kind": "dungeon"}),
                Mode::Plan,
                &ToolCtx::headless(),
            )
        });
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("authored: 0"), "{}", out.content);
    }

    #[test]
    fn query_content_finds_authored_souls() {
        let out = with_real_root(|| {
            ToolRegistry::new().dispatch(
                "query_content",
                &json!({"kind": "soul"}),
                Mode::Plan,
                &ToolCtx::headless(),
            )
        });
        assert!(!out.is_error, "{}", out.content);
        assert!(
            out.content.contains("tib"),
            "expected an authored soul id, got:\n{}",
            out.content
        );
    }

    #[test]
    fn query_content_rejects_an_unknown_kind_with_the_valid_list() {
        let out = ToolRegistry::new().dispatch(
            "query_content",
            &json!({"kind": "nonsense"}),
            Mode::Plan,
            &ToolCtx::headless(),
        );
        assert!(out.is_error);
        assert!(out.content.contains("soul"), "{}", out.content);
    }

    #[test]
    fn find_references_reports_an_unknown_id_plainly() {
        let out = with_real_root(|| {
            ToolRegistry::new().dispatch(
                "find_references",
                &json!({"id": "definitely_not_a_real_id"}),
                Mode::Plan,
                &ToolCtx::headless(),
            )
        });
        assert!(!out.is_error);
        assert!(out.content.contains("not defined"), "{}", out.content);
    }

    #[test]
    fn check_tree_reports_the_authored_tree() {
        let out = with_real_root(|| {
            ToolRegistry::new().dispatch("check_tree", &json!({}), Mode::Plan, &ToolCtx::headless())
        });
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("ids defined"), "{}", out.content);
    }

    #[test]
    fn read_file_returns_authored_ron() {
        // The file is discovered, not named. `make check-purity` forbids
        // engine code from naming a specific ship or crew member, and a test
        // that hardcodes an authored soul id is exactly that.
        let out = with_real_root(|| {
            let souls = content_root_for_tests().join("souls");
            let first = std::fs::read_dir(&souls)
                .expect("souls/ exists")
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .min()
                .expect("at least one authored soul");
            let rel = format!("souls/{}", first.file_name().unwrap().to_str().unwrap());
            ToolRegistry::new().dispatch(
                "read_file",
                &json!({ "path": rel }),
                Mode::Plan,
                &ToolCtx::headless(),
            )
        });
        assert!(!out.is_error, "{}", out.content);
        // Returned verbatim, including the hand-written comments RON drops on
        // a deserialize -> serialize round trip. That is the point of reading
        // the file rather than a parsed document: the model sees how the
        // content is actually authored.
        assert!(out.content.contains("id:"), "{}", out.content);
    }

    /// `path` is model-supplied. Without the containment check it could read
    /// the editor's own settings file, which holds API keys.
    #[test]
    fn read_file_refuses_to_escape_the_content_root() {
        for attempt in [
            "../../save/editor-settings.ron",
            "../../../etc/passwd",
            "/etc/passwd",
        ] {
            let out = with_real_root(|| {
                ToolRegistry::new().dispatch(
                    "read_file",
                    &json!({ "path": attempt }),
                    Mode::Plan,
                    &ToolCtx::headless(),
                )
            });
            assert!(
                out.is_error,
                "`{attempt}` was allowed to escape:\n{}",
                out.content
            );
        }
    }

    #[test]
    fn missing_required_arguments_are_errors_not_panics() {
        let reg = ToolRegistry::new();
        for name in ["find_references", "read_file"] {
            let out = reg.dispatch(name, &json!({}), Mode::Plan, &ToolCtx::headless());
            assert!(out.is_error, "`{name}` accepted empty arguments");
        }
    }
}
