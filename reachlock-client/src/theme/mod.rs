//! The UI theme engine — ReachLock's stylesheet layer.
//!
//! UI code never names a color. It names a *style class*:
//!
//! ```ignore
//! commands.spawn(theme::text("panel.title", "IDENTITY"));
//! ```
//!
//! A resolver system reads the class out of [`Theme`] and writes the concrete
//! `TextColor` / `TextFont` / `BackgroundColor` / `BorderColor` / `Node`
//! values onto the entity. Restyling the game is therefore an edit to
//! `assets/ui/*.ron` and no Rust changes at all — press **F5** in-game to
//! reload the stylesheet live.
//!
//! Two limits worth knowing:
//!
//! - The resolver *patches* only the properties a class declares, so removing
//!   a property from a class takes effect on restart, not on live reload.
//! - Classes that set `letter_spacing` own their entity's `Text`, because
//!   tracking is applied by rewriting the string (Bevy 0.18 has no
//!   letter-spacing property). Don't use a tracked class for text you mutate
//!   at runtime.

mod data;

pub use data::{apply_tracking, Style, ThemeFile};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bevy::prelude::*;

/// Names the style class an entity is drawn with. The single thing UI code
/// says about appearance.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Styled(pub String);

impl Styled {
    pub fn new(class: impl Into<String>) -> Self {
        Styled(class.into())
    }
}

/// The untransformed text behind a tracked [`Styled`] entity. Tracking is
/// recomputed from this every time, so a live reload can never compound
/// spacing on already-spaced text.
#[derive(Component, Clone, Debug)]
pub struct SourceText(pub String);

/// The loaded stylesheet: classes resolved to concrete values, plus the two
/// font handles they select between.
#[derive(Resource)]
pub struct Theme {
    pub id: String,
    pub name: String,
    classes: HashMap<String, Style>,
    regular: Handle<Font>,
    bold: Handle<Font>,
    source_path: PathBuf,
    loaded_at: Option<SystemTime>,
}

impl Theme {
    /// The resolved style for a class, or `None` if the stylesheet has no
    /// such class.
    pub fn style(&self, class: &str) -> Option<&Style> {
        self.classes.get(class)
    }

    /// The font handle a style selects.
    pub fn font(&self, style: &Style) -> Handle<Font> {
        if style.bold {
            self.bold.clone()
        } else {
            self.regular.clone()
        }
    }

    pub fn class_names(&self) -> impl Iterator<Item = &str> {
        self.classes.keys().map(String::as_str)
    }
}

/// Where the stylesheet lives. `REACHLOCK_THEME` overrides it, so a theme can
/// be tried without touching the tree.
fn theme_path() -> PathBuf {
    if let Ok(p) = std::env::var("REACHLOCK_THEME") {
        return PathBuf::from(p);
    }
    reachlock_core::paths::install_root().join("assets/ui/phosphor.ron")
}

/// Read and resolve a stylesheet from disk.
fn read_theme(path: &Path) -> Result<(ThemeFile, HashMap<String, Style>), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read theme {}: {e}", path.display()))?;
    let file: ThemeFile = data::parse_theme(&text)
        .map_err(|e| format!("theme {} failed to parse: {e}", path.display()))?;
    let (classes, problems) = file.resolve();
    for p in &problems {
        warn!("theme: {p}");
    }
    Ok((file, classes))
}

/// Install the theme before anything spawns UI.
fn load_theme(mut commands: Commands, assets: Res<AssetServer>) {
    let path = theme_path();
    let loaded_at = std::fs::metadata(&path).and_then(|m| m.modified()).ok();

    match read_theme(&path) {
        Ok((file, classes)) => {
            info!(
                "theme: loaded '{}' ({} classes) from {}",
                file.name,
                classes.len(),
                path.display()
            );
            commands.insert_resource(Theme {
                id: file.id,
                name: file.name,
                classes,
                regular: assets.load(file.fonts.regular),
                bold: assets.load(file.fonts.bold),
                source_path: path,
                loaded_at,
            });
        }
        Err(e) => {
            // No stylesheet is survivable — the game renders with Bevy's
            // defaults rather than not rendering at all — but it is never
            // silent, because "the UI looks wrong" is otherwise unattributable.
            error!("theme: {e}");
            error!("theme: falling back to unstyled UI; fix the stylesheet and press F5");
            commands.insert_resource(Theme {
                id: "none".into(),
                name: "Unstyled".into(),
                classes: HashMap::new(),
                regular: Handle::default(),
                bold: Handle::default(),
                source_path: path,
                loaded_at: None,
            });
        }
    }
}

