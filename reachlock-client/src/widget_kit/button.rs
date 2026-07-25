use bevy::prelude::*;

#[derive(Component)]
pub struct WidgetButton {
    pub enabled: bool,
}

pub fn spawn_button(commands: &mut Commands, label: &str) -> Entity {
    commands
        .spawn((
            Node {
                width: Val::Px(200.0),
                height: Val::Px(36.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.3, 0.3, 0.35)),
            BackgroundColor(Color::srgb(0.12, 0.12, 0.15)),
            WidgetButton { enabled: true },
            Button,
            Interaction::default(),
        ))
        .with_child((
            Text::new(label.to_string()),
            TextFont {
                font_size: 14.0,
                ..default()
            },
            TextColor(Color::srgb(0.85, 0.9, 0.95)),
        ))
        .id()
}

pub fn update_button_style(
    mut query: Query<(&Interaction, &mut BackgroundColor, &WidgetButton), Changed<Interaction>>,
) {
    for (interaction, mut bg, btn) in query.iter_mut() {
        if !btn.enabled {
            bg.0 = Color::srgb(0.08, 0.08, 0.08);
            continue;
        }
        match *interaction {
            Interaction::Pressed => bg.0 = Color::srgb(0.08, 0.08, 0.1),
            Interaction::Hovered => bg.0 = Color::srgb(0.2, 0.2, 0.25),
            Interaction::None => bg.0 = Color::srgb(0.12, 0.12, 0.15),
        }
    }
}
