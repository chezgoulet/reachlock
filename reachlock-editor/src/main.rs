mod agent;
mod ai;
mod app;
mod browser;
mod command_palette;
mod cross_ref;
mod dialogs;
mod diff;
pub mod editors;
mod help_window;
mod io;
mod mcp;
mod mcp_http;
mod preferences_window;
mod preview;
mod schema;
mod seed_workflow;
mod settings_window;
mod template_manager;
mod validation;

use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app::{build_default_registry, ContentType, Editor, EditorRegistry};
use browser::{BrowserAction, ContentBrowser};
use dialogs::{confirmation_dialog, ConfirmationResult};
use help_window::HelpWindow;
use preferences_window::PreferencesWindow;
use preview::PreviewPanel;
use reachlock_core::content::refs::ContentTree;
use schema::SchemaCache;
use seed_workflow::{SeedAction, SeedWorkflow};
use settings_window::AiSettingsWindow;

/// Snapshot undo: keep at most this many steps per tab.
const UNDO_CAP: usize = 50;
/// Changes landing within this window coalesce into one undo step, so
/// typing a sentence doesn't cost one step per keystroke.
const UNDO_COALESCE: Duration = Duration::from_millis(800);

/// A modal decision the user still owes us. One at a time.
enum PendingAction {
    /// Close tab `idx`, which has unsaved changes.
    CloseTab(usize),
    /// Close every tab; the listed (dirty) tabs need a save decision.
    CloseAll,
    /// Quit the app; some tabs have unsaved changes.
    Quit,
    /// Delete the selected entry (named) in the active editor.
    DeleteEntry(String),
    /// Save tab `idx` despite RON comment loss (user confirmed).
    CommentLossSave(usize),
}

struct EditorApp {
    registry: EditorRegistry,
    open_editors: Vec<OpenEditor>,
    browser: ContentBrowser,
    seed_workflow: SeedWorkflow,
    preview: PreviewPanel,
    ai_settings: AiSettingsWindow,
    ai_prompt: String,
    ai_running: bool,
    ai_status: Arc<std::sync::Mutex<String>>,
    ai_result_rx: Option<std::sync::mpsc::Receiver<ai::AiGenOutcome>>,
    schemas: SchemaCache,
    status_text: String,
    /// Mirror of the last status text; a change re-arms the expiry timer.
    last_status: String,
    /// When to clear a non-error status message (5s after it was set).
    status_expiry: Option<Instant>,
    /// Last window title pushed via ViewportCommand, to avoid re-sending.
    last_title: String,
    active_tab: Option<usize>,
    show_browser: bool,
    help: HelpWindow,
    preferences: PreferencesWindow,
    /// Visuals from loaded preferences apply on the first frame.
    prefs_applied: bool,
    last_autosave: Instant,
    /// File > Validate All results: (tab name, issues) per editor, shown in
    /// a window until dismissed. Empty issue lists mean the tab is clean.
    validation_report: Option<Vec<(String, Vec<String>)>>,
    /// Ctrl+Shift+P command palette (S67).
    palette: command_palette::CommandPalette,
    /// Bundled starting-point documents for File > New from Template.
    templates: template_manager::TemplateManager,
    /// Cross-reference index over the content tree (S69). Built on demand and
    /// dropped when the content root moves, since it is a snapshot of disk.
    cross_refs: Option<cross_ref::CrossReferenceIndex>,
    /// Find Usages query and its last results.
    find_usages: Option<FindUsages>,
    /// Save preview: the tab it was taken from and the computed diff.
    diff_preview: Option<(String, diff::DiffResult)>,
    pending: Option<PendingAction>,
    /// Set once a quit is confirmed so the close request passes through.
    allow_close: bool,
    /// Repaint requested by a state change outside direct input (timers,
    /// async apply). Avoids a busy per-frame `request_repaint`.
    repaint_requested: bool,
    /// Warnings from editor constructor scans: unparseable files.
    load_warnings: Vec<String>,
    /// Warnings from the startup content-tree scan (directories no tab has open).
    startup_warnings: Vec<String>,
    /// Whether the Content Warnings window is visible.
    show_warnings: bool,
    /// Whether the startup content-tree scan has run.
    startup_scan_done: bool,
    /// Requests from the agent thread, executed on this thread once a frame.
    session_queue: agent::bridge::SessionQueue,
    /// Plan (read-only) vs Build (writes unlocked).
    agent_mode: agent::mode::Mode,
    /// The running task's event channel, if one is running.
    agent_task: Option<agent::session::AgentTask>,
    /// Everything the current task has emitted, oldest first.
    agent_transcript: Vec<agent::session::AgentEvent>,
    agent_prompt: String,
    /// Whether the assistant side panel is visible.
    show_assistant: bool,
    /// The MCP-over-HTTP endpoint, when the author has switched it on.
    /// Dropping it stops the listener.
    mcp_http: Option<mcp_http::McpHttpServer>,
    /// The running conversation: message history and its log. Shared with the
    /// agent thread, and deliberately **not** reset per Send — see
    /// [`agent::session::Conversation`].
    agent_conversation: std::sync::Arc<std::sync::Mutex<agent::session::Conversation>>,
}

/// Find Usages state: what the author typed, and what the index answered.
struct FindUsages {
    query: String,
    /// `(source id, field path)` for each place the query id is referenced.
    results: Vec<(String, String)>,
    /// True once a search has run, so "no results" reads differently from
    /// "nothing searched yet".
    searched: bool,
}

struct OpenEditor {
    editor: Box<dyn Editor>,
    name: String,
    path: Option<std::path::PathBuf>,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
    /// Editor snapshot as of the end of the previous frame.
    last_seen: Option<String>,
    /// When the last undo step was pushed (drives coalescing).
    last_push: Option<Instant>,
    /// Whether the raw file text contained RON comments (`//` or `/*`).
    /// If true, the first save shows a confirmation dialog about comment loss.
    has_comments: bool,
}

impl OpenEditor {
    fn new(editor: Box<dyn Editor>, name: String, path: Option<std::path::PathBuf>) -> Self {
        OpenEditor {
            editor,
            name,
            path,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_seen: None,
            last_push: None,
            has_comments: false,
        }
    }

    /// Called once per frame (after every mutation path has run) to detect
    /// state changes and push undo steps. Works for any editor that
    /// implements `snapshot`; the rest silently opt out.
    fn track_changes(&mut self) {
        let Some(now) = self.editor.snapshot() else {
            return;
        };
        match &self.last_seen {
            None => self.last_seen = Some(now),
            Some(prev) if *prev != now => {
                let past_window = self.last_push.is_none_or(|t| t.elapsed() >= UNDO_COALESCE);
                if past_window {
                    self.undo_stack.push(prev.clone());
                    if self.undo_stack.len() > UNDO_CAP {
                        self.undo_stack.remove(0);
                    }
                    self.last_push = Some(Instant::now());
                }
                self.redo_stack.clear();
                self.last_seen = Some(now);
            }
            Some(_) => {}
        }
    }

    /// Guarantee the next `track_changes` records an undo step.
    ///
    /// Undo pushes are coalesced on an 800ms window so a burst of typing is
    /// one step. An agent write is a single deliberate action, not typing —
    /// without this it can land inside the author's coalescing window and
    /// become unundoable.
    fn force_undo_point(&mut self) {
        self.last_push = None;
    }

    fn undo(&mut self) -> String {
        let Some(top) = self.undo_stack.pop() else {
            return "Nothing to undo".into();
        };
        if let Some(cur) = self.editor.snapshot() {
            self.redo_stack.push(cur);
        }
        match self.editor.restore_snapshot(&top) {
            Ok(()) => {
                self.last_seen = Some(top);
                self.last_push = None;
                format!("Undo ({} left)", self.undo_stack.len())
            }
            Err(e) => format!("Undo failed: {e}"),
        }
    }

    fn redo(&mut self) -> String {
        let Some(top) = self.redo_stack.pop() else {
            return "Nothing to redo".into();
        };
        if let Some(cur) = self.editor.snapshot() {
            self.undo_stack.push(cur);
        }
        match self.editor.restore_snapshot(&top) {
            Ok(()) => {
                self.last_seen = Some(top);
                self.last_push = None;
                format!("Redo ({} left)", self.redo_stack.len())
            }
            Err(e) => format!("Redo failed: {e}"),
        }
    }
}

impl Default for EditorApp {
    fn default() -> Self {
        Self {
            registry: build_default_registry(),
            open_editors: Vec::new(),
            browser: ContentBrowser::new(),
            seed_workflow: SeedWorkflow::new(),
            preview: PreviewPanel::new(),
            ai_settings: AiSettingsWindow::load(),
            ai_prompt: String::new(),
            ai_running: false,
            ai_status: Arc::new(std::sync::Mutex::new(String::new())),
            ai_result_rx: None,
            schemas: SchemaCache::load_all(),
            status_text: "Ready".into(),
            last_status: "Ready".into(),
            status_expiry: None,
            last_title: String::new(),
            active_tab: None,
            show_browser: true,
            help: HelpWindow::new(),
            preferences: PreferencesWindow::load(),
            prefs_applied: false,
            last_autosave: Instant::now(),
            validation_report: None,
            palette: command_palette::CommandPalette::new(),
            templates: template_manager::TemplateManager::new(),
            cross_refs: None,
            find_usages: None,
            diff_preview: None,
            pending: None,
            allow_close: false,
            repaint_requested: true,
            load_warnings: Vec::new(),
            startup_warnings: Vec::new(),
            show_warnings: false,
            startup_scan_done: false,
            session_queue: agent::bridge::SessionQueue::new(),
            agent_mode: agent::mode::Mode::default(),
            agent_task: None,
            agent_transcript: Vec::new(),
            agent_prompt: String::new(),
            show_assistant: false,
            mcp_http: None,
            agent_conversation: Default::default(),
        }
    }
}

