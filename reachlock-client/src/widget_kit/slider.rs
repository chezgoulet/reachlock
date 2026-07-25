use bevy::prelude::*;

#[derive(Component)]
pub struct SliderValue(pub f32);

pub struct SliderHandle(pub Entity);

pub fn spawn_slider(
    commands: &mut Commands,
    label: &str,
    min: f32,
    max: f32,
    value: f32,
) -> (Entity, SliderHandle) {
    let pct = ((value - min) / (max - min)).clamp(0.0, 1.0);

    let fill_id = commands
        .spawn((
            Node {
                width: Val::Percent(pct * 100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::srgb(0.3, 0.6, 0.9)),
        ))
        .id();

    let track_id = commands
        .spawn((
            Node {
                width: Val::Px(150.0),
                height: Val::Px(12.0),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.3, 0.3, 0.35)),
            BackgroundColor(Color::srgb(0.12, 0.12, 0.15)),
            SliderValue(value),
            Button,
            Interaction::default(),
        ))
        .add_child(fill_id)
        .id();

    let handle = SliderHandle(track_id);

    let row_id = commands
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
            Text::new(format!("{}: {:.2}", label, value)),
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
