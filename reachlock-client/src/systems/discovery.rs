//! Discovery log (S39, S85): species cards panel, ecosystem summary, and
//! discovery log of systems charted by the player. Sub-tabs: Ecosystem | Discoveries.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use reachlock_core::galaxy::GalaxyCoord;
use reachlock_core::generator::Ecosystem;

use crate::settings::{InputAction, Settings};
use crate::theme;

/// Panel visibility toggle.
#[derive(Resource, Default)]
pub struct DiscoveryPanelVisible(pub bool);

/// Which sub-tab is active.
#[derive(Resource, Default)]
pub struct DiscoveryTab(pub Tab);

#[derive(Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Ecosystem,
    Discoveries,
}

/// Marker on the discovery panel text entity.
#[derive(Component)]
pub struct DiscoveryPanel;

/// The current planet's ecosystem. Populated when the player scans a
/// habitable planet or an ecosystem override is loaded.
#[derive(Resource, Default)]
pub struct EcosystemResource(pub Option<Ecosystem>);

/// One entry in the player's personal discovery log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryLogEntry {
    pub system_name: String,
    pub galaxy_coord: GalaxyCoord,
    pub discovered_at: i64,
    pub system_id: String,
}

/// Resource holding the player's discovery history, persisted to save.
#[derive(Debug, Resource, Default, Clone, Serialize, Deserialize)]
pub struct DiscoveryLog {
    #[serde(default)]
    pub entries: Vec<DiscoveryLogEntry>,
}

/// Toast notification component — text entity that despawns after expiry.
#[derive(Component)]
pub struct NotifTimer {
    pub expires_at: std::time::Instant,
}

impl DiscoveryLog {
    pub fn push(&mut self, entry: DiscoveryLogEntry) {
        self.entries.insert(0, entry);
        if self.entries.len() > 500 {
            self.entries.truncate(500);
        }
    }
}

/// Toggle on the assigned key.
pub fn discovery_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<DiscoveryPanelVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenDiscoveryPanel)) {
        visible.0 = !visible.0;
    }
}

/// Tab switching with Tab/Shift+Tab.
pub fn discovery_panel_tab(
    keys: Res<ButtonInput<KeyCode>>,
    visible: Res<DiscoveryPanelVisible>,
    mut tab: ResMut<DiscoveryTab>,
) {
    if !visible.0 {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        tab.0 = match tab.0 {
            Tab::Ecosystem => Tab::Discoveries,
            Tab::Discoveries => Tab::Ecosystem,
        };
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
        theme::fg("text.ok"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(120.0),
            left: Val::Px(8.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Spawn a toast notification entity.
pub fn spawn_notification(commands: &mut Commands, text: &str) {
    commands.spawn((
        NotifTimer {
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(5),
        },
        Text::new(text.to_string()),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        theme::fg("text.ok"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(5.0),
            left: Val::Percent(50.0),
            ..default()
        },
    ));
}

/// Queue of pending toast notifications (spawned by a separate system).
#[derive(Resource, Default)]
pub struct NotificationQueue(pub Vec<String>);

/// Spawn queued notifications and despawn expired ones.
pub fn process_notifications(
    mut commands: Commands,
    mut queue: ResMut<NotificationQueue>,
    notifications: Query<(Entity, &NotifTimer)>,
) {
    for text in queue.0.drain(..) {
        commands.spawn((
            NotifTimer {
                expires_at: std::time::Instant::now() + std::time::Duration::from_secs(5),
            },
            Text::new(text),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            theme::fg("text.ok"),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(5.0),
                left: Val::Percent(50.0),
                ..default()
            },
        ));
    }
    for (entity, timer) in &notifications {
        if std::time::Instant::now() >= timer.expires_at {
            commands.entity(entity).despawn();
        }
    }
}

/// Render the panel when visible.
pub fn render_discovery_panel(
    visible: Res<DiscoveryPanelVisible>,
    ecosystem: Res<EcosystemResource>,
    tab: Res<DiscoveryTab>,
    log: Res<DiscoveryLog>,
    mut query: Query<(&mut Text, &mut Visibility), With<DiscoveryPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let tab_label = match tab.0 {
                Tab::Ecosystem => "[Ecosystem]",
                Tab::Discoveries => "[Discoveries]",
            };
            let mut lines = vec![format!("── DISCOVERY LOG ──  {tab_label}  (Tab)")];
            match tab.0 {
                Tab::Ecosystem => match &ecosystem.0 {
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
                },
                Tab::Discoveries => {
                    if log.entries.is_empty() {
                        lines.push(
                            "  No discoveries yet. Scan an uncharted system to claim it.".into(),
                        );
                    } else {
                        for entry in &log.entries {
                            let date = chrono_like(entry.discovered_at);
                            lines.push(format!(
                                "  {} — ({}, {}) — {}",
                                entry.system_name, entry.galaxy_coord.x, entry.galaxy_coord.y, date
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

/// Simple date string from unix timestamp (UTC).
fn chrono_like(ts: i64) -> String {
    // Avoid chrono dependency: compute a human-readable UTC date.
    let secs_per_day: i64 = 86400;
    let days = ts / secs_per_day;
    // Approximate: days since epoch -> year/month/day.
    let mut y = 1970i64;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months_days: &[(i64, i64)] = if is_leap(y) {
        &[
            (31, 0),
            (29, 31),
            (31, 60),
            (30, 91),
            (31, 121),
            (30, 152),
            (31, 182),
            (31, 213),
            (30, 244),
            (31, 274),
            (30, 305),
            (31, 335),
        ]
    } else {
        &[
            (31, 0),
            (28, 31),
            (31, 59),
            (30, 90),
            (31, 120),
            (30, 151),
            (31, 181),
            (31, 212),
            (30, 243),
            (31, 273),
            (30, 304),
            (31, 334),
        ]
    };
    let mut m = 0;
    for (i, &(days_in_month, offset)) in months_days.iter().enumerate() {
        if remaining < offset + days_in_month {
            m = i + 1;
            remaining -= offset;
            break;
        }
    }
    if m == 0 {
        m = 12;
    }
    let d = remaining + 1;
    format!("{y:04}-{m:02}-{d:02}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}