/// "New Soul" → "new_soul" (suggested file stem for Save As).
fn suggest_stem(name: &str) -> String {
    let stem: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = stem.trim_matches('_');
    if trimmed.is_empty() {
        "untitled".into()
    } else {
        trimmed.to_string()
    }
}

impl EditorApp {
    fn open_new_editor(&mut self, name: &str, ct: ContentType) {
        if let Some(editor) = self.registry.create(ct) {
            self.collect_load_warnings(&*editor);
            let idx = self.open_editors.len();
            self.open_editors
                .push(OpenEditor::new(editor, name.to_string(), None));
            self.active_tab = Some(idx);
            self.status_text = format!("Opened {name}");
        } else {
            self.status_text = format!("No editor for {:?}", ct);
        }
    }

    /// Open an editor for `ct` and load `path` into it. Focuses the
    /// existing tab instead if that file is already open.
    fn open_editor_for_file(&mut self, name: &str, ct: ContentType, path: &std::path::Path) {
        if let Some(idx) = self
            .open_editors
            .iter()
            .position(|o| o.path.as_deref() == Some(path))
        {
            self.active_tab = Some(idx);
            return;
        }
        // Check the raw file text for RON comments before loading, so we can
        // warn before a save strips them. A naive `//` / `/*` search is fine
        // (false positives in string literals are acceptable for this guard).
        let has_comments = std::fs::read_to_string(path)
            .ok()
            .is_some_and(|text| text.contains("//") || text.contains("/*"));
        let Some(mut editor) = self.registry.create(ct) else {
            self.status_text = format!("No editor for {:?}", ct);
            return;
        };
        self.collect_load_warnings(&*editor);
        match editor.load(path) {
            Ok(()) => {
                let idx = self.open_editors.len();
                self.open_editors.push(OpenEditor {
                    editor,
                    name: name.to_string(),
                    path: Some(path.to_path_buf()),
                    undo_stack: Vec::new(),
                    redo_stack: Vec::new(),
                    last_seen: None,
                    last_push: None,
                    has_comments,
                });
                self.active_tab = Some(idx);
                self.status_text = format!("Opened {}", path.display());
                self.preferences.prefs.push_recent(path);
                self.preferences.save();
            }
            Err(e) => {
                self.status_text = format!("Open failed: {e}");
            }
        }
    }

    /// If the editor has constructor-scan warnings, surface them.
    fn collect_load_warnings(&mut self, editor: &dyn Editor) {
        let warns = editor.load_warnings();
        // Always replace so switching from a broken tab to a clean tab
        // properly clears the previous warnings.
        self.load_warnings = warns.to_vec();
        if !warns.is_empty() {
            self.show_warnings = true;
            let n = warns.len();
            self.status_text = format!("{n} file(s) failed to parse — see Warnings");
        }
    }

    fn handle_browser_actions(&mut self, actions: Vec<BrowserAction>) {
        for action in actions {
            match action {
                BrowserAction::Open { name, ct, path } => match path {
                    Some(path) => self.open_editor_for_file(&name, ct, &path),
                    None => self.open_new_editor(&name, ct),
                },
                BrowserAction::Status(msg) => self.status_text = msg,
            }
        }
    }

    /// Save tab `idx` to its path, falling back to Save As when it has
    /// none. Returns true when the file hit disk.
    fn save_editor(&mut self, idx: usize) -> bool {
        // Comment-loss guard: if the file had RON comments when loaded, show a
        // confirmation dialog before the first save strips them. The pending
        // action clears `has_comments` after confirmation so we don't loop.
        if let Some(open) = self.open_editors.get(idx) {
            if open.has_comments {
                self.pending = Some(PendingAction::CommentLossSave(idx));
                return false;
            }
        }
        let Some(open) = self.open_editors.get_mut(idx) else {
            return false;
        };
        // Multi-entry editors (souls, systems, enemies, …) persist each dirty
        // entry to its own path via `save_all`. Single-entry editors return
        // Ok(false) from save_all and fall through to path-based save below.
        match open.editor.save_all() {
            Ok(true) => {
                open.editor.mark_saved();
                self.browser.invalidate();
                self.invalidate_cross_refs();
                self.status_text = "Saved".into();
                true
            }
            Ok(false) => {
                let Some(path) = open.path.clone() else {
                    return self.save_editor_as(idx);
                };
                match open.editor.save(&path) {
                    Ok(()) => {
                        open.editor.mark_saved();
                        self.browser.invalidate();
                        self.invalidate_cross_refs();
                        self.status_text = format!("Saved {}", path.display());
                        true
                    }
                    Err(e) => {
                        self.status_text = format!("Save error: {e}");
                        false
                    }
                }
            }
            Err(e) => {
                tracing::error!("save_all failed: {e}");
                self.status_text = format!("Save error: {e}");
                false
            }
        }
    }

