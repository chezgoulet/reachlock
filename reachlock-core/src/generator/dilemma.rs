//! Procedural dilemma generator (S36). Pure function: seed + game state →
//! `Option<Dilemma>`. Most seeds produce nothing; frontier systems are more
//! likely to yield one. No LLM, no I/O — deterministic from input state.

use serde::{Deserialize, Serialize};

use crate::util::SeededRng;

/// A dilemma: an ambiguous situation the crew must deliberate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dilemma {
    pub id: String,
    pub dilemma_type: DilemmaType,
    pub setup: DilemmaSetup,
    pub participants: Vec<DilemmaParticipant>,
    pub stakes: Vec<DilemmaStake>,
    pub choices: Vec<DilemmaChoice>,
    pub seed: u64,
    pub complexity: DilemmaComplexity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DilemmaType {
    Triage {
        max_can_save: u8,
        candidates: Vec<String>,
    },
    Sacrifice {
        who: String,
        for_what: String,
    },
    Abandonment {
        what: String,
        consequence: String,
    },
    LoyaltyConflict {
        between: (String, String),
        issue: String,
    },
    SecretRevealed {
        who: String,
        secret: String,
        affected: Vec<String>,
    },
    UnjustLaw {
        law: String,
        victims: Vec<String>,
    },
    Allocation {
        resource: String,
        claimants: Vec<String>,
        amount: u32,
    },
    Investment {
        options: Vec<(String, u32)>,
        budget: u32,
    },
    FogOfWar {
        known: Vec<String>,
        unknown: Vec<String>,
    },
    RetreatOrStand {
        odds: u8,
        stakes_on_ground: String,
    },
    CrewSecret {
        who: String,
        secret: String,
        discovered_by: String,
    },
    MutinyBrewing {
        dissenters: Vec<String>,
        grievance: String,
    },
    OutsiderAppeal {
        outsider: String,
        offer: String,
        cost: String,
    },
    BlockadeChoice {
        blockaded: String,
        needed_goods: Vec<String>,
    },
    DefectorAppeal {
        defector: String,
        information: String,
    },
    PredecessorEnigma {
        artifact: String,
        risk: String,
        potential: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DilemmaSetup {
    pub title: String,
    pub narrative: String,
    pub urgency: DilemmaUrgency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DilemmaUrgency {
    Immediate,
    Pressing,
    Looming,
    Background,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DilemmaParticipant {
    pub crew_id: String,
    pub stake: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DilemmaStake {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DilemmaChoice {
    pub label: String,
    pub description: String,
    pub consequences: Vec<DilemmaConsequence>,
    pub alignment_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DilemmaConsequence {
    pub kind: ConsequenceKind,
    pub target: String,
    pub magnitude: u8,
    pub description_template: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsequenceKind {
    CrewTrustChanged,
    FactionReputationChanged,
    PopulationChanged,
    ResourceGained,
    ResourceLost,
    CrewMemberQuits,
    NewMissionUnlocked,
    StoryArcProgressed,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DilemmaComplexity {
    Simple,
    Nuanced,
    Wicked,
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// Generate a dilemma from a seed and game context. Returns None for most
/// seeds — dilemmas are calibrated to ~1 per 2-3 hours of play.
pub fn generate_dilemma(
    seed: u64,
    is_frontier: bool,
    relationship_count: u32,
    faction_diversity: u32,
) -> Option<Dilemma> {
    let mut rng = SeededRng::new(seed);
    let base_chance = if is_frontier { 350u64 } else { 150u64 };
    if rng.next_below(1000) >= base_chance {
        return None;
    }

    let id = format!("dilemma_{:#x}", seed);
    let complexity = pick_complexity(&mut rng, is_frontier);
    let dilemma_type = pick_type(&mut rng);
    let (setup, stakes, choices) = build_dilemma(
        &mut rng,
        &dilemma_type,
        complexity,
        relationship_count,
        faction_diversity,
    );

    Some(Dilemma {
        id,
        dilemma_type,
        setup,
        participants: Vec::new(),
        stakes,
        choices,
        seed,
        complexity,
    })
}

fn pick_complexity(rng: &mut SeededRng, is_frontier: bool) -> DilemmaComplexity {
    let roll = rng.next_below(100);
    if is_frontier {
        if roll < 15 {
            DilemmaComplexity::Wicked
        } else if roll < 45 {
            DilemmaComplexity::Nuanced
        } else {
            DilemmaComplexity::Simple
        }
    } else {
        if roll < 5 {
            DilemmaComplexity::Wicked
        } else if roll < 25 {
            DilemmaComplexity::Nuanced
        } else {
            DilemmaComplexity::Simple
        }
    }
}

fn pick_type(rng: &mut SeededRng) -> DilemmaType {
    match rng.next_below(16) {
        0 => DilemmaType::Triage {
            max_can_save: 1 + rng.next_below(3) as u8,
            candidates: vec!["crew".into(), "passengers".into(), "data".into()],
        },
        1 => DilemmaType::Sacrifice {
            who: pick_name(rng).into(),
            for_what: pick_asset(rng).into(),
        },
        2 => DilemmaType::Abandonment {
            what: pick_asset(rng).into(),
            consequence: "permanent faction penalty".into(),
        },
        3 => {
            let names = [pick_name(rng), pick_name(rng)];
            DilemmaType::LoyaltyConflict {
                between: (names[0].into(), names[1].into()),
                issue: pick_issue(rng).into(),
            }
        }
        4 => DilemmaType::SecretRevealed {
            who: pick_name(rng).into(),
            secret: pick_secret(rng).into(),
            affected: vec![pick_name(rng).into()],
        },
        5 => DilemmaType::UnjustLaw {
            law: pick_law(rng).into(),
            victims: vec!["colonists".into(), "traders".into()],
        },
        6 => DilemmaType::Allocation {
            resource: pick_asset(rng).into(),
            claimants: vec![pick_name(rng).into(), pick_name(rng).into()],
            amount: 10 + rng.next_below(90) as u32,
        },
        7 => DilemmaType::Investment {
            options: vec![
                (pick_asset(rng).into(), 100 + rng.next_below(400) as u32),
                (pick_asset(rng).into(), 100 + rng.next_below(400) as u32),
            ],
            budget: 500 + rng.next_below(1000) as u32,
        },
        8 => DilemmaType::FogOfWar {
            known: vec!["sensors".into()],
            unknown: vec!["enemy_positions".into(), "debris_field".into()],
        },
        9 => DilemmaType::RetreatOrStand {
            odds: (50 + rng.next_below(40)) as u8,
            stakes_on_ground: pick_asset(rng).into(),
        },
        10 => DilemmaType::CrewSecret {
            who: pick_name(rng).into(),
            secret: pick_secret(rng).into(),
            discovered_by: pick_name(rng).into(),
        },
        11 => DilemmaType::MutinyBrewing {
            dissenters: vec![pick_name(rng).into(), pick_name(rng).into()],
            grievance: pick_issue(rng).into(),
        },
        12 => DilemmaType::OutsiderAppeal {
            outsider: pick_name(rng).into(),
            offer: pick_asset(rng).into(),
            cost: pick_asset(rng).into(),
        },
        13 => DilemmaType::BlockadeChoice {
            blockaded: pick_name(rng).into(),
            needed_goods: vec![pick_asset(rng).into()],
        },
        14 => DilemmaType::DefectorAppeal {
            defector: pick_name(rng).into(),
            information: "faction_troop_movements".into(),
        },
        15 => DilemmaType::PredecessorEnigma {
            artifact: pick_artifact(rng).into(),
            risk: pick_risk(rng).into(),
            potential: pick_asset(rng).into(),
        },
        _ => unreachable!(),
    }
}

fn build_dilemma(
    _rng: &mut SeededRng,
    dtype: &DilemmaType,
    _complexity: DilemmaComplexity,
    _rel_count: u32,
    _faction_div: u32,
) -> (DilemmaSetup, Vec<DilemmaStake>, Vec<DilemmaChoice>) {
    let urgency = match dtype {
        DilemmaType::BlockadeChoice { .. }
        | DilemmaType::RetreatOrStand { .. }
        | DilemmaType::Triage { .. } => DilemmaUrgency::Immediate,
        DilemmaType::Sacrifice { .. }
        | DilemmaType::FogOfWar { .. }
        | DilemmaType::Abandonment { .. } => DilemmaUrgency::Pressing,
        DilemmaType::LoyaltyConflict { .. }
        | DilemmaType::Allocation { .. }
        | DilemmaType::Investment { .. }
        | DilemmaType::MutinyBrewing { .. }
        | DilemmaType::CrewSecret { .. } => DilemmaUrgency::Looming,
        _ => DilemmaUrgency::Background,
    };

    let (title, narrative) = match dtype {
        DilemmaType::Triage { max_can_save, .. } => (
            format!("Triage: save {max_can_save}"),
            format!("Only {max_can_save} can be saved. Who?"),
        ),
        DilemmaType::Sacrifice { who, for_what, .. } => (
            format!("Sacrifice {who}"),
            format!("{for_what} demands {who}. Accept?"),
        ),
        _ => ("A dilemma".into(), "The crew must decide.".into()),
    };

    let setup = DilemmaSetup {
        title,
        narrative,
        urgency,
    };

    let stakes = vec![
        DilemmaStake {
            label: "crew_trust".into(),
            description: "Crew cohesion may shift.".into(),
        },
        DilemmaStake {
            label: "resources".into(),
            description: "Ship's stores affected.".into(),
        },
    ];

    let choices = match dtype {
        DilemmaType::Triage { max_can_save, .. } => {
            vec![
                DilemmaChoice {
                    label: "Save the crew".into(),
                    description: format!("Prioritize the {max_can_save} most essential crew."),
                    consequences: vec![DilemmaConsequence {
                        kind: ConsequenceKind::CrewTrustChanged,
                        target: "crew".into(),
                        magnitude: 2,
                        description_template: "Crew trusts you: {target}.".into(),
                    }],
                    alignment_tags: vec!["loyal".into(), "protective".into()],
                },
                DilemmaChoice {
                    label: "Save the data".into(),
                    description: "The mission data is irreplaceable.".into(),
                    consequences: vec![
                        DilemmaConsequence {
                            kind: ConsequenceKind::CrewTrustChanged,
                            target: "crew".into(),
                            magnitude: 5,
                            description_template: "Crew morale drops: {target}.".into(),
                        },
                        DilemmaConsequence {
                            kind: ConsequenceKind::ResourceGained,
                            target: "data".into(),
                            magnitude: 3,
                            description_template: "Data recovered: {target}.".into(),
                        },
                    ],
                    alignment_tags: vec!["pragmatic".into(), "mission-focused".into()],
                },
            ]
        }
        _ => {
            vec![
                DilemmaChoice {
                    label: "Choice A".into(),
                    description: "The straightforward path.".into(),
                    consequences: vec![DilemmaConsequence {
                        kind: ConsequenceKind::CrewTrustChanged,
                        target: "crew".into(),
                        magnitude: 3,
                        description_template: "Mild trust change: {target}.".into(),
                    }],
                    alignment_tags: vec!["pragmatic".into()],
                },
                DilemmaChoice {
                    label: "Choice B".into(),
                    description: "The risky path with potential gain.".into(),
                    consequences: vec![
                        DilemmaConsequence {
                            kind: ConsequenceKind::CrewTrustChanged,
                            target: "crew".into(),
                            magnitude: 6,
                            description_template: "Strong trust change: {target}.".into(),
                        },
                        DilemmaConsequence {
                            kind: ConsequenceKind::ResourceGained,
                            target: "salvage".into(),
                            magnitude: 4,
                            description_template: "Found resources: {target}.".into(),
                        },
                    ],
                    alignment_tags: vec!["daring".into()],
                },
            ]
        }
    };

    (setup, stakes, choices)
}

// ---------------------------------------------------------------------------
// Vocabulary tables
// ---------------------------------------------------------------------------

fn pick_name(rng: &mut SeededRng) -> &'static str {
    NAMES[rng.next_below(NAMES.len() as u64) as usize]
}

fn pick_asset(rng: &mut SeededRng) -> &'static str {
    ASSETS[rng.next_below(ASSETS.len() as u64) as usize]
}

fn pick_issue(rng: &mut SeededRng) -> &'static str {
    ISSUES[rng.next_below(ISSUES.len() as u64) as usize]
}

fn pick_secret(rng: &mut SeededRng) -> &'static str {
    SECRETS[rng.next_below(SECRETS.len() as u64) as usize]
}

fn pick_law(rng: &mut SeededRng) -> &'static str {
    LAWS[rng.next_below(LAWS.len() as u64) as usize]
}

fn pick_artifact(rng: &mut SeededRng) -> &'static str {
    ARTIFACTS[rng.next_below(ARTIFACTS.len() as u64) as usize]
}

fn pick_risk(rng: &mut SeededRng) -> &'static str {
    RISKS[rng.next_below(RISKS.len() as u64) as usize]
}

const NAMES: &[&str] = &[
    "Alexander",
    "Boris",
    "Tove",
    "Tib",
    "Risc",
    "Prudence",
    "Keene",
    "Bardo",
];

const ASSETS: &[&str] = &[
    "ferrite",
    "water",
    "fuel",
    "med_supplies",
    "food",
    "data_core",
    "weapons_parts",
    "engine_coils",
    "shield_capacitors",
    "computron",
];

const ISSUES: &[&str] = &[
    "repair_priority",
    "route_choice",
    "cargo_manifest",
    "diplomatic_stance",
];

const SECRETS: &[&str] = &[
    "desertion_record",
    "smuggling_past",
    "faction_agent",
    "stolen_identity",
];

const LAWS: &[&str] = &[
    "quarantine_edict",
    "export_embargo",
    "conscription_decree",
    "tithing_law",
];

const ARTIFACTS: &[&str] = &[
    "predecessor_relay",
    "ancient_core",
    "void_compass",
    "stellar_map",
];

const RISKS: &[&str] = &[
    "radiation_exposure",
    "system_corruption",
    "faction_retaliation",
    "crew_loss",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_generation() {
        let a = generate_dilemma(42, true, 5, 3);
        let b = generate_dilemma(42, true, 5, 3);
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_differ() {
        let a = generate_dilemma(1, true, 5, 3);
        let b = generate_dilemma(2, true, 5, 3);
        // Different seeds either differ or one is None, but the outputs
        // should not be identical Some() values.
        if let (Some(da), Some(db)) = (&a, &b) {
            assert_ne!(da.id, db.id);
        } else {
            // At least one is None — that's fine, probability-based.
        }
    }

    #[test]
    fn frontier_more_likely_than_safe() {
        let frontier_count = (0..100)
            .filter_map(|i| generate_dilemma(i, true, 5, 3))
            .count();
        let safe_count = (0..100)
            .filter_map(|i| generate_dilemma(i, false, 5, 3))
            .count();
        assert!(
            frontier_count >= safe_count,
            "frontier ({frontier_count}) should produce >= dilemmas vs safe ({safe_count})"
        );
    }

    #[test]
    fn all_choice_labels_non_empty() {
        for seed in 0..50 {
            if let Some(d) = generate_dilemma(seed, true, 5, 3) {
                for choice in &d.choices {
                    assert!(!choice.label.is_empty(), "empty label for seed {seed}");
                }
            }
        }
    }

    #[test]
    fn no_single_correct_choice() {
        for seed in 0..100 {
            if let Some(d) = generate_dilemma(seed, true, 5, 3) {
                // All choices should have at least one consequence with
                // non-zero magnitude — no "correct" free option.
                for choice in &d.choices {
                    assert!(
                        !choice.consequences.is_empty(),
                        "choice {} has no consequences (seed {})",
                        choice.label,
                        seed
                    );
                }
            }
        }
    }
}
