use bevy::prelude::*;

#[derive(Component)]
pub struct Focusable {
    pub order: usize,
}

#[derive(Component)]
pub struct FocusHighlight;

#[derive(Resource, Default)]
pub struct FocusRing {
    pub focused: Option<Entity>,
}

pub fn collect_focusables(
    mut ring: ResMut<FocusRing>,
    query: Query<Entity, (With<Button>, With<Focusable>)>,
) {
    let mut entities: Vec<Entity> = query.iter().collect();
    if entities.is_empty() {
        return;
    }
    entities.sort();
    if ring.focused.is_none() || !query.iter().any(|e| Some(e) == ring.focused) {
        ring.focused = Some(entities[0]);
    }
}

pub fn advance_focus(
    mut ring: ResMut<FocusRing>,
    keys: Res<ButtonInput<KeyCode>>,
    query: Query<(Entity, &Focusable)>,
) {
    if !keys.just_pressed(KeyCode::Tab) {
        return;
    }
    let mut entities: Vec<(Entity, usize)> = query.iter().map(|(e, f)| (e, f.order)).collect();
    entities.sort_by_key(|(_, order)| *order);
    if entities.is_empty() {
        return;
    }
    let current = ring.focused;
    let pos = entities.iter().position(|(e, _)| Some(*e) == current);
    let next = match pos {
        Some(i) => (i + 1) % entities.len(),
        None => 0,
    };
    ring.focused = Some(entities[next].0);
}

pub fn highlight_focused(
    mut commands: Commands,
    ring: Res<FocusRing>,
    highlights: Query<Entity, With<FocusHighlight>>,
    all_buttons: Query<Entity, With<Button>>,
) {
    for e in highlights.iter() {
        commands.entity(e).remove::<FocusHighlight>();
    }
    if let Some(focused) = ring.focused {
        if all_buttons.iter().any(|e| e == focused) {
            commands.entity(focused).insert(FocusHighlight);
        }
    }
}
