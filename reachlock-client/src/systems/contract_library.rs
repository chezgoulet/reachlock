//! Contract library browser (S34, S86). Browse, sort, filter, import, search,
//! and share contracts. Network-synced with the server library service.

use bevy::prelude::*;
use std::time::Instant;

use reachlock_core::contract::metadata::{
    ContractLibraryEntry, ContractMetadata, ContractStory, CrewRole,
};
use reachlock_core::contract::Contract;
use reachlock_core::network::ClientMessage;

use crate::net::NetOutbox;
use crate::settings::{InputAction, Settings};
use crate::systems::contract_crafting::ContractWorkshopState;
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
    /// S86: search text input.
    pub search_buffer: String,
    /// S86: search active.
    pub searching: bool,
    /// S86: current page (0-indexed) for pagination.
    pub page: u32,
    /// S86: total matching entries from server.
    pub total: u32,
    /// S86: stores stories for the currently viewed contract.
    pub stories: Vec<ContractStory>,
    /// S86: share code input buffer.
    pub share_code_buffer: String,
    /// S86: share code lookup mode.
    pub share_lookup: bool,
    /// S86: publish metadata input mode.
    pub publish_mode: bool,
    /// S86: publish author, description buffer.
    pub publish_author: String,
    pub publish_description: String,
    pub publish_tags: String,
    pub publish_story_tags: String,
    /// S86: last sync timestamp (cooldown).
    pub last_sync: Instant,
    /// S86: stories tab active vs details.
    pub show_stories: bool,
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
            search_buffer: String::new(),
            searching: false,
            page: 0,
            total: 0,
            stories: Vec::new(),
            share_code_buffer: String::new(),
            share_lookup: false,
            publish_mode: false,
            publish_author: String::new(),
            publish_description: String::new(),
            publish_tags: String::new(),
            publish_story_tags: String::new(),
            last_sync: Instant::now(),
            show_stories: false,
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
    focus_stack: Res<crate::focus_stack::FocusStack>,
    mut state: ResMut<ContractLibraryState>,
    mut workshop: ResMut<ContractWorkshopState>,
    mut outbox: ResMut<NetOutbox>,
) {
    if *panel != ActivePanel::ContractLibrary {
        if state.detail {
            state.detail = false;
        }
        state.importing = false;
        state.searching = false;
        state.share_lookup = false;
        state.publish_mode = false;
        return;
    }
    if focus_stack.top_captures_input() {
        return;
    }

    // ---- Auto-sync on panel open ----
    if state.last_sync.elapsed().as_secs() > 5 && state.entries.is_empty() {
        outbox.push(ClientMessage::LibrarySync {
            role_filter: state.filter_role.map(|r| format!("{r:?}")),
            sort: Some(sort_name(state.sort).to_lowercase()),
            search: None,
            page: 0,
            page_size: 50,
        });
        state.last_sync = Instant::now();
        state.status = "syncing…".into();
    }

    // ---- Tab switching (Tab cycles modes) ----
    if keys.just_pressed(settings.key(InputAction::EditorTabNext)) {
        state.importing = false;
        state.searching = false;
        state.share_lookup = false;
        state.publish_mode = false;
        if state.detail {
            state.detail = false;
            state.show_stories = false;
            state.status.clear();
        } else {
            state.status.clear();
        }
    }

    // ---- Back out of current mode ----
    if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        if state.publish_mode {
            state.publish_mode = false;
            state.status.clear();
            return;
        }
        if state.share_lookup {
            state.share_lookup = false;
            state.share_code_buffer.clear();
            state.status.clear();
            return;
        }
        if state.searching {
            state.searching = false;
            state.search_buffer.clear();
            state.page = 0;
            state.status.clear();
            // Re-sync without search.
            outbox.push(ClientMessage::LibrarySync {
                role_filter: state.filter_role.map(|r| format!("{r:?}")),
                sort: Some(sort_name(state.sort).to_lowercase()),
                search: None,
                page: 0,
                page_size: 50,
            });
            return;
        }
        if state.importing {
            state.importing = false;
            state.status.clear();
            return;
        }
        if state.detail {
            state.detail = false;
            state.show_stories = false;
            state.status.clear();
            return;
        }
    }

    // ---- Detail view interactions ----
    if state.detail {
        // Toggle stories tab with S key.
        if keys.just_pressed(KeyCode::KeyS) {
            state.show_stories = !state.show_stories;
            state.status.clear();
        }
        // Import to workshop with Enter.
        if keys.just_pressed(settings.key(InputAction::EditorConfirm))
            && !state.show_stories
            && state.entries.get(state.detail_idx).is_some()
        {
            let entry = state.entries[state.detail_idx].clone();
            match ron::from_str::<Contract>(&entry.contract_ron) {
                Ok(contract) => {
                    workshop.draft = Some(contract);
                    workshop.importing = false;
                    workshop.metrics.imports += 1;
                    state.status = format!(
                        "imported to workshop: {} by {}",
                        entry.metadata.crew_member_name, entry.metadata.author
                    );
                    info!(
                        "contract imported: {} ({})",
                        entry.metadata.crew_member_name, entry.metadata.author
                    );
                }
                Err(e) => {
                    state.status = format!("parse error: {e}");
                }
            }
        }
        return;
    }

    // ---- Publish mode ----
    if state.publish_mode {
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
            let author = if state.publish_author.is_empty() {
                "anonymous".to_string()
            } else {
                state.publish_author.clone()
            };
            let description = if state.publish_description.is_empty() {
                "A published contract.".to_string()
            } else {
                state.publish_description.clone()
            };
            let meta = ContractMetadata::new(
                author,
                "published_contract".into(),
                state.filter_role.unwrap_or(CrewRole::Engineer),
                description,
            );
            let code =
                "(id:\"pub\",label:\"published\",trigger:Manual,rules:[],llm_authority:None)";
            let entry = ContractLibraryEntry {
                metadata: meta,
                contract_ron: code.into(),
            };
            outbox.push(ClientMessage::LibraryPublish {
                metadata: entry.metadata.clone(),
                contract_ron: entry.contract_ron.clone(),
            });
            state.status = "publishing…".into();
            state.publish_mode = false;
            return;
        }
        // Text input handling for publish fields.
        handle_text_input(&keys, &mut state);
        return;
    }

    // ---- Share lookup mode ----
    if state.share_lookup {
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
            let code = state.share_code_buffer.trim().to_string();
            if code.len() == 8 {
                outbox.push(ClientMessage::LibraryShareLookup { share_code: code });
                state.status = "looking up share code…".into();
            } else {
                state.status = "share code must be 8 characters".into();
            }
            state.share_lookup = false;
            return;
        }
        handle_text_input(&keys, &mut state);
        return;
    }

    // ---- Search mode ----
    if state.searching {
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
            let query = state.search_buffer.trim().to_string();
            outbox.push(ClientMessage::LibrarySync {
                role_filter: state.filter_role.map(|r| format!("{r:?}")),
                sort: Some(sort_name(state.sort).to_lowercase()),
                search: if query.is_empty() { None } else { Some(query) },
                page: 0,
                page_size: 50,
            });
            state.searching = false;
            state.page = 0;
            state.status = "searching…".into();
            return;
        }
        handle_text_input(&keys, &mut state);
        return;
    }

    // ---- Import mode ----
    if state.importing {
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
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
        handle_text_input(&keys, &mut state);
        return;
    }

    // ---- Browse mode navigation ----
    let view = build_view(&state.entries, state.sort, state.filter_role);
    let count = view.len();

    if count > 0 {
        if keys.just_pressed(settings.key(InputAction::EditorCursorUp)) {
            state.sel = (state.sel + count - 1) % count;
            state.status.clear();
        }
        if keys.just_pressed(settings.key(InputAction::EditorCursorDown)) {
            state.sel = (state.sel + 1) % count;
            state.status.clear();
        }
    }

    // ---- Enter to view detail ----
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) && count > 0 {
        state.detail_idx = state.sel;
        state.detail = true;
        state.show_stories = false;
        state.status.clear();
    }

    // ---- I to import ----
    if keys.just_pressed(KeyCode::KeyI) {
        state.importing = true;
        state.import_buffer.clear();
        state.status = "paste contract RON then Enter".into();
    }

    // ---- / to search ----
    if keys.just_pressed(KeyCode::Slash) {
        state.searching = true;
        state.search_buffer.clear();
        state.status = "type search, Enter to search, Esc to clear".into();
    }

    // ---- C for share code lookup ----
    if keys.just_pressed(KeyCode::KeyC) {
        state.share_lookup = true;
        state.share_code_buffer.clear();
        state.status = "enter 8-char share code then Enter".into();
    }

    // ---- P for publish mode ----
    if keys.just_pressed(KeyCode::KeyP) {
        state.publish_mode = true;
        state.publish_author.clear();
        state.publish_description.clear();
        state.publish_tags.clear();
        state.publish_story_tags.clear();
        state.status = "Enter publish metadata, then Enter to confirm".into();
    }

    // ---- Sort cycling (S key) ----
    if keys.just_pressed(KeyCode::KeyS) {
        state.sort = match state.sort {
            LibrarySort::Newest => LibrarySort::MostStories,
            LibrarySort::MostStories => LibrarySort::MostInteresting,
            LibrarySort::MostInteresting => LibrarySort::Newest,
        };
        state.sel = 0;
        state.page = 0;
        state.status = format!("sort: {}", sort_name(state.sort));
        // Re-sync with new sort.
        outbox.push(ClientMessage::LibrarySync {
            role_filter: state.filter_role.map(|r| format!("{r:?}")),
            sort: Some(sort_name(state.sort).to_lowercase()),
            search: None,
            page: 0,
            page_size: 50,
        });
    }

    // ---- Pagination ----
    if keys.just_pressed(KeyCode::Comma) {
        // Previous page
        if state.page > 0 {
            state.page -= 1;
            outbox.push(ClientMessage::LibrarySync {
                role_filter: state.filter_role.map(|r| format!("{r:?}")),
                sort: Some(sort_name(state.sort).to_lowercase()),
                search: None,
                page: state.page,
                page_size: 50,
            });
            state.status = format!("page {}", state.page + 1);
        }
    }
    if keys.just_pressed(KeyCode::Period) {
        // Next page
        let max_page = state.total.saturating_sub(1) / 50;
        if state.page < max_page {
            state.page += 1;
            outbox.push(ClientMessage::LibrarySync {
                role_filter: state.filter_role.map(|r| format!("{r:?}")),
                sort: Some(sort_name(state.sort).to_lowercase()),
                search: None,
                page: state.page,
                page_size: 50,
            });
            state.status = format!("page {}", state.page + 1);
        }
    }
}

