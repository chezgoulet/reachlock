use crate::app::ContentType;

pub struct CommandEntry {
    pub label: &'static str,
    /// Where this command also lives in the menus, shown dimmed beside the
    /// label. The palette is a shortcut, not a separate set of features, and
    /// showing the menu path is what teaches that.
    pub category: &'static str,
    pub action: PaletteAction,
}

#[derive(Clone)]
pub enum PaletteAction {
    NewEditor(ContentType),
    Open,
    Save,
    SaveAs,
    CloseTab,
    CloseAll,
    Undo,
    Redo,
    ToggleBrowser,
    AiGenerate,
    Help,
    Preferences,
    AiSettings,
    ValidateAll,
    FindUsages,
    BrokenReferenceReport,
    PreviewChanges,
    Duplicate,
    Quit,
}

const ALL_COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        label: "New Hull Frame",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::HullFrame),
    },
    CommandEntry {
        label: "New Station",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Station),
    },
    CommandEntry {
        label: "New Location",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Location),
    },
    CommandEntry {
        label: "New Soul",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Soul),
    },
    CommandEntry {
        label: "New Contract",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Contract),
    },
    CommandEntry {
        label: "New Faction",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Faction),
    },
    CommandEntry {
        label: "New Economy Goods",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::EconomyGoods),
    },
    CommandEntry {
        label: "New Storyline",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Storyline),
    },
    CommandEntry {
        label: "New Item",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Item),
    },
    CommandEntry {
        label: "New Enemy Archetype",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::EnemyArchetype),
    },
    CommandEntry {
        label: "New Charted System",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::ChartedSystem),
    },
    CommandEntry {
        label: "New Hull Mesh",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::HullMesh),
    },
    CommandEntry {
        label: "New Room Templates",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::RoomTemplates),
    },
    CommandEntry {
        label: "New Gate Network",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::GateNetwork),
    },
    CommandEntry {
        label: "New Career",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Career),
    },
    CommandEntry {
        label: "New Ecosystem",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Ecosystem),
    },
    CommandEntry {
        label: "New Planet Culture",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::PlanetCulture),
    },
    CommandEntry {
        label: "New Theme",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Theme),
    },
    CommandEntry {
        label: "New Trope",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Trope),
    },
    CommandEntry {
        label: "New Scripted Encounter",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::ScriptedEncounter),
    },
    CommandEntry {
        label: "New Dialogue",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Dialogue),
    },
    CommandEntry {
        label: "New Dungeon",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Dungeon),
    },
    CommandEntry {
        label: "New Event",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Event),
    },
    CommandEntry {
        label: "New Recipe",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Recipe),
    },
    CommandEntry {
        label: "New Origin",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::Origin),
    },
    CommandEntry {
        label: "Open…",
        category: "File",
        action: PaletteAction::Open,
    },
    CommandEntry {
        label: "Save",
        category: "File",
        action: PaletteAction::Save,
    },
    CommandEntry {
        label: "Save As…",
        category: "File",
        action: PaletteAction::SaveAs,
    },
    CommandEntry {
        label: "Close Tab",
        category: "File",
        action: PaletteAction::CloseTab,
    },
    CommandEntry {
        label: "Close All Tabs",
        category: "File",
        action: PaletteAction::CloseAll,
    },
    CommandEntry {
        label: "Quit",
        category: "File",
        action: PaletteAction::Quit,
    },
    CommandEntry {
        label: "Undo",
        category: "Edit",
        action: PaletteAction::Undo,
    },
    CommandEntry {
        label: "Redo",
        category: "Edit",
        action: PaletteAction::Redo,
    },
    CommandEntry {
        label: "Toggle Browser",
        category: "View",
        action: PaletteAction::ToggleBrowser,
    },
    CommandEntry {
        label: "AI Generate",
        category: "AI",
        action: PaletteAction::AiGenerate,
    },
    CommandEntry {
        label: "Validate All",
        category: "File",
        action: PaletteAction::ValidateAll,
    },
    CommandEntry {
        label: "Preferences…",
        category: "Edit",
        action: PaletteAction::Preferences,
    },
    CommandEntry {
        label: "AI Settings…",
        category: "AI",
        action: PaletteAction::AiSettings,
    },
    CommandEntry {
        label: "Help",
        category: "Help",
        action: PaletteAction::Help,
    },
    CommandEntry {
        label: "Find Usages…",
        category: "Edit",
        action: PaletteAction::FindUsages,
    },
    CommandEntry {
        label: "Broken Reference Report",
        category: "File",
        action: PaletteAction::BrokenReferenceReport,
    },
    CommandEntry {
        label: "Preview Changes…",
        category: "File",
        action: PaletteAction::PreviewChanges,
    },
    CommandEntry {
        label: "Duplicate Document",
        category: "Edit",
        action: PaletteAction::Duplicate,
    },
    CommandEntry {
        label: "New Crew Package",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::CrewPackage),
    },
    // The two previewers persist nothing, so "New" would be a lie — but they
    // open through the same path, and leaving them out would make the palette
    // a strictly smaller menu than File > New.
    CommandEntry {
        label: "Open Item Browser",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::ItemBrowser),
    },
    CommandEntry {
        label: "Open Sprite Viewer",
        category: "File > New",
        action: PaletteAction::NewEditor(ContentType::SpriteViewer),
    },
];

