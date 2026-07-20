//! reachlock codex — agent self-service CLI (S30).
//!
//! Subcommands that answer structural questions about the codebase without
//! grepping. Output is markdown (human) or JSON (machine), both to stdout.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum CodexCommand {
    /// Print a pattern template for a task type.
    Pattern {
        /// Task kind (e.g. "add-generator", "add-system").
        #[arg(long)]
        r#type: String,
    },
    /// Summarize a sprint brief with cross-references to existing code.
    Brief {
        /// Sprint id (e.g. "S19").
        id: String,
    },
    /// List public types in a crate.
    #[command(name = "types")]
    CrateTypes {
        #[arg(long)]
        crate_name: String,
    },
    /// Print the dependency graph for a module path.
    Deps {
        /// Module path (e.g. "core::generator::hull").
        #[arg(long)]
        module: String,
    },
    /// Regenerate agent context files (AGENT-INDEX.md, AGENT-TYPES.md).
    Update,
    /// Scan a diff (from a git ref) for iron-rule violations.
    Diff {
        /// Git reference to diff against (e.g. "origin/testing").
        #[arg(long)]
        since: String,
    },
}

pub fn run(cmd: CodexCommand) -> Result<(), String> {
    match cmd {
        CodexCommand::Pattern { r#type } => cmd_pattern(&r#type),
        CodexCommand::Brief { id } => cmd_brief(&id),
        CodexCommand::CrateTypes { crate_name } => cmd_types(&crate_name),
        CodexCommand::Deps { module } => cmd_deps(&module),
        CodexCommand::Update => cmd_update(),
        CodexCommand::Diff { since } => cmd_diff(&since),
    }
}

fn cmd_pattern(kind: &str) -> Result<(), String> {
    let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/agents/patterns");
    let path = format!("{base}/{kind}.md");
    let text = std::fs::read_to_string(&path)
        .map_err(|_| format!("pattern not found: {kind} — expected at {base}/{kind}.md"))?;
    println!("{}", text);
    Ok(())
}

fn cmd_brief(id: &str) -> Result<(), String> {
    let brief_path = format!("docs/sprints/{id}.md");
    let text = std::fs::read_to_string(&brief_path)
        .map_err(|e| format!("cannot read {brief_path}: {e}"))?;
    println!("# {id} — Sprint Brief\n");
    println!("```\n{}\n```", text);
    Ok(())
}

fn cmd_types(crate_name: &str) -> Result<(), String> {
    let src_dir = format!("reachlock-{crate_name}/src");
    if !std::path::Path::new(&src_dir).is_dir() {
        return Err(format!("unknown crate: {crate_name}"));
    }
    println!("# Public Types in reachlock-{crate_name}\n");
    let mut types: Vec<String> = Vec::new();
    for entry in walkdir::WalkDir::new(&src_dir) {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        for line in text.lines() {
            let trimmed = line.trim();
            // Match: pub struct/enum/type/trait Name
            for kw in &["struct", "enum", "type", "trait"] {
                let pat = format!("pub {kw} ");
                if let Some(rest) = trimmed.strip_prefix(&pat) {
                    let name = rest
                        .split(|c: char| c.is_whitespace() || c == '(' || c == '<')
                        .next()
                        .unwrap_or("");
                    if !name.is_empty() && !name.contains(';') {
                        types.push(format!("- `{name}` ({kw})"));
                    }
                }
            }
        }
    }
    types.sort();
    types.dedup();
    for t in types {
        println!("{t}");
    }
    Ok(())
}

fn cmd_deps(module: &str) -> Result<(), String> {
    // Simple heuristic: find the module file, extract use statements.
    let file_path = module.replace("::", "/");
    let candidates = [format!("{file_path}.rs"), format!("{file_path}/mod.rs")];
    let mut text = None;
    for c in &candidates {
        let full = format!("reachlock-{c}");
        if let Ok(t) = std::fs::read_to_string(&full) {
            text = Some(t);
            break;
        }
    }
    let text = text.ok_or_else(|| format!("module not found: {module}"))?;

    println!("# Dependencies of {module}\n");
    println!("## Imports");
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") && !trimmed.contains("::{") {
            println!("- `{trimmed}`");
        }
    }

    // Find reverse deps by scanning lib.rs/mod.rs.
    println!("\n## Imported by (from lib.rs/mod.rs)");
    for (lib, label) in &[
        ("reachlock-core/src/lib.rs", "reachlock-core"),
        ("reachlock-client/src/main.rs", "reachlock-client"),
        ("reachlock-server/src/main.rs", "reachlock-server"),
        ("reachlock-cli/src/main.rs", "reachlock-cli"),
    ] {
        if let Ok(t) = std::fs::read_to_string(lib) {
            if t.contains(&file_path.replace('/', "::")) {
                println!("- `{label}`");
            }
        }
    }
    Ok(())
}

