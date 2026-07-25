use bevy::prelude::*;

#[derive(Component)]
pub struct ListWidget {
    pub selected: usize,
    pub items: Vec<String>,
}

pub fn spawn_list(commands: &mut Commands, items: &[&str], selected: usize) -> Entity {
    let item_strs: Vec<String> = items.iter().map(|s| s.to_string()).collect();

    let container = commands
        .spawn((
            Node {
                width: Val::Px(300.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.08, 0.08, 0.1)),
            ListWidget {
                selected,
                items: item_strs.clone(),
            },
        ))
        .id();

    for (i, item) in item_strs.iter().enumerate() {
        let is_selected = i == selected;
        let item_id = commands
            .spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(22.0),
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(if is_selected {
                    Color::srgb(0.3, 0.6, 0.9)
                } else {
                    Color::NONE
                }),
                Button,
                Interaction::default(),
            ))
            .with_child((
                Text::new(item.clone()),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.85, 0.9, 0.95)),
            ))
            .id();

        commands.entity(container).add_child(item_id);
    }

    container
}