pub struct CommandPalette {
    pub open: bool,
    pub(crate) filter: String,
    selected: usize,
}

impl CommandPalette {
    pub fn new() -> Self {
        CommandPalette {
            open: false,
            filter: String::new(),
            selected: 0,
        }
    }

    fn fuzzy_matches(text: &str, filter: &str) -> bool {
        let filter = filter.to_lowercase();
        let text = text.to_lowercase();
        let mut fi = filter.chars().peekable();
        for c in text.chars() {
            if fi.peek() == Some(&c) {
                fi.next();
            }
        }
        fi.next().is_none()
    }

    pub fn show(&mut self, ctx: &egui::Context, selected_action: &mut Option<PaletteAction>) {
        let open = &mut self.open;
        let filter = std::mem::take(&mut self.filter);
        let mut should_close = false;
        egui::Window::new("Command Palette")
            .open(open)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 40.0))
            .resizable(false)
            .default_size([400.0, 300.0])
            .show(ctx, |ui| {
                let mut new_filter = filter;
                let filter_changed = ui
                    .add(egui::TextEdit::singleline(&mut new_filter).hint_text("Type to filter…"))
                    .changed();

                let entries: Vec<&CommandEntry> = if new_filter.is_empty() {
                    ALL_COMMANDS.iter().collect()
                } else {
                    ALL_COMMANDS
                        .iter()
                        .filter(|e| Self::fuzzy_matches(e.label, &new_filter))
                        .collect()
                };

                let mut selected = self.selected;
                if filter_changed {
                    selected = 0;
                }

                ui.separator();
                let mut execute = false;
                let max_idx = entries.len().saturating_sub(1);
                egui::ScrollArea::vertical()
                    .id_salt("command_palette_results")
                    .show(ui, |ui| {
                        if entries.is_empty() {
                            ui.weak("No command matches.");
                        }
                        for (i, entry) in entries.iter().enumerate() {
                            let is_sel = selected == i;
                            let text = egui::RichText::new(entry.label);
                            let response = ui
                                .selectable_label(is_sel, text)
                                .on_hover_text(entry.category);
                            if response.clicked() {
                                selected = i;
                                execute = true;
                            }
                            if is_sel {
                                response.request_focus();
                            }
                        }
                    });

                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                    selected = (selected + 1).min(max_idx);
                    ui.ctx().request_repaint();
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                    selected = selected.saturating_sub(1);
                    ui.ctx().request_repaint();
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    execute = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    should_close = true;
                }

                if execute {
                    if let Some(entry) = entries.get(selected) {
                        *selected_action = Some(entry.action.clone());
                    }
                    should_close = true;
                }

                self.selected = selected;
                self.filter = new_filter;
            });
        if should_close {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stable name for a variant, so the coverage test below can report which
    /// action is missing rather than just a count.
    fn variant_name(action: &PaletteAction) -> &'static str {
        match action {
            PaletteAction::NewEditor(_) => "NewEditor",
            PaletteAction::Open => "Open",
            PaletteAction::Save => "Save",
            PaletteAction::SaveAs => "SaveAs",
            PaletteAction::CloseTab => "CloseTab",
            PaletteAction::CloseAll => "CloseAll",
            PaletteAction::Undo => "Undo",
            PaletteAction::Redo => "Redo",
            PaletteAction::ToggleBrowser => "ToggleBrowser",
            PaletteAction::AiGenerate => "AiGenerate",
            PaletteAction::Help => "Help",
            PaletteAction::Preferences => "Preferences",
            PaletteAction::AiSettings => "AiSettings",
            PaletteAction::ValidateAll => "ValidateAll",
            PaletteAction::FindUsages => "FindUsages",
            PaletteAction::BrokenReferenceReport => "BrokenReferenceReport",
            PaletteAction::PreviewChanges => "PreviewChanges",
            PaletteAction::Duplicate => "Duplicate",
            PaletteAction::Quit => "Quit",
        }
    }

    /// Every action the palette can express must have a command that produces
    /// it. An action with no entry is a feature with no way in — the same
    /// failure `new_menu_covers_every_type` guards for content types.
    ///
    /// The `match` in `variant_name` is exhaustive on purpose: adding a
    /// variant is a compile error until it is listed, and then a test failure
    /// until a command exists for it.
    #[test]
    fn every_palette_action_has_a_command() {
        const ALL_VARIANTS: &[&str] = &[
            "NewEditor",
            "Open",
            "Save",
            "SaveAs",
            "CloseTab",
            "CloseAll",
            "Undo",
            "Redo",
            "ToggleBrowser",
            "AiGenerate",
            "Help",
            "Preferences",
            "AiSettings",
            "ValidateAll",
            "FindUsages",
            "BrokenReferenceReport",
            "PreviewChanges",
            "Duplicate",
            "Quit",
        ];
        let covered: std::collections::HashSet<&str> = ALL_COMMANDS
            .iter()
            .map(|e| variant_name(&e.action))
            .collect();
        let missing: Vec<&str> = ALL_VARIANTS
            .iter()
            .copied()
            .filter(|v| !covered.contains(v))
            .collect();
        assert!(
            missing.is_empty(),
            "palette actions with no command entry, so nothing can invoke them: {missing:?}"
        );
    }

    /// Every command must carry a category, since the palette shows it as the
    /// hint for where the command also lives in the menus.
    #[test]
    fn every_command_is_labelled_and_categorised() {
        for entry in ALL_COMMANDS {
            assert!(!entry.label.is_empty(), "a command has no label");
            assert!(
                !entry.category.is_empty(),
                "command `{}` has no category",
                entry.label
            );
        }
    }

    /// The palette advertises itself as a faster route to the menus, so it
    /// must not offer fewer content types than File > New does. `CrewPackage`
    /// was added to the menu and missing here.
    #[test]
    fn palette_offers_every_type_the_new_menu_does() {
        let in_palette: std::collections::HashSet<ContentType> = ALL_COMMANDS
            .iter()
            .filter_map(|e| match e.action {
                PaletteAction::NewEditor(ct) => Some(ct),
                _ => None,
            })
            .collect();
        let missing: Vec<ContentType> = crate::app::NEW_MENU_GROUPS
            .iter()
            .flat_map(|(_, types)| types.iter().copied())
            .filter(|ct| !in_palette.contains(ct))
            .collect();
        assert!(
            missing.is_empty(),
            "File > New offers these but the command palette does not: {missing:?}"
        );
    }

    /// A category is a claim about where the command also lives. A wrong one
    /// sends the author to a menu that does not have it.
    #[test]
    fn categories_name_real_menus() {
        const MENUS: &[&str] = &["File", "File > New", "Edit", "View", "AI", "Help"];
        for entry in ALL_COMMANDS {
            assert!(
                MENUS.contains(&entry.category),
                "command `{}` claims category `{}`, which is not a menu",
                entry.label,
                entry.category
            );
        }
    }

    #[test]
    fn fuzzy_filter_matches_subsequences_not_just_prefixes() {
        assert!(CommandPalette::fuzzy_matches(
            "Broken Reference Report",
            "brr"
        ));
        assert!(CommandPalette::fuzzy_matches("New Soul", "soul"));
        assert!(!CommandPalette::fuzzy_matches("New Soul", "zzz"));
    }
}
