//! Culture view (S47): planet culture summary panel. Shows language, customs,
//! architecture, clothing, attitude, and allegiance. Mirrors the factions
//! panel pattern.

use bevy::prelude::*;

use reachlock_core::generator::culture::PlanetCulture;

use crate::settings::{InputAction, Settings};

/// Panel visibility toggle.
#[derive(Resource, Default)]
pub struct CulturePanelVisible(pub bool);

/// Marker on the culture panel text entity.
#[derive(Component)]
pub struct CulturePanel;

/// The current planet's culture. Populated when entering a planet's orbit or
/// loading a culture override.
#[derive(Resource, Default)]
pub struct CultureResource(pub Option<PlanetCulture>);

/// Toggle on the assigned key.
pub fn culture_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<CulturePanelVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenCrewRoster)) {
        visible.0 = !visible.0;
    }
}

/// Spawn the panel entity (hidden by default).
pub fn spawn_culture_panel(mut commands: Commands) {
    commands.spawn((
        CulturePanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.75, 0.85, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Render the panel when visible.
pub fn render_culture_panel(
    visible: Res<CulturePanelVisible>,
    culture: Res<CultureResource>,
    mut query: Query<(&mut Text, &mut Visibility), With<CulturePanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── CULTURE ──".to_string()];
            match &culture.0 {
                None => {
                    lines.push("  No culture data for this planet.".into());
                }
                Some(c) => {
                    lines.push(format!("  Language: {}", c.language.base_language));
                    lines.push(format!("  Greeting: \"{}\"", c.language.greeting));
                    lines.push(format!("  Farewell: \"{}\"", c.language.farewell));
                    for custom in &c.customs {
                        lines.push(format!(
                            "  {:?}: {} (trigger: {})",
                            custom.custom_type, custom.description, custom.trigger
                        ));
                    }
                    lines.push(format!(
                        "  Social structure: {:?}",
                        c.social_structure
                    ));
                    lines.push(format!(
                        "  Attitude toward outsiders: {:?}",
                        c.attitude_toward_outsiders
                    ));
                    lines.push(format!(
                        "  Architectural style: {}",
                        c.architecture.style_name
                    ));
                    lines.push(format!(
                        "  Clothing: {}",
                        c.clothing.style_name
                    ));
                    let values: Vec<String> =
                        c.dominant_values.iter().map(|v| format!("{:?}", v)).collect();
                    lines.push(format!("  Values: {}", values.join(", ")));
                    lines.push(format!("  Quirk: {}", c.cultural_quirk));
                }
            }
            **text = lines.join("\n");
        } else {
            *vis = Visibility::Hidden;
            **text = String::new();
        }
    }
}
