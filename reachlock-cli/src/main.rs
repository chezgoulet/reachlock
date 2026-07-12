//! reachlock — CLI tools (spec §3): run generators from the terminal,
//! emit/verify determinism manifests, preview assets without a Bevy window.

mod content;
mod determinism;
mod gen;

use clap::{Parser, Subcommand};

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
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}
