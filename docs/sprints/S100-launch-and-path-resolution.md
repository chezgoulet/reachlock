# S100 — Launch & Path Resolution

**Spec:** New (install-root resolution, launch ergonomics) ·
**Depends on:** nothing · **Branch:** `sprint-v2/s100-launch-paths` cut from `testing`

> **Execute this brief literally.** Every edit below gives the exact old text
> and the exact new text. Do not redesign anything, do not rename anything, do
> not add features that are not listed. If an instruction does not match the
> file you find, stop and report which one — do not improvise a fix.

---

## Outcome

Nothing in ReachLock currently knows where it is installed. Twelve hardcoded
relative paths across three crates mean every binary must be launched from the
repository root, and when it is not, it fails **silently**: the client shows a
character creation screen with zero origins and no error, and writes the save
to `./save/` wherever it happened to be started.

After this sprint there is one resolver in `reachlock-core`, every binary uses
it, running from any directory works, a failed resolution says so loudly, and
the Wayland workaround is applied in-process so no launch command needs a
prefix.

## Non-goals

- Do **not** move saves to an XDG/AppData user directory. Saves stay next to
  the install; only the *resolution* of "the install" changes.
- Do **not** merge the `content/` and `mods/reachlock/` trees. They stay
  separate; each just gets resolved properly.
- Do **not** touch `reachlock-editor/src/command_palette.rs`, `cross_ref.rs`,
  `diff.rs`, `template_manager.rs`, or `validation.rs`. Those files are not in
  the module tree and are out of scope.
- Do **not** add any new crate dependency. The resolver uses `std` only.

---

## Deliverable 1 — `reachlock-core/src/paths.rs` (new file)

### 1a. Create the file with exactly this content

```rust
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
        // /tmp and / must not contain mods/reachlock for this to be meaningful.
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
            return; // an override is in play; the invariant does not apply
        }
        let root = install_root();
        assert!(mods_root().starts_with(root));
        assert!(content_root().starts_with(root));
        assert!(save_dir().starts_with(root));
        assert!(data_dir().starts_with(root));
        assert!(server_content_dir().starts_with(root));
    }
}
```

### 1b. Register the module

In `reachlock-core/src/lib.rs`, the `pub mod` declarations are alphabetical.
Insert this line between `pub mod network;` and `pub mod seed;`:

```rust
pub mod paths;
```

### 1c. Add the foreign-CWD integration test

Create `reachlock-core/tests/resolves_from_a_foreign_cwd.rs`:

```rust
//! The resolver's whole point: find the content tree when the process was
//! not started from the repository root. This test binary lives under
//! `target/debug/deps/`, so resolution step 3 (walk up from the executable)
//! is what has to succeed once the CWD is useless.
//!
//! This file must contain EXACTLY ONE test. `set_current_dir` is
//! process-global and the root is cached on first use, so a second test in
//! this binary would race it.

#[test]
fn content_root_resolves_when_the_cwd_is_not_the_repo() {
    std::env::set_current_dir("/").expect("chdir to /");
    assert!(
        reachlock_core::paths::content_found(),
        "expected the executable walk to find mods/reachlock — {}",
        reachlock_core::paths::describe()
    );
    assert!(
        reachlock_core::paths::content_root().join("origins").is_dir(),
        "resolved content root has no origins/ — {}",
        reachlock_core::paths::describe()
    );
}
```

---

## Deliverable 2 — replace every hardcoded path

Work through this table in order. Each row is a literal find-and-replace.

### 2a. `reachlock-client/src/save_backend.rs`

**Delete** line 9:

```rust
    const PATH: &'static str = "save/player.ron";
```

**Replace** the whole `impl FsSaveBackend { … }` block (lines 8–23) with:

```rust
impl FsSaveBackend {
    fn path() -> std::path::PathBuf {
        reachlock_core::paths::save_dir().join("player.ron")
    }

    pub fn read(&self) -> Option<String> {
        std::fs::read_to_string(Self::path()).ok()
    }

    pub fn write(&self, data: &str) {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, data) {
            log::warn!("save: could not write {}: {e}", path.display());
        }
    }
}
```

### 2b. `reachlock-client/src/settings.rs`

**Replace** line 1275:

```rust
pub const SETTINGS_PATH: &str = "save/settings.ron";
```

with:

```rust
/// Was a `const &str` relative to the CWD, so settings written from one
/// working directory were invisible from another.
pub fn settings_path() -> std::path::PathBuf {
    reachlock_core::paths::save_dir().join("settings.ron")
}
```

