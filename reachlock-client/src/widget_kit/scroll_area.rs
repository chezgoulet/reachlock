use bevy::prelude::*;

#[derive(Component)]
pub struct ScrollArea {
    pub content_height: f32,
    pub scroll_offset: f32,
}

pub fn spawn_scroll_area(commands: &mut Commands, children: &[Entity]) -> Entity {
    let container = commands
        .spawn((
            Node {
                width: Val::Px(300.0),
                height: Val::Px(200.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::srgb(0.06, 0.06, 0.08)),
            ScrollArea {
                content_height: 0.0,
                scroll_offset: 0.0,
            },
        ))
        .id();

    for child in children {
        commands.entity(container).add_child(*child);
    }

    container
}
