use std::path::PathBuf;

use bevy::prelude::*;

use crate::settings::Settings;
use reachlock_core::contract::stage::{DeliberationStage, RuleSnapshot, StagePhase};

const ONBOARDING_FLAG: &str = "save/onboarding_completed.flag";

#[derive(Resource, Default)]
pub struct OnboardingState {
    pub active: bool,
    pub step: usize,
    pub steps: Vec<OnboardingStep>,
}

pub struct OnboardingStep {
    pub title: String,
    pub body: String,
    pub demo_stage: Option<DeliberationStage>,
}

#[derive(Component)]
pub struct OnboardingOverlay;

#[derive(Component)]
pub struct OnboardingText;

fn onboarding_completed_flag_path() -> PathBuf {
    PathBuf::from(ONBOARDING_FLAG)
}

pub fn check_first_run(settings: Res<Settings>, mut state: ResMut<OnboardingState>) {
    if !settings.gameplay.show_tutorial_hints {
        return;
    }
    if state.active || state.step > 0 {
        return;
    }
    let flag = onboarding_completed_flag_path();
    if flag.exists() {
        return;
    }
    state.active = true;
    state.steps = default_steps();
    state.step = 0;
}

pub fn advance_onboarding(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<OnboardingState>) {
    if !state.active {
        return;
    }
    if !keys.just_pressed(KeyCode::Enter) && !keys.just_pressed(KeyCode::Tab) {
        return;
    }
    if state.step + 1 >= state.steps.len() {
        state.active = false;
        state.step = 0;
        let _ = std::fs::write(onboarding_completed_flag_path(), "1\n");
        return;
    }
    state.step += 1;
}

fn default_steps() -> Vec<OnboardingStep> {
    vec![
        OnboardingStep {
            title: "Welcome to ReachLock".into(),
            body: "This is your ship's auto-helm console. The rules you write here control your ship when you're not at the controls.".into(),
            demo_stage: None,
        },
        OnboardingStep {
            title: "How Rules Work".into(),
            body: "Each rule says: IF [condition] THEN [action]. Your crew follows these rules. If no rule covers a situation, the crew deliberates.".into(),
            demo_stage: None,
        },
        OnboardingStep {
            title: "Watching Deliberation".into(),
            body: "Watch what happens when a situation isn't covered.".into(),
            demo_stage: Some(demo_deliberation_stage()),
        },
        OnboardingStep {
            title: "Interjection".into(),
            body: "You can interject at any time. The crew reacts. Override too often and they stop trusting your judgment. Let them decide and they grow.".into(),
            demo_stage: None,
        },
        OnboardingStep {
            title: "Gaps & Improvement".into(),
            body: "Your rules left gaps this cycle. Write tighter rules and your crew deliberates less, decides faster, and trusts your vision.".into(),
            demo_stage: None,
        },
        OnboardingStep {
            title: "Ready to Fly".into(),
            body: "That's the core loop. Write rules. Watch them run. Fill the gaps. You're in command.".into(),
            demo_stage: None,
        },
    ]
}

fn demo_deliberation_stage() -> DeliberationStage {
    DeliberationStage {
        phase: StagePhase::Weighing,
        crew_member: "the engineer".into(),
        crew_portrait_id: "boris_default".into(),
        crew_mood: "ANXIOUS".into(),
        trust_with_player: 256,
        context_summary: "Unknown signal detected — none of my rules cover it.".into(),
        matched_rules: vec![],
        unmatched_rules: vec![
            RuleSnapshot {
                index: 0,
                label: "fuel_warning".into(),
                action: "warn_low_fuel".into(),
                condition_summary: "fuel < 15%".into(),
                matched: false,
            },
            RuleSnapshot {
                index: 1,
                label: "maintain_course".into(),
                action: "hold_steady".into(),
                condition_summary: "unknown_signal == 0".into(),
                matched: false,
            },
        ],
        verdict: None,
        cost: None,
        recent_uncovered: 2,
        remaining_secs: 0.0,
        total_secs: 4.0,
    }
}

pub fn spawn_onboarding_overlay(state: Res<OnboardingState>, mut commands: Commands) {
    if !state.active {
        return;
    }
    commands.spawn((
        OnboardingOverlay,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.05, 0.05, 0.1)),
        ZIndex(1000),
    ));
    commands.spawn((
        OnboardingText,
        Text::new(""),
        TextFont {
            font_size: 22.0,
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.9, 1.0)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(30.0),
            left: Val::Percent(20.0),
            max_width: Val::Px(600.0),
            ..default()
        },
    ));
}

pub fn render_onboarding(
    state: Res<OnboardingState>,
    mut query: Query<&mut Text, With<OnboardingText>>,
) {
    if !state.active {
        return;
    }
    if let Ok(mut text) = query.single_mut() {
        let step = &state.steps[state.step];
        let mut content = format!("{}\n\n{}", step.title, step.body);
        if step.demo_stage.is_some() {
            content.push_str("\n\n(Demonstration deliberation would render here)");
        }
        content.push_str(&format!(
            "\n\nStep {} of {} — [Enter/Tab] continue",
            state.step + 1,
            state.steps.len()
        ));
        **text = content;
    }
}

pub fn despawn_onboarding(
    mut commands: Commands,
    overlays: Query<Entity, With<OnboardingOverlay>>,
    texts: Query<Entity, With<OnboardingText>>,
) {
    for entity in &overlays {
        commands.entity(entity).despawn();
    }
    for entity in &texts {
        commands.entity(entity).despawn();
    }
}