Then update all six usages — lines 1280, 1307, 1312, 1407, 1410, 1415 — by
replacing the bare identifier `SETTINGS_PATH` with `settings_path()`. Two
need a borrow:

| Line | Old | New |
|---|---|---|
| 1280 | `std::fs::read_to_string(SETTINGS_PATH)` | `std::fs::read_to_string(settings_path())` |
| 1307 | `std::path::Path::new(SETTINGS_PATH).parent()` | `settings_path().parent().map(\|p\| p.to_path_buf())` |
| 1312 | `std::fs::write(SETTINGS_PATH, text)` | `std::fs::write(settings_path(), text)` |
| 1407 | `std::fs::write(SETTINGS_PATH, "this is not ron {")` | `std::fs::write(settings_path(), "this is not ron {")` |
| 1410 | `std::fs::remove_file(SETTINGS_PATH)` | `std::fs::remove_file(settings_path())` |
| 1415 | `std::fs::remove_file(SETTINGS_PATH)` | `std::fs::remove_file(settings_path())` |

If line 1307's `if let Some(parent) = …` no longer compiles because of the
borrow, use this form instead:

```rust
    let path = settings_path();
    if let Some(parent) = path.parent() {
```

and leave the body unchanged.

Finally, grep the whole workspace for `SETTINGS_PATH` and fix any remaining
reference the same way. There must be zero left in `reachlock-client`.

### 2c. `reachlock-client/src/systems/onboarding.rs`

**Delete** line 8:

```rust
const ONBOARDING_FLAG: &str = "save/onboarding_completed.flag";
```

**Replace** lines 29–31 (`fn onboarding_completed_flag_path`) with:

```rust
fn onboarding_completed_flag_path() -> PathBuf {
    reachlock_core::paths::save_dir().join("onboarding_completed.flag")
}
```

Leave the two call sites (lines ~40 and ~59) alone — they already call the
function.

### 2d. `reachlock-client/src/systems/content_index.rs`

**Replace** line 49:

```rust
    let root = std::path::Path::new("mods");
```

with:

```rust
    let root = reachlock_core::paths::mods_root();
```

If the next line (`if !root.is_dir()`) or the later `read_dir(root)` fails to
compile, change `read_dir(root)` to `read_dir(&root)`. Change nothing else.

### 2e. `reachlock-client/src/systems/character_creation.rs`

**Replace** lines 189–190:

```rust
    for root in ["mods/reachlock/origins", "../mods/reachlock/origins"] {
        let dir = std::path::Path::new(root);
```

with:

```rust
    for dir in [reachlock_core::paths::content_root().join("origins")] {
```

Then on the following line change `std::fs::read_dir(dir)` to
`std::fs::read_dir(&dir)`. Leave the rest of the function, including the
trailing `if !out.is_empty() { break; }`, exactly as it is.

### 2f. `reachlock-client/src/systems/crew.rs` — two sites

**Site 1**, line 273:

```rust
        let crews_dir = std::path::Path::new("mods/reachlock/crews");
```

becomes:

```rust
        let crews_dir = reachlock_core::paths::content_root().join("crews");
```

If `read_dir(crews_dir)` on the following line stops compiling, change it to
`read_dir(&crews_dir)`.

**Site 2**, lines 799–800:

```rust
    for root in ["mods/reachlock/hulls", "../mods/reachlock/hulls"] {
        let dir = std::path::Path::new(root);
```

becomes:

```rust
    for dir in [reachlock_core::paths::content_root().join("hulls")] {
```

Then change `std::fs::read_dir(dir)` to `std::fs::read_dir(&dir)` on the line
below. Leave `if !dir.is_dir()` and the trailing `break` as they are.

### 2g. `reachlock-client/src/systems/soul.rs`

**Replace** lines 125–126:

```rust
    for root in ["mods/reachlock/storylines", "../mods/reachlock/storylines"] {
        let Ok(entries) = std::fs::read_dir(root) else {
```

with:

```rust
    for root in [reachlock_core::paths::content_root().join("storylines")] {
        let Ok(entries) = std::fs::read_dir(&root) else {
```

Leave the rest of the loop, including the trailing `break`, alone.

### 2h. `reachlock-server/src/main.rs`

**Replace** lines 33–35:

```rust
    if let Err(e) =
        reachlock_core::content::faction_loader::load_faction_profiles("content/factions")
```

with:

```rust
    if let Err(e) = reachlock_core::content::faction_loader::load_faction_profiles(
        reachlock_core::paths::server_content_dir().join("factions"),
    )
```

**Replace** lines 38–40:

```rust
    if let Err(e) =
        reachlock_core::content::faction_loader::load_storyline_files("content/storylines")
```

