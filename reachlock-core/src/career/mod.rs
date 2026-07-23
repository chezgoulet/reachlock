//! Unified career progression (S42). One progression engine with parallel,
//! shareable career tracks (military, trade, exploration, science, political,
//! criminal, freelance). Progress is earned by in-fiction actions, not XP
//! grinding. Pure functions over `PlayerCareer` state — no I/O, deterministic,
//! wasm-safe (iron rule #1).
//!
//! NOTE on iron rule #2: the brief's freeze block used `f64` for
//! `ProgressionCriterion::weight`, `CareerPerk::magnitude`, and the various
//! `PerkType` percentage fields. Those are gameplay-affecting values, so they
//! are stored as `Fixed` (1/1024) here — floats would break the cross-platform
//! determinism harness.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::util::Fixed;

/// A career track the player can join. Authored as `content/careers/*.ron`;
/// the struct is the shared shape (spec §10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CareerPath {
    pub id: String,
    pub path_type: PathType,
    pub name: String,
    pub description: String,
    /// `None` = independent (freelance, criminal).
    pub faction_id: Option<String>,
    pub ranks: Vec<CareerRank>,
    pub progression_criteria: Vec<ProgressionCriterion>,
    pub perks: Vec<CareerPerk>,
    /// Symmetric conflict set — validated at content load.
    pub conflicting_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathType {
    Military,
    Trade,
    Exploration,
    Science,
    Political,
    Criminal,
    Freelance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CareerRank {
    pub rank: u8,
    pub title: String,
    pub required_criteria: Vec<ProgressionRequirement>,
    /// Perk IDs unlocked when this rank is reached.
    pub rank_perks: Vec<String>,
    pub faction_standing_bonus: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressionCriterion {
    pub criterion_type: ProgressionCriterionType,
    pub target: String,
    pub threshold: u64,
    /// Relative weight of this criterion (Fixed, 1/1024).
    pub weight: Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressionCriterionType {
    CombatVictories,
    TradeVolume,
    SystemsDiscovered,
    SpeciesScanned,
    MissionsCompleted,
    FactionReputationGained,
    CrewTrustBuilt,
    ArtifactsRecovered,
    ShipsCaptured,
    ContrabandSmuggled,
    BountiesCollected,
    ResearchPointsEarned,
    StoryMissionsCompleted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressionRequirement {
    pub criterion_type: ProgressionCriterionType,
    pub threshold: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CareerPerk {
    pub id: String,
    pub name: String,
    pub description: String,
    pub perk_type: PerkType,
    /// Magnitude in Fixed (1/1024); meaning depends on `perk_type`.
    pub magnitude: Fixed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerkType {
    StationDiscount { faction_id: String, pct: Fixed },
    RestrictedAreaAccess { area_id: String },
    UniqueShipComponent { item_id: String },
    ExclusiveContract { contract_id: String },
    CrewRecruitUnlock { crew_id: String },
    MissionBonus { mission_type: String, bonus_pct: Fixed },
    ScannerBoost { pct: Fixed },
    CombatBonus { damage_type: String, pct: Fixed },
    TradeBonus { good_category: String, pct: Fixed },
    DiplomaticImmunity { faction_id: String },
    /// Crimes forgiven in this faction (pirate havens) — S43 handshake.
    BountyPass { faction_id: String },
}

/// The player's whole career state across all tracks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerCareer {
    pub player_id: String,
    pub active_paths: Vec<ActiveCareerPath>,
    pub completed_paths: Vec<CompletedPath>,
    pub total_prestige: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveCareerPath {
    pub path_id: String,
    pub current_rank: u8,
    pub progress: HashMap<ProgressionCriterionType, u64>,
    pub joined_at_tick: u64,
    pub last_advanced_at_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletedPath {
    pub path_id: String,
    pub final_rank: u8,
    pub completed_at_tick: u64,
    pub reason: CompletionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionReason {
    ReachedMaxRank,
    Resigned,
    Expelled,
    Defected,
}

/// Direction of a reputation/faction change — shared with S41's
/// `EncounterTrigger::OnFactionReputation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
    Either,
}

/// Errors from attempting to join a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinError {
    AlreadyActive,
    ConflictingPath(String),
    MaxPathsReached,
}

/// A consequence of leaving a path (perk/standing revocation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CareerConsequence {
    PerkRevoked(String),
    StandingLost(i64),
    PathCompleted(String),
}

const MAX_ACTIVE_PATHS: usize = 3;

impl PlayerCareer {
    pub fn new(player_id: &str) -> Self {
        PlayerCareer {
            player_id: player_id.to_string(),
            active_paths: Vec::new(),
            completed_paths: Vec::new(),
            total_prestige: 0,
        }
    }
}

/// Record progress for an action type across every active path. Magnitude is
/// additive; returns the new (immutable) state.
pub fn record_progress(mut pc: PlayerCareer, action: ProgressionCriterionType, magnitude: u64) -> PlayerCareer {
    for active in &mut pc.active_paths {
        *active.progress.entry(action).or_insert(0) += magnitude;
    }
    pc
}

/// If the active path's next rank's requirements are all met, return it.
pub fn check_rank_advancement(pc: &PlayerCareer, path: &CareerPath) -> Option<u8> {
    let active = pc.active_paths.iter().find(|a| a.path_id == path.id)?;
    let next_rank = active.current_rank + 1;
    let rank_def = path.ranks.iter().find(|r| r.rank == next_rank)?;
    let met = rank_def.required_criteria.iter().all(|req| {
        active.progress.get(&req.criterion_type).copied().unwrap_or(0) >= req.threshold
    });
    if met {
        Some(next_rank)
    } else {
        None
    }
}

/// Advance the path one rank if requirements are met. Returns the new state and
/// the perks unlocked at the new rank.
pub fn advance_rank(
    mut pc: PlayerCareer,
    path: &CareerPath,
    tick: u64,
) -> (PlayerCareer, Vec<CareerPerk>) {
    let Some(next_rank) = check_rank_advancement(&pc, path) else {
        return (pc, Vec::new());
    };
    let rank_def = path
        .ranks
        .iter()
        .find(|r| r.rank == next_rank)
        .expect("rank exists (check passed)");
    if let Some(active) = pc.active_paths.iter_mut().find(|a| a.path_id == path.id) {
        active.current_rank = next_rank;
        active.last_advanced_at_tick = tick;
    }
    // Diminishing prestige: 1st path full, 2nd 50%, 3rd 25%, ...
    let factor = 1u64 << pc.completed_paths.len().min(3);
    pc.total_prestige += (next_rank as u64 * 100 / factor).max(1);
    let unlocked = path
        .perks
        .iter()
        .filter(|p| rank_def.rank_perks.contains(&p.id))
        .cloned()
        .collect();
    (pc, unlocked)
}

/// Join a path. Rejects duplicates, conflicts (symmetric), and over-capacity.
pub fn join_path(mut pc: PlayerCareer, path: &CareerPath) -> Result<PlayerCareer, JoinError> {
    if pc.active_paths.iter().any(|a| a.path_id == path.id) {
        return Err(JoinError::AlreadyActive);
    }
    if pc
        .active_paths
        .iter()
        .any(|a| path.conflicting_paths.contains(&a.path_id))
        || pc
            .completed_paths
            .iter()
            .any(|c| path.conflicting_paths.contains(&c.path_id))
    {
        return Err(JoinError::ConflictingPath(
            path.conflicting_paths
                .iter()
                .find(|c| {
                    pc.active_paths.iter().any(|a| &a.path_id == *c)
                        || pc.completed_paths.iter().any(|x| &x.path_id == *c)
                })
                .cloned()
                .unwrap_or_default(),
        ));
    }
    if pc.active_paths.len() >= MAX_ACTIVE_PATHS {
        return Err(JoinError::MaxPathsReached);
    }
    pc.active_paths.push(ActiveCareerPath {
        path_id: path.id.clone(),
        current_rank: 0,
        progress: HashMap::new(),
        joined_at_tick: 0,
        last_advanced_at_tick: 0,
    });
    Ok(pc)
}

/// Leave a path (resign/expel/defect/reached-max). Moves it to completed and
/// reports the consequences (perk/standing revocation).
pub fn leave_path(
    mut pc: PlayerCareer,
    path_id: &str,
    reason: CompletionReason,
    tick: u64,
) -> (PlayerCareer, Vec<CareerConsequence>) {
    let idx = match pc.active_paths.iter().position(|a| a.path_id == path_id) {
        Some(i) => i,
        None => return (pc, Vec::new()),
    };
    let active = pc.active_paths.remove(idx);
    pc.completed_paths.push(CompletedPath {
        path_id: active.path_id.clone(),
        final_rank: active.current_rank,
        completed_at_tick: tick,
        reason,
    });
    let consequences = vec![
        CareerConsequence::PerkRevoked(active.path_id.clone()),
        CareerConsequence::PathCompleted(active.path_id),
    ];
    (pc, consequences)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path() -> CareerPath {
        CareerPath {
            id: "compact_navy".into(),
            path_type: PathType::Military,
            name: "Compact Navy".into(),
            description: "Military track".into(),
            faction_id: Some("compact".into()),
            ranks: vec![
                CareerRank {
                    rank: 1,
                    title: "Ensign".into(),
                    required_criteria: vec![ProgressionRequirement {
                        criterion_type: ProgressionCriterionType::CombatVictories,
                        threshold: 3,
                    }],
                    rank_perks: vec!["nav_boost".into()],
                    faction_standing_bonus: 10,
                },
                CareerRank {
                    rank: 2,
                    title: "Lieutenant".into(),
                    required_criteria: vec![ProgressionRequirement {
                        criterion_type: ProgressionCriterionType::CombatVictories,
                        threshold: 10,
                    }],
                    rank_perks: vec!["shipyard_access".into()],
                    faction_standing_bonus: 25,
                },
            ],
            progression_criteria: vec![ProgressionCriterion {
                criterion_type: ProgressionCriterionType::CombatVictories,
                target: "*".into(),
                threshold: 10,
                weight: Fixed::from_int(1),
            }],
            perks: vec![
                CareerPerk {
                    id: "nav_boost".into(),
                    name: "Nav Boost".into(),
                    description: "Combat bonus".into(),
                    perk_type: PerkType::CombatBonus {
                        damage_type: "kinetic".into(),
                        pct: Fixed::from_int(5),
                    },
                    magnitude: Fixed::from_int(5),
                },
                CareerPerk {
                    id: "shipyard_access".into(),
                    name: "Shipyard Access".into(),
                    description: "Restricted area".into(),
                    perk_type: PerkType::RestrictedAreaAccess {
                        area_id: "nav_shipyard".into(),
                    },
                    magnitude: Fixed::from_int(1),
                },
            ],
            conflicting_paths: vec!["reach_pirates".into()],
        }
    }

    #[test]
    fn record_then_advance() {
        let path = sample_path();
        let pc = PlayerCareer::new("p1");
        let pc = join_path(pc, &path).unwrap();
        let pc = record_progress(pc, ProgressionCriterionType::CombatVictories, 3);
        assert_eq!(check_rank_advancement(&pc, &path), Some(1));
        let (pc, perks) = advance_rank(pc, &path, 100);
        assert_eq!(pc.active_paths[0].current_rank, 1);
        assert_eq!(perks.len(), 1);
        assert_eq!(perks[0].id, "nav_boost");
    }

    #[test]
    fn cannot_advance_without_progress() {
        let path = sample_path();
        let pc = join_path(PlayerCareer::new("p1"), &path).unwrap();
        assert_eq!(check_rank_advancement(&pc, &path), None);
    }

    #[test]
    fn conflict_detection() {
        let path = sample_path();
        // A second path that conflicts with compact_navy.
        let mut pirate = path.clone();
        pirate.id = "reach_pirates".into();
        pirate.path_type = PathType::Criminal;
        pirate.conflicting_paths = vec!["compact_navy".into()];
        let pc = join_path(PlayerCareer::new("p1"), &path).unwrap();
        assert_eq!(join_path(pc, &pirate), Err(JoinError::ConflictingPath("compact_navy".into())));
    }

    #[test]
    fn leave_revokes() {
        let path = sample_path();
        let pc = join_path(PlayerCareer::new("p1"), &path).unwrap();
        let (pc, cons) = leave_path(pc, "compact_navy", CompletionReason::Resigned, 50);
        assert_eq!(pc.active_paths.len(), 0);
        assert_eq!(pc.completed_paths.len(), 1);
        assert!(cons.iter().any(|c| matches!(c, CareerConsequence::PathCompleted(_))));
    }

    #[test]
    fn duplicate_join_rejected() {
        let path = sample_path();
        let pc = join_path(PlayerCareer::new("p1"), &path).unwrap();
        assert_eq!(join_path(pc, &path), Err(JoinError::AlreadyActive));
    }
}
