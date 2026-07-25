use std::collections::HashMap;

use bevy::prelude::*;

use reachlock_core::generator::storyline::{generate_storyline, StoryChapter};

use crate::systems::ticker::UniverseTicker;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorylineProgress {
    pub chapter_index: u32,
    pub chapters: Vec<StoryChapter>,
    pub last_triggered_tick: u64,
}

#[derive(Resource, Default)]
pub struct StorylineState(pub HashMap<String, StorylineProgress>);

#[derive(Resource, Default)]
pub struct StorylineNotifications(pub Vec<String>);

pub fn storyline_driver_system(
    ticker: Res<UniverseTicker>,
    mut state: ResMut<StorylineState>,
    mut notifications: ResMut<StorylineNotifications>,
) {
    let tick = ticker.state.factions.tick;
    for faction in &ticker.state.factions.catalog.factions {
        let rep = ticker.state.factions.rep(&faction.id);
        let faction_id = faction.id.as_str().to_string();
        let progress = state
            .0
            .entry(faction_id.clone())
            .or_insert(StorylineProgress {
                chapter_index: 0,
                chapters: Vec::new(),
                last_triggered_tick: 0,
            });
        if tick < progress.last_triggered_tick + 100 {
            continue;
        }
        let trust_level = rep.trust / reachlock_core::faction::REP_ONE;
        let target_chapter = if trust_level >= 80 {
            5u32
        } else if trust_level >= 50 {
            4
        } else if trust_level >= 20 {
            3
        } else if trust_level >= 0 {
            2
        } else {
            1
        }
        .max(1);
        if target_chapter > progress.chapter_index && progress.chapters.is_empty() {
            let seed = faction_id
                .as_str()
                .chars()
                .fold(0u64, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u64));
            let chapters = generate_storyline(seed ^ target_chapter as u64, target_chapter.max(3));
            progress.chapters = chapters;
        }
        if target_chapter > progress.chapter_index && !progress.chapters.is_empty() {
            let next_idx = progress.chapter_index;
            if let Some(chapter) = progress.chapters.get(next_idx as usize) {
                notifications.0.push(format!(
                    "Storyline advanced: {} — {}",
                    faction.name, chapter.title
                ));
                progress.chapter_index = target_chapter.min(progress.chapters.len() as u32);
                progress.last_triggered_tick = tick;
            }
        }
        if target_chapter <= progress.chapter_index {
            progress.last_triggered_tick = tick;
        }
    }
    if notifications.0.len() > 20 {
        let excess = notifications.0.len() - 20;
        notifications.0.drain(0..excess);
    }
}

#[expect(dead_code)]
pub fn render_storyline_log(
    state: Res<StorylineState>,
    visible: Res<StorylineLogVisible>,
) -> Option<String> {
    if !visible.0 {
        return None;
    }
    if state.0.is_empty() {
        return Some("No storylines active yet.".into());
    }
    let mut s = String::new();
    for (faction_id, progress) in &state.0 {
        s.push_str(&format!("--- {} ---\n", faction_id));
        for i in 0..progress.chapters.len().min(5) {
            if let Some(ch) = progress.chapters.get(i) {
                let marker = if (i as u32) < progress.chapter_index {
                    "✓"
                } else if (i as u32) == progress.chapter_index {
                    "▶"
                } else {
                    " "
                };
                s.push_str(&format!(" {} {}: {}\n", marker, i + 1, ch.title));
            }
        }
        s.push('\n');
    }
    Some(s)
}

#[derive(Resource, Default)]
#[allow(dead_code)]
pub struct StorylineLogVisible(pub bool);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_no_crash() {
        let state = StorylineState(HashMap::new());
        let visible = StorylineLogVisible(true);
        let text = render_storyline_log_impl(&state, &visible);
        assert!(text.is_some());
    }

    fn render_storyline_log_impl(
        state: &StorylineState,
        visible: &StorylineLogVisible,
    ) -> Option<String> {
        if !visible.0 {
            return None;
        }
        if state.0.is_empty() {
            return Some("No storylines active yet.".into());
        }
        let mut s = String::new();
        for (faction_id, progress) in &state.0 {
            s.push_str(&format!("--- {} ---\n", faction_id));
            for i in 0..progress.chapters.len().min(5) {
                if let Some(ch) = progress.chapters.get(i) {
                    let marker = if (i as u32) < progress.chapter_index {
                        "✓"
                    } else if (i as u32) == progress.chapter_index {
                        "▶"
                    } else {
                        " "
                    };
                    s.push_str(&format!(" {} {}: {}\n", marker, i + 1, ch.title));
                }
            }
            s.push('\n');
        }
        Some(s)
    }
}
