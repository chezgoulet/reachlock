use bevy::prelude::*;

#[derive(Component)]
pub struct ToggleValue(pub bool);

#[derive(Component)]
pub struct ToggleWidget;

pub struct ToggleHandle(pub Entity);

pub fn spawn_toggle(commands: &mut Commands, label: &str, value: bool) -> (Entity, ToggleHandle) {
    let track_id = commands
        .spawn((
            Node {
                width: Val::Px(40.0),
                height: Val::Px(20.0),
                justify_content: if value {
                    JustifyContent::FlexEnd
                } else {
                    JustifyContent::FlexStart
                },
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.3, 0.3, 0.35)),
            BackgroundColor(if value {
                Color::srgb(0.3, 0.6, 0.9)
            } else {
                Color::srgb(0.12, 0.12, 0.15)
            }),
            ToggleValue(value),
            ToggleWidget,
            Button,
            Interaction::default(),
        ))
        .with_child((
            Node {
                width: Val::Px(16.0),
                height: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.85, 0.9, 0.95)),
        ))
        .id();

    let handle = ToggleHandle(track_id);

    let row_id = commands
        .spawn((
            Node {
                width: Val::Px(260.0),
                height: Val::Px(24.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .with_child((
            Text::new(label.to_string()),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            TextColor(Color::srgb(0.85, 0.9, 0.95)),
        ))
        .add_child(track_id)
        .id();

    (row_id, handle)
}

pub fn set_toggle(commands: &mut Commands, handle: &ToggleHandle, value: bool) {
    if let Ok(mut entity) = commands.get_entity(handle.0) {
        entity.insert(ToggleValue(value));
    }
}
