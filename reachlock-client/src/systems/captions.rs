use std::collections::VecDeque;

use bevy::prelude::*;

use crate::settings::Settings;
use crate::theme;

const MAX_CAPTION_LINES: usize = 3;
// Fade timings for a caption transition the overlay does not animate yet.
#[allow(dead_code)]
const FADE_IN_MS: f32 = 200.0;
const MIN_DISPLAY_MS: f32 = 2000.0;
const FADE_OUT_MS: f32 = 500.0;

/// A single caption line.
#[derive(Clone)]
pub struct CaptionLine {
    pub speaker: String,
    pub text: String,
    pub duration_ms: f32,
    pub start_time: f64,
}

/// Queue of pending caption lines.
#[derive(Resource, Default)]
pub struct CaptionQueue {
    pub lines: VecDeque<CaptionLine>,
}

/// Marker for the captions overlay text entity.
#[derive(Component)]
pub struct CaptionsOverlay;

/// Spawn the captions overlay (bottom-center, hidden by default).
pub fn spawn_captions_overlay(mut commands: Commands) {
    commands.spawn((
        CaptionsOverlay,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        theme::fg("text"),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(40.0),
            left: Val::Percent(10.0),
            width: Val::Percent(80.0),
            ..default()
        },
    ));
}

/// Push a caption line programmatically (from voice chat or NPC speech).
pub fn push_caption(queue: &mut CaptionQueue, speaker: &str, text: &str, duration_ms: f32) {
    let line = CaptionLine {
        speaker: speaker.to_string(),
        text: text.to_string(),
        duration_ms: duration_ms.max(MIN_DISPLAY_MS),
        start_time: 0.0, // set when displayed
    };
    if queue.lines.len() >= MAX_CAPTION_LINES {
        queue.lines.pop_front();
    }
    queue.lines.push_back(line);
}

/// Turn NPC dialogue beats into captions.
///
/// `push_caption` had no callers, so the whole captions feature — overlay,
/// queue, the `subtitles` and `subtitle_size` settings — was unreachable no
/// matter what the player set.
///
/// This observes the dialogue session rather than being called from
/// `resolve_dialogue_response`, which is a plain function on the network poll
/// path with no access to the queue. Watching the line is also what makes
/// authored, deflected and generated beats all caption identically.
pub fn caption_dialogue_lines(
    session: Res<crate::systems::dialogue::DialogueSession>,
    souls: Res<crate::systems::soul::SoulRegistry>,
    mut queue: ResMut<CaptionQueue>,
    mut last_line: Local<String>,
) {
    let Some(active) = &session.active else {
        if !last_line.is_empty() {
            last_line.clear();
        }
        return;
    };
    if active.npc_line.is_empty() || *last_line == active.npc_line {
        return;
    }
    *last_line = active.npc_line.clone();
    let speaker = souls
        .files
        .get(&active.soul_id)
        .map(|f| f.name.clone())
        .unwrap_or_else(|| active.soul_id.clone());
    push_caption(&mut queue, &speaker, &active.npc_line, MIN_DISPLAY_MS);
}

/// Drive the captions overlay: read queue, render visible lines, respect
/// subtitles toggle, subtitle_size, text_scale, and high_contrast_ui.
pub fn update_captions(
    time: Res<Time>,
    settings: Res<Settings>,
    mut queue: ResMut<CaptionQueue>,
    mut query: Query<
        (&mut Text, &mut TextFont, &mut TextColor, &mut Visibility),
        With<CaptionsOverlay>,
    >,
) {
    if !settings.accessibility.subtitles {
        if let Ok((_, _, _, mut vis)) = query.single_mut() {
            *vis = Visibility::Hidden;
        }
        return;
    }

    let Ok((mut text, mut font, mut color, mut vis)) = query.single_mut() else {
        return;
    };

    *vis = Visibility::Visible;
    font.font_size =
        16.0 * settings.accessibility.subtitle_size * settings.accessibility.text_scale;

    let now = time.elapsed_secs_f64();
    let mut lines_out: Vec<String> = Vec::new();

    // Mark start times for new lines
    for line in &mut queue.lines {
        if line.start_time == 0.0 {
            line.start_time = now;
        }
    }

    // Remove expired lines
    let elapsed = now;
    queue.lines.retain(|line| {
        let age_ms = (elapsed - line.start_time) * 1000.0;
        age_ms < (line.duration_ms * 1.5 + FADE_OUT_MS) as f64
    });

    for line in &queue.lines {
        let line_text = format!("[{}] {}", line.speaker, line.text);
        lines_out.push(line_text);
    }

    **text = lines_out.join("\n");

    if settings.accessibility.high_contrast_ui {
        color.0 = Color::srgb(0.1, 0.1, 0.1);
    } else {
        color.0 = Color::srgb(0.9, 0.9, 0.9);
    }
}
