//! Help window (handoff completion §Priority 4) — toggled with F1.
//!
//! Sections cover every editor (reusing the AI prompt's per-type context
//! paragraphs), keyboard shortcuts, the AI and procedural generation
//! workflows, and where files land on disk. A search field filters
//! sections to those containing the query.

use crate::app::ContentType;

struct Section {
    title: String,
    paragraphs: Vec<String>,
}

pub struct HelpWindow {
    pub open: bool,
    search: String,
    sections: Vec<Section>,
}

fn shortcut_lines() -> Vec<String> {
    [
        ("Ctrl+S", "Save the active editor"),
        ("Ctrl+Shift+S", "Save As (native file dialog)"),
        ("Ctrl+O", "Open a .ron content file"),
        ("Ctrl+Z", "Undo (when no text field is focused)"),
        ("Ctrl+Y / Ctrl+Shift+Z", "Redo"),
        ("Ctrl+W", "Close the active tab"),
        ("Ctrl+Q", "Quit (asks about unsaved changes)"),
        ("Delete", "Delete the selected entry (with confirmation)"),
        ("Escape", "Close the AI settings window / cancel a dialog"),
        ("F1", "Toggle this help window"),
    ]
    .iter()
    .map(|(key, what)| format!("{key:<24} {what}"))
    .collect()
}

fn build_sections() -> Vec<Section> {
    let mut sections = vec![
        Section {
            title: "Getting Started".into(),
            paragraphs: vec![
                "1. Pick a file in the Content Browser (left panel) — the tree mirrors \
                 mods/reachlock/ on disk. Click a file to open it in its editor."
                    .into(),
                "2. Or start fresh: File → New lists every editor, grouped by domain. \
                 Each editor supports three creation paths: Generate from Seed \
                 (procedural), the AI bar (natural language), and plain manual editing."
                    .into(),
                "3. Edit fields in the center panel. A * on the tab marks unsaved \
                 changes; Ctrl+S saves, and closing a dirty tab asks first."
                    .into(),
                "4. The right panel previews the active editor's selection at a glance.".into(),
            ],
        },
        Section {
            title: "Keyboard Shortcuts".into(),
            paragraphs: shortcut_lines(),
        },
        Section {
            title: "AI Generation".into(),
            paragraphs: vec![
                "The AI bar (below the seed panel) sends your description plus the active \
                 editor's JSON schema to any OpenAI-compatible endpoint — by default a \
                 local Ollama at http://localhost:11434/v1 (pull a model first, e.g. \
                 `ollama pull llama3.2:3b`)."
                    .into(),
                "Configure endpoint, model, API key, and token budget under AI → AI \
                 Settings…, and use Test Connection to verify. Generation runs in the \
                 background; the result populates the active editor's fields."
                    .into(),
                "AI output is a starting point: review it, tweak fields, then save \
                 explicitly. Nothing is saved automatically, and nothing leaves your \
                 machine when using local Ollama."
                    .into(),
                "Example prompts: \"a grizzled Voidborn smuggler who distrusts the \
                 Compact\", \"a nebula system on the far frontier\", \"a tier-7 kinetic \
                 railgun\"."
                    .into(),
            ],
        },
        Section {
            title: "Procedural Generation & Seeds".into(),
            paragraphs: vec![
                "Every editor's \"Generate from Seed\" button fills its fields \
                 deterministically from a seed — the same seed always produces the same \
                 content, on every platform."
                    .into(),
                "The seed panel at the top drives bulk exploration: \"Reroll All\" \
                 applies the current seed to every open editor that supports rerolling \
                 (and auto-increments it, so repeated clicks explore new seeds). \
                 \"Lock Current\" derives a stable seed from the active tab to riff on."
                    .into(),
                "Seeds stay below 2^53 so they survive JSON round-trips.".into(),
            ],
        },
        Section {
            title: "Where Files Save".into(),
            paragraphs: vec![
                "Content lives under mods/reachlock/, one directory per type: \
                 systems/, gate_network/, hulls/ (frames, meshes, and room templates \
                 share it), stations/, souls/, combat/ (enemy archetypes), factions/, \
                 storylines/, locations/, economy/, items/, contracts/."
                    .into(),
                "Files are RON (Rusty Object Notation). Save As suggests the right \
                 directory for the active editor; the browser rescans automatically."
                    .into(),
            ],
        },
    ];

    // One paragraph per editor, drawn from the AI prompt context (ai.rs).
    let mut editor_docs = Vec::new();
    for ct in ContentType::all() {
        editor_docs.push(format!("{} — {}", ct.name(), crate::ai::type_context(ct)));
    }
    sections.push(Section {
        title: "The Editors".into(),
        paragraphs: editor_docs,
    });
    sections
}

impl HelpWindow {
    pub fn new() -> Self {
        HelpWindow {
            open: false,
            search: String::new(),
            sections: build_sections(),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Help")
            .open(&mut open)
            .resizable(true)
            .default_size([560.0, 480.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("search help…")
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.separator();
                let needle = self.search.trim().to_lowercase();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for section in &self.sections {
                        let matching: Vec<&String> = if needle.is_empty()
                            || section.title.to_lowercase().contains(&needle)
                        {
                            section.paragraphs.iter().collect()
                        } else {
                            section
                                .paragraphs
                                .iter()
                                .filter(|p| p.to_lowercase().contains(&needle))
                                .collect()
                        };
                        if matching.is_empty() {
                            continue;
                        }
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(&section.title).heading().strong());
                        ui.add_space(2.0);
                        let monospace = section.title == "Keyboard Shortcuts";
                        for paragraph in matching {
                            if monospace {
                                ui.monospace(paragraph);
                            } else {
                                ui.label(paragraph);
                            }
                            ui.add_space(4.0);
                        }
                    }
                    if !needle.is_empty()
                        && self.sections.iter().all(|s| {
                            !s.title.to_lowercase().contains(&needle)
                                && s.paragraphs
                                    .iter()
                                    .all(|p| !p.to_lowercase().contains(&needle))
                        })
                    {
                        ui.weak(format!("No help entries match \"{}\".", self.search));
                    }
                });
            });
        self.open = open;
    }
}
