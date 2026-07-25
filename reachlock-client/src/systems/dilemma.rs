use bevy::prelude::*;

use reachlock_core::generator::dilemma::{generate_dilemma, ConsequenceKind, Dilemma};

use crate::settings::{InputAction, Settings};
use crate::states::CurrentLocation;
use crate::systems::interaction::ActivePanel;

pub const DILEMMA_COOLDOWN_SECS: f64 = 150.0;

#[derive(Resource)]
pub struct DilemmaCooldown(pub Timer);

impl Default for DilemmaCooldown {
    fn default() -> Self {
        let mut t = Timer::from_seconds(DILEMMA_COOLDOWN_SECS as f32, TimerMode::Once);
        t.finish();
        DilemmaCooldown(t)
    }
}

#[derive(Resource, Default)]
pub struct ActiveDilemma(pub Option<Dilemma>);

#[derive(Resource, Default)]
pub struct DilemmaOutcomeText(pub Option<String>);

#[derive(Resource, Default)]
pub struct DilemmaChoiceSelected(pub Option<usize>);

pub fn dilemma_trigger_system(
    mut cooldown: ResMut<DilemmaCooldown>,
    mut active: ResMut<ActiveDilemma>,
    mut panel: ResMut<ActivePanel>,
    location: Res<CurrentLocation>,
    time: Res<Time>,
) {
    cooldown.0.tick(time.delta());
    if active.0.is_some() {
        return;
    }
    if !cooldown.0.is_finished() {
        return;
    }
    let is_frontier = matches!(
        location.system_biome,
        reachlock_core::seed::types::Biome::Frontier
    );
    let relationship_count = 5u32;
    let faction_diversity = 3u32;
    let seed = location
        .system_seed
        .wrapping_add(cooldown.0.elapsed_secs() as u64);
    if let Some(dilemma) =
        generate_dilemma(seed, is_frontier, relationship_count, faction_diversity)
    {
        active.0 = Some(dilemma);
        *panel = ActivePanel::Dilemma;
        cooldown.0.reset();
    }
}

pub fn dilemma_input_system(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut active: ResMut<ActiveDilemma>,
    mut outcome: ResMut<DilemmaOutcomeText>,
    mut selected: ResMut<DilemmaChoiceSelected>,
    mut panel: ResMut<ActivePanel>,
    focus_stack: Res<crate::focus_stack::FocusStack>,
) {
    if focus_stack.top_captures_input() { return; }
    let Some(ref dilemma) = active.0 else { return };
    if outcome.0.is_some() {
        if keys.just_pressed(KeyCode::Space)
            || keys.just_pressed(KeyCode::Enter)
            || keys.just_pressed(settings.key(InputAction::Interact))
        {
            outcome.0 = None;
            active.0 = None;
            *panel = ActivePanel::None;
        }
        return;
    }
    for i in 0..dilemma.choices.len().min(9) {
        let digit = match i {
            0 => KeyCode::Digit1,
            1 => KeyCode::Digit2,
            2 => KeyCode::Digit3,
            3 => KeyCode::Digit4,
            4 => KeyCode::Digit5,
            5 => KeyCode::Digit6,
            6 => KeyCode::Digit7,
            7 => KeyCode::Digit8,
            _ => KeyCode::Digit9,
        };
        if keys.just_pressed(digit) {
            selected.0 = Some(i);
        }
    }
    if let Some(choice_idx) = selected.0 {
        if keys.just_pressed(KeyCode::Enter)
            || keys.just_pressed(settings.key(InputAction::EditorConfirm))
        {
            if let Some(choice) = dilemma.choices.get(choice_idx) {
                let mut lines: Vec<String> = Vec::new();
                for c in &choice.consequences {
                    lines.push(format!(
                        "  • {}: {}.",
                        describe_consequence_kind(c.kind),
                        c.target
                    ));
                }
                outcome.0 = Some(format!(
                    "You chose: {}\n{}\n{}",
                    choice.label,
                    choice.description,
                    lines.join("\n")
                ));
                selected.0 = None;
            }
        }
        if keys.just_pressed(KeyCode::Escape) {
            selected.0 = None;
        }
    }
}

fn describe_consequence_kind(kind: ConsequenceKind) -> &'static str {
    match kind {
        ConsequenceKind::CrewTrustChanged => "Crew trust changed",
        ConsequenceKind::FactionReputationChanged => "Faction reputation changed",
        ConsequenceKind::PopulationChanged => "Population changed",
        ConsequenceKind::ResourceGained => "Resource gained",
        ConsequenceKind::ResourceLost => "Resource lost",
        ConsequenceKind::CrewMemberQuits => "Crew member quits",
        ConsequenceKind::NewMissionUnlocked => "New mission unlocked",
        ConsequenceKind::StoryArcProgressed => "Story arc progressed",
        ConsequenceKind::Nothing => "No immediate effect",
    }
}

pub fn dilemma_panel_text(
    active: Res<ActiveDilemma>,
    outcome: Res<DilemmaOutcomeText>,
    selected: Res<DilemmaChoiceSelected>,
) -> Option<String> {
    let dilemma = active.0.as_ref()?;
    if let Some(ref text) = outcome.0 {
        return Some(format!("== OUTCOME ==\n{}\n\n[Space/Enter] Close", text));
    }
    let urgency = match dilemma.setup.urgency {
        reachlock_core::generator::dilemma::DilemmaUrgency::Immediate => "IMMEDIATE",
        reachlock_core::generator::dilemma::DilemmaUrgency::Pressing => "PRESSING",
        reachlock_core::generator::dilemma::DilemmaUrgency::Looming => "LOOMING",
        reachlock_core::generator::dilemma::DilemmaUrgency::Background => "BACKGROUND",
    };
    let mut s = format!(
        "══ {} ══\n{}[ {} ]\n\n",
        dilemma.setup.title, dilemma.setup.narrative, urgency
    );
    for (i, choice) in dilemma.choices.iter().enumerate() {
        let marker = if selected.0 == Some(i) { ">" } else { " " };
        s.push_str(&format!(
            "{} {}: {}\n   {}\n",
            marker,
            i + 1,
            choice.label,
            choice.description
        ));
    }
    if selected.0.is_some() {
        s.push_str("\n[Enter] Confirm  [Esc] Cancel");
    } else {
        s.push_str("\n[1-9] Select choice");
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_default_is_finished() {
        let cd = DilemmaCooldown::default();
        assert!(cd.0.is_finished());
    }
}