with:

```rust
    if let Err(e) = reachlock_core::content::faction_loader::load_storyline_files(
        reachlock_core::paths::server_content_dir().join("storylines"),
    )
```

Both loaders take `P: AsRef<Path>`, so a `PathBuf` works without conversion.

### 2i. `reachlock-server/src/services/tick.rs`

**Replace** lines 29–32:

```rust
        let p = std::env::var("REACHLOCK_SNAPSHOT_PATH")
            .unwrap_or_else(|_| "data/tick/snap.json".into());
        PathBuf::from(p)
```

with:

```rust
        match std::env::var("REACHLOCK_SNAPSHOT_PATH") {
            Ok(p) if !p.is_empty() => PathBuf::from(p),
            _ => reachlock_core::paths::data_dir().join("tick").join("snap.json"),
        }
```

### 2j. `reachlock-server/src/ws/mod.rs`

**Replace** lines 192–194:

```rust
            let file_dir = std::env::var("REACHLOCK_EMAIL_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("data/emails"));
```

with:

```rust
            let file_dir = match std::env::var("REACHLOCK_EMAIL_DIR") {
                Ok(d) if !d.is_empty() => std::path::PathBuf::from(d),
                _ => reachlock_core::paths::data_dir().join("emails"),
            };
```

### 2k. `reachlock-editor/src/app.rs`

**Replace** line 27:

```rust
    PathBuf::from("mods/reachlock")
```

with:

```rust
    reachlock_core::paths::content_root()
```

### 2l. `reachlock-editor/src/browser.rs`

**Replace** line 109:

```rust
            root: PathBuf::from("mods/reachlock"),
```

with:

```rust
            root: reachlock_core::paths::content_root(),
```

Do **not** touch lines 360, 362, 364, or 370 — those are string literals in
path-classification tests and are correct as they are.

### 2m. `reachlock-editor/src/preferences_window.rs`

**Delete** line 8:

```rust
const PREFS_PATH: &str = "save/editor-preferences.ron";
```

**Add** this function immediately after the `use` statements at the top:

```rust
fn prefs_path() -> PathBuf {
    reachlock_core::paths::save_dir().join("editor-preferences.ron")
}
```

**Replace** line 41:

```rust
            content_root: "mods/reachlock".into(),
```

with:

```rust
            content_root: reachlock_core::paths::content_root().display().to_string(),
```

**Replace** line 73:

```rust
        let prefs = std::fs::read_to_string(PathBuf::from(PREFS_PATH))
```

with:

```rust
        let prefs = std::fs::read_to_string(prefs_path())
```

**Replace** line 81:

```rust
        let path = PathBuf::from(PREFS_PATH);
```

with:

```rust
        let path = prefs_path();
```

### 2n. `reachlock-editor/src/settings_window.rs`

**Delete** line 12:

```rust
const SETTINGS_PATH: &str = "save/editor-settings.ron";
```

**Replace** line 156 (currently `PathBuf::from(SETTINGS_PATH)`) so the
enclosing function body becomes:

```rust
    reachlock_core::paths::save_dir().join("editor-settings.ron")
```

Fix any other reference to `SETTINGS_PATH` in this file the same way. There
must be zero left.

### 2o. `reachlock-editor/src/schema.rs`

**Replace** the entire body of `schemas_dir()` (lines 16 to its closing brace,
including the `CARGO_MANIFEST_DIR` fallback) with:

```rust
pub fn schemas_dir() -> std::path::PathBuf {
    reachlock_core::paths::content_root().join("schemas")
}
```

Delete the doc comment lines above it that describe the old two-guess
fallback, and replace them with:

```rust
/// The schema directory inside the resolved content root.
```

The `CARGO_MANIFEST_DIR` fallback existed only so unit tests run from the
crate directory could find the schemas. The resolver's executable walk covers
that case, so the fallback goes.

### 2p. Add the dependency the editor now needs

`reachlock-editor/Cargo.toml` already has `reachlock-core = { path = "../reachlock-core" }`.
Confirm the same line exists in `reachlock-client/Cargo.toml` and
`reachlock-server/Cargo.toml`. It does — add nothing.

---

## Deliverable 3 — report a failed resolution loudly

Add this block as the **first statement** in each of the three `main()`
functions listed below. It must run before anything reads content.

```rust
    if !reachlock_core::paths::content_found() {
        eprintln!(
            "reachlock: could not find `mods/reachlock` from the current directory \
             or from the executable.\n  {}\n  Content will be EMPTY — no origins, \
             no ship templates, no souls.\n  Run from the repository root, or set \
             REACHLOCK_ROOT to the directory that holds `mods/`.",
            reachlock_core::paths::describe()
        );
    }
```