/// Re-read the stylesheet when it changes on disk, or on F5. Mutating the
/// resource re-runs the resolver over every styled entity.
fn poll_theme_reload(
    mut theme: ResMut<Theme>,
    assets: Res<AssetServer>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut since_check: Local<f32>,
) {
    let forced = keys.just_pressed(KeyCode::F5);
    *since_check += time.delta_secs();
    if !forced && *since_check < 0.5 {
        return;
    }
    *since_check = 0.0;

    let mtime = std::fs::metadata(&theme.source_path)
        .and_then(|m| m.modified())
        .ok();
    if !forced && mtime == theme.loaded_at {
        return;
    }

    match read_theme(&theme.source_path) {
        Ok((file, classes)) => {
            info!(
                "theme: reloaded '{}' ({} classes)",
                file.name,
                classes.len()
            );
            // Touch the resource only on success, so a broken edit keeps the
            // last good theme on screen instead of blanking the UI.
            let theme = theme.as_mut();
            theme.id = file.id;
            theme.name = file.name;
            theme.classes = classes;
            theme.regular = assets.load(file.fonts.regular);
            theme.bold = assets.load(file.fonts.bold);
            theme.loaded_at = mtime;
        }
        Err(e) => error!("theme: reload failed, keeping the previous theme: {e}"),
    }
}

/// Write resolved styles onto entities.
///
/// Runs over every styled entity but does work only for those whose class
/// changed — or for all of them on the frame the stylesheet reloads.
#[allow(clippy::type_complexity)]
fn apply_styles(
    theme: Res<Theme>,
    mut query: Query<(
        Ref<Styled>,
        Option<&mut TextColor>,
        Option<&mut TextFont>,
        Option<&mut BackgroundColor>,
        Option<&mut BorderColor>,
        Option<&mut Node>,
        Option<&mut Text>,
        Option<&SourceText>,
    )>,
    mut warned: Local<HashSet<String>>,
) {
    let theme_changed = theme.is_changed();
    for (styled, color, font, bg, border, node, text, source) in &mut query {
        if !theme_changed && !styled.is_changed() {
            continue;
        }
        let Some(style) = theme.style(&styled.0) else {
            // Warn once per class: a missing class means UI code and the
            // stylesheet disagree, which is a bug in one of them.
            if warned.insert(styled.0.clone()) {
                warn!("theme: no class '{}' in stylesheet", styled.0);
            }
            continue;
        };

        if let (Some(mut color), Some(fg)) = (color, style.fg) {
            color.0 = fg;
        }
        if let Some(mut font) = font {
            if let Some(size) = style.font_size {
                font.font_size = size;
            }
            font.font = theme.font(style);
        }
        if let (Some(mut bg), Some(c)) = (bg, style.bg) {
            bg.0 = c;
        }
        if let (Some(mut border), Some(c)) = (border, style.border_color) {
            *border = BorderColor::all(c);
        }
        if let Some(mut node) = node {
            if let Some(b) = style.border {
                node.border = b;
            }
            if let Some(p) = style.padding {
                node.padding = p;
            }
            if let Some(m) = style.margin {
                node.margin = m;
            }
            if let Some(g) = style.row_gap {
                node.row_gap = Val::Px(g);
            }
            if let Some(g) = style.column_gap {
                node.column_gap = Val::Px(g);
            }
            if let Some(w) = style.width {
                node.width = Val::Px(w);
            }
            if let Some(h) = style.height {
                node.height = Val::Px(h);
            }
        }
        // Only tracked classes own their Text, so everything else is free to
        // mutate Text directly at runtime.
        if let (Some(mut text), Some(source), Some(spacing)) = (text, source, style.letter_spacing)
        {
            let tracked = apply_tracking(&source.0, spacing);
            if text.0 != tracked {
                text.0 = tracked;
            }
        }
    }
}

// ── Spawn helpers ────────────────────────────────────────────────────────
//
// These return bundles carrying the components the resolver writes into, so
// a caller never mentions a color, a size, or a font.

/// A themed text entity.
pub fn text(class: &str, content: impl Into<String>) -> impl Bundle {
    let content = content.into();
    (
        Styled::new(class),
        SourceText(content.clone()),
        Text::new(content),
        TextFont::default(),
        TextColor::default(),
    )
}

