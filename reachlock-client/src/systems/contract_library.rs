//! Contract library browser (S34). Browse, sort, filter, and import contracts.
//! Keyboard-driven: Tab cycles tabs (Browse / My Contracts), W/S navigates,
//! Enter opens detail view, I imports.

use bevy::prelude::*;

use reachlock_core::contract::metadata::{ContractLibraryEntry, ContractMetadata, CrewRole};

use crate::settings::{InputAction, Settings};
use crate::systems::interaction::ActivePanel;

// ---------------------------------------------------------------------------
// Sort modes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum LibrarySort {
    Newest,
    MostStories,
    MostInteresting,
}

fn sort_name(s: LibrarySort) -> &'static str {
    match s {
        LibrarySort::Newest => "NEWEST",
        LibrarySort::MostStories => "STORIES",
        LibrarySort::MostInteresting => "INTERESTING",
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Resource)]
pub struct ContractLibraryState {
    /// All known contracts (local + synced).
    pub entries: Vec<ContractLibraryEntry>,
    /// Which sort mode is active.
    pub sort: LibrarySort,
    /// Filter by crew role (None = all).
    pub filter_role: Option<CrewRole>,
    /// Selected index in the current view.
    pub sel: usize,
    /// Are we showing a single contract's detail view?
    pub detail: bool,
    /// Index in entries of the contract being viewed in detail.
    pub detail_idx: usize,
    /// Import buffer text (player pastes RON here).
    pub import_buffer: String,
    /// Import mode active.
    pub importing: bool,
    pub status: String,
}

impl Default for ContractLibraryState {
    fn default() -> Self {
        ContractLibraryState {
            entries: Vec::new(),
            sort: LibrarySort::Newest,
            filter_role: None,
            sel: 0,
            detail: false,
            detail_idx: 0,
            import_buffer: String::new(),
            importing: false,
            status: String::new(),
        }
    }
}

/// Marker component for the library panel text node.
#[derive(Component)]
pub struct ContractLibraryPanel;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_view(
    entries: &[ContractLibraryEntry],
    sort: LibrarySort,
    role: Option<CrewRole>,
) -> Vec<&ContractLibraryEntry> {
    let mut v: Vec<_> = match role {
        Some(r) => entries
            .iter()
            .filter(|e| e.metadata.crew_role == r)
            .collect(),
        None => entries.iter().collect(),
    };
    match sort {
        LibrarySort::Newest => v.sort_by_key(|a| std::cmp::Reverse(a.metadata.created)),
        LibrarySort::MostStories => v.sort_by_key(|a| std::cmp::Reverse(a.metadata.updated)),
        LibrarySort::MostInteresting => v.sort_by_key(|a| std::cmp::Reverse(a.metadata.updated)),
    }
    v
}

fn role_display(r: CrewRole) -> &'static str {
    match r {
        CrewRole::Pilot => "PILOT",
        CrewRole::Engineer => "ENG",
        CrewRole::Navigator => "NAV",
        CrewRole::Medic => "MEDIC",
        CrewRole::Gunner => "GUNNER",
        CrewRole::Tactical => "TAC",
    }
}

// ---------------------------------------------------------------------------
// Library system
// ---------------------------------------------------------------------------

pub fn library_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    panel: Res<ActivePanel>,
    mut state: ResMut<ContractLibraryState>,
) {
    if *panel != ActivePanel::ContractLibrary {
        if state.detail {
            state.detail = false;
        }
        state.importing = false;
        return;
    }

    // ---- Tab switching (Tab cycles modes) ----
    if keys.just_pressed(settings.key(InputAction::EditorTabNext)) {
        state.importing = false;
        if state.detail {
            state.detail = false;
            state.status.clear();
        } else {
            state.status.clear();
        }
    }

    // ---- Back out of detail view ----
    if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        if state.importing {
            state.importing = false;
            state.status.clear();
            return;
        }
        if state.detail {
            state.detail = false;
            state.status.clear();
            return;
        }
    }

    // ---- Import mode ----
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) && state.importing {
        let trimmed = state.import_buffer.trim();
        if let Ok(entry) = ron::from_str::<ContractLibraryEntry>(trimmed) {
            state.entries.push(entry);
            state.status = "contract imported".into();
        } else if let Ok(_meta) = ron::from_str::<ContractMetadata>(trimmed) {
            state.status = "metadata only — need full entry".into();
        } else {
            state.status = "invalid RON".into();
        }
        state.importing = false;
        return;
    }

    // ---- Enter/Import key in browse mode ----
    if state.detail {
        if keys.just_pressed(settings.key(InputAction::EditorConfirm))
            && state.entries.get(state.detail_idx).is_some()
        {
            state.status = "imported to workshop (placeholder)".into();
        }
        return;
    }

    // ---- Browse / My Contracts navigation ----
    let view = build_view(&state.entries, state.sort, state.filter_role);

    if view.is_empty() {
        if keys.just_pressed(KeyCode::KeyI) {
            state.importing = true;
            state.import_buffer.clear();
            state.status = "paste contract RON then Enter".into();
        }
        return;
    }

    let count = view.len();
    if keys.just_pressed(settings.key(InputAction::EditorCursorUp)) {
        state.sel = (state.sel + count - 1) % count;
        state.status.clear();
    }
    if keys.just_pressed(settings.key(InputAction::EditorCursorDown)) {
        state.sel = (state.sel + 1) % count;
        state.status.clear();
    }

    // ---- Enter to view detail ----
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        state.detail_idx = state.sel;
        state.detail = true;
        state.status.clear();
    }

    // ---- I to import ----
    if keys.just_pressed(KeyCode::KeyI) {
        state.importing = true;
        state.import_buffer.clear();
        state.status = "paste contract RON then Enter".into();
    }

    // ---- Sort cycling (S key) ----
    if keys.just_pressed(KeyCode::KeyS) {
        state.sort = match state.sort {
            LibrarySort::Newest => LibrarySort::MostStories,
            LibrarySort::MostStories => LibrarySort::MostInteresting,
            LibrarySort::MostInteresting => LibrarySort::Newest,
        };
        state.sel = 0;
        state.status = format!("sort: {}", sort_name(state.sort));
    }
}

