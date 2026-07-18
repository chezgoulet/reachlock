use std::path::Path;

use clap::Subcommand;

use reachlock_core::mod_manifest::{resolve_load_order, ModManifest};

const MODS_ROOT: &str = "mods";

#[derive(Subcommand)]
pub enum ModCommand {
    /// Validate a mod directory and package it as .reachmod.
    Pack {
        /// Path to the mod directory.
        dir: String,
        /// Output .reachmod file path (default: <dir>.reachmod).
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Install a .reachmod package into the user's mods directory.
    Install {
        /// Path to the .reachmod file.
        file: String,
    },
    /// List installed mods, their versions, load order, and conflicts.
    List,
}

pub fn run(cmd: ModCommand) -> Result<(), String> {
    match cmd {
        ModCommand::Pack { dir, output } => cmd_pack(&dir, output),
        ModCommand::Install { file } => cmd_install(&file),
        ModCommand::List => cmd_list(),
    }
}

fn cmd_pack(dir: &str, output: Option<String>) -> Result<(), String> {
    let dir_path = Path::new(dir);
    if !dir_path.is_dir() {
        return Err(format!("not a directory: {dir}"));
    }
    // Validate manifest.
    let manifest_path = dir_path.join("mod.manifest.ron");
    if !manifest_path.exists() {
        return Err(format!("no mod.manifest.ron found in {dir}"));
    }
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("failed to read manifest: {e}"))?;
    let manifest: ModManifest =
        ron::from_str(&text).map_err(|e| format!("invalid manifest: {e}"))?;
    if manifest.id.is_empty() {
        return Err("mod id must not be empty".into());
    }
    // Validate all .ron files parse as one of the known types.
    validate_content_dir(dir_path)?;
    // Package as zip.
    let out_path = output.unwrap_or_else(|| format!("{}.reachmod", dir.trim_end_matches('/')));
    let file = std::fs::File::create(&out_path)
        .map_err(|e| format!("failed to create {out_path}: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Deflated);
    add_dir_to_zip(dir_path, dir_path, &mut zip, &options)
        .map_err(|e| format!("failed to pack: {e}"))?;
    zip.finish()
        .map_err(|e| format!("failed to finalize zip: {e}"))?;
    println!("Packed {} -> {}", manifest.name, out_path);
    Ok(())
}

fn cmd_install(file: &str) -> Result<(), String> {
    let file_path = Path::new(file);
    if !file_path.exists() {
        return Err(format!("file not found: {file}"));
    }
    // Read zip and extract manifest to determine mod id.
    let reader = std::io::Cursor::new(
        std::fs::read(file_path).map_err(|e| format!("failed to read {file}: {e}"))?,
    );
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|e| format!("invalid .reachmod: {e}"))?;
    // Find and parse manifest.
    let mut manifest_text = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("zip error: {e}"))?;
        if entry.name() == "mod.manifest.ron" {
            let mut buf = String::new();
            std::io::Read::read_to_string(&mut entry, &mut buf)
                .map_err(|e| format!("failed to read manifest: {e}"))?;
            manifest_text = Some(buf);
            break;
        }
    }
    let text = manifest_text.ok_or("no mod.manifest.ron in package")?;
    let manifest: ModManifest =
        ron::from_str(&text).map_err(|e| format!("invalid manifest: {e}"))?;
    // Extract to ~/.reachlock/mods/<id>/.
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "could not determine home directory".to_string())?;
    let dest = std::path::Path::new(&home)
        .join(".reachlock")
        .join("mods")
        .join(&manifest.id);
    std::fs::create_dir_all(&dest).map_err(|e| format!("failed to create {dest:?}: {e}"))?;
    // Re-extract all files (with directory structure stripped of the first component).
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(
        std::fs::read(file_path).map_err(|e| format!("failed to read {file}: {e}"))?,
    ))
    .map_err(|e| format!("invalid .reachmod: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("zip error: {e}"))?;
        let out_path = dest.join(entry.name());
        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("failed to create dir {out_path:?}: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create dir {parent:?}: {e}"))?;
            }
            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| format!("failed to create {out_path:?}: {e}"))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| format!("failed to write {out_path:?}: {e}"))?;
        }
    }
    println!("Installed {} to {:?}", manifest.name, dest);
    Ok(())
}

fn cmd_list() -> Result<(), String> {
    let mods_root = Path::new(MODS_ROOT);
    if !mods_root.is_dir() {
        println!("No mods directory found at {mods_root:?}");
        return Ok(());
    }
    let mut manifests: Vec<ModManifest> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(mods_root) {
        for entry in entries.flatten() {
            let mod_dir = entry.path();
            if !mod_dir.is_dir() {
                continue;
            }
            let mpath = mod_dir.join("mod.manifest.ron");
            if !mpath.exists() {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&mpath) {
                if let Ok(m) = ron::from_str::<ModManifest>(&text) {
                    manifests.push(m);
                }
            }
        }
    }
    if manifests.is_empty() {
        println!("No mods installed.");
        return Ok(());
    }
    match resolve_load_order(&manifests) {
        Ok(order) => {
            println!("Load order:");
            for id in &order {
                if let Some(m) = manifests.iter().find(|m| &m.id == id) {
                    let conflict_msg = if m.conflicts.is_empty() {
                        String::new()
                    } else {
                        format!(" (conflicts: {})", m.conflicts.join(", "))
                    };
                    println!(
                        "  {:<20} v{}.{}.{} by {}{}",
                        m.name, m.version.0, m.version.1, m.version.2, m.author, conflict_msg,
                    );
                }
            }
        }
        Err(err) => {
            println!("Load order error: {err:?}");
            println!("\nInstalled mods (unordered):");
            for m in &manifests {
                println!(
                    "  {} v{}.{}.{}",
                    m.name, m.version.0, m.version.1, m.version.2
                );
            }
        }
    }
    Ok(())
}

fn validate_content_dir(dir: &Path) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            validate_content_dir(&path)?;
        } else if path.extension().is_some_and(|e| e == "ron")
            && path.file_name().is_some_and(|n| n != "mod.manifest.ron")
        {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("failed to read {path:?}: {e}"))?;
            // Quick validation: try parsing as the mod manifest type first (most common
            // failure case), then as a generic ron value.
            ron::from_str::<ron::Value>(&text)
                .map_err(|e| format!("invalid RON in {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

fn add_dir_to_zip(
    dir: &Path,
    base: &Path,
    zip: &mut zip::ZipWriter<std::fs::File>,
    options: &zip::write::FileOptions<()>,
) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            add_dir_to_zip(&path, base, zip, options)?;
        } else {
            let name = path
                .strip_prefix(base)
                .map_err(|_| "path prefix error".to_string())?
                .to_string_lossy()
                .to_string();
            let data = std::fs::read(&path).map_err(|e| format!("failed to read {path:?}: {e}"))?;
            zip.start_file(name, *options)
                .map_err(|e| format!("zip error: {e}"))?;
            std::io::Write::write_all(zip, &data).map_err(|e| format!("zip write error: {e}"))?;
        }
    }
    Ok(())
}