/// A themed layout node.
pub fn node(class: &str) -> impl Bundle {
    (
        Styled::new(class),
        Node::default(),
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    )
}

/// A themed layout node with caller-supplied layout. Class properties patch
/// over these, so set structure here and appearance in the stylesheet.
pub fn node_with(class: &str, layout: Node) -> impl Bundle {
    (
        Styled::new(class),
        layout,
        BackgroundColor(Color::NONE),
        BorderColor::all(Color::NONE),
    )
}

/// Replace a themed entity's text, keeping tracking consistent.
pub fn set_text(text: &mut Text, source: Option<&mut SourceText>, content: impl Into<String>) {
    let content = content.into();
    if let Some(source) = source {
        source.0 = content.clone();
    }
    text.0 = content;
}

/// Registers the theme resource, the live-reload watcher, and the resolver.
pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_theme)
            .add_systems(Update, poll_theme_reload)
            // After Update so entities spawned this frame are styled before
            // they are ever laid out or drawn — no unstyled first frame.
            .add_systems(PostUpdate, apply_styles.before(bevy::ui::UiSystems::Layout));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped stylesheet must parse and resolve cleanly. This is the
    /// gate that stops a broken theme from reaching a player.
    fn shipped() -> (ThemeFile, HashMap<String, Style>) {
        let path = reachlock_core::paths::install_root().join("assets/ui/phosphor.ron");
        read_theme(&path).expect("the shipped stylesheet must load")
    }

    #[test]
    fn shipped_theme_resolves_without_problems() {
        let path = reachlock_core::paths::install_root().join("assets/ui/phosphor.ron");
        let text = std::fs::read_to_string(&path).expect("stylesheet exists");
        let file: ThemeFile = data::parse_theme(&text).expect("stylesheet parses");
        let (classes, problems) = file.resolve();
        assert!(
            problems.is_empty(),
            "shipped stylesheet has unresolved references: {problems:#?}"
        );
        assert!(!classes.is_empty(), "stylesheet defines no classes");
    }

    #[test]
    /// The fonts the stylesheet names have to actually be on disk, or every
    /// glyph silently falls back to Bevy's tiny built-in subset — which is
    /// exactly the tofu this theme exists to fix.
    fn shipped_theme_fonts_exist() {
        let (file, _) = shipped();
        let root = reachlock_core::paths::install_root().join("assets");
        for font in [&file.fonts.regular, &file.fonts.bold] {
            let path = root.join(font);
            assert!(path.is_file(), "stylesheet names a missing font: {font}");
        }
    }

    #[test]
    /// Every class the client asks for must exist in the stylesheet. Catches
    /// the rename-one-side mistake that would otherwise show up only as an
    /// unstyled widget at runtime.
    fn every_class_used_by_the_client_is_defined() {
        let (_, classes) = shipped();
        let src_root = reachlock_core::paths::install_root().join("reachlock-client/src");
        let mut missing: Vec<String> = Vec::new();
        let mut used = 0usize;

        for path in rust_sources(&src_root) {
            // The theme module itself contains test fixtures naming classes
            // that deliberately do not exist.
            if path.components().any(|c| c.as_os_str() == "theme") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for class in class_literals(&text) {
                used += 1;
                if !classes.contains_key(&class) {
                    missing.push(format!("{}: '{class}'", path.display()));
                }
            }
        }
        assert!(used > 0, "found no theme call sites to check");
        assert!(
            missing.is_empty(),
            "client names {} style class(es) the stylesheet does not define:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }

    fn rust_sources(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(rust_sources(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        out
    }

    /// Pull the class strings out of `theme::text("x", ..)`, `theme::node("x")`,
    /// `node_with("x", ..)` and `Styled::new("x")`.
    fn class_literals(text: &str) -> Vec<String> {
        const CALLS: [&str; 4] = ["theme::text(", "theme::node(", "node_with(", "Styled::new("];
        let mut out = Vec::new();
        for call in CALLS {
            let mut rest = text;
            while let Some(i) = rest.find(call) {
                rest = &rest[i + call.len()..];
                let trimmed = rest.trim_start();
                // Only literal arguments are checkable; a computed class is
                // skipped rather than guessed at.
                if let Some(body) = trimmed.strip_prefix('"') {
                    if let Some(end) = body.find('"') {
                        out.push(body[..end].to_string());
                    }
                }
            }
        }
        out
    }
}