    /// Save As via the native file dialog. Rebinds the tab to the chosen
    /// path on success.
    fn save_editor_as(&mut self, idx: usize) -> bool {
        let Some(open) = self.open_editors.get_mut(idx) else {
            return false;
        };
        // Multi-entry editors save each dirty entry to its own path; the Save
        // As dialog is meaningless for them, so just persist the dirty set.
        match open.editor.save_all() {
            Ok(true) => {
                open.editor.mark_saved();
                self.browser.invalidate();
                self.invalidate_cross_refs();
                self.status_text = "Saved".into();
                return true;
            }
            Ok(false) => {} // Single-entry editor — show Save As dialog.
            Err(e) => {
                tracing::error!("save_all failed: {e}");
                self.status_text = format!("Save error: {e}");
                return false;
            }
        }
        let ct = open.editor.content_type();
        let default_dir = self.browser.root.join(ct.directory());
        let mut dialog = rfd::FileDialog::new()
            .add_filter("RON content", &["ron"])
            .set_file_name(format!("{}.ron", suggest_stem(&open.name)));
        if default_dir.is_dir() {
            // Prefer an absolute path so the dialog lands in the workspace.
            let dir = default_dir.canonicalize().unwrap_or(default_dir);
            dialog = dialog.set_directory(dir);
        }
        let Some(mut path) = dialog.save_file() else {
            self.status_text = "Save As canceled".into();
            return false;
        };
        if path.extension().is_none() {
            path.set_extension("ron");
        }
        match open.editor.save(&path) {
            Ok(()) => {
                open.name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&open.name)
                    .to_string();
                open.path = Some(path.clone());
                open.editor.mark_saved();
                self.browser.invalidate();
                self.invalidate_cross_refs();
                self.status_text = format!("Saved {}", path.display());
                self.preferences.prefs.push_recent(&path);
                self.preferences.save();
                true
            }
            Err(e) => {
                self.status_text = format!("Save As error: {e}");
                false
            }
        }
    }

    /// File > Open: native picker, content type detected from the path.
    fn open_file_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter("RON content", &["ron"]);
        if self.browser.root.is_dir() {
            let root = self
                .browser
                .root
                .canonicalize()
                .unwrap_or_else(|_| self.browser.root.clone());
            dialog = dialog.set_directory(root);
        }
        let Some(path) = dialog.pick_file() else {
            return;
        };
        let Some(ct) = browser::detect_content_type(&path) else {
            self.status_text = format!(
                "Can't tell which editor owns {} — open it from a mods/reachlock content directory",
                path.display()
            );
            return;
        };
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        self.open_editor_for_file(&name, ct, &path);
    }

    /// Remove tab `idx` without any dirty check, fixing up `active_tab`.
    fn close_tab(&mut self, idx: usize) {
        if idx >= self.open_editors.len() {
            return;
        }
        self.open_editors.remove(idx);
        // After removing tab at `idx`, fix `active_tab`:
        // Case 1: active_tab > idx — shift down by 1
        // Case 2: active_tab >= new len — tab was removed or was the last tab
        // Case 3: active_tab < idx — unchanged
        self.active_tab = if self.open_editors.is_empty() {
            None
        } else {
            match self.active_tab {
                Some(a) if a > idx => Some(a - 1),
                Some(a) if a >= self.open_editors.len() => Some(self.open_editors.len() - 1),
                other => other,
            }
        };
    }

    /// Close a tab, routing through the confirmation dialog when dirty.
    fn request_close_tab(&mut self, idx: usize) {
        let Some(open) = self.open_editors.get(idx) else {
            return;
        };
        if open.editor.has_unsaved_changes() {
            self.pending = Some(PendingAction::CloseTab(idx));
        } else {
            self.close_tab(idx);
        }
    }

    fn dirty_tab_indices(&self) -> Vec<usize> {
        self.open_editors
            .iter()
            .enumerate()
            .filter(|(_, o)| o.editor.has_unsaved_changes())
            .map(|(i, _)| i)
            .collect()
    }

    fn dirty_tab_names(&self) -> Vec<String> {
        self.dirty_tab_indices()
            .into_iter()
            .filter_map(|i| self.open_editors.get(i).map(|o| o.name.clone()))
            .collect()
    }

    /// Save every dirty tab. Returns false if any save failed or was
    /// canceled (the caller should then abort the close/quit).
    fn save_all_dirty(&mut self) -> bool {
        // Indices shift only on close, not on save, so this is stable.
        for idx in self.dirty_tab_indices() {
            if !self.save_editor(idx) {
                return false;
            }
        }
        true
    }

    fn request_quit(&mut self, ctx: &egui::Context) {
        if self.dirty_tab_indices().is_empty() || self.allow_close {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.pending = Some(PendingAction::Quit);
        }
    }

    /// Build (or reuse) the cross-reference index over the content tree.
    ///
    /// It is a snapshot of what is on disk, so unsaved edits are not in it.
    /// Rebuilt whenever the content root moves or a save lands, which is what
    /// `invalidate_cross_refs` is for.
    fn cross_ref_index(&mut self) -> &cross_ref::CrossReferenceIndex {
        if self.cross_refs.is_none() {
            let snapshot = cross_ref::ContentIndexSnapshot::from_content_root(&self.browser.root);
            self.cross_refs = Some(cross_ref::CrossReferenceIndex::build(&snapshot));
        }
        self.cross_refs.as_ref().expect("just built")
    }

    /// Drop the cached index so the next query re-reads disk.
    fn invalidate_cross_refs(&mut self) {
        self.cross_refs = None;
    }

    /// File > Broken Reference Report — every reference in the content tree
    /// that points at an id nothing defines, plus each open tab's own
    /// validation issues.
    fn run_broken_reference_report(&mut self) {
        self.invalidate_cross_refs();
        let index = self.cross_ref_index().clone();
        let editors: Vec<(String, &dyn Editor)> = self
            .open_editors
            .iter()
            .map(|o| (o.name.clone(), o.editor.as_ref()))
            .collect();
        let report = validation::broken_reference_report(&editors, &index);
        let broken = index.broken_references().len();
        // How many open tabs are actually implicated, so the author knows
        // whether this is something they can fix from here or a problem
        // elsewhere in the tree.
        let affected_tabs = self
            .open_editors
            .iter()
            .filter(|o| validation::count_broken_refs_in_editor(o.editor.as_ref(), &index) > 0)
            .count();
        self.status_text = if broken == 0 {
            "Reference check: every reference in the content tree resolves".into()
        } else if affected_tabs == 0 {
            format!("Reference check: {broken} broken reference(s), none in an open tab")
        } else {
            format!(
                "Reference check: {broken} broken reference(s) across {affected_tabs} open tab(s)"
            )
        };
        self.validation_report = Some(report);
    }

    /// Edit > Find Usages — where an id is referenced from.
    fn run_find_usages(&mut self) {
        let Some(state) = self.find_usages.as_ref() else {
            return;
        };
        let query = state.query.trim().to_string();
        if query.is_empty() {
            if let Some(state) = self.find_usages.as_mut() {
                state.results.clear();
                state.searched = false;
            }
            return;
        }
        let results: Vec<(String, String)> = self
            .cross_ref_index()
            .usages_of(&query)
            .iter()
            .map(|r| (r.source_id.clone(), r.field_path.clone()))
            .collect();
        if let Some(state) = self.find_usages.as_mut() {
            state.results = results;
            state.searched = true;
        }
    }

    /// File > Preview Changes — what Save would write, against what is on
    /// disk. Read-only: it writes to a temp file, diffs, and deletes it, so a
    /// preview can never be the thing that corrupts the document.
    fn preview_changes(&mut self) {
        let Some(idx) = self.active_tab else {
            self.status_text = "No editor open to preview".into();
            return;
        };
        let Some(open) = self.open_editors.get(idx) else {
            return;
        };
        let Some(path) = open.path.clone() else {
            self.status_text = "This document has never been saved — nothing to compare".into();
            return;
        };
        let scratch = std::env::temp_dir().join(format!("reachlock_preview_{idx}.ron"));
        if let Err(e) = open.editor.save(&scratch) {
            self.status_text = format!("Preview failed: {e}");
            return;
        }
        let new_text = match std::fs::read_to_string(&scratch) {
            Ok(t) => t,
            Err(e) => {
                self.status_text = format!("Preview failed: {e}");
                let _ = std::fs::remove_file(&scratch);
                return;
            }
        };
        let _ = std::fs::remove_file(&scratch);
        match diff::DiffResult::compute(&path, &new_text) {
            Ok(d) => {
                self.status_text = if d.unchanged {
                    format!("{} is up to date", path.display())
                } else {
                    format!("Previewing changes to {}", path.display())
                };
                self.diff_preview = Some((open.name.clone(), d));
            }
            Err(e) => self.status_text = format!("Preview failed: {e}"),
        }
    }

    /// File > New from Template — open a fresh tab seeded from a bundled
    /// starting-point document.
    ///
    /// The new tab is deliberately left with no path, so the first Save opens
    /// Save As. Binding it to the template file would make an ordinary Ctrl+S
    /// overwrite the template for every future document.
    fn open_from_template(&mut self, entry: &template_manager::TemplateEntry) {
        let text = match self.templates.load_template(entry) {
            Ok(t) => t,
            Err(e) => {
                self.status_text = format!("Template failed: {e}");
                return;
            }
        };
        let Some(mut editor) = self.registry.create(entry.content_type) else {
            self.status_text = format!("No editor for {:?}", entry.content_type);
            return;
        };
        // Templates are content files, so load them the same way any file is
        // loaded — via a scratch path, since `load` takes a path.
        let scratch = std::env::temp_dir().join(format!(
            "reachlock_template_{}.ron",
            entry.content_type.directory()
        ));
        if let Err(e) = std::fs::write(&scratch, &text) {
            self.status_text = format!("Template failed: {e}");
            return;
        }
        let loaded = editor.load(&scratch);
        let _ = std::fs::remove_file(&scratch);
        if let Err(e) = loaded {
            self.status_text = format!("Template failed: {e}");
            return;
        }
        editor.touch();
        let name = format!("New {} (template)", entry.content_type.name());
        let idx = self.open_editors.len();
        self.open_editors
            .push(OpenEditor::new(editor, name.clone(), None));
        self.active_tab = Some(idx);
        self.status_text = format!("{name} — Save As to choose a filename");
    }

    /// Run one command-palette action. Every arm routes to the same method the
    /// menu item does, so the palette can never drift into being a second,
    /// subtly different way to do things.
    fn run_palette_action(&mut self, action: command_palette::PaletteAction, ctx: &egui::Context) {
        use command_palette::PaletteAction as A;
        match action {
            A::NewEditor(ct) => self.open_new_editor(&format!("New {}", ct.name()), ct),
            A::Open => self.open_file_dialog(),
            A::Save => {
                if let Some(idx) = self.active_tab {
                    self.save_editor(idx);
                }
            }
            A::SaveAs => {
                if let Some(idx) = self.active_tab {
                    self.save_editor_as(idx);
                }
            }
            A::CloseTab => {
                if let Some(idx) = self.active_tab {
                    self.request_close_tab(idx);
                }
            }
            A::CloseAll => {
                if self.dirty_tab_indices().is_empty() {
                    self.open_editors.clear();
                    self.active_tab = None;
                } else {
                    self.pending = Some(PendingAction::CloseAll);
                }
            }
            A::Undo => {
                if let Some(open) = self.active_open_mut() {
                    self.status_text = open.undo();
                }
            }
            A::Redo => {
                if let Some(open) = self.active_open_mut() {
                    self.status_text = open.redo();
                }
            }
            A::ToggleBrowser => self.show_browser = !self.show_browser,
            A::AiGenerate => {
                self.status_text = "Use the Generate bar below the seed panel".into();
            }
            A::Help => self.help.open = true,
            A::Preferences => self.preferences.open = true,
            A::AiSettings => self.ai_settings.open = true,
            A::ValidateAll => self.run_validate_all(),
            A::FindUsages => self.open_find_usages(),
            A::BrokenReferenceReport => self.run_broken_reference_report(),
            A::PreviewChanges => self.preview_changes(),
            A::Duplicate => self.duplicate_active_tab(),
            A::Quit => self.request_quit(ctx),
        }
    }

    /// Open the Find Usages window, pre-filled with the active document's id
    /// when there is an obvious one.
    fn open_find_usages(&mut self) {
        let seed = self
            .active_open()
            .and_then(|o| o.editor.document_ids().into_iter().next())
            .unwrap_or_default();
        self.find_usages = Some(FindUsages {
            query: seed,
            results: Vec::new(),
            searched: false,
        });
        self.run_find_usages();
    }

    /// File > Validate All Open Editors.
    fn run_validate_all(&mut self) {
        let report: Vec<(String, Vec<String>)> = self
            .open_editors
            .iter()
            .map(|o| (o.name.clone(), o.editor.validate()))
            .collect();
        let clean = report.iter().filter(|(_, v)| v.is_empty()).count();
        let dirty = report.len() - clean;
        self.status_text = format!("Validation: {clean} editor(s) clean, {dirty} with issues");
        self.validation_report = Some(report);
    }

    /// Duplicate the active tab's document into a new, unsaved tab.
    fn duplicate_active_tab(&mut self) {
        let Some(open) = self.active_open() else {
            self.status_text = "No editor open to duplicate".into();
            return;
        };
        let Some(state) = open.editor.snapshot() else {
            self.status_text = "This editor does not support duplication".into();
            return;
        };
        let ct = open.editor.content_type();
        let name = format!("{} (copy)", open.name);
        let Some(mut editor) = self.registry.create(ct) else {
            return;
        };
        if let Err(e) = editor.restore_snapshot(&state) {
            self.status_text = format!("Duplicate failed: {e}");
            return;
        }
        let idx = self.open_editors.len();
        self.open_editors
            .push(OpenEditor::new(editor, name.clone(), None));
        self.active_tab = Some(idx);
        self.status_text = format!("{name} — Save As to choose a filename");
    }

    /// Global keyboard shortcuts (handoff completion §Priority 2).
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};

        // Check Ctrl+Shift combos before their Ctrl siblings: consume_key
        // matches modifiers exactly, but keeping the order explicit guards
        // against surprises.
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::S)) {
            if let Some(idx) = self.active_tab {
                self.save_editor_as(idx);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::S)) {
            if let Some(idx) = self.active_tab {
                self.save_editor(idx);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::O)) {
            self.open_file_dialog();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::W)) {
            if let Some(idx) = self.active_tab {
                self.request_close_tab(idx);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Q)) {
            self.request_quit(ctx);
        }
        // Ctrl+Shift+P before Ctrl+P: consume_key matches modifiers exactly,
        // but the ordering keeps the intent readable.
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::P)) {
            self.palette.open = !self.palette.open;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::F)) {
            self.open_find_usages();
        }

        // Undo/redo stay out of the way while a text field has focus so
        // TextEdit keeps its own in-field undo.
        let typing = ctx.wants_keyboard_input();

        if assistant_mode_shortcut(ctx) {
            self.agent_mode = self.agent_mode.toggled();
            // Say it out loud: the mode changes what the assistant is allowed
            // to do, and a silent flip is how an author ends up surprised
            // either by a refusal or by a write.
            self.status_text = format!(
                "Assistant mode: {} — {}",
                self.agent_mode.label(),
                match self.agent_mode {
                    agent::mode::Mode::Plan => "read-only",
                    agent::mode::Mode::Build => "writes unlocked",
                }
            );
            self.show_assistant = true;
        }

        if !typing {
            let redo = ctx.input_mut(|i| {
                i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z)
                    || i.consume_key(Modifiers::CTRL, Key::Y)
            });
            if redo {
                if let Some(open) = self.active_open_mut() {
                    self.status_text = open.redo();
                }
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::CTRL, Key::Z)) {
                if let Some(open) = self.active_open_mut() {
                    self.status_text = open.undo();
                }
            }
            // Delete removes the selected entry in the active editor, with
            // confirmation. Editors without a deletable selection (or with
            // their own Delete handling, like the gate canvas) opt out via
            // selected_entry_name.
            if self.pending.is_none() && ctx.input(|i| i.key_pressed(Key::Delete)) {
                if let Some(open) = self.active_open() {
                    if let Some(name) = open.editor.selected_entry_name() {
                        self.pending = Some(PendingAction::DeleteEntry(name));
                    }
                }
            }
        }

        // Escape dismisses the topmost transient surface, innermost first, so
        // one press never closes two things at once.
        if self.pending.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.palette.open {
                self.palette.open = false;
            } else if self.find_usages.is_some() {
                self.find_usages = None;
            } else if self.diff_preview.is_some() {
                self.diff_preview = None;
            } else if self.validation_report.is_some() {
                self.validation_report = None;
            } else if self.show_warnings {
                self.show_warnings = false;
            } else if self.ai_settings.open {
                self.ai_settings.open = false;
            }
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.help.open = !self.help.open;
        }
    }

    /// Background auto-save (Preferences): file-backed dirty editors save to
    /// their existing paths on the configured interval.
    fn autosave_tick(&mut self) {
        let secs = self.preferences.prefs.auto_save_secs;
        if secs == 0 || self.last_autosave.elapsed() < Duration::from_secs(u64::from(secs)) {
            return;
        }
        self.last_autosave = Instant::now();
        let mut saved = 0usize;
        let mut failed = 0usize;
        for open in &mut self.open_editors {
            if !open.editor.has_unsaved_changes() {
                continue;
            }
            // Don't autosave files with RON comments — the save would
            // silently strip them and the guard only shows on manual save.
            if open.has_comments {
                continue;
            }
            // Multi-entry editors persist each dirty entry to its own path;
            // fall back to the tab path for single-entry editors.
            let saved_ok = match open.editor.save_all() {
                Ok(true) => {
                    open.editor.mark_saved();
                    true
                }
                Ok(false) => {
                    let Some(path) = &open.path else {
                        continue; // Never Save-As from a timer.
                    };
                    match open.editor.save(path) {
                        Ok(()) => {
                            open.editor.mark_saved();
                            true
                        }
                        Err(_) => false,
                    }
                }
                Err(_) => false,
            };
            if saved_ok {
                saved += 1;
            } else {
                failed += 1;
            }
        }
        if saved > 0 || failed > 0 {
            self.browser.invalidate();
            self.invalidate_cross_refs();
            self.request_repaint();
            self.status_text = if failed == 0 {
                format!("Auto-saved {saved} editor(s)")
            } else {
                format!("Auto-saved {saved} editor(s), {failed} failed")
            };
        }
    }

    fn active_open(&self) -> Option<&OpenEditor> {
        self.active_tab.and_then(|i| self.open_editors.get(i))
    }

    fn active_open_mut(&mut self) -> Option<&mut OpenEditor> {
        self.active_tab.and_then(|i| self.open_editors.get_mut(i))
    }

    /// Ask for exactly one repaint on the next frame, without busy-looping.
    fn request_repaint(&mut self) {
        self.repaint_requested = true;
    }

    /// Render and resolve the pending confirmation dialog, if any.
    /// Execute queued agent tool requests against the live tabs.
    ///
    /// Runs on the UI thread, once per frame, unconditionally. Every request
    /// taken must be answered — dropping one leaves the agent waiting out its
    /// full timeout for no reason.
    fn drain_session_requests(&mut self) {
        for req in self.session_queue.drain() {
            let outcome = self.run_session_op(&req.op);
            req.reply(outcome);
        }
    }

    fn run_session_op(&mut self, op: &agent::bridge::SessionOp) -> agent::tools::ToolOutcome {
        use agent::bridge::SessionOp;
        use agent::tools::ToolOutcome;

        match op {
            SessionOp::ListTabs => {
                if self.open_editors.is_empty() {
                    return ToolOutcome::ok(
                        "No tabs are open. Use open_tab with a path from query_content.",
                    );
                }
                let mut out = String::new();
                for (i, o) in self.open_editors.iter().enumerate() {
                    out.push_str(&format!(
                        "{}{} [{}]{}{}\n",
                        if Some(i) == self.active_tab {
                            "* "
                        } else {
                            "  "
                        },
                        o.name,
                        o.editor.content_type().name(),
                        if o.editor.has_unsaved_changes() {
                            " (unsaved)"
                        } else {
                            ""
                        },
                        o.path
                            .as_ref()
                            .map(|p| format!(" — {}", p.display()))
                            .unwrap_or_default(),
                    ));
                }
                out.push_str("\n* marks the active tab; document tools act on it.");
                ToolOutcome::ok(out)
            }

            SessionOp::OpenTab { path } => {
                let full = crate::app::content_root().join(path);
                if !full.is_file() {
                    return ToolOutcome::error(format!(
                        "No such file under the content root: {path}"
                    ));
                }
                let Some(ct) = browser::detect_content_type(&full) else {
                    return ToolOutcome::error(format!(
                        "Nothing in the editor handles {path}. Its directory is not one \
                         of the known content directories."
                    ));
                };
                let name = full
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .unwrap_or("document")
                    .to_string();
                self.open_editor_for_file(&name, ct, &full);
                match self.active_open() {
                    Some(_) => ToolOutcome::ok(format!(
                        "Opened {path} in the {} editor. It is now the active tab.",
                        ct.name()
                    )),
                    // `open_editor_for_file` reports its own failure to the
                    // status bar; surface it to the model too rather than
                    // claiming success.
                    None => {
                        ToolOutcome::error(format!("Could not open {path}: {}", self.status_text))
                    }
                }
            }

            SessionOp::ReadDocument => match self.active_open() {
                None => ToolOutcome::error("No tab is active. Use open_tab first."),
                Some(open) => match open.editor.snapshot() {
                    Some(ron) => ToolOutcome::ok(ron),
                    None => ToolOutcome::error(format!(
                        "The {} tab is a live previewer and has no document to read.",
                        open.editor.content_type().name()
                    )),
                },
            },

            SessionOp::WriteDocument { ron } => {
                if self.active_tab.is_none() {
                    return ToolOutcome::error("No tab is active. Use open_tab first.");
                }
                // Force the undo point before the write, so `track_changes`
                // records this step even if the author was typing moments ago.
                if let Some(open) = self.active_open_mut() {
                    open.force_undo_point();
                    if let Err(e) = open.editor.restore_snapshot(ron) {
                        // RON is unforgiving and the message names the struct
                        // rather than the line's real problem, so hand the
                        // model the raw parse error to repair against.
                        return ToolOutcome::error(format!(
                            "The document was not written — it did not parse:\n{e}\n\n\
                             Send the complete document in the shape read_document returns. \
                             In RON a fixed-size array is a tuple, a newtype needs its parens, \
                             and enum variants are snake_case."
                        ));
                    }
                    open.editor.touch();
                }
                self.invalidate_cross_refs();
                let findings = self.validation_findings();
                ToolOutcome::ok(format!(
                    "Written to the active tab (not yet saved to disk; Ctrl+Z undoes it).\n\n{findings}"
                ))
            }

            SessionOp::Validate => {
                if self.active_tab.is_none() {
                    return ToolOutcome::error("No tab is active. Use open_tab first.");
                }
                ToolOutcome::ok(self.validation_findings())
            }

            SessionOp::SaveAll => {
                let dirty: Vec<usize> = self
                    .open_editors
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.editor.has_unsaved_changes())
                    .map(|(i, _)| i)
                    .collect();
                if dirty.is_empty() {
                    return ToolOutcome::ok("Nothing to save — no tab has unsaved changes.");
                }
                let mut saved = Vec::new();
                let mut failed = Vec::new();
                for idx in dirty {
                    let name = self.open_editors[idx].name.clone();
                    if self.save_editor(idx) {
                        saved.push(name);
                    } else {
                        failed.push(format!("{name}: {}", self.status_text));
                    }
                }
                let mut out = String::new();
                if !saved.is_empty() {
                    out.push_str(&format!("Saved: {}\n", saved.join(", ")));
                }
                if !failed.is_empty() {
                    out.push_str(&format!("Failed: {}\n", failed.join("; ")));
                    return ToolOutcome::error(out);
                }
                out.push_str("Run check_tree to confirm the whole tree still resolves.");
                ToolOutcome::ok(out)
            }
        }
    }

    /// Validation findings for the active tab, rendered for the model.
    fn validation_findings(&mut self) -> String {
        // Clone rather than borrow: `cross_ref_index` takes `&mut self`, and
        // the findings below need `&self` for the active tab at the same time.
        let index = self.cross_ref_index().clone();
        let Some(open) = self.active_open() else {
            return "No tab is active.".to_string();
        };
        let mut issues = open.editor.validate();
        for (_field, msg) in open.editor.validate_cross_refs(&index) {
            issues.push(msg);
        }
        if issues.is_empty() {
            "Validation: clean.".to_string()
        } else {
            format!(
                "Validation found {} issue(s):\n  {}",
                issues.len(),
                issues.join("\n  ")
            )
        }
    }

    /// Poll the running task and fold its events into the transcript.
    fn poll_agent(&mut self) {
        let Some(task) = &self.agent_task else {
            return;
        };
        // Collect first, then mutate: the receiver borrows `self.agent_task`
        // and the transcript and status both live on `self`.
        // `try_iter` rather than a blocking recv — this runs inside the frame.
        let events: Vec<_> = task.events.try_iter().collect();
        if events.is_empty() {
            return;
        }
        let mut finished = false;
        for event in events {
            if let agent::session::AgentEvent::Done(result) = &event {
                finished = true;
                self.status_text = match result {
                    Ok(()) => "Assistant finished.".into(),
                    Err(e) => format!("Assistant stopped: {e}"),
                };
            }
            self.agent_transcript.push(event);
        }
        self.request_repaint();
        if finished {
            self.agent_task = None;
        }
    }

    /// The transcript as plain text, for the clipboard.
    fn transcript_text(&self) -> String {
        use agent::session::AgentEvent as E;
        let mut out = String::new();
        for e in &self.agent_transcript {
            match e {
                E::UserPrompt(t) => out.push_str(&format!("\n> {t}\n")),
                E::Text(t) => out.push_str(&format!("\n{t}\n")),
                E::ToolStarted { summary, .. } => out.push_str(&format!("-> {summary}\n")),
                E::ToolFinished { name, is_error } => {
                    if *is_error {
                        out.push_str(&format!("   {name}: FAILED\n"));
                    }
                }
                E::Done(Ok(())) => out.push_str("\n[done]\n"),
                E::Done(Err(e)) => out.push_str(&format!("\n[stopped] {e}\n")),
            }
        }
        out
    }

    fn agent_running(&self) -> bool {
        self.agent_task.is_some()
    }

    /// Start a task from the prompt box.
    fn start_agent(&mut self) {
        let prompt = self.agent_prompt.trim().to_string();
        if prompt.is_empty() || self.agent_running() {
            return;
        }
        let profile = self.ai_settings.active_profile().clone();
        let provider = match profile.build() {
            Ok(p) => p,
            Err(e) => {
                self.status_text = format!("Assistant: {e}");
                return;
            }
        };
        // The transcript is NOT cleared: a Send continues the conversation, so
        // the panel should read as one exchange rather than resetting each
        // time. "New conversation" is the explicit way to start over.
        self.agent_transcript
            .push(agent::session::AgentEvent::UserPrompt(prompt.clone()));
        self.agent_prompt.clear();
        self.status_text = format!("Assistant working ({} mode)…", self.agent_mode.label());
        self.agent_task = Some(agent::session::spawn(
            provider,
            self.session_queue.handle(),
            self.agent_mode,
            profile.max_tokens,
            prompt,
            self.agent_conversation.clone(),
        ));
    }

    fn assistant_panel(&mut self, ctx: &egui::Context) {
        if !self.show_assistant {
            return;
        }
        egui::SidePanel::right("assistant")
            .default_width(380.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Assistant");
                    // The mode button carries the whole Plan/Build story, so
                    // it says what the mode means rather than just naming it.
                    let (label, hover) = match self.agent_mode {
                        agent::mode::Mode::Plan => (
                            "Plan (read-only)",
                            "Read-only: the assistant can investigate and propose, but no tool \
                             it can call writes to a tab or to disk. Ctrl+Shift+M switches to Build.",
                        ),
                        agent::mode::Mode::Build => (
                            "Build (can edit)",
                            "The assistant can open tabs, write documents and save. Every write \
                             is undoable with Ctrl+Z. Ctrl+Shift+M switches to Plan.",
                        ),
                    };
                    if ui.button(label).on_hover_text(hover).clicked() {
                        self.agent_mode = self.agent_mode.toggled();
                    }
                    ui.weak(self.ai_settings.active_profile().name.clone());
                });
                ui.separator();

                let transcript_height = ui.available_height() - 96.0;
                egui::ScrollArea::vertical()
                    .max_height(transcript_height.max(80.0))
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.agent_transcript.is_empty() {
                            ui.weak(
                                "Ask for something. In Plan mode it will investigate and \
                                 propose; in Build mode it can make the change.",
                            );
                        }
                        for event in &self.agent_transcript {
                            match event {
                                agent::session::AgentEvent::UserPrompt(t) => {
                                    ui.add_space(6.0);
                                    ui.strong(format!("> {t}"));
                                    ui.add_space(2.0);
                                }
                                agent::session::AgentEvent::Text(t) => {
                                    ui.label(t);
                                    ui.add_space(4.0);
                                }
                                agent::session::AgentEvent::ToolStarted { summary, .. } => {
                                    ui.weak(format!("→ {summary}"));
                                }
                                agent::session::AgentEvent::ToolFinished { name, is_error } => {
                                    if *is_error {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(0xF4, 0x43, 0x36),
                                            format!("   {name} failed"),
                                        );
                                    }
                                }
                                agent::session::AgentEvent::Done(Ok(())) => {
                                    ui.add_space(4.0);
                                    ui.weak("done");
                                }
                                agent::session::AgentEvent::Done(Err(e)) => {
                                    ui.add_space(4.0);
                                    ui.colored_label(egui::Color32::from_rgb(0xF4, 0x43, 0x36), e);
                                }
                            }
                        }
                    });

                ui.horizontal(|ui| {
                    // Selecting a transcript out of an egui panel with the
                    // mouse is miserable, so neither reading it nor copying it
                    // depends on doing that.
                    if ui
                        .add_enabled(
                            !self.agent_transcript.is_empty(),
                            egui::Button::new("Copy transcript"),
                        )
                        .clicked()
                    {
                        let text = self.transcript_text();
                        ui.ctx().copy_text(text);
                        self.status_text = "Transcript copied.".into();
                    }
                    let (turns, size, log_path) = {
                        let c = self
                            .agent_conversation
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        (c.turns(), c.size_hint(), c.log_path.clone())
                    };
                    if ui
                        .add_enabled(turns > 0, egui::Button::new("New conversation"))
                        .on_hover_text(
                            "Forget the exchange so far and start fresh. Sends otherwise \
                             continue the same conversation, so the assistant remembers \
                             what it already looked up.",
                        )
                        .clicked()
                    {
                        self.agent_conversation
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .reset();
                        self.agent_transcript.clear();
                        self.status_text = "Started a new conversation.".into();
                    }
                    if let Some(path) = &log_path {
                        ui.weak(format!("log: {}", path.display()))
                            .on_hover_text(
                                "Every session is written here as it runs, so a run that \
                                 hangs or fails can still be read with ordinary tools.",
                            );
                    }
                    if turns > 0 {
                        // Characters, not tokens — enough to warn before a
                        // context limit bites without pretending to be exact.
                        let label = format!("{turns} msgs / ~{}k chars", size / 1000);
                        if size > 120_000 {
                            ui.colored_label(egui::Color32::from_rgb(0xFF, 0xB3, 0x00), label)
                                .on_hover_text(
                                    "This conversation is getting long and every Send \
                                     resends all of it. Start a new one when you change \
                                     task.",
                                );
                        } else {
                            ui.weak(label);
                        }
                    }
                });

                ui.separator();
                let running = self.agent_running();
                ui.add_enabled_ui(!running, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.agent_prompt)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .hint_text("what should it do?"),
                    );
                });
                ui.collapsing("External access (MCP)", |ui| {
                    let mut on = self.mcp_http.is_some();
                    if ui
                        .checkbox(&mut on, "Serve this session over HTTP")
                        .on_hover_text(
                            "Lets an external MCP client (Claude Code, Claude Desktop) drive \
                             these same tools against your open tabs. Loopback only, and a \
                             bearer token is required.",
                        )
                        .changed()
                    {
                        if on {
                            match mcp_http::McpHttpServer::start(0, self.session_queue.handle()) {
                                Ok(server) => {
                                    self.status_text =
                                        format!("MCP endpoint listening on {}", server.addr);
                                    self.mcp_http = Some(server);
                                }
                                Err(e) => self.status_text = format!("MCP endpoint: {e}"),
                            }
                        } else {
                            // Drop stops the listener.
                            self.mcp_http = None;
                            self.status_text = "MCP endpoint stopped.".into();
                        }
                    }
                    if let Some(server) = &self.mcp_http {
                        let hint = server.client_hint();
                        ui.horizontal_wrapped(|ui| {
                            ui.monospace(&hint);
                        });
                        if ui.button("Copy").clicked() {
                            ui.ctx().copy_text(hint);
                        }
                        ui.weak(
                            "The token changes every time you switch this on. Anything with \
                             the token can edit and save your content.",
                        );
                    }
                });

                ui.horizontal(|ui| {
                    if running {
                        ui.spinner();
                        ui.label("working…");
                    } else if ui
                        .add_enabled(
                            !self.agent_prompt.trim().is_empty(),
                            egui::Button::new("Send"),
                        )
                        .clicked()
                    {
                        self.start_agent();
                    }
                });
            });
    }

    fn handle_pending(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        match pending {
            PendingAction::CloseTab(idx) => {
                let name = self
                    .open_editors
                    .get(idx)
                    .map(|o| o.name.clone())
                    .unwrap_or_default();
                match confirmation_dialog(
                    ctx,
                    "Unsaved changes",
                    &format!("Save changes to \"{name}\" before closing?"),
                    "Save",
                    "Cancel",
                    Some("Discard"),
                ) {
                    Some(ConfirmationResult::Ok) => {
                        if self.save_editor(idx) {
                            self.close_tab(idx);
                        }
                    }
                    Some(ConfirmationResult::Extra) => self.close_tab(idx),
                    Some(ConfirmationResult::Cancel) => {}
                    None => self.pending = Some(PendingAction::CloseTab(idx)),
                }
            }
            PendingAction::CloseAll => {
                let names = self.dirty_tab_names();
                match confirmation_dialog(
                    ctx,
                    "Close all tabs",
                    &format!(
                        "You have unsaved changes in {} editor(s): {}. Save before closing?",
                        names.len(),
                        names.join(", ")
                    ),
                    "Save All & Close",
                    "Cancel",
                    Some("Discard All"),
                ) {
                    Some(ConfirmationResult::Ok) => {
                        if self.save_all_dirty() {
                            self.open_editors.clear();
                            self.active_tab = None;
                        }
                    }
                    Some(ConfirmationResult::Extra) => {
                        self.open_editors.clear();
                        self.active_tab = None;
                    }
                    Some(ConfirmationResult::Cancel) => {}
                    None => self.pending = Some(PendingAction::CloseAll),
                }
            }
            PendingAction::Quit => {
                let names = self.dirty_tab_names();
                match confirmation_dialog(
                    ctx,
                    "Quit",
                    &format!(
                        "You have unsaved changes in {} editor(s): {}. Save all before quitting?",
                        names.len(),
                        names.join(", ")
                    ),
                    "Save All & Quit",
                    "Cancel",
                    Some("Quit Without Saving"),
                ) {
                    Some(ConfirmationResult::Ok) => {
                        if self.save_all_dirty() {
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    Some(ConfirmationResult::Extra) => {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Some(ConfirmationResult::Cancel) => {}
                    None => self.pending = Some(PendingAction::Quit),
                }
            }
            PendingAction::DeleteEntry(name) => {
                match confirmation_dialog(
                    ctx,
                    "Delete entry",
                    &format!("Delete \"{name}\" from this editor? (Undo with Ctrl+Z.)"),
                    "Delete",
                    "Cancel",
                    None,
                ) {
                    Some(ConfirmationResult::Ok) => {
                        if let Some(open) = self.active_open_mut() {
                            if open.editor.delete_selected() {
                                self.status_text = format!("Deleted {name}");
                            }
                        }
                    }
                    Some(_) => {}
                    None => self.pending = Some(PendingAction::DeleteEntry(name)),
                }
            }
            PendingAction::CommentLossSave(idx) => {
                match confirmation_dialog(
                    ctx,
                    "Comment Loss Warning",
                    "This file has authored comments. RON cannot preserve them \
                     through a save — they will be removed. Save anyway?",
                    "Save Anyway",
                    "Cancel",
                    None,
                ) {
                    Some(ConfirmationResult::Ok) => {
                        if let Some(open) = self.open_editors.get_mut(idx) {
                            open.has_comments = false; // Clear for this save.
                        }
                        self.save_editor(idx);
                    }
                    Some(_) => {}
                    None => self.pending = Some(PendingAction::CommentLossSave(idx)),
                }
            }
        }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.menu_button("New", |ui| {
                        let mut pick: Option<ContentType> = None;
                        // Driven by app::NEW_MENU_GROUPS so a new ContentType
                        // cannot ship without a way to create one.
                        for (group, types) in app::NEW_MENU_GROUPS {
                            ui.menu_button(*group, |ui| {
                                for ct in *types {
                                    if ui.button(ct.name()).clicked() {
                                        pick = Some(*ct);
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                        if let Some(ct) = pick {
                            self.open_new_editor(&format!("New {}", ct.name()), ct);
                            ui.close_menu();
                        }
                    });
                    let templates = self.templates.list_templates();
                    let templates_dir = self.templates.templates_dir().display().to_string();
                    ui.menu_button("New from Template", |ui| {
                        if templates.is_empty() {
                            // Say where they go rather than showing an empty
                            // menu — an author who wants templates can't guess
                            // the directory.
                            ui.weak("No templates found.");
                            ui.weak(format!("Drop .ron files in {templates_dir}"));
                            return;
                        }
                        let mut pick: Option<template_manager::TemplateEntry> = None;
                        for entry in &templates {
                            if ui
                                .button(&entry.label)
                                .on_hover_text(entry.path.display().to_string())
                                .clicked()
                            {
                                pick = Some(entry.clone());
                                ui.close_menu();
                            }
                        }
                        if let Some(entry) = pick {
                            self.open_from_template(&entry);
                        }
                    });
                    if ui.button("Open…            Ctrl+O").clicked() {
                        self.open_file_dialog();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Save              Ctrl+S").clicked() {
                        if let Some(idx) = self.active_tab {
                            self.save_editor(idx);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save As…    Ctrl+Shift+S").clicked() {
                        if let Some(idx) = self.active_tab {
                            self.save_editor_as(idx);
                        }
                        ui.close_menu();
                    }
                    let has_saved_tab = self.active_open().is_some_and(|o| o.path.is_some());
                    if ui
                        .add_enabled(has_saved_tab, egui::Button::new("Preview Changes…"))
                        .on_hover_text("Show what Save would write, against the file on disk")
                        .clicked()
                    {
                        self.preview_changes();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Validate All Open Editors").clicked() {
                        self.run_validate_all();
                        ui.close_menu();
                    }
                    if ui
                        .button("Broken Reference Report")
                        .on_hover_text(
                            "Scan the whole content tree for references to ids nothing defines",
                        )
                        .clicked()
                    {
                        self.run_broken_reference_report();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Close Tab        Ctrl+W").clicked() {
                        if let Some(idx) = self.active_tab {
                            self.request_close_tab(idx);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Close All Tabs").clicked() {
                        if self.dirty_tab_indices().is_empty() {
                            self.open_editors.clear();
                            self.active_tab = None;
                        } else {
                            self.pending = Some(PendingAction::CloseAll);
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit              Ctrl+Q").clicked() {
                        self.request_quit(ctx);
                        ui.close_menu();
                    }
                });

                ui.menu_button("Edit", |ui| {
                    let can_undo = self.active_open().is_some_and(|o| !o.undo_stack.is_empty());
                    let can_redo = self.active_open().is_some_and(|o| !o.redo_stack.is_empty());
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo    Ctrl+Z"))
                        .clicked()
                    {
                        if let Some(open) = self.active_open_mut() {
                            self.status_text = open.undo();
                        }
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(can_redo, egui::Button::new("Redo    Ctrl+Y"))
                        .clicked()
                    {
                        if let Some(open) = self.active_open_mut() {
                            self.status_text = open.redo();
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Find Usages…    Ctrl+Shift+F").clicked() {
                        self.open_find_usages();
                        ui.close_menu();
                    }
                    if ui
                        .add_enabled(
                            self.active_open().is_some(),
                            egui::Button::new("Duplicate Document"),
                        )
                        .clicked()
                    {
                        self.duplicate_active_tab();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Preferences…").clicked() {
                        self.preferences.open = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    let mut browser_visible = self.show_browser;
                    if ui
                        .checkbox(&mut browser_visible, "Content Browser")
                        .changed()
                    {
                        self.show_browser = browser_visible;
                    }
                    let mut assistant_visible = self.show_assistant;
                    if ui
                        .checkbox(&mut assistant_visible, "Assistant")
                        .on_hover_text("Ctrl+Shift+M toggles Plan / Build mode.")
                        .changed()
                    {
                        self.show_assistant = assistant_visible;
                    }
                    if ui.button("Command Palette    Ctrl+Shift+P").clicked() {
                        self.palette.open = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button("AI", |ui| {
                    if ui.button("AI Settings…").clicked() {
                        self.ai_settings.open = true;
                        ui.close_menu();
                    }
                    ui.label("Generate from the bar below the seed panel.");
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("Help            F1").clicked() {
                        self.help.open = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Loaded preferences (theme, zoom, content root) apply once at start.
        if !self.prefs_applied {
            self.prefs_applied = true;
            self.preferences.prefs.apply_visuals(ctx);
            self.browser.root = std::path::PathBuf::from(&self.preferences.prefs.content_root);
            crate::app::set_content_root(Some(std::path::PathBuf::from(
                &self.preferences.prefs.content_root,
            )));
        }

        // Startup content-tree scan: catches malformed files in directories
        // no tab has open. Only runs once.
        if !self.startup_scan_done {
            self.startup_scan_done = true;
            let root = crate::app::content_root();
            let tree = ContentTree::scan(&root);
            let report = tree.check();
            if !report.unparseable.is_empty() {
                self.startup_warnings = report
                    .unparseable
                    .iter()
                    .map(|u| format!("{}: {}", u.file.display(), u.reason))
                    .collect();
                self.show_warnings = true;
                self.status_text = format!(
                    "{} file(s) failed to parse — see Warnings",
                    report.unparseable.len()
                );
            }
        }

        self.autosave_tick();

        // Status housekeeping: newly-set messages get a 5s lifetime unless
        // they look like errors; expired messages fall back to "Ready".
        if self.status_text != self.last_status {
            self.last_status = self.status_text.clone();
            let lower = self.status_text.to_lowercase();
            let sticky =
                lower.contains("error") || lower.contains("failed") || lower.contains("issue");
            self.status_expiry = (!sticky).then(|| Instant::now() + Duration::from_secs(5));
        } else if self.status_expiry.is_some_and(|t| Instant::now() >= t) {
            self.status_text = "Ready".into();
            self.last_status = "Ready".into();
            self.status_expiry = None;
            self.request_repaint();
        }

        // Window title mirrors the unsaved count.
        let modified_count = self
            .open_editors
            .iter()
            .filter(|o| o.editor.has_unsaved_changes())
            .count();
        let title = if self.open_editors.is_empty() {
            "ReachLock Content Editor".to_string()
        } else if modified_count == 0 {
            "ReachLock Content Editor — all saved".to_string()
        } else {
            format!("ReachLock Content Editor — {modified_count} unsaved")
        };
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }

        // Intercept the window close button when there are unsaved changes.
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && !self.dirty_tab_indices().is_empty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(PendingAction::Quit);
        }

        // Poll the background AI generation thread.
        if let Some(rx) = &self.ai_result_rx {
            if let Ok(outcome) = rx.try_recv() {
                self.ai_running = false;
                self.ai_result_rx = None;
                self.request_repaint();
                match outcome {
                    ai::AiGenOutcome::Ok { ct, result } => {
                        let mut applied = false;
                        if let Some(idx) = self.active_tab {
                            if let Some(open) = self.open_editors.get_mut(idx) {
                                if open.editor.content_type() == ct {
                                    match open.editor.apply_ai_json(&result.json_value) {
                                        Ok(_) => {
                                            applied = true;
                                            if !result.warnings.is_empty() {
                                                *self.ai_status.lock().unwrap() = format!(
                                                    "Applied with {} schema warning(s).",
                                                    result.warnings.len()
                                                );
                                            } else {
                                                *self.ai_status.lock().unwrap() =
                                                    "AI content applied.".into();
                                            }
                                        }
                                        Err(e) => {
                                            *self.ai_status.lock().unwrap() =
                                                format!("Applied parse failed: {e}");
                                        }
                                    }
                                }
                            }
                        }
                        if !applied {
                            *self.ai_status.lock().unwrap() =
                                "Generation returned, but the active editor changed.".into();
                        }
                    }
                    ai::AiGenOutcome::Err(e) => {
                        *self.ai_status.lock().unwrap() = format!("AI error: {e}");
                    }
                }
            }
        }

        self.handle_shortcuts(ctx);
        self.menu_bar(ctx);

        if self.show_browser {
            let actions = self.browser.ui(ctx);
            self.handle_browser_actions(actions);
        }

        egui::TopBottomPanel::bottom("status_line").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_text);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let modified_count = self
                        .open_editors
                        .iter()
                        .filter(|o| o.editor.has_unsaved_changes())
                        .count();
                    if modified_count > 0 {
                        let color = if modified_count > 5 {
                            egui::Color32::from_rgb(0xF4, 0x43, 0x36)
                        } else {
                            egui::Color32::from_rgb(0xFF, 0xB3, 0x00)
                        };
                        ui.colored_label(color, format!("{modified_count} unsaved"));
                    }
                    ui.label(format!("{} editor(s) open", self.open_editors.len()));
                });
            });
        });

        let mut seed_action = None;
        egui::TopBottomPanel::top("seed_panel").show(ctx, |ui| {
            seed_action = self.seed_workflow.ui(ui);
        });
        match seed_action {
            Some(SeedAction::RerollAll(seed)) => {
                let total = self.open_editors.len();
                let mut rerolled = 0;
                for open in &mut self.open_editors {
                    if open.editor.accept_seed_reroll() {
                        open.editor.apply_seed(seed);
                        rerolled += 1;
                    }
                }
                self.status_text = if total == 0 {
                    "No editors open to reroll".into()
                } else {
                    format!("Rerolled {rerolled}/{total} editor(s) with seed {seed}")
                };
            }
            Some(SeedAction::LockCurrent) => {
                use std::hash::{DefaultHasher, Hash, Hasher};
                match self.active_open().map(|open| open.name.clone()) {
                    Some(name) => {
                        let mut hasher = DefaultHasher::new();
                        name.hash(&mut hasher);
                        let seed = hasher.finish() & seed_workflow::SEED_MASK;
                        self.seed_workflow.set_seed(seed);
                        self.status_text = format!("Locked seed {seed} from \"{name}\"");
                    }
                    None => {
                        self.status_text = "No active tab to lock a seed from".into();
                    }
                }
            }
            None => {}
        }

        // AI generation bar (handoff §Phase 2.5).
        egui::TopBottomPanel::top("ai_bar")
            .resizable(false)
            .show_separator_line(true)
            .show(ctx, |ui| {
                let active_ct = self
                    .active_tab
                    .and_then(|idx| self.open_editors.get(idx).map(|o| o.editor.content_type()));
                ui.horizontal(|ui| {
                    ui.label("AI:");
                    ui.text_edit_multiline(&mut self.ai_prompt);

                    let has_schema = match active_ct {
                        Some(ct) => self.schemas.has(&ct),
                        None => false,
                    };

                    let can_generate = active_ct.is_some()
                        && has_schema
                        && !self.ai_prompt.trim().is_empty()
                        && !self.ai_running;

                    let btn = if self.ai_running {
                        "Generating…"
                    } else {
                        "Generate"
                    };
                    if self.ai_running {
                        ui.spinner();
                    }
                    if ui
                        .add_enabled(can_generate, egui::Button::new(btn))
                        .clicked()
                    {
                        let ct = active_ct.expect("guarded by can_generate");
                        self.ai_running = true;
                        *self.ai_status.lock().unwrap() = format!("Generating {ct:?} content…");
                        let profile = self.ai_settings.active_profile().clone();
                        let prompt = self.ai_prompt.trim().to_string();
                        let (tx, rx) = channel();
                        self.ai_result_rx = Some(rx);
                        // The provider owns its own runtime, so this thread no
                        // longer stands up a multi-thread tokio pool per
                        // generation just to make one request.
                        std::thread::spawn(move || {
                            let schemas = SchemaCache::load_all();
                            let outcome = match profile.build() {
                                Ok(p) => match ai::generate_content(
                                    p.as_ref(),
                                    profile.max_tokens,
                                    ct,
                                    &schemas,
                                    &prompt,
                                ) {
                                    Ok(result) => ai::AiGenOutcome::Ok { ct, result },
                                    Err(e) => ai::AiGenOutcome::Err(e),
                                },
                                Err(e) => ai::AiGenOutcome::Err(ai::GenerationError::HttpError(e)),
                            };
                            let _ = tx.send(outcome);
                        });
                    }

                    if ui.button("Clear").clicked() {
                        self.ai_prompt.clear();
                    }
                });
                ui.horizontal(|ui| {
                    let status = self.ai_status.lock().unwrap().clone();
                    ui.label(&status);
                    if let Some(ct) = active_ct {
                        if !self.schemas.has(&ct) {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "No schema for this type — AI generation unavailable.",
                            );
                        } else if matches!(ct, ContentType::ItemBrowser | ContentType::SpriteViewer)
                        {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Previewers have no AI target.",
                            );
                        }
                    }
                });
            });

        self.assistant_panel(ctx);

        let mut open_recent = None;
        egui::SidePanel::right("preview_panel")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                let active = self
                    .active_tab
                    .and_then(|i| self.open_editors.get(i))
                    .map(|o| (o.name.as_str(), o.editor.as_ref()));
                open_recent = self
                    .preview
                    .show(ui, active, &self.preferences.prefs.recent_files);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.open_editors.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("ReachLock Content Editor");
                    ui.add_space(8.0);
                    ui.label("Open a file from the Content Browser or use File → New.");
                    ui.add_space(4.0);
                    ui.label("Press F1 for help.");
                });
            } else {
                egui::TopBottomPanel::top("editor_tabs")
                    .resizable(false)
                    .show_separator_line(false)
                    .show_inside(ui, |ui| {
                        let mut close_request: Option<usize> = None;
                        ui.horizontal(|ui| {
                            for (i, open) in self.open_editors.iter().enumerate() {
                                let title = format!(
                                    "{}{}",
                                    open.name,
                                    if open.editor.has_unsaved_changes() {
                                        " *"
                                    } else {
                                        ""
                                    }
                                );
                                let selected = self.active_tab == Some(i);
                                let tooltip = format!(
                                    "{}\n{}",
                                    open.path
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_else(|| "(unsaved file)".into()),
                                    open.editor.content_type().name()
                                );
                                if ui
                                    .selectable_label(selected, &title)
                                    .on_hover_text(tooltip)
                                    .clicked()
                                {
                                    self.active_tab = Some(i);
                                }
                                if ui.button("x").clicked() {
                                    close_request = Some(i);
                                }
                            }
                        });
                        if let Some(i) = close_request {
                            self.request_close_tab(i);
                        }
                    });

                if let Some(idx) = self.active_tab {
                    if let Some(open) = self.open_editors.get_mut(idx) {
                        open.editor.ui(ui);
                    }
                }
            }
        });

        if let Some(path) = open_recent {
            match browser::detect_content_type(&path) {
                Some(ct) => {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("file")
                        .to_string();
                    self.open_editor_for_file(&name, ct, &path);
                }
                None => {
                    self.status_text = format!("Can't open {} — unknown type", path.display());
                }
            }
        }

        self.ai_settings.show(ctx);
        self.help.show(ctx);
        if let Some(report) = &self.validation_report {
            let mut open = true;
            egui::Window::new("Validation Report")
                .open(&mut open)
                .resizable(true)
                .default_size([440.0, 320.0])
                .show(ctx, |ui| {
                    if report.is_empty() {
                        ui.label("No editors open.");
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (name, issues) in report {
                            if issues.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(0x4C, 0xAF, 0x50),
                                        "✔",
                                    );
                                    ui.strong(name);
                                    ui.weak("clean");
                                });
                            } else {
                                ui.horizontal(|ui| {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(0xF4, 0x43, 0x36),
                                        "✘",
                                    );
                                    ui.strong(name);
                                    ui.label(format!("{} issue(s)", issues.len()));
                                });
                                for issue in issues {
                                    ui.indent((name, issue), |ui| {
                                        ui.label(issue);
                                    });
                                }
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
            if !open {
                self.validation_report = None;
            }
        }

        // Content Warnings: files that failed to parse during constructor scan
        // or the startup content-tree scan.
        let all_warnings: Vec<&str> = self
            .startup_warnings
            .iter()
            .chain(self.load_warnings.iter())
            .map(|s| s.as_str())
            .collect();
        if self.show_warnings && !all_warnings.is_empty() {
            let mut open = true;
            egui::Window::new("Content Warnings")
                .open(&mut open)
                .resizable(true)
                .default_size([440.0, 280.0])
                .show(ctx, |ui| {
                    ui.label(format!(
                        "{} file(s) could not be parsed and were skipped:",
                        all_warnings.len()
                    ));
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for w in all_warnings {
                            ui.label(w);
                        }
                    });
                });
            if !open {
                self.show_warnings = false;
            }
        }

        // Command palette. It writes an action rather than acting, so the
        // window closes before the action runs and a command that opens
        // another window isn't fighting the palette for focus.
        let mut palette_action = None;
        self.palette.show(ctx, &mut palette_action);
        if let Some(action) = palette_action {
            self.run_palette_action(action, ctx);
        }

        if let Some(state) = self.find_usages.as_mut() {
            let mut open = true;
            let mut search = false;
            let mut jump: Option<String> = None;
            egui::Window::new("Find Usages")
                .open(&mut open)
                .resizable(true)
                .default_size([420.0, 300.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Id:");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut state.query)
                                .hint_text("a content id from the tree"),
                        );
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            search = true;
                        }
                        if ui.button("Search").clicked() {
                            search = true;
                        }
                    });
                    ui.separator();
                    if !state.searched {
                        ui.weak("Enter an id and press Search.");
                    } else if state.results.is_empty() {
                        ui.label(format!("Nothing references `{}`.", state.query.trim()));
                        ui.weak(
                            "Unreferenced content is not necessarily wrong — but it is content \
                             no player can reach unless something points at it.",
                        );
                    } else {
                        ui.label(format!(
                            "{} reference(s) to `{}`:",
                            state.results.len(),
                            state.query.trim()
                        ));
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (source, field) in &state.results {
                                ui.horizontal(|ui| {
                                    if ui.link(source).clicked() {
                                        jump = Some(source.clone());
                                    }
                                    ui.weak(format!("via {field}"));
                                });
                            }
                        });
                    }
                });
            if search {
                self.run_find_usages();
            }
            // Clicking a result searches from there, so the author can walk the
            // reference graph without retyping ids.
            if let Some(source) = jump {
                if let Some(state) = self.find_usages.as_mut() {
                    state.query = source;
                }
                self.run_find_usages();
            }
            if !open {
                self.find_usages = None;
            }
        }

        if let Some((name, result)) = &self.diff_preview {
            let mut open = true;
            egui::Window::new(format!("Preview Changes — {name}"))
                .open(&mut open)
                .resizable(true)
                .default_size([620.0, 440.0])
                .show(ctx, |ui| {
                    diff::render_diff_ui(ui, result);
                });
            if !open {
                self.diff_preview = None;
            }
        }

        if self.preferences.show(ctx) {
            // A preference changed — pick up a possible content-root move.
            let root = std::path::PathBuf::from(&self.preferences.prefs.content_root);
            crate::app::set_content_root(Some(root.clone()));
            if self.browser.root != root {
                self.browser.root = root;
                self.browser.invalidate();
                self.invalidate_cross_refs();
                // Both of these are snapshots of a tree that just moved.
                self.invalidate_cross_refs();
                self.templates.reload();
            }
        }
        // Agent tool requests. Drained unconditionally — including while a
        // modal is up. If this were skipped whenever the editor was busy, an
        // agent blocked on a reply would hang until the author happened to
        // dismiss the dialog, and the editor would look frozen from both
        // sides. Runs before `track_changes` so an agent write gets its undo
        // step in the same frame it lands.
        self.drain_session_requests();
        self.poll_agent();

        self.handle_pending(ctx);

        // Undo bookkeeping: one diff point per frame, after every mutation
        // path (editor UI, AI apply, dialogs) has run.
        if let Some(open) = self.active_open_mut() {
            open.track_changes();
        }

        // Only repaint when there is input this frame or a state change
        // requested one (timer/async). Unconditional repaint busy-loops at
        // 100% CPU; egui already repaints on interactive input.
        if self.repaint_requested || ctx.input(|i| !i.events.is_empty()) {
            ctx.request_repaint();
            self.repaint_requested = false;
        }
    }
}

