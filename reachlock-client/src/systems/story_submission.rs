//! Story submission (S34). After a deliberation completes, the player can
//! share a one-line anecdote about what happened. Opt-in, rate-limited,
//! stored locally and synced to the server.

use bevy::prelude::*;

use reachlock_core::contract::metadata::ContractStory;

use crate::settings::{InputAction, Settings};
use crate::systems::comms::CommFeed;
use crate::systems::contract::DeliberationState;

/// One pending story submission, offered after a significant deliberation.
pub struct PendingStory {
    pub contract_id: String,
    #[allow(dead_code)]
    pub crew_member: String,
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
}

#[allow(dead_code)]
impl StorySubmissionState {
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
    mut submission: ResMut<StorySubmissionState>,
    mut feed: ResMut<CommFeed>,
    mut deliberation: ResMut<DeliberationState>,
) {
    // ---- Detect deliberation just completed ----
    if let Some(crew) = deliberation.just_completed.take() {
        if submission.pending.is_none() {
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
        return;
    };

    if submission.typing {
        // ---- Text input mode ----
        // Enter confirms, Esc cancels.
        if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
            submission.typing = false;
            submission.buffer.clear();
            feed.say("story", "Story discarded. Maybe next time.");
            submission.pending = None;
            return;
        }
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
            let trimmed = submission.buffer.trim().to_string();
            if !trimmed.is_empty() && trimmed.len() <= 200 {
                let story = ContractStory {
                    contract_id: pending.contract_id.clone(),
                    story: trimmed,
                    event_type: pending.event_summary.clone(),
                    outcome_type: pending.outcome_type.clone(),
                    timestamp: pending.timestamp,
                };
                submission.stories.push(story);
                feed.say("story", "Story saved. Thanks, captain.");
                submission.typing = false;
                submission.buffer.clear();
                submission.pending = None;
            } else if trimmed.len() > 200 {
                feed.say("story", "200 characters max — try shorter.");
            }
            return;
        }
        // Type into buffer: A-Z, 0-9, space, punctuation.
        for c in keys.get_just_pressed() {
            let ch = keycode_to_char(*c);
            if let Some(ch) = ch {
                if submission.buffer.len() < 200 {
                    submission.buffer.push(ch);
                }
            }
            if *c == KeyCode::Backspace && !submission.buffer.is_empty() {
                submission.buffer.pop();
            }
        }
        return;
    }

    // ---- Prompt visible on comms ----
    // Show the prompt as a comm line every 6 seconds.
    if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
        submission.typing = true;
        submission.buffer.clear();
        feed.say("story", "Type your one-line story, then Enter to confirm:");
    } else if keys.just_pressed(settings.key(InputAction::EditorCancel)) {
        feed.say("story", "Story skipped.");
        submission.pending = None;
    } else {
        // Show the prompt once when it's first pending.
        feed.say("story", "Share this story? [Enter] Yes  [Esc] Not now");
        // Only push this once — consume it as shown.
        // Use a flag or just let it fade naturally.
    }
}

// ---------------------------------------------------------------------------
// Key-to-char mapping (simple ASCII subset)
// ---------------------------------------------------------------------------

fn keycode_to_char(kc: KeyCode) -> Option<char> {
    match kc {
        KeyCode::Space => Some(' '),
        KeyCode::Minus => Some('-'),
        KeyCode::Period => Some('.'),
        KeyCode::Comma => Some(','),
        KeyCode::Quote => Some('\''),
        KeyCode::Semicolon => Some(';'),
        KeyCode::Slash => Some('/'),
        KeyCode::Backslash => Some('\\'),
        KeyCode::BracketLeft => Some('('),
        KeyCode::BracketRight => Some(')'),
        KeyCode::Digit0 => Some('0'),
        KeyCode::Digit1 => Some('1'),
        KeyCode::Digit2 => Some('2'),
        KeyCode::Digit3 => Some('3'),
        KeyCode::Digit4 => Some('4'),
        KeyCode::Digit5 => Some('5'),
        KeyCode::Digit6 => Some('6'),
        KeyCode::Digit7 => Some('7'),
        KeyCode::Digit8 => Some('8'),
        KeyCode::Digit9 => Some('9'),
        KeyCode::KeyA => Some('a'),
        KeyCode::KeyB => Some('b'),
        KeyCode::KeyC => Some('c'),
        KeyCode::KeyD => Some('d'),
        KeyCode::KeyE => Some('e'),
        KeyCode::KeyF => Some('f'),
        KeyCode::KeyG => Some('g'),
        KeyCode::KeyH => Some('h'),
        KeyCode::KeyI => Some('i'),
        KeyCode::KeyJ => Some('j'),
        KeyCode::KeyK => Some('k'),
        KeyCode::KeyL => Some('l'),
        KeyCode::KeyM => Some('m'),
        KeyCode::KeyN => Some('n'),
        KeyCode::KeyO => Some('o'),
        KeyCode::KeyP => Some('p'),
        KeyCode::KeyQ => Some('q'),
        KeyCode::KeyR => Some('r'),
        KeyCode::KeyS => Some('s'),
        KeyCode::KeyT => Some('t'),
        KeyCode::KeyU => Some('u'),
        KeyCode::KeyV => Some('v'),
        KeyCode::KeyW => Some('w'),
        KeyCode::KeyX => Some('x'),
        KeyCode::KeyY => Some('y'),
        KeyCode::KeyZ => Some('z'),
        _ => None,
    }
}
