use crate::theme;
use bevy::prelude::*;

#[derive(Component)]
pub struct TooltipTarget {
    pub text: String,
}

#[derive(Component)]
pub struct TooltipOverlay;

#[derive(Resource, Default)]
pub struct TooltipTimer {
    pub hover_start: Option<f32>,
}

pub fn tooltip_system(
    time: Res<Time>,
    mut timer: ResMut<TooltipTimer>,
    mut commands: Commands,
    targets: Query<(Entity, &TooltipTarget, Ref<Interaction>)>,
    overlays: Query<Entity, With<TooltipOverlay>>,
) {
    let mut hovered = false;

    for (_entity, target, interaction) in targets.iter() {
        if interaction.is_changed() && *interaction == Interaction::Hovered {
            timer.hover_start = Some(time.elapsed_secs());
        }

        if *interaction == Interaction::Hovered {
            if let Some(start) = timer.hover_start {
                if time.elapsed_secs() - start >= 0.5 {
                    if overlays.is_empty() {
                        commands
                            .spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    padding: UiRect::all(Val::Px(4.0)),
                                    ..default()
                                },
                                theme::surface("surface"),
                                TooltipOverlay,
                            ))
                            .with_child((
                                Text::new(target.text.clone()),
                                TextFont {
                                    font_size: 11.0,
                                    ..default()
                                },
                                theme::fg("text"),
                            ));
                    }
                    hovered = true;
                }
            }
        }
    }

    if !hovered {
        for e in overlays.iter() {
            commands.entity(e).despawn();
        }
        timer.hover_start = None;
    }
}
