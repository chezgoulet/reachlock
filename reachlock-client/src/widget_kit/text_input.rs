use crate::theme;
use bevy::prelude::*;

#[derive(Component)]
pub struct TextInputWidget {
    pub value: String,
    pub focused: bool,
}

pub fn spawn_text_input(commands: &mut Commands, label: &str, value: &str) -> Entity {
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
                width: Val::Px(160.0),
                height: Val::Px(22.0),
                align_items: AlignItems::Center,
                padding: UiRect::horizontal(Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            theme::surface("surface.control"),
            TextInputWidget {
                value: value.to_string(),
                focused: false,
            },
            Button,
            Interaction::default(),
        ))
        .with_child((
            Text::new(value.to_string()),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            theme::fg("text"),
        ))
        .id()
}
