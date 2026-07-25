use bevy::prelude::*;

use crate::settings::{InputAction, Settings};

#[derive(Clone)]
pub enum PanelRow {
    Header(String),
    Label(String),
    KeyValue { key: String, value: String },
    Separator,
}

#[derive(Component)]
pub struct InfoPanel {
    pub title: String,
    pub rows: Vec<PanelRow>,
}

#[derive(Clone)]
pub enum SelectableRow {
    Toggle {
        label: String,
        value: bool,
    },
    Slider {
        label: String,
        value: f32,
        min: f32,
        max: f32,
    },
    Choice {
        label: String,
        choices: Vec<String>,
        selected: usize,
    },
    Action {
        label: String,
    },
}

#[derive(Component)]
pub struct SelectablePanel {
    pub title: String,
    pub subtitle: String,
    pub tabs: Vec<String>,
    pub active_tab: usize,
    pub rows: Vec<SelectableRow>,
    pub selected_row: usize,
    pub status: String,
}

/// Move the cursor up/down within the panel's rows.
pub fn navigate_selectable_panel(
    keys: &ButtonInput<KeyCode>,
    settings: &Settings,
    panel: &mut SelectablePanel,
) {
    let row_count = panel.rows.len().max(1);
    if keys.just_pressed(settings.key(InputAction::EditorCursorUp)) {
        panel.selected_row = (panel.selected_row + row_count - 1) % row_count;
    }
    if keys.just_pressed(settings.key(InputAction::EditorCursorDown)) {
        panel.selected_row = (panel.selected_row + 1) % row_count;
    }
}

/// Modify a row's value (Toggle ⟂, Slider ±step, Choice cycle).
pub fn cycle_selectable_row_value(row: &mut SelectableRow, step: i64) {
    match row {
        SelectableRow::Toggle { value, .. } => *value = !*value,
        SelectableRow::Slider {
            value, min, max, ..
        } => {
            *value = (*value + step as f32).clamp(*min, *max);
        }
        SelectableRow::Choice {
            selected, choices, ..
        } if !choices.is_empty() => {
            let len = choices.len().max(1);
            *selected = ((*selected as i64 + step).rem_euclid(len as i64)) as usize;
        }
        _ => {}
    }
}

pub fn format_selectable_panel_text(panel: &SelectablePanel) -> String {
    let mut lines = vec![format!("── {} ──", panel.title)];

    if !panel.subtitle.is_empty() {
        lines.push(panel.subtitle.clone());
    }

    if !panel.tabs.is_empty() {
        let tabs: Vec<String> = panel
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if i == panel.active_tab {
                    format!("[{t}]")
                } else {
                    t.clone()
                }
            })
            .collect();
        lines.push(tabs.join(" "));
    }

    let cur = |i: usize| {
        if i == panel.selected_row && !panel.rows.is_empty() {
            ">"
        } else {
            " "
        }
    };

    for (i, row) in panel.rows.iter().enumerate() {
        match row {
            SelectableRow::Toggle { label, value } => {
                lines.push(format!("{}{}: {value}", cur(i), label));
            }
            SelectableRow::Slider { label, value, .. } => {
                lines.push(format!("{}{}: {value}", cur(i), label));
            }
            SelectableRow::Choice {
                label,
                choices,
                selected,
            } => {
                let current = choices.get(*selected).map(|s| s.as_str()).unwrap_or("?");
                lines.push(format!("{}{}: {current}", cur(i), label));
            }
            SelectableRow::Action { label } => {
                lines.push(format!("{}{}", cur(i), label));
            }
        }
    }

    if !panel.status.is_empty() {
        lines.push(format!("  · {}", panel.status));
    }

    lines.join("\n")
}

/// Generic system: render every `SelectablePanel` entity as `Text`.
pub fn render_selectable_panels(mut query: Query<(&SelectablePanel, &mut Text)>) {
    for (panel, mut text) in &mut query {
        **text = format_selectable_panel_text(panel);
    }
}

pub fn render_info_panels(mut query: Query<(&InfoPanel, &mut Text, &mut Visibility)>) {
    for (panel, mut text, mut vis) in &mut query {
        *vis = Visibility::Visible;
        let mut lines = vec![format!("── {} ──", panel.title)];
        for row in &panel.rows {
            match row {
                PanelRow::Header(h) => lines.push(h.clone()),
                PanelRow::Label(l) => lines.push(format!("  {l}")),
                PanelRow::KeyValue { key, value } => lines.push(format!("  {key}: {value}")),
                PanelRow::Separator => lines.push(String::new()),
            }
        }
        **text = lines.join("\n");
    }
}
