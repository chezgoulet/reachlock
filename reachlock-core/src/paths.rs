//! Where this installation lives on disk.
//!
//! Every binary used to hard-code relative paths (`"mods/reachlock/origins"`,
//! `"save/settings.ron"`, `"content/factions"`), so running any of them from
//! a directory other than the repository root silently read and wrote the
//! wrong tree — character creation offered zero origins and said nothing.
//! Three copies of a two-guess `["x", "../x"]` fallback had grown up around
//! the problem without solving it.
//!
//! Resolution order, first hit wins:
//!
//! 1. `$REACHLOCK_ROOT`, used verbatim.
//! 2. The current directory and each of its ancestors — first one that
//!    contains `mods/reachlock`.
//! 3. The executable's directory and each of its ancestors, same test. This
//!    covers `target/debug/reachlock-client` in a dev tree, and a shipped
//!    layout where the binary sits beside `mods/`.
//! 4. The current directory, unchanged. Resolution failed; [`content_found`]
//!    returns false and callers are expected to say so.
//!
//! The root is resolved once and cached, so environment changes after the
//! first call have no effect.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What identifies an install root: the default mod's content tree.
const MARKER: &str = "mods/reachlock";

static ROOT: OnceLock<Resolved> = OnceLock::new();

#[derive(Debug, Clone)]
struct Resolved {
    root: PathBuf,
    how: &'static str,
    found_marker: bool,
}

/// First ancestor of `start` (inclusive) that contains [`MARKER`].
fn walk_up(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join(MARKER).is_dir() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

fn resolve() -> Resolved {
    if let Some(root) = env_override("REACHLOCK_ROOT") {
        let found_marker = root.join(MARKER).is_dir();
        return Resolved {
            root,
            how: "REACHLOCK_ROOT",
            found_marker,
        };
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(root) = walk_up(&cwd) {
            return Resolved {
                root,
                how: "found by walking up from the current directory",
                found_marker: true,
            };
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(root) = walk_up(parent) {
                return Resolved {
                    root,
                    how: "found by walking up from the executable",
                    found_marker: true,
                };
            }
        }
    }
    Resolved {
        root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        how: "not found; fell back to the current directory",
        found_marker: false,
    }
}

fn resolved() -> &'static Resolved {
    ROOT.get_or_init(resolve)
}

fn env_override(key: &str) -> Option<PathBuf> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

/// The directory that holds `mods/`, `save/`, `content/`, and `data/`.
pub fn install_root() -> &'static Path {
    &resolved().root
}

/// False means `mods/reachlock` was never located and every content path
/// below is a guess. Binaries must report this at startup rather than
/// continuing with an empty content tree.
pub fn content_found() -> bool {
    resolved().found_marker
}

/// One line naming the root and how it was found, for startup diagnostics.
pub fn describe() -> String {
    let r = resolved();
    format!("install root: {} ({})", r.root.display(), r.how)
}

/// All mods. Override with `$REACHLOCK_MODS_DIR`.
pub fn mods_root() -> PathBuf {
    env_override("REACHLOCK_MODS_DIR").unwrap_or_else(|| install_root().join("mods"))
}

/// The default mod's content tree. Override with `$REACHLOCK_CONTENT_ROOT`.
pub fn content_root() -> PathBuf {
    env_override("REACHLOCK_CONTENT_ROOT").unwrap_or_else(|| mods_root().join("reachlock"))
}

/// Player saves, settings, and editor preferences. Override with
/// `$REACHLOCK_SAVE_DIR`.
pub fn save_dir() -> PathBuf {
    env_override("REACHLOCK_SAVE_DIR").unwrap_or_else(|| install_root().join("save"))
}

/// Server runtime data — tick snapshots, spooled email. Override with
/// `$REACHLOCK_DATA_DIR`.
pub fn data_dir() -> PathBuf {
    env_override("REACHLOCK_DATA_DIR").unwrap_or_else(|| install_root().join("data"))
}

/// The server's authored content tree. This is `content/`, a different tree
/// from `mods/` — see docs/USER-GUIDE.md §7. Override with
/// `$REACHLOCK_SERVER_CONTENT_DIR`.
pub fn server_content_dir() -> PathBuf {
    env_override("REACHLOCK_SERVER_CONTENT_DIR").unwrap_or_else(|| install_root().join("content"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_up_finds_the_marker_from_a_nested_directory() {
        let base = std::env::temp_dir().join("reachlock_paths_walk_up");
        let nested = base.join("a").join("b").join("c");
        std::fs::create_dir_all(base.join(MARKER)).expect("create marker");
        std::fs::create_dir_all(&nested).expect("create nested");
        assert_eq!(walk_up(&nested), Some(base.clone()));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn walk_up_is_none_without_the_marker() {
        let base = std::env::temp_dir().join("reachlock_paths_no_marker");
        let nested = base.join("x").join("y");
        std::fs::create_dir_all(&nested).expect("create nested");
        if walk_up(&nested).is_some() {
            let _ = std::fs::remove_dir_all(&base);
            return;
        }
        assert_eq!(walk_up(&nested), None);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn env_override_ignores_an_empty_value() {
        assert_eq!(env_override("REACHLOCK_DEFINITELY_UNSET_12345"), None);
    }

    /// Every derived path must descend from the install root, so that
    /// relocating the root moves all of them together.
    #[test]
    fn derived_paths_descend_from_the_root() {
        if std::env::var_os("REACHLOCK_MODS_DIR").is_some()
            || std::env::var_os("REACHLOCK_CONTENT_ROOT").is_some()
            || std::env::var_os("REACHLOCK_SAVE_DIR").is_some()
        {
            return;
        }
        let root = install_root();
        assert!(mods_root().starts_with(root));
        assert!(content_root().starts_with(root));
        assert!(save_dir().starts_with(root));
        assert!(data_dir().starts_with(root));
        assert!(server_content_dir().starts_with(root));
    }
}