/// Handle text input for import/search/share/publish buffers.
#[allow(unused_variables)]
fn handle_text_input(keys: &Res<ButtonInput<KeyCode>>, state: &mut ContractLibraryState) {
    // Text input handler placeholder.
    // Enter/Escape handled in the caller.
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

    // ---- Search mode ----
    if state.searching {
        lines.push("── SEARCH ──".into());
        lines.push("Search contracts (name/description/tags):".into());
        lines.push(format!("> {}", state.search_buffer));
        lines.push("(Enter search · Esc clear)".into());
        return lines.join("\n");
    }

    // ---- Share lookup mode ----
    if state.share_lookup {
        lines.push("── IMPORT BY SHARE CODE ──".into());
        lines.push("Enter 8-char share code:".into());
        lines.push(format!("> {}", state.share_code_buffer));
        lines.push("(Enter confirm · Esc cancel)".into());
        return lines.join("\n");
    }

    // ---- Publish mode ----
    if state.publish_mode {
        lines.push("── PUBLISH CONTRACT ──".into());
        lines.push("Fill in metadata to publish:".into());
        lines.push(format!("  author: {}", state.publish_author));
        lines.push(format!("  description: {}", state.publish_description));
        lines.push(format!("  tags: {}", state.publish_tags));
        lines.push(format!("  story tags: {}", state.publish_story_tags));
        lines.push("(Enter confirm · Esc cancel)".into());
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
            lines.push(String::new());
            lines.push(format!("  description: {}", meta.description));
            if !meta.usage_notes.is_empty() {
                lines.push(format!("  notes: {}", meta.usage_notes));
            }
            lines.push(String::new());

            if state.show_stories {
                // Stories tab.
                lines.push("── STORIES ──  (S for details)".into());
                if state.stories.is_empty() {
                    lines.push("  No stories yet.".into());
                } else {
                    for story in &state.stories {
                        let truncated = if story.story.len() > 200 {
                            format!("{}…", &story.story[..200])
                        } else {
                            story.story.clone()
                        };
                        lines.push(format!("  {truncated}"));
                        lines.push(format!(
                            "    — {:?}, {:?}, author: unknown",
                            story.event_type, story.outcome_type
                        ));
                    }
                }
            } else {
                // Rules tab.
                lines.push("── Rules ──".into());
                for line in entry.contract_ron.lines().take(8) {
                    lines.push(format!("  {line}"));
                }
                if entry.contract_ron.lines().count() > 8 {
                    lines.push("  … (truncated)".into());
                }
            }
            lines.push(String::new());
            lines.push("  [Enter] import to workshop  [S] stories  [Esc] back".into());
        }
        return lines.join("\n");
    }

    // ---- Browse / My Contracts ----
    let view = build_view(&state.entries, state.sort, state.filter_role);

    let sort_label = sort_name(state.sort);
    let filter_label = state.filter_role.map(role_display).unwrap_or("ALL");
    let max_page = state.total.saturating_sub(1) / 50;
    let page_info = format!("Page {}/{}", state.page + 1, max_page + 1);
    lines.push(format!(
        "  sort: [{sort_label}]  filter: [{filter_label}]  {page_info}  ({}) entries",
        view.len()
    ));

    if view.is_empty() {
        lines.push("  (no contracts — I import · / search · C share code)".into());
        if !state.status.is_empty() {
            lines.push(format!("  · {}", state.status));
        }
        return lines.join("\n");
    }

    let cursor = |i: usize| if i == state.sel { ">" } else { " " };
    for (i, entry) in view.iter().enumerate() {
        let meta = &entry.metadata;
        let shared = "·";
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
    lines.push("  [I] import  [S] sort  [/] search  [C] share code  [<] [>] page".into());

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