// ---------------------------------------------------------------------------
// Panel text rendering
// ---------------------------------------------------------------------------

pub fn library_panel_text(state: &ContractLibraryState) -> String {
    let mut lines =
        vec!["── CONTRACT LIBRARY ──  W/S select · Enter detail · I import · S sort".into()];

    // ---- Import mode ----
    if state.importing {
        lines.push("── IMPORT ──".into());
        lines.push("Paste ContractLibraryEntry RON then Enter:".into());
        lines.push(format!("> {}", state.import_buffer));
        lines.push("(Esc cancel)".into());
        return lines.join("\n");
    }

    // ---- Detail view ----
    if state.detail {
        if let Some(entry) = state.entries.get(state.detail_idx) {
            let meta = &entry.metadata;
            lines.push(format!("── {} ──", meta.crew_member_name));
            lines.push(format!("  author: {}", meta.author));
            lines.push(format!("  role: {}", role_display(meta.crew_role)));
            lines.push(format!("  tags: {}", meta.personality_tags.join(", ")));
            lines.push(format!("  story tags: {}", meta.story_tags.join(", ")));
            lines.push(format!("  shareable: {}", meta.shareable));
            lines.push(String::new());
            lines.push(format!("  description: {}", meta.description));
            if !meta.usage_notes.is_empty() {
                lines.push(format!("  notes: {}", meta.usage_notes));
            }
            lines.push(String::new());
            lines.push("── Rules ──".into());
            // The rules are embedded in the RON string — print first 3 lines.
            for line in entry.contract_ron.lines().take(8) {
                lines.push(format!("  {line}"));
            }
            if entry.contract_ron.lines().count() > 8 {
                lines.push("  … (truncated)".into());
            }
            lines.push(String::new());
            lines.push("  [Enter] import to workshop  [Esc] back".into());
        }
        return lines.join("\n");
    }

    // ---- Browse / My Contracts ----
    let view = build_view(&state.entries, state.sort, state.filter_role);

    let sort_label = sort_name(state.sort);
    let filter_label = state.filter_role.map(role_display).unwrap_or("ALL");
    lines.push(format!(
        "  sort: [{sort_label}]  filter: [{filter_label}]  ({}) entries",
        view.len()
    ));

    if view.is_empty() {
        lines.push("  (no contracts — press I to import)".into());
        return lines.join("\n");
    }

    let cursor = |i: usize| if i == state.sel { ">" } else { " " };
    for (i, entry) in view.iter().enumerate() {
        let meta = &entry.metadata;
        let shared = if meta.shareable { "↑" } else { "·" };
        lines.push(format!(
            "{} {} {:8} {}  {:20}  {}",
            cursor(i),
            shared,
            role_display(meta.crew_role),
            meta.crew_member_name,
            meta.description.chars().take(20).collect::<String>(),
            meta.author,
        ));
    }

    if !state.status.is_empty() {
        lines.push(format!("  · {}", state.status));
    }
    lines.push(String::new());
    lines.push("  [I] import  [S] sort  [Enter] detail".into());

    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------

pub fn spawn_library_panel(mut commands: Commands) {
    commands.spawn((
        ContractLibraryPanel,
        Text::new(""),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.9, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(100.0),
            left: Val::Px(300.0),
            max_width: Val::Px(520.0),
            ..default()
        },
    ));
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

pub fn render_library_panel(
    panel: Res<ActivePanel>,
    state: Res<ContractLibraryState>,
    mut texts: Query<&mut Text, With<ContractLibraryPanel>>,
) {
    if let Ok(mut text) = texts.single_mut() {
        match &*panel {
            ActivePanel::ContractLibrary => {
                **text = library_panel_text(&state);
            }
            _ => {
                **text = String::new();
            }
        }
    }
}
