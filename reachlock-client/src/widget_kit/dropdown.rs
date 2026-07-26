use crate::theme;
use bevy::prelude::*;

#[derive(Component)]
pub struct DropdownWidget {
    pub options: Vec<String>,
    pub selected: usize,
}

pub fn spawn_dropdown(
    commands: &mut Commands,
    label: &str,
    options: &[&str],
    selected: usize,
) -> Entity {
    let opts: Vec<String> = options.iter().map(|s| s.to_string()).collect();
    let current = opts.get(selected).cloned().unwrap_or_default();

    commands
        .spawn((
            Node {
                width: Val::Px(300.0),
                height: Val::Px(24.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_child((
            Text::new(format!("{}:", label)),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            theme::fg("text"),
        ))
        .with_child((
            Node {
                width: Val::Px(120.0),
                height: Val::Px(24.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            theme::surface("surface.control"),
            DropdownWidget {
                options: opts.clone(),
                selected,
            },
            Button,
            Interaction::default(),
        ))
        .with_child((
            Text::new(current),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            theme::fg("text"),
        ))
        .id()
}
