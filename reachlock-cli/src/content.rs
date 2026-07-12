//! `reachlock content …` — validate and preview authored `.ron` content
//! files (spec §10, Stage 2: CLI Validation). No Bevy window needed: the
//! structural integrity checks live in `reachlock-core::content::validate`,
//! and previews reuse the SVG/PPM exporters from the `gen` module — the
//! same path a generated asset would take (spec §10: "the bridge doesn't
//! know the difference").

use clap::Subcommand;
use reachlock_core::content::{validate_content, ContentFile, ContentPayload};
use std::path::{Path, PathBuf};

use crate::gen;

#[derive(Subcommand)]
pub enum ContentCommand {
    /// Validate an authored content file's structural integrity (seed range,
    /// universe, no degenerate triangles, doors reference real rooms).
    /// Exit 0 if clean, 1 if any check fails — each failure is named.
    Validate {
        /// Path to a `.ron` content file.
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
        ContentCommand::Validate { path } => {
            let content = load(&path)?;
            let errors = validate_content(&content);
            if errors.is_empty() {
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

/// Read and deserialize a `.ron` content file into the shared envelope.
fn load(path: &Path) -> Result<ContentFile, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    ron::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
}