Insert it into:

1. `reachlock-client/src/main.rs`, at the top of `fn main()` (line 75),
   **before** `save_backend::init_save_backend();`.
2. `reachlock-editor/src/main.rs`, at the top of `fn main()` (line 1271),
   before `let options = …`.
3. `reachlock-server/src/main.rs`, at the top of `async fn main()` (line 12,
   under the `#[tokio::main]` attribute).

Use `eprintln!`, not `tracing`/`log` — this must print before the logging
stack is initialised.

---

## Deliverable 4 — apply the Wayland workaround in-process

Add this block **immediately after** the block from Deliverable 3, in
`reachlock-client/src/main.rs` and `reachlock-editor/src/main.rs` only (not
the server — it opens no window):

```rust
    // FIXME(winit-0.30.13): the Wayland backend panics at
    // winit/src/platform_impl/linux/wayland/window/state.rs:694 with
    // `NonZeroU32::new(self.size.width).unwrap()` when the compositor does
    // not send a configure event before the first render. Forcing
    // X11/XWayland avoids it. Doing this in-process covers every launch
    // path — `cargo run`, an IDE, a packaged binary — instead of only the
    // Makefile targets, which is why the editor had no working launch
    // command at all. Remove when bevy's winit dependency moves past
    // 0.30.13. A backend the user chose explicitly always wins.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WINIT_UNIX_BACKEND").is_none() {
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
        std::env::remove_var("WAYLAND_DISPLAY");
    }
```

The workspace is edition 2021, so `set_var` and `remove_var` are safe
functions — do **not** wrap them in `unsafe`.

---

## Deliverable 5 — Makefile

### 5a. Remove the now-redundant prefixes

The `run` and `run-debug` targets currently start with
`WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11`. **Delete that prefix from both**,
so the in-process fix is the only thing keeping them working — otherwise a
regression would go unnoticed. Delete the long `FIXME(winit-0.30.13)` comment
above `run` as well; it now lives in the source.

The two targets become:

```make
# Launch the game (native). The winit/Wayland workaround is applied
# in-process by the binary — see the FIXME in reachlock-client/src/main.rs.
run:
	cargo run -p reachlock-client

# Launch with Bevy's `debug` feature so ECS errors (e.g. B0001 query
# conflicts) print real component/system names instead of a placeholder.
run-debug:
	cargo run -p reachlock-client --features debug-names
```

### 5b. Add four targets

Insert after `run-debug`:

```make
# Launch the content editor. There was no target for this at all, so the
# documented way to start it was a hand-typed command with an env prefix.
editor:
	cargo run -p reachlock-editor

# Put `reachlock` on PATH as a real command, so nothing has to say
# ./target/debug/reachlock.
install-cli:
	cargo install --path reachlock-cli
	@echo "installed: $$(command -v reachlock || echo '~/.cargo/bin/reachlock — add it to PATH')"

# One command from clean checkout to a running server: stack up, config in
# place, server running against it.
dev: db
	@[ -f .env ] || (cp .env.example .env && echo "created .env from .env.example")
	$(MAKE) server-db
```

### 5c. Make `server-db` create `.env` if it is missing

Replace the `server-db` recipe with:

```make
server-db:
	@[ -f .env ] || (cp .env.example .env && echo "created .env from .env.example")
	set -a; [ -f .env ] && . ./.env; set +a; \
	  cargo run -p reachlock-server --features postgres,redis
```

### 5d. Update `.PHONY`

Add `editor`, `install-cli`, and `dev` to the `.PHONY` list at the top.

---

## Deliverable 6 — documentation

### 6a. `docs/USER-GUIDE.md`

1. In §4, **delete** the whole blockquote titled "Run it from the repository
   root" and replace it with:

   > The editor finds its content tree by looking for `mods/reachlock` — first
   > from the current directory upward, then from the executable's own
   > location. You can launch it from anywhere. Set `REACHLOCK_ROOT` to
   > override, or point `content_root` at an absolute path in Preferences to
   > edit a different tree.

2. Everywhere a launch command carries `WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11`
   (§3, §4, §9), delete the prefix. Replace the editor's launch command with
   `make editor`.

3. In §9, replace the winit troubleshooting row's fix with: "Handled
   in-process by both binaries since S100. If it recurs, set
   `WINIT_UNIX_BACKEND=x11` yourself — an explicit value always wins."

4. In §9, replace the "Editor writes files somewhere unexpected" row's cause
   with: "Resolved automatically since S100. If it still happens, check
   `REACHLOCK_ROOT` and the `content_root` preference."

