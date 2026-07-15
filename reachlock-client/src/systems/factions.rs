//! Faction state, reputation, and UI (S11). Mirrors the core `FactionState`
//! as a Bevy resource, ticks it each frame, and provides the reputation panel
//! (P key) and faction-tinted HUD banners.

use bevy::prelude::*;

use reachlock_core::faction::{
    load_faction_catalog, tick_factions as core_tick, FactionState as CoreFactionState,
};

/// The live faction simulation, ticked every frame from the canon catalog.
/// Offline-safe: the embedded RON is always available.
#[derive(Resource)]
pub struct FactionState(pub CoreFactionState);

/// Toggle for the reputation panel.
#[derive(Resource, Default)]
pub struct ReputationPanelVisible(pub bool);

/// Marker on the reputation panel text entity.
#[derive(Component)]
pub struct ReputationPanel;

/// Marker on the faction banner text entity (tinted by controlling faction).
#[derive(Component)]
pub struct FactionBanner;

/// Initialise the faction state from the embedded canon catalog.
pub fn init_faction_state(mut commands: Commands) {
    let catalog = load_faction_catalog();
    let state = CoreFactionState::new(catalog);
    commands.insert_resource(FactionState(state));
    commands.insert_resource(ReputationPanelVisible(false));
}

/// Tick the faction engine each frame (deterministic from the same state).
/// Uses frame elapsed as seed so replays are reproducible.
pub fn tick_faction_system(time: Res<Time>, mut state: ResMut<FactionState>) {
    let seed = (time.elapsed_secs_f64() as u64).wrapping_mul(0x9E3779B1);
    let (new_state, _events) = core_tick(state.0.clone());
    state.0 = new_state;
    // Events (DiplomaticShift, ContentRelease, MissionUnlock) would be
    // broadcast here by a future sprint; S11 only produces them.
    let _ = seed; // consumed by future broadcast system
}

/// Toggle the reputation panel on P key press.
pub fn reputation_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    mut visible: ResMut<ReputationPanelVisible>,
) {
    if keys.just_pressed(KeyCode::KeyP) {
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
    state: Res<FactionState>,
    mut query: Query<(&mut Text, &mut Visibility), With<ReputationPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── REPUTATION ──  (P to close)".to_string()];
            for f in &state.0.catalog.factions {
                let rep = state.0.rep(&f.id);
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
            lines.push(format!("tick: {}", state.0.tick));
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
    state: Res<FactionState>,
    economy: Res<crate::systems::market::Economy>,
    mut query: Query<(&mut Text, &mut Visibility, &mut TextColor), With<FactionBanner>>,
) {
    if let Ok((mut text, mut vis, mut color)) = query.single_mut() {
        // Find the station's faction from the economy station record.
        let faction_id = economy
            .0
            .stations
            .get(&location.station_id)
            .and_then(|s| s.station_faction.as_ref());
        if let Some(fid) = faction_id {
            if let Some(f) = state.0.catalog.factions.iter().find(|f| f.id.0 == *fid) {
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
