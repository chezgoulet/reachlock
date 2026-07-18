//! Faction reputation UI (S11): the reputation panel (P key) and the
//! faction-tinted HUD banner. The live faction simulation itself lives in
//! `UniverseTicker.state.factions` (S12) — one universe, one clock.

use bevy::prelude::*;

use crate::settings::{InputAction, Settings};
use crate::systems::ticker::UniverseTicker;

/// Toggle for the reputation panel.
#[derive(Resource, Default)]
pub struct ReputationPanelVisible(pub bool);

/// Marker on the reputation panel text entity.
#[derive(Component)]
pub struct ReputationPanel;

/// Marker on the faction banner text entity (tinted by controlling faction).
#[derive(Component)]
pub struct FactionBanner;

/// Toggle the reputation panel on P key press.
pub fn reputation_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<ReputationPanelVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenCrewRoster)) {
        visible.0 = !visible.0;
    }
}

/// Spawn the reputation panel text entity (hidden by default).
pub fn spawn_reputation_panel(mut commands: Commands) {
    commands.spawn((
        ReputationPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.7, 0.95, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Spawn the faction banner (tinted by controlling faction).
pub fn spawn_faction_banner(mut commands: Commands) {
    commands.spawn((
        FactionBanner,
        Text::new(""),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.9, 0.95)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(48.0),
            left: Val::Percent(40.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Update the reputation panel text when visible.
pub fn render_reputation_panel(
    visible: Res<ReputationPanelVisible>,
    ticker: Res<UniverseTicker>,
    mut query: Query<(&mut Text, &mut Visibility), With<ReputationPanel>>,
) {
    let state = &ticker.state.factions;
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── REPUTATION ──  (P to close)".to_string()];
            for f in &state.catalog.factions {
                let rep = state.rep(&f.id);
                let color = f.color;
                let hex = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
                lines.push(format!(
                    "  {hex} {}  trust {}  contribution {}  notoriety {}",
                    f.name,
                    rep.trust / 1024,
                    rep.contribution / 1024,
                    rep.notoriety / 1024,
                ));
            }
            lines.push(format!("tick: {}", state.tick));
            **text = lines.join("\n");
        } else {
            *vis = Visibility::Hidden;
            **text = String::new();
        }
    }
}

/// Update the faction banner tint (color + name of the controlling faction at
/// the current location). Only visible when landed/onboard at a station that
/// has a faction.
pub fn render_faction_banner(
    location: Res<crate::states::CurrentLocation>,
    ticker: Res<UniverseTicker>,
    mut query: Query<(&mut Text, &mut Visibility, &mut TextColor), With<FactionBanner>>,
) {
    let state = &ticker.state.factions;
    if let Ok((mut text, mut vis, mut color)) = query.single_mut() {
        // Find the station's faction from the economy station record.
        let faction_id = ticker
            .state
            .economy
            .stations
            .get(&location.station_id)
            .and_then(|s| s.station_faction.as_ref());
        if let Some(fid) = faction_id {
            if let Some(f) = state.catalog.factions.iter().find(|f| f.id.0 == *fid) {
                let [r, g, b, _a] = f.color;
                color.0 = Color::srgb_u8(r, g, b);
                **text = format!("{} SYSTEM — {}", f.name, location.display_name);
                *vis = Visibility::Visible;
                return;
            }
        }
        // Fallback: no faction → dim banner with just location.
        color.0 = Color::srgb(0.5, 0.55, 0.6);
        **text = location.display_name.clone();
        *vis = if location.display_name.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
    }
}
