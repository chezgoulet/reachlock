//! Career panel (S42): shows the player's active career paths, ranks,
//! progress, and perks. Lines of text on a toggleable panel.

use bevy::prelude::*;

use std::collections::HashMap;

use reachlock_core::career::piracy::PiracyState;
use reachlock_core::career::{CareerPath, PlayerCareer, ProgressionCriterionType};

use crate::settings::{InputAction, Settings};
use crate::theme;

/// The authored career definitions, keyed by `CareerPath::id`.
///
/// These carry the rank titles, perks and progression criteria. Ten of them
/// are authored, and nothing consumed them: `dispatch` parsed the files into
/// the stash and `take_careers()` had no callers, so the panel could only ever
/// show the raw path id and a rank number.
#[derive(Resource, Default)]
pub struct CareerCatalog(pub HashMap<String, CareerPath>);

impl CareerCatalog {
    /// The authored title for a rank, e.g. "Ensign". `None` when the career
    /// is not authored or the rank is past what the author defined — the
    /// caller falls back to the bare number rather than inventing a title.
    pub fn rank_title(&self, path_id: &str, rank: u8) -> Option<&str> {
        self.0
            .get(path_id)?
            .ranks
            .iter()
            .find(|r| r.rank == rank)
            .map(|r| r.title.as_str())
    }

    /// The authored display name for a career path.
    pub fn display_name(&self, path_id: &str) -> Option<&str> {
        self.0.get(path_id).map(|p| p.name.as_str())
    }
}

/// Populate [`CareerCatalog`] from the stash `dispatch_content` filled.
/// Chained after `dispatch_content`, like the trope and theme registries.
pub fn init_career_catalog(mut catalog: ResMut<CareerCatalog>) {
    let paths = crate::systems::dispatch::stash::take_careers();
    if !paths.is_empty() {
        info!("careers: loaded {} authored path(s)", paths.len());
    }
    catalog.0 = paths.into_iter().map(|p| (p.id.clone(), p)).collect();
}

/// Career panel visibility toggle.
#[derive(Resource, Default)]
pub struct CareerPanelVisible(pub bool);

/// Marker on the career panel text entity.
#[derive(Component)]
pub struct CareerPanel;

/// The player's career state.
#[derive(Resource, Default)]
pub struct CareerResource(pub Option<PlayerCareer>);

/// The player's piracy state (notoriety, bounties, contraband knowledge).
#[derive(Resource, Default)]
pub struct PiracyResource(pub Option<PiracyState>);

/// Toggle on the assigned key.
pub fn career_panel_toggle(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut visible: ResMut<CareerPanelVisible>,
) {
    if keys.just_pressed(settings.key(InputAction::OpenCareerPanel)) {
        visible.0 = !visible.0;
    }
}

