//! Mission engine (S46): context-aware mission generation from economy,
//! faction politics, player reputation, and ship capability. Pure &
//! deterministic: `seed + context → Vec<Mission>`.

use serde::{Deserialize, Serialize};

use crate::career::piracy::NotorietyLevel;
use crate::career::ProgressionCriterionType;
use crate::contract::metadata::CrewRole;
use crate::util::rng::SeededRng;

/// A generated mission — deterministic from context state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mission {
    pub id: String,
    pub mission_type: MissionType,
    pub title: String,
    pub briefing: String,
    pub issuer: MissionIssuer,
    pub objectives: Vec<MissionObjective>,
    pub requirements: MissionRequirements,
    pub rewards: MissionRewards,
    pub expires_at_tick: Option<u64>,
    pub chain: Option<MissionChain>,
    pub tags: Vec<MissionTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionType {
    Transport,
    Combat,
    Exploration,
    Diplomacy,
    Investigation,
    Mining,
    Salvage,
    Smuggling,
    Bounty,
    Escort,
    Survey,
    Construction,
    Rescue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionIssuer {
    Faction {
        faction_id: String,
        division_id: Option<String>,
    },
    Station { station_id: String },
    Npc { npc_id: String },
    Personal,
    DistressBeacon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionObjective {
    pub objective_type: ObjectiveType,
    pub target: String,
    pub quantity: Option<u64>,
    pub destination: Option<String>,
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveType {
    Deliver,
    Destroy,
    Scan,
    TalkTo,
    Retrieve,
    Extract,
    Protect,
    Escort,
    Reach,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MissionRequirements {
    pub min_cargo_space: Option<u64>,
    pub min_crew_count: Option<u32>,
    pub required_crew_roles: Vec<CrewRole>,
    pub required_ship_upgrades: Vec<String>,
    pub min_career_rank: Option<(String, u8)>,
    pub min_faction_reputation: Option<(String, i64)>,
    pub required_items: Vec<String>,
    pub max_notoriety: Option<NotorietyLevel>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MissionRewards {
    pub credits: u64,
    pub reputation_gains: Vec<(String, i64)>,
    pub items: Vec<String>,
    pub career_progress: Vec<(ProgressionCriterionType, u64)>,
    pub unlock: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionChain {
    pub chain_id: String,
    pub position: u8,
    pub total_missions: u8,
    pub next_mission_seed: u64,
    pub chain_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionTag {
    HighRisk,
    HighReward,
    TimeSensitive,
    Repeatable,
    StoryCritical,
    Coop(Vec<String>),
    FactionSecret,
    Beginner,
    Expert,
}

/// State snapshot used to generate missions at a station.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionGenerationContext {
    pub seed: u64,
    pub system_kind: String,
    pub threat_level: u8,
    pub station_faction: String,
    pub player_career_ranks: Vec<(String, u8)>,
    pub player_notoriety: NotorietyLevel,
    pub player_credits: u64,
    pub tick: u64,
}

/// Default mission generation: 5–15 missions per call, weighted by system
/// state. Pure & deterministic — same seed + context always yields the
/// same mission set.
pub fn generate_missions(context: &MissionGenerationContext) -> Vec<Mission> {
    let mut rng = SeededRng::new(context.seed);
    let count = 5 + rng.next_below(11) as usize; // 5..=15
    let mut missions = Vec::with_capacity(count);

    for i in 0..count {
        let mt = weighted_type(&mut rng, context);
        let id = format!("mis-{:#x}-{}", context.seed, i);
        missions.push(Mission {
            id,
            mission_type: mt,
            title: format!("{:?} Mission", mt),
            briefing: format!("A {:?} assignment from {} at tick {}.", mt, context.station_faction, context.tick),
            issuer: MissionIssuer::Faction {
                faction_id: context.station_faction.clone(),
                division_id: None,
            },
            objectives: vec![MissionObjective {
                objective_type: ObjectiveType::Reach,
                target: "destination".into(),
                quantity: None,
                destination: Some("unknown".into()),
                optional: false,
            }],
            requirements: MissionRequirements {
                min_cargo_space: rng.next_below(200).into(),
                min_crew_count: (rng.next_below(5) as u32).into(),
                ..Default::default()
            },
            rewards: MissionRewards {
                credits: 100 + rng.next_below(1000),
                reputation_gains: vec![(context.station_faction.clone(), (rng.next_below(20) as i64 + 1))],
                career_progress: vec![(ProgressionCriterionType::MissionsCompleted, 1)],
                ..Default::default()
            },
            expires_at_tick: Some(context.tick + 10000),
            chain: if i == 0 && rng.next_below(100) < 30 {
                Some(MissionChain {
                    chain_id: format!("chain-{}", context.seed),
                    position: 0,
                    total_missions: 2 + rng.next_below(4) as u8,
                    next_mission_seed: SeededRng::new(context.seed ^ i as u64).next_u64(),
                    chain_title: format!("{:?} Chain", mt),
                })
            } else {
                None
            },
            tags: vec![MissionTag::Repeatable],
        });
    }
    missions
}

/// Weighted type distribution based on system state.
fn weighted_type(rng: &mut SeededRng, ctx: &MissionGenerationContext) -> MissionType {
    let roll = rng.next_below(100);
    match ctx.system_kind.as_str() {
        "war" | "blockade" => match roll {
            0..=39 => MissionType::Combat,
            40..=54 => MissionType::Escort,
            55..=69 => MissionType::Salvage,
            _ => MissionType::Survey,
        },
        "trade" | "hub" => match roll {
            0..=39 => MissionType::Transport,
            40..=59 => MissionType::Diplomacy,
            60..=74 => MissionType::Survey,
            _ => MissionType::Investigation,
        },
        "frontier" => match roll {
            0..=29 => MissionType::Exploration,
            30..=49 => MissionType::Survey,
            50..=64 => MissionType::Mining,
            65..=79 => MissionType::Salvage,
            _ => MissionType::Rescue,
        },
        "pirate" | "haven" => match roll {
            0..=24 => MissionType::Smuggling,
            25..=49 => MissionType::Bounty,
            50..=69 => MissionType::Salvage,
            _ => MissionType::Combat,
        },
        _ => match roll {
            0..=19 => MissionType::Transport,
            20..=34 => MissionType::Combat,
            35..=49 => MissionType::Exploration,
            50..=64 => MissionType::Survey,
            65..=79 => MissionType::Mining,
            _ => MissionType::Rescue,
        },
    }
}

/// Compute the next mission seed in a chain deterministically.
pub fn next_chain_seed(current_seed: u64, current_position: u8) -> u64 {
    SeededRng::new(current_seed.wrapping_add(current_position as u64 * 0x9E37_79B9))
        .next_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> MissionGenerationContext {
        MissionGenerationContext {
            seed: 12345,
            system_kind: "frontier".into(),
            threat_level: 30,
            station_faction: "compact".into(),
            player_career_ranks: vec![("compact_navy".into(), 1)],
            player_notoriety: NotorietyLevel::Clean,
            player_credits: 1000,
            tick: 50000,
        }
    }

    #[test]
    fn deterministic_generation() {
        let ctx = sample_context();
        let a = generate_missions(&ctx);
        let b = generate_missions(&ctx);
        assert_eq!(a, b);
    }

    #[test]
    fn generates_5_to_15_missions() {
        for seed in [0u64, 1, 42, 9999] {
            let mut ctx = sample_context();
            ctx.seed = seed;
            let ms = generate_missions(&ctx);
            assert!(
                ms.len() >= 5 && ms.len() <= 15,
                "seed {seed}: {} missions out of range",
                ms.len()
            );
        }
    }

    #[test]
    fn chain_seed_deterministic() {
        assert_eq!(next_chain_seed(100, 1), next_chain_seed(100, 1));
        assert_ne!(next_chain_seed(100, 1), next_chain_seed(100, 2));
    }

    #[test]
    fn different_seeds_different_missions() {
        let mut a = sample_context();
        let mut b = sample_context();
        a.seed = 1;
        b.seed = 999;
        let ma = generate_missions(&a);
        let mb = generate_missions(&b);
        assert_ne!(ma, mb);
    }

    #[test]
    fn war_zone_skewed_to_combat() {
        let mut ctx = sample_context();
        ctx.system_kind = "war".into();
        let ms = generate_missions(&ctx);
        let combat = ms.iter().filter(|m| matches!(m.mission_type, MissionType::Combat | MissionType::Escort)).count();
        assert!(combat > ms.len() / 3, "expected many combat in war zone");
    }
}
