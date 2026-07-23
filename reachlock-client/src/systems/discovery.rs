//! Discovery log (S39): species cards panel and ecosystem summary.
//! Mirrors the factions/career panel pattern.

use bevy::prelude::*;

use reachlock_core::generator::Ecosystem;

use crate::settings::{InputAction, Settings};

/// Panel visibility toggle.
#[derive(Resource, Default)]
pub struct DiscoveryPanelVisible(pub bool);

/// Marker on the discovery panel text entity.
#[derive(Component)]
pub struct DiscoveryPanel;

/// The current planet's ecosystem. Populated when the player scans a
/// habitable planet or an ecosystem override is loaded.
#[derive(Resource, Default)]
pub struct EcosystemResource(pub Option<Ecosystem>);

/// Toggle on the assigned key.
pub fn discovery_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<DiscoveryPanelVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenCrewRoster)) {
        visible.0 = !visible.0;
    }
}

/// Spawn the panel entity (hidden by default).
pub fn spawn_discovery_panel(mut commands: Commands) {
    commands.spawn((
        DiscoveryPanel,
        Text::new(""),
        TextFont {
            font_size: 13.0,
            ..default()
        },
        TextColor(Color::srgb(0.6, 0.9, 0.7)),
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
pub fn render_discovery_panel(
    visible: Res<DiscoveryPanelVisible>,
    ecosystem: Res<EcosystemResource>,
    mut query: Query<(&mut Text, &mut Visibility), With<DiscoveryPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── DISCOVERY LOG ──".to_string()];
            match &ecosystem.0 {
                None => {
                    lines.push("  No planet scanned yet.".into());
                }
                Some(eco) => {
                    lines.push(format!(
                        "  Complexity: {:?} — {} species across {} biome(s)",
                        eco.ecological_complexity,
                        eco.global_species_count,
                        eco.biomes.len(),
                    ));
                    for biome in &eco.biomes {
                        lines.push(format!("  Biome: {:?}", biome.biome));
                        for sp in &biome.species {
                            let scanned = if sp.discoverable {
                                format!("{:?}", sp.common_name)
                            } else {
                                "?".to_string()
                            };
                            lines.push(format!(
                                "    {} ({:?}, {:?})",
                                scanned, sp.ecological_role, sp.rarity,
                            ));
                        }
                    }
                }
            }
            **text = lines.join("\n");
        } else {
            *vis = Visibility::Hidden;
            **text = String::new();
        }
    }
}
