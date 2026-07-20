mod ai;
mod app;
mod browser;
mod dialogs;
pub mod editors;
mod help_window;
mod io;
mod preview;
mod schema;
mod seed_workflow;
mod settings_window;

use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use app::{build_default_registry, ContentType, Editor, EditorRegistry};
use browser::{BrowserAction, ContentBrowser};
use dialogs::{confirmation_dialog, ConfirmationResult};
use help_window::HelpWindow;
use preview::PreviewPanel;
use schema::SchemaCache;
use seed_workflow::SeedWorkflow;
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
    active_tab: Option<usize>,
    show_browser: bool,
    help: HelpWindow,
    pending: Option<PendingAction>,
    /// Set once a quit is confirmed so the close request passes through.
    allow_close: bool,
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
            active_tab: None,
            show_browser: true,
            help: HelpWindow::new(),
            pending: None,
            allow_close: false,
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
        let Some(mut editor) = self.registry.create(ct) else {
            self.status_text = format!("No editor for {:?}", ct);
            return;
        };
        match editor.load(path) {
            Ok(()) => {
                let idx = self.open_editors.len();
                self.open_editors.push(OpenEditor::new(
                    editor,
                    name.to_string(),
                    Some(path.to_path_buf()),
                ));
                self.active_tab = Some(idx);
                self.status_text = format!("Opened {}", path.display());
            }
            Err(e) => {
                self.status_text = format!("Open failed: {e}");
            }
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
        let Some(open) = self.open_editors.get_mut(idx) else {
            return false;
        };
        let Some(path) = open.path.clone() else {
            return self.save_editor_as(idx);
        };
        match open.editor.save(&path) {
            Ok(()) => {
                open.editor.mark_saved();
                self.browser.invalidate();
                self.status_text = format!("Saved {}", path.display());
                true
            }
            Err(e) => {
                self.status_text = format!("Save error: {e}");
                false
            }
        }
    }

    /// Save As via the native file dialog. Rebinds the tab to the chosen
    /// path on success.
    fn save_editor_as(&mut self, idx: usize) -> bool {
        let Some(open) = self.open_editors.get(idx) else {
            return false;
        };
        let ct = open.editor.content_type();
        let default_dir = self.browser.root.join(browser::content_dir(ct));
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
        let Some(open) = self.open_editors.get_mut(idx) else {
            return false;
        };
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
                self.status_text = format!("Saved {}", path.display());
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

        // Undo/redo stay out of the way while a text field has focus so
        // TextEdit keeps its own in-field undo.
        let typing = ctx.wants_keyboard_input();
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

        // Escape closes the AI settings window.
        if self.ai_settings.open
            && self.pending.is_none()
            && ctx.input(|i| i.key_pressed(Key::Escape))
        {
            self.ai_settings.open = false;
        }

        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.help.open = !self.help.open;
        }
    }

    fn active_open(&self) -> Option<&OpenEditor> {
        self.active_tab.and_then(|i| self.open_editors.get(i))
    }

    fn active_open_mut(&mut self) -> Option<&mut OpenEditor> {
        self.active_tab.and_then(|i| self.open_editors.get_mut(i))
    }

    /// Render and resolve the pending confirmation dialog, if any.
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
        }
    }

    fn menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    ui.menu_button("New", |ui| {
                        let mut pick: Option<ContentType> = None;
                        ui.menu_button("Systems", |ui| {
                            for ct in [ContentType::ChartedSystem, ContentType::GateNetwork] {
                                if ui.button(ct.name()).clicked() {
                                    pick = Some(ct);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("Ships", |ui| {
                            for ct in [
                                ContentType::HullFrame,
                                ContentType::HullMesh,
                                ContentType::Station,
                                ContentType::RoomTemplates,
                            ] {
                                if ui.button(ct.name()).clicked() {
                                    pick = Some(ct);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("Characters", |ui| {
                            for ct in [ContentType::Soul, ContentType::EnemyArchetype] {
                                if ui.button(ct.name()).clicked() {
                                    pick = Some(ct);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("World", |ui| {
                            for ct in [
                                ContentType::Faction,
                                ContentType::Storyline,
                                ContentType::Location,
                                ContentType::Contract,
                            ] {
                                if ui.button(ct.name()).clicked() {
                                    pick = Some(ct);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("Economy", |ui| {
                            for ct in [ContentType::EconomyGoods, ContentType::Item] {
                                if ui.button(ct.name()).clicked() {
                                    pick = Some(ct);
                                    ui.close_menu();
                                }
                            }
                        });
                        ui.menu_button("Preview", |ui| {
                            for ct in [ContentType::ItemBrowser, ContentType::SpriteViewer] {
                                if ui.button(ct.name()).clicked() {
                                    pick = Some(ct);
                                    ui.close_menu();
                                }
                            }
                        });
                        if let Some(ct) = pick {
                            self.open_new_editor(&format!("New {}", ct.name()), ct);
                            ui.close_menu();
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
                    if ui.button("Preferences").clicked() {
                        self.status_text = "Preferences not yet implemented".into();
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
                    ui.close_menu();
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
                        ui.label(format!("{modified_count} unsaved"));
                    }
                    ui.label(format!("{} editor(s) open", self.open_editors.len()));
                });
            });
        });

        egui::TopBottomPanel::top("seed_panel").show(ctx, |ui| {
            self.seed_workflow.ui(ui);
        });

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
                    if ui
                        .add_enabled(can_generate, egui::Button::new(btn))
                        .clicked()
                    {
                        let ct = active_ct.expect("guarded by can_generate");
                        self.ai_running = true;
                        *self.ai_status.lock().unwrap() = format!("Generating {ct:?} content…");
                        let cfg = self.ai_settings.config().clone();
                        let prompt = self.ai_prompt.trim().to_string();
                        let (tx, rx) = channel();
                        self.ai_result_rx = Some(rx);
                        std::thread::spawn(move || {
                            let schemas = SchemaCache::load_all();
                            let rt = tokio::runtime::Builder::new_multi_thread()
                                .enable_all()
                                .build()
                                .unwrap();
                            let outcome = rt.block_on(async {
                                match ai::generate_content(&cfg, ct, &schemas, &prompt).await {
                                    Ok(result) => ai::AiGenOutcome::Ok { ct, result },
                                    Err(e) => ai::AiGenOutcome::Err(e),
                                }
                            });
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
                    .show(ctx, |ui| {
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
                                if ui.selectable_label(selected, &title).clicked() {
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
                        open.editor.ui(ctx);
                    }
                }
            }
        });

        egui::SidePanel::right("preview_panel")
            .resizable(true)
            .default_width(250.0)
            .show(ctx, |ui| {
                self.preview.ui(ctx, ui);
            });

        self.ai_settings.show(ctx);
        self.help.show(ctx);
        self.handle_pending(ctx);

        // Undo bookkeeping: one diff point per frame, after every mutation
        // path (editor UI, AI apply, dialogs) has run.
        if let Some(open) = self.active_open_mut() {
            open.track_changes();
        }

        ctx.request_repaint();
    }
}

fn main() -> eframe::Result<()> {
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