/// Whether the Plan/Build shortcut fired this frame.
///
/// **Ctrl+Shift+M, not Tab.** Tab was the original binding and was wrong twice
/// over: it is egui's focus-navigation key, and — the reason it changed — it
/// collides with assistive technology, where Tab is the primary means of
/// moving through a UI. A shortcut that a screen-reader or switch-access user
/// cannot avoid triggering is not a shortcut, it is a trap. The mode is also
/// reachable without any key at all, from the button in the panel header.
///
/// The `wants_keyboard_input` guard stays even though Ctrl+Shift+M is not a
/// character a field would swallow: it is the same guard undo and redo use,
/// and a shortcut that fires while someone is typing into a document is a
/// surprise regardless of which keys it needs.
///
/// Consuming is deliberately inside the guard — `consume_key` removes the
/// event, so checking the guard afterwards would still swallow the keystroke.
///
/// A free function rather than a method so a test can drive it with a real
/// `egui::Context`.
fn assistant_mode_shortcut(ctx: &egui::Context) -> bool {
    if ctx.wants_keyboard_input() {
        return false;
    }
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::M))
}

fn main() -> eframe::Result<()> {
    // Headless MCP server. Checked before anything touches winit or eframe:
    // an MCP client spawns this as a subprocess and speaks JSON-RPC over the
    // pipe, so there is no display to open, and a stray byte written to
    // stdout would corrupt the protocol stream.
    if std::env::args().any(|a| a == "--mcp-stdio") {
        std::process::exit(mcp::serve_stdio());
    }

    // FIXME(winit-0.30.13): same Wayland workaround as the client.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WINIT_UNIX_BACKEND").is_none() {
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
        std::env::remove_var("WAYLAND_DISPLAY");
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ReachLock Content Editor")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ReachLock Content Editor",
        options,
        Box::new(|_cc| Ok(Box::new(EditorApp::default()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive a real `egui::Context` for one frame with a Tab keypress, with
    /// and without a focused text field, and check what the guard decides.
    ///
    /// This is the regression that would hurt most: Tab is how you move
    /// between fields, and the editor is almost nothing but fields.
    mod mode_shortcut {
        use super::*;

        /// The real chord, so the test breaks if the binding moves.
        fn shortcut_event() -> egui::Event {
            egui::Event::Key {
                key: egui::Key::M,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            }
        }

        /// A bare Tab, which must no longer do anything: it is how assistive
        /// technology moves through the UI, and how egui moves focus.
        fn tab_event() -> egui::Event {
            egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }
        }

        /// Run one frame; `body` draws the UI. Returns the guard's verdict.
        ///
        /// The key is a parameter, and separate from the frames that set up
        /// focus, because egui consumes Tab for its own focus navigation:
        /// sending a key on the frame that establishes focus would move focus
        /// straight back off the field, and the test would be measuring its
        /// own setup rather than the guard.
        fn frame(
            ctx: &egui::Context,
            key: Option<egui::Event>,
            mut body: impl FnMut(&mut egui::Ui),
        ) -> bool {
            let input = egui::RawInput {
                events: key.into_iter().collect(),
                ..Default::default()
            };
            let mut verdict = false;
            let _ = ctx.run(input, |ctx| {
                // Guard first, then draw — the real frame calls
                // `handle_shortcuts` before any panel. That ordering is what
                // makes the guard work: `wants_keyboard_input` reports the
                // focus carried in from the previous frame, before egui has
                // had a chance to consume Tab for its own focus navigation.
                // Checking after drawing measures the wrong thing entirely —
                // egui will have moved focus off a lone text field by then,
                // and the guard looks broken when it is not.
                verdict = assistant_mode_shortcut(ctx);
                egui::CentralPanel::default().show(ctx, |ui| body(ui));
            });
            verdict
        }

        #[test]
        fn the_chord_toggles_the_mode_when_no_field_has_focus() {
            let ctx = egui::Context::default();
            // Warm-up frame: egui needs one pass to settle focus state.
            frame(&ctx, None, |ui| {
                ui.label("nothing focusable");
            });
            assert!(
                frame(&ctx, Some(shortcut_event()), |ui| {
                    ui.label("nothing focusable");
                }),
                "Ctrl+Shift+M should toggle the assistant mode"
            );
        }

        /// Tab is how assistive technology walks a UI. It must be inert here.
        #[test]
        fn a_bare_tab_does_nothing() {
            let ctx = egui::Context::default();
            frame(&ctx, None, |ui| {
                ui.label("nothing focusable");
            });
            assert!(
                !frame(&ctx, Some(tab_event()), |ui| {
                    ui.label("nothing focusable");
                }),
                "Tab must not toggle the mode — it collides with assistive tech"
            );
        }

        #[test]
        fn the_chord_is_left_alone_while_a_text_field_has_focus() {
            let ctx = egui::Context::default();
            let mut text = String::from("typing here");

            // Focus a TextEdit and keep it focused across frames.
            let id = egui::Id::new("guarded_field");
            // Two Tab-free frames: one to draw the field, one for the
            // requested focus to take effect.
            frame(&ctx, None, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut text).id(id));
                r.request_focus();
            });
            let focused = frame(&ctx, None, |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id));
            });
            assert!(!focused, "no key was sent, so nothing should have fired");
            assert!(
                ctx.wants_keyboard_input(),
                "test setup failed: the field never took focus, so this would \
                 not be testing the guard at all"
            );

            let verdict = frame(&ctx, Some(shortcut_event()), |ui| {
                ui.add(egui::TextEdit::singleline(&mut text).id(id));
            });
            assert!(
                !verdict,
                "the shortcut must not fire while a text field has focus"
            );
        }
    }

    #[test]
    fn suggest_stem_normalizes_names() {
        assert_eq!(suggest_stem("New Soul"), "new_soul");
        assert_eq!(suggest_stem("My-Cool Station!"), "my_cool_station");
        assert_eq!(suggest_stem("   "), "untitled");
        assert_eq!(suggest_stem(""), "untitled");
    }
}
