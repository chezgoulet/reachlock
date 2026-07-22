//! Story submission (S34). After a deliberation completes, the player can
//! share a one-line anecdote about what happened. Opt-in, rate-limited,
//! stored locally and synced to the server.

use bevy::ecs::message::MessageReader;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use reachlock_core::contract::metadata::ContractStory;

use crate::settings::{InputAction, Settings};
use crate::systems::comms::CommFeed;
use crate::systems::contract::DeliberationState;

const MAX_STORIES_PER_SESSION: u32 = 5;
const MAX_STORY_CHARS: usize = 200;

/// One pending story submission, offered after a significant deliberation.
pub struct PendingStory {
    pub contract_id: String,
    #[allow(dead_code)]
    pub crew_member: String,
    #[allow(dead_code)]
    pub event_summary: String,
    #[allow(dead_code)]
    pub outcome_type: String,
    pub timestamp: u64,
}

/// Story submission state: pending prompt, text input, story log.
#[derive(Resource, Default)]
pub struct StorySubmissionState {
    pub pending: Option<PendingStory>,
    pub typing: bool,
    pub buffer: String,
    pub stories: Vec<ContractStory>,
    pub story_prompt_count: u32,
}

impl StorySubmissionState {
    #[allow(dead_code)]
    pub fn stories_for(&self, contract_id: &str) -> Vec<&ContractStory> {
        self.stories
            .iter()
            .filter(|s| s.contract_id == contract_id)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// System — handles player response to story prompt
// ---------------------------------------------------------------------------

/// Run after deliberation systems. Checks for recently completed deliberation
/// (sets pending story) and handles the "Share this story?" prompt + input.
pub fn story_prompt_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut chars: MessageReader<KeyboardInput>,
    mut submission: ResMut<StorySubmissionState>,
    mut feed: ResMut<CommFeed>,
    mut deliberation: ResMut<DeliberationState>,
) {
    // ---- Detect deliberation just completed ----
    if let Some(crew) = deliberation.just_completed.take() {
        if submission.pending.is_none() && submission.story_prompt_count < MAX_STORIES_PER_SESSION {
            submission.pending = Some(PendingStory {
                contract_id: String::new(),
                crew_member: crew,
                event_summary: "deliberation".into(),
                outcome_type: "deliberation".into(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            });
        }
    }

    let Some(pending) = submission.pending.as_ref() else {
        submission.typing = false;
        chars.clear();
        return;
    };

    if submission.typing {
        // ---- Text input mode (using Bevy's logical Key for Unicode) ----
        if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
            submission.typing = false;
            submission.buffer.clear();
            feed.say("story", "Story discarded. Maybe next time.");
            submission.pending = None;
            return;
        }
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
            let trimmed = submission.buffer.trim().to_string();
            if !trimmed.is_empty() && trimmed.len() <= MAX_STORY_CHARS {
                let story = ContractStory {
                    contract_id: pending.contract_id.clone(),
                    story: trimmed,
                    event_type: pending.event_summary.clone(),
                    outcome_type: pending.outcome_type.clone(),
                    timestamp: pending.timestamp,
                };
                submission.stories.push(story);
                submission.story_prompt_count += 1;
                feed.say("story", "Story saved. Thanks, captain.");
                submission.typing = false;
                submission.buffer.clear();
                submission.pending = None;
            } else if trimmed.len() > MAX_STORY_CHARS {
                feed.say("story", "200 characters max — try shorter.");
            }
            chars.clear();
            return;
        }
        for input in chars.read() {
            if input.state != ButtonState::Pressed {
                continue;
            }
            match &input.logical_key {
                Key::Character(text) => {
                    if submission.buffer.chars().count() < MAX_STORY_CHARS {
                        submission.buffer.push_str(text.as_str());
                    }
                }
                Key::Space => {
                    if submission.buffer.chars().count() < MAX_STORY_CHARS {
                        submission.buffer.push(' ');
                    }
                }
                Key::Backspace => {
                    submission.buffer.pop();
                }
                _ => {}
            }
        }
        return;
    }

    // ---- Prompt visible on comms ----
    if submission.story_prompt_count >= MAX_STORIES_PER_SESSION {
        submission.pending = None;
        return;
    }
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        submission.typing = true;
        submission.buffer.clear();
        feed.say("story", "Type your one-line story, then Enter to confirm:");
    } else if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        feed.say("story", "Story skipped.");
        submission.pending = None;
    } else {
        feed.say("story", "Share this story? [Enter] Yes  [Esc] Not now");
    }
}