fn cmd_update() -> Result<(), String> {
    let agents_dir = std::path::Path::new("docs/agents");
    std::fs::create_dir_all(agents_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(agents_dir.join("patterns")).map_err(|e| e.to_string())?;

    // Write AGENT-INDEX.md
    let index = generate_agent_index()?;
    std::fs::write(agents_dir.join("AGENT-INDEX.md"), &index)
        .map_err(|e| format!("write AGENT-INDEX.md: {e}"))?;

    // Write AGENT-TYPES.md
    let types = generate_agent_types()?;
    std::fs::write(agents_dir.join("AGENT-TYPES.md"), &types)
        .map_err(|e| format!("write AGENT-TYPES.md: {e}"))?;

    println!("Updated docs/agents/AGENT-INDEX.md and AGENT-TYPES.md");
    Ok(())
}

fn generate_agent_index() -> Result<String, String> {
    let mut lines = vec![
        "# ReachLock Agent Index".into(),
        format!("Generated: {}", chrono_timestamp()),
        "".into(),
    ];

    // Crate map
    lines.push("## Crate Map".into());
    for (name, dir) in &[
        ("reachlock-core", "reachlock-core/src"),
        ("reachlock-client", "reachlock-client/src"),
        ("reachlock-server", "reachlock-server/src"),
        ("reachlock-cli", "reachlock-cli/src"),
    ] {
        let count = count_files(dir);
        lines.push(format!("- {name}: {count} modules"));
    }

    // Sprint status
    lines.push("\n## Sprint Status".into());
    let branch = git_branch();
    lines.push(format!("- Active branch: {branch}"));
    if let Ok(text) = std::fs::read_to_string("docs/sprints/00-INDEX.md") {
        for line in text.lines() {
            if line.contains("|") && !line.starts_with("|") && !line.starts_with("+-") {
                let trimmed = line.trim().trim_matches('|');
                lines.push(format!("- {}", trimmed));
            }
        }
    }

    lines.push("\n## Build Commands".into());
    lines.push("```".into());
    lines.push("cargo test -p reachlock-core        # core only".into());
    lines.push("cargo test -p reachlock-client      # client only".into());
    lines.push("make check                           # full gate".into());
    lines.push("make web                             # WASM release build".into());
    lines.push("```".into());

    Ok(lines.join("\n"))
}

fn generate_agent_types() -> Result<String, String> {
    let mut lines = vec![
        "# ReachLock Agent Type Index".into(),
        format!("Generated: {}", chrono_timestamp()),
        "".into(),
    ];

    for (crate_name, src_dir) in &[
        ("reachlock-core", "reachlock-core/src"),
        ("reachlock-client", "reachlock-client/src"),
    ] {
        lines.push(format!("\n## {crate_name}"));
        for entry in walkdir::WalkDir::new(src_dir) {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            let rel = path.to_string_lossy().replace('\\', "/");
            for line in text.lines() {
                let trimmed = line.trim();
                for kw in &["pub struct", "pub enum", "pub type", "pub trait"] {
                    if let Some(rest) = trimmed.strip_prefix(kw) {
                        let name = rest
                            .split(|c: char| c.is_whitespace() || c == '(' || c == '<')
                            .next()
                            .unwrap_or("");
                        if !name.is_empty() && !name.contains(';') {
                            lines.push(format!("- `{name}` — {rel}"));
                        }
                    }
                }
            }
        }
    }

    Ok(lines.join("\n"))
}

fn cmd_diff(since: &str) -> Result<(), String> {
    let output = std::process::Command::new("git")
        .args([
            "diff",
            since,
            "HEAD",
            "--",
            "reachlock-core/src/",
            "reachlock-client/src/",
        ])
        .output()
        .map_err(|e| format!("git diff failed: {e}"))?;
    let diff = String::from_utf8_lossy(&output.stdout);
    let mut violations = Vec::new();

    // Same checks as the agent gate, but on diff content.
    for line in diff.lines() {
        if line.starts_with('+') && line != "+++" {
            let content = &line[1..];
            if content.contains("f32") || content.contains("f64") {
                violations.push(format!("no-floats: {content}"));
            }
        }
    }

    for v in &violations {
        println!("VIOLATION: {v}");
    }
    if violations.is_empty() {
        println!("No iron-rule violations in diff.");
    }
    Ok(())
}

// Helpers

fn count_files(dir: &str) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|e| e == "rs") {
                count += 1;
            }
        }
    }
    count
}

fn git_branch() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

fn chrono_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}
