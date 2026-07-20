//! Content browser (handoff completion §Priority 1): a real file tree over
//! `mods/reachlock/`. Scans each content type's directory, renders a
//! collapsible tree with a filter search, opens files on click, and deletes
//! files via a right-click context menu with confirmation.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::app::ContentType;
use crate::dialogs::{confirmation_dialog, ConfirmationResult};

/// How long a directory scan stays fresh before the tree re-reads disk.
const SCAN_TTL: Duration = Duration::from_secs(2);

/// What the browser asks the app shell to do this frame.
pub enum BrowserAction {
    /// Open an editor for `ct`; when `path` is set, load that file into it.
    Open {
        name: String,
        ct: ContentType,
        path: Option<PathBuf>,
    },
    /// Show a message in the status bar.
    Status(String),
}

/// The file-backed content types, in browser display order. Previewers
/// (ItemBrowser, SpriteViewer) persist nothing and are omitted from the tree.
const FILE_TYPES: [ContentType; 14] = [
    ContentType::ChartedSystem,
    ContentType::GateNetwork,
    ContentType::HullFrame,
    ContentType::HullMesh,
    ContentType::RoomTemplates,
    ContentType::Station,
    ContentType::Soul,
    ContentType::EnemyArchetype,
    ContentType::Faction,
    ContentType::Storyline,
    ContentType::Location,
    ContentType::EconomyGoods,
    ContentType::Item,
    ContentType::Contract,
];

/// Which editor owns a `.ron` file, judged by its parent directory (and for
/// the shared `hulls/` directory, by payload tag). Used by File > Open.
/// Delegates to [`ContentType::from_directory`] (the single source of truth)
/// so the directory↔type mapping is maintained in one place.
pub fn detect_content_type(path: &Path) -> Option<ContentType> {
    let dir = path.parent()?.file_name()?.to_str()?;
    if dir == "hulls" {
        return Some(classify_hull_file(path));
    }
    ContentType::from_directory(dir)
}

/// Three content types share `hulls/`. Peek at the RON payload tag to sort a
/// file into the right editor. Defaults to HullMesh for unrecognized files.
fn classify_hull_file(path: &Path) -> ContentType {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ContentType::HullMesh;
    };
    if text.contains("RoomTemplates(") {
        ContentType::RoomTemplates
    } else if text.contains("HullFrame(") {
        ContentType::HullFrame
    } else {
        ContentType::HullMesh
    }
}

pub struct ContentBrowser {
    filter: String,
    /// Scan results per type, in FILE_TYPES order.
    files: Vec<(ContentType, Vec<PathBuf>)>,
    last_scan: Option<Instant>,
    pending_delete: Option<(ContentType, PathBuf)>,
    /// Root of the mods tree; overridable in Preferences.
    pub root: PathBuf,
}

impl ContentBrowser {
    pub fn new() -> Self {
        Self {
            filter: String::new(),
            files: Vec::new(),
            last_scan: None,
            pending_delete: None,
            root: PathBuf::from("mods/reachlock"),
        }
    }

    /// Force a rescan on the next frame (e.g. after a save creates a file).
    pub fn invalidate(&mut self) {
        self.last_scan = None;
    }

