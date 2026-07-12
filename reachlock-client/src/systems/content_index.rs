//! Authored content index (spec §10, Loader deliverable): at startup, read
//! every `.ron` under `content/` into a Bevy resource so `setup::spawn_world`
//! can resolve authored overrides. The bridge stays plain-data-only — this
//! module is the only content-aware piece on the client side; everything it
//! produces is a `Vec<ContentFile>` the same generators already understand.

use bevy::prelude::*;
use reachlock_core::content::ContentFile;

/// The local override index (spec §10, "Loader reads `content/` from disk
/// at startup (local mode)"). Empty on wasm — there is no filesystem to
/// read; server distribution of overrides is S23's problem.
#[derive(Resource, Default)]
pub struct ContentIndex {
    pub files: Vec<ContentFile>,
}

/// Directory under `content/` that holds test fixtures, not real authored
/// assets (spec §10 deliverable: "skip `content/_fixtures/`").
#[cfg(not(target_arch = "wasm32"))]
const FIXTURES_DIR: &str = "_fixtures";

/// Native loader: walks `content/` from the working directory, parsing each
/// `.ron` file into a `ContentFile`. Files that fail to parse are logged and
/// skipped rather than aborting startup — one bad authored file shouldn't
/// take down the whole index.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_content_index(mut commands: Commands) {
    let mut files = Vec::new();
    let root = std::path::Path::new("content");
    if root.is_dir() {
        walk(root, &mut files);
    } else {
        warn!("content index: no content/ directory found at {root:?}; index is empty");
    }
    info!("content index: loaded {} authored file(s)", files.len());
    commands.insert_resource(ContentIndex { files });
}

#[cfg(not(target_arch = "wasm32"))]
fn walk(dir: &std::path::Path, out: &mut Vec<ContentFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        warn!("content index: failed to read directory {dir:?}");
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == FIXTURES_DIR) {
                continue;
            }
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "ron") {
            match std::fs::read_to_string(&path) {
                Ok(text) => match ron::from_str::<ContentFile>(&text) {
                    Ok(file) => out.push(file),
                    Err(err) => {
                        warn!("content index: failed to parse {}: {err}", path.display())
                    }
                },
                Err(err) => warn!("content index: failed to read {}: {err}", path.display()),
            }
        }
    }
}

/// Wasm loader: no filesystem access, so the index starts (and stays)
/// empty. Server distribution of overrides (S23) is what fills this in on
/// wasm eventually.
#[cfg(target_arch = "wasm32")]
pub fn load_content_index(mut commands: Commands) {
    commands.insert_resource(ContentIndex::default());
}
