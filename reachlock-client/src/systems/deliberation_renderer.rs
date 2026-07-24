use bevy::prelude::*;

use crate::systems::contract::DeliberationState;

#[derive(Clone, Debug)]
pub enum DeliberationStatus {
    Thinking,
    Success,
    Failure,
}

#[derive(Clone, Debug)]
pub struct DeliberationTrack {
    pub crew_member_name: String,
    pub context_summary: String,
    pub status: DeliberationStatus,
    pub started_at_ticks: u64,
    pub finished_at: f32,
}

#[derive(Resource, Default)]
pub struct DeliberationRenderState {
    pub tracks: Vec<DeliberationTrack>,
    /// Running total of ticks for animation timing.
    pub tick_counter: u64,
}

#[derive(Component)]
pub struct DeliberationUi;

pub fn render_deliberation(
    time: Res<Time>,
    deliberation: Res<DeliberationState>,
    mut render: ResMut<DeliberationRenderState>,
    mut query: Query<(&mut Text, &mut TextColor), With<DeliberationUi>>,
) {
    render.tick_counter = render.tick_counter.wrapping_add(1);

    if let Some(active) = &deliberation.active {
        if active.overlay_visible {
            let pulse = (time.elapsed_secs() * 3.0).sin() * 0.15 + 0.85;
            let mut lines = Vec::new();
            lines.push(format!("── {} is considering ──", active.crew_member));
            lines.push(String::new());
            lines.push(format!("  \"{}\"", active.context_summary));
            lines.push(String::new());
            lines.push("  ████████░░ thinking".into());
            if let Ok((mut text, mut color)) = query.single_mut() {
                **text = lines.join("\n");
                color.0 = Color::srgb(pulse, 0.85 * pulse, 0.5 * pulse);
            }
            return;
        }
    }

    if let Ok((mut text, _)) = query.single_mut() {
        **text = String::new();
    }
}

pub fn cleanup_completed_deliberations(
    time: Res<Time>,
    deliberation: Res<DeliberationState>,
    mut render: ResMut<DeliberationRenderState>,
) {
    if let Some(ref name) = deliberation.just_completed {
        let tick = render.tick_counter;
        let already = render.tracks.iter().any(|t| t.crew_member_name == *name && matches!(t.status, DeliberationStatus::Success | DeliberationStatus::Failure));
        if !already {
            render.tracks.push(DeliberationTrack {
                crew_member_name: name.clone(),
                context_summary: String::new(),
                status: DeliberationStatus::Success,
                started_at_ticks: tick,
                finished_at: time.elapsed_secs(),
            });
        }
    }

    render.tracks.retain(|t| {
        if matches!(t.status, DeliberationStatus::Success | DeliberationStatus::Failure) {
            time.elapsed_secs() - t.finished_at < 3.0
        } else {
            true
        }
    });
}

pub fn spawn_deliberation_ui(mut commands: Commands) {
    commands.spawn((
        DeliberationUi,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.85, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(40.0),
            left: Val::Percent(35.0),
            max_width: Val::Px(400.0),
            ..default()
        },
    ));
}