    fn scan_if_stale(&mut self) {
        if self.last_scan.is_some_and(|t| t.elapsed() < SCAN_TTL) {
            return;
        }
        self.last_scan = Some(Instant::now());
        let mut files: Vec<(ContentType, Vec<PathBuf>)> =
            FILE_TYPES.iter().map(|ct| (*ct, Vec::new())).collect();
        let mut push = |ct: ContentType, path: PathBuf| {
            if let Some((_, list)) = files.iter_mut().find(|(t, _)| *t == ct) {
                list.push(path);
            }
        };
        let mut scanned_dirs: Vec<&str> = Vec::new();
        for ct in FILE_TYPES {
            let dir = ct.directory();
            if scanned_dirs.contains(&dir) {
                continue;
            }
            scanned_dirs.push(dir);
            let Ok(entries) = std::fs::read_dir(self.root.join(dir)) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "ron"))
                .collect();
            paths.sort();
            for path in paths {
                let owner = if dir == "hulls" {
                    classify_hull_file(&path)
                } else {
                    ct
                };
                push(owner, path);
            }
        }
        self.files = files;
    }

    /// Case-insensitive substring match; returns the byte range of the match
    /// in `name` for highlighting.
    fn filter_match(&self, name: &str) -> Option<std::ops::Range<usize>> {
        if self.filter.is_empty() {
            return Some(0..0);
        }
        let lower_name = name.to_lowercase();
        let lower_filter = self.filter.to_lowercase();
        lower_name
            .find(&lower_filter)
            .map(|start| start..start + lower_filter.len())
    }

    /// File name label with the filter match highlighted, in monospace.
    fn file_label(
        ui: &egui::Ui,
        name: &str,
        highlight: &std::ops::Range<usize>,
    ) -> egui::text::LayoutJob {
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let normal = ui.visuals().text_color();
        let hilite = ui.visuals().strong_text_color();
        let mut job = egui::text::LayoutJob::default();
        if highlight.is_empty() {
            job.append(name, 0.0, egui::TextFormat::simple(font, normal));
            return job;
        }
        // The range comes from a lowercased copy; char boundaries match
        // for ASCII file names, and we fall back to no highlight otherwise.
        if !name.is_char_boundary(highlight.start) || !name.is_char_boundary(highlight.end) {
            job.append(name, 0.0, egui::TextFormat::simple(font, normal));
            return job;
        }
        job.append(
            &name[..highlight.start],
            0.0,
            egui::TextFormat::simple(font.clone(), normal),
        );
        let mut strong = egui::TextFormat::simple(font.clone(), hilite);
        strong.background = ui.visuals().selection.bg_fill.linear_multiply(0.4);
        job.append(&name[highlight.clone()], 0.0, strong);
        job.append(
            &name[highlight.end..],
            0.0,
            egui::TextFormat::simple(font, normal),
        );
        job
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Vec<BrowserAction> {
        self.scan_if_stale();
        let mut actions = Vec::new();

        egui::SidePanel::left("browser_panel")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("Content Browser");
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text("filter files…")
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let filtering = !self.filter.is_empty();
                    // Collect UI events first; mutating self inside the
                    // closures fights the borrow checker.
                    let mut open_file: Option<(ContentType, PathBuf)> = None;
                    let mut delete_file: Option<(ContentType, PathBuf)> = None;
                    let mut new_editor: Option<ContentType> = None;

                    for (ct, paths) in &self.files {
                        let visible: Vec<(&PathBuf, std::ops::Range<usize>)> = paths
                            .iter()
                            .filter_map(|p| {
                                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                self.filter_match(name).map(|r| (p, r))
                            })
                            .collect();
                        if filtering && visible.is_empty() {
                            continue;
                        }
                        let header = format!("{} ({})", ct.name(), paths.len());
                        egui::CollapsingHeader::new(header)
                            .id_salt(("browser_type", *ct))
                            .open(filtering.then_some(true))
                            .show(ui, |ui| {
                                if ui.small_button("➕ New").clicked() {
                                    new_editor = Some(*ct);
                                }
                                if visible.is_empty() {
                                    ui.weak("No files yet.");
                                }
                                for (path, highlight) in &visible {
                                    let name =
                                        path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                                    ui.indent(("browser_file", name), |ui| {
                                        let job = Self::file_label(ui, name, highlight);
                                        let response = ui
                                            .selectable_label(false, job)
                                            .on_hover_text(path.display().to_string());
                                        if response.clicked() {
                                            open_file = Some((*ct, (*path).clone()));
                                        }
                                        response.context_menu(|ui| {
                                            if ui.button("Delete").clicked() {
                                                delete_file = Some((*ct, (*path).clone()));
                                                ui.close_menu();
                                            }
                                        });
                                    });
                                }
                            });
                    }

                    if let Some((ct, path)) = open_file {
                        let name = path
                            .file_stem()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        actions.push(BrowserAction::Open {
                            name,
                            ct,
                            path: Some(path),
                        });
                    }
                    if let Some(pending) = delete_file {
                        self.pending_delete = Some(pending);
                    }
                    if let Some(ct) = new_editor {
                        actions.push(BrowserAction::Open {
                            name: format!("New {}", ct.name()),
                            ct,
                            path: None,
                        });
                    }

                    ui.separator();
                    ui.collapsing("Previewers", |ui| {
                        for ct in [ContentType::ItemBrowser, ContentType::SpriteViewer] {
                            if ui.button(ct.name()).clicked() {
                                actions.push(BrowserAction::Open {
                                    name: ct.name().to_string(),
                                    ct,
                                    path: None,
                                });
                            }
                        }
                    });
                });
            });

        // Deletion confirmation modal.
        if let Some((_, path)) = &self.pending_delete {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();
            match confirmation_dialog(
                ctx,
                "Delete file",
                &format!("Permanently delete \"{name}\" from disk?"),
                "Delete",
                "Cancel",
                None,
            ) {
                Some(ConfirmationResult::Ok) => {
                    let (_, path) = self.pending_delete.take().expect("checked above");
                    match std::fs::remove_file(&path) {
                        Ok(()) => {
                            actions.push(BrowserAction::Status(format!("Deleted {name}")));
                        }
                        Err(e) => {
                            actions.push(BrowserAction::Status(format!("Delete failed: {e}")));
                        }
                    }
                    self.invalidate();
                }
                Some(_) => {
                    self.pending_delete = None;
                }
                None => {}
            }
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_content_type_from_directory() {
        let p = Path::new("mods/reachlock/souls/hero.ron");
        assert_eq!(detect_content_type(p), Some(ContentType::Soul));
        let p = Path::new("mods/reachlock/contracts/escort.ron");
        assert_eq!(detect_content_type(p), Some(ContentType::Contract));
        let p = Path::new("mods/reachlock/combat/raider.ron");
        assert_eq!(detect_content_type(p), Some(ContentType::EnemyArchetype));
    }

    #[test]
    fn detect_content_type_ignores_unknown_dirs() {
        let p = Path::new("mods/reachlock/schemas/foo.json");
        assert_eq!(detect_content_type(p), None);
    }

    #[test]
    fn classify_hull_file_by_payload_tag() {
        let dir = std::env::temp_dir().join("reachlock_browser_tests/hulls");
        let _ = std::fs::create_dir_all(&dir);
        let cases = [
            (
                "frame.ron",
                "HullFrame(\n    id: \"x\",\n)",
                ContentType::HullFrame,
            ),
            (
                "rooms.ron",
                "RoomTemplates(\n    templates: [],\n)",
                ContentType::RoomTemplates,
            ),
            (
                "mesh.ron",
                "HullConfiguration(\n    hull_id: \"x\",\n)",
                ContentType::HullMesh,
            ),
        ];
        for (name, body, expected) in cases {
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            assert_eq!(detect_content_type(&path), Some(expected), "{name}");
            let _ = std::fs::remove_file(&path);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
