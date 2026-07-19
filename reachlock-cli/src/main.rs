//! reachlock — CLI tools (spec §3): run generators from the terminal,
//! emit/verify determinism manifests, preview assets without a Bevy window.

mod agent_check;
mod codex;
mod content;
mod determinism;
mod gen;
mod mod_cmd;

use clap::{Parser, Subcommand};

#[derive(Subcommand)]
enum AgentCheckCommand {
    /// Run the full agent CI battery.
    Agent {
        /// Output machine-parseable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Parser)]
#[command(name = "reachlock", version, about = "ReachLock v2 tooling")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a generator and inspect or export its output.
    Gen {
        #[command(subcommand)]
        what: gen::GenCommand,
    },
    /// Cross-platform determinism harness (spec §5).
    Determinism {
        #[command(subcommand)]
        action: determinism::DeterminismCommand,
    },
    /// Mod packing, installation, and listing (S22).
    Mod {
        #[command(subcommand)]
        action: mod_cmd::ModCommand,
    },
    /// Agent CI gate: run iron-rule checks (S30).
    Check {
        #[command(subcommand)]
        action: AgentCheckCommand,
    },
    /// Agent self-service queries: brief, types, deps, update, diff (S30).
    Codex {
        #[command(subcommand)]
        action: codex::CodexCommand,
    },
    /// Validate and preview authored content files (spec §10).
    Content {
        #[command(subcommand)]
        action: content::ContentCommand,
    },
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Gen { what } => gen::run(what),
        Command::Determinism { action } => determinism::run(action),
        Command::Content { action } => content::run(action),
        Command::Mod { action } => mod_cmd::run(action),
        Command::Check { action: AgentCheckCommand::Agent { json } } => {
            let results = agent_check::run();
            if json {
                let mut first = true;
                println!("[");
                for r in &results {
                    if !first { println!(","); }
                    first = false;
                    print!("  {{\"name\":\"{}\",\"passed\":{},\"detail\":\"{}\"}}",
                        r.name, r.passed, r.detail);
                }
                println!("\n]");
            } else {
                for r in &results {
                    let status = if r.passed { "✓" } else { "✗" };
                    println!("{status} {}: {}", r.name, r.detail);
                }
            }
            if results.iter().all(|r| r.passed) {
                Ok(())
            } else {
                Err("some checks failed".into())
            }
        }
        Command::Codex { action } => codex::run(action),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}
