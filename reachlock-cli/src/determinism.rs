//! `reachlock determinism …` — the cross-platform test harness (spec §5).
//!
//! CI builds this binary for each target, runs `emit` on each, and `check`s
//! the manifests against each other. Bit-for-bit or the merge fails.

use clap::Subcommand;
use reachlock_core::determinism::{diff, manifest, Manifest};

#[derive(Subcommand)]
pub enum DeterminismCommand {
    /// Print the local manifest (generator checksums over canonical seeds)
    /// as JSON on stdout.
    Emit,
    /// Compare the local manifest against a previously emitted one; exit
    /// nonzero on any divergence.
    Check {
        /// Path to a manifest JSON produced by `emit` (possibly on another
        /// platform).
        manifest_path: std::path::PathBuf,
    },
}

pub fn run(cmd: DeterminismCommand) -> Result<(), String> {
    match cmd {
        DeterminismCommand::Emit => {
            let m = manifest();
            println!(
                "{}",
                serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?
            );
            Ok(())
        }
        DeterminismCommand::Check { manifest_path } => {
            let text = std::fs::read_to_string(&manifest_path)
                .map_err(|e| format!("reading {}: {e}", manifest_path.display()))?;
            let theirs: Manifest =
                serde_json::from_str(&text).map_err(|e| format!("parsing manifest: {e}"))?;
            let ours = manifest();
            let problems = diff(&ours, &theirs);
            if problems.is_empty() {
                println!(
                    "determinism check passed: {} entries identical",
                    ours.entries.len()
                );
                Ok(())
            } else {
                for p in &problems {
                    eprintln!("DIVERGENCE: {p}");
                }
                Err(format!(
                    "{} divergence(s) against {}",
                    problems.len(),
                    manifest_path.display()
                ))
            }
        }
    }
}