/// Spawn the panel entity (hidden by default).
pub fn spawn_career_panel(mut commands: Commands) {
    commands.spawn((
        CareerPanel,
        Text::new(""),
        TextFont {
            font_size: 14.0,
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

/// Render the panel text when visible.
pub fn render_career_panel(
    visible: Res<CareerPanelVisible>,
    career: Res<CareerResource>,
    piracy: Res<PiracyResource>,
    catalog: Res<CareerCatalog>,
    mut query: Query<(&mut Text, &mut Visibility), With<CareerPanel>>,
) {
    if let Ok((mut text, mut vis)) = query.single_mut() {
        if visible.0 {
            *vis = Visibility::Visible;
            let mut lines = vec!["── CAREERS ──".to_string()];

            // Career section.
            match &career.0 {
                None => lines.push("  No career data loaded.".into()),
                Some(pc) => {
                    // Systems discovered counter (S85).
                    let sys_count: u64 = pc
                        .active_paths
                        .iter()
                        .flat_map(|ap| {
                            ap.progress
                                .get(&ProgressionCriterionType::SystemsDiscovered)
                        })
                        .sum();
                    if sys_count > 0 {
                        lines.push(format!("  Systems Discovered: {sys_count}"));
                    }
                    if pc.active_paths.is_empty() && pc.completed_paths.is_empty() {
                        lines.push("  No career paths joined yet.".into());
                    }
                    // Authored names and rank titles when the career is in the
                    // catalog; the raw id and number when it is not, so an
                    // unauthored path still reads as something.
                    for ap in &pc.active_paths {
                        let name = catalog.display_name(&ap.path_id).unwrap_or(&ap.path_id);
                        let rank = match catalog.rank_title(&ap.path_id, ap.current_rank) {
                            Some(title) => format!("{title} (rank {})", ap.current_rank),
                            None => format!("rank {}", ap.current_rank),
                        };
                        lines.push(format!("  {name} — {rank}  prestige {}", pc.total_prestige));
                        for (action, count) in &ap.progress {
                            lines.push(format!("    {:?}: {}", action, count));
                        }
                    }
                    for cp in &pc.completed_paths {
                        let name = catalog.display_name(&cp.path_id).unwrap_or(&cp.path_id);
                        let rank = match catalog.rank_title(&cp.path_id, cp.final_rank) {
                            Some(title) => format!("{title} (rank {})", cp.final_rank),
                            None => format!("final rank {}", cp.final_rank),
                        };
                        lines.push(format!("  [done] {name} — {rank} ({:?})", cp.reason));
                    }
                }
            }

            // Piracy section.
            lines.push("".into());
            lines.push("── NOTORIETY ──".into());
            match &piracy.0 {
                None => lines.push("  No piracy data loaded.".into()),
                Some(ps) => {
                    lines.push(format!(
                        "  Level: {:?} (value: {})",
                        ps.notoriety.level, ps.notoriety.value
                    ));
                    if !ps.active_bounties.is_empty() {
                        lines.push(format!("  Bounties: {}", ps.active_bounties.len()));
                        for b in &ps.active_bounties {
                            lines.push(format!(
                                "    {} {}cr ({})",
                                b.issuer_faction, b.amount, b.crime
                            ));
                        }
                    }
                    if !ps.current_havens_known.is_empty() {
                        lines.push(format!(
                            "  Pirate havens known: {}",
                            ps.current_havens_known.len()
                        ));
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

#[cfg(test)]
mod catalog_tests {
    use super::*;
    use reachlock_core::content::{ContentFile, ContentPayload};

    fn authored_paths() -> Vec<CareerPath> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("mods/reachlock/careers");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("careers dir").flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "ron") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read career");
            let file: ContentFile = ron::from_str(&text)
                .unwrap_or_else(|e| panic!("{} is not an envelope: {e}", path.display()));
            if let ContentPayload::Career(c) = file.payload {
                out.push(*c);
            }
        }
        out
    }

    /// The ten authored career files must reach the catalog. They used to stop
    /// in the dispatch stash: `take_careers()` had no callers, so every rank
    /// title and perk the author wrote was unreachable.
    #[test]
    fn authored_careers_reach_the_catalog() {
        let paths = authored_paths();
        assert!(
            paths.len() >= 10,
            "expected at least 10 authored careers, found {}",
            paths.len()
        );
        let catalog = CareerCatalog(paths.into_iter().map(|p| (p.id.clone(), p)).collect());
        assert!(catalog.display_name("compact_navy").is_some());
        // Rank 1 is authored for every path, so a title must resolve.
        for (id, path) in &catalog.0 {
            if let Some(first) = path.ranks.first() {
                assert_eq!(
                    catalog.rank_title(id, first.rank),
                    Some(first.title.as_str()),
                    "rank title for {id} did not resolve"
                );
            }
        }
    }

    /// An unauthored path must fall back to its raw id rather than vanish from
    /// the panel or render as an empty string.
    #[test]
    fn unknown_careers_fall_back_to_their_id() {
        let catalog = CareerCatalog::default();
        assert_eq!(catalog.display_name("mystery_path"), None);
        assert_eq!(catalog.rank_title("mystery_path", 1), None);
    }
}