5. In §5, change `./target/debug/reachlock` to `reachlock` and add a line:
   "Run `make install-cli` once to put it on PATH."

6. Add a new table at the end of §10 listing the six override variables:
   `REACHLOCK_ROOT`, `REACHLOCK_MODS_DIR`, `REACHLOCK_CONTENT_ROOT`,
   `REACHLOCK_SAVE_DIR`, `REACHLOCK_DATA_DIR`, `REACHLOCK_SERVER_CONTENT_DIR`.

### 6b. `docs/CONTENT-READINESS.md`

In §5, delete gap row **#2** ("Content root is CWD-relative"). Renumber the
rows below it.

### 6c. `README.md`

In the Quick start block, add `make editor` and `make dev` with one-line
comments matching the Makefile's.

### 6d. `AGENTS.md` and `docs/sprints/00-INDEX.md`

Add this entry to the gotcha ledger in **both** files:

```
- Never hard-code a relative path to content, saves, or data. `reachlock_core::paths` resolves the install root (env → walk up from CWD → walk up from the executable) and every binary uses it. Twelve hardcoded literals used to make every binary CWD-dependent, and the client failed silently: run it from the wrong directory and character creation offered zero origins with no error. Three copies of a `["x", "../x"]` two-guess fallback had grown up around the problem instead of fixing it.
```

---

## Acceptance gates

Run all of these. Every one must pass. Paste the output into the PR.

```sh
export PATH="$HOME/.cargo/bin:$PATH"

# 1. The whole gate battery.
make check

# 2. The new resolver tests specifically.
cargo test -p reachlock-core paths
cargo test -p reachlock-core --test resolves_from_a_foreign_cwd

# 3. No hardcoded content/save/data paths remain outside tests.
#    Expected output: ONLY the four browser.rs test literals
#    (lines 360, 362, 364, 370) and nothing else.
rg -n '"(save|mods|content|data)/' reachlock-client/src reachlock-server/src reachlock-editor/src

# 4. No two-guess fallbacks remain. Expected: no matches.
rg -n '"\.\./mods|"\.\./content|"\.\./save' reachlock-client/src reachlock-editor/src reachlock-server/src

# 5. The Makefile no longer sets the winit env vars. Expected: no matches.
rg -n 'WAYLAND_DISPLAY|WINIT_UNIX_BACKEND' Makefile

# 6. Both binaries build.
cargo build -p reachlock-client -p reachlock-editor -p reachlock-server

# 7. The content tree still checks out clean.
cargo run -q -p reachlock-cli -- content check mods/reachlock
```

**Manual check 8 — the whole point of the sprint.** From a directory that is
not the repo, the client must start without printing the "could not find
`mods/reachlock`" line:

```sh
cd /tmp && /absolute/path/to/repo/target/debug/reachlock-client
```

If you have no display, run this instead and confirm it prints nothing:

```sh
cd /tmp && /absolute/path/to/repo/target/debug/reachlock-client 2>&1 | grep 'could not find'
```

**Manual check 9 — the failure path is loud.** This must print the multi-line
error:

```sh
cd /tmp && REACHLOCK_ROOT=/tmp /absolute/path/to/repo/target/debug/reachlock-client 2>&1 | head -5
```

---

## Gotchas

- **`cargo fmt --all` reformats ~33 unrelated files** because of rustfmt
  version skew. Format only what you touched:
  `cargo fmt -p reachlock-core -p reachlock-client -p reachlock-editor -p reachlock-server`.
  If `make check`'s `fmt` step then fails on a file you did not touch, stop
  and report it — do not commit a tree-wide reformat.
- **Do not add a dependency to `reachlock-core`.** `make check-purity` fails
  the build if core's dependency tree gains a rendering, async-runtime, or
  HTTP crate. `paths.rs` uses `std` only, which is fine.
- **`reachlock-core/tests/resolves_from_a_foreign_cwd.rs` must contain exactly
  one test.** `set_current_dir` is process-global and the resolved root is
  cached on first use; a second test in the same binary would race it.
- **Do not run `git add -A`.** Concurrent sessions may have unrelated changes
  in the tree. Add the files you edited by name.
- **The workspace builds with `debug = false`.** Do not change it.
- The toolchain is pinned to 1.96.0 in `rust-toolchain.toml`. Do not bump it.

## PR

Open against `testing`, title `S100: resolve the install root instead of
guessing from the CWD`. In the body, list which of the twelve call sites you
changed and paste the output of acceptance gates 3, 4, 5, 8, and 9.
