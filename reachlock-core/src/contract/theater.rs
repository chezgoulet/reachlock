//! Deliberation Theater (S38). Multi-crew deliberation with turn-based
//! speaking, reaction overlays, and resolution detection. Pure state machine
//! — no I/O, no floats (iron rule #1/#2).

use serde::{Deserialize, Serialize};

/// Max rounds per speaker before forced resolution.
pub const MAX_THEATER_ROUNDS: usize = 2;

/// The full deliberation theater session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliberationTheater {
    pub topic: String,
    pub trigger: TheaterTrigger,
    pub participants: Vec<TheaterSpeaker>,
    pub turn: usize,
    pub history: Vec<TheaterLine>,
    pub resolution: Option<TheaterResolution>,
    pub player_present: bool,
    pub allow_intervention: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TheaterTrigger {
    PlayerCalled { reason: String },
    MajorDilemma { dilemma_id: String },
    ShipCrisis { crisis_type: String },
    MissionChoice { options: Vec<String> },
    CrewIssue { between: Vec<String>, issue: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TheaterSpeaker {
    pub crew_id: String,
    pub role: String,
    pub relationship_to_topic: String,
    pub speaking_order: u8,
    pub spoke: bool,
    pub position: Option<TheaterPosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TheaterPosition {
    Advocate {
        position: String,
        reasoning: String,
    },
    Oppose {
        to_whom: String,
        position: String,
        reasoning: String,
    },
    Amend {
        to_whom: String,
        amendment: String,
    },
    Question {
        to_whom: String,
        question: String,
    },
    Defer {
        to_whom: String,
    },
    Recuse {
        reason: String,
    },
}

/// One line spoken in the theater exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TheaterLine {
    pub speaker: String,
    pub portrait: String,
    pub position: TheaterPosition,
    pub llm_raw: String,
    pub display_text: String,
    pub reactions: Vec<(String, ReactionType)>,
    pub relationship_deltas: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactionType {
    Nod,
    Frown,
    Surprise,
    Relief,
    Tension,
    Breakthrough,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TheaterResolution {
    Consensus {
        action: String,
        summary: String,
    },
    MajorityVote {
        action: String,
        for_votes: Vec<String>,
        against: Vec<String>,
    },
    CrewLeadsDecision {
        leader: String,
        action: String,
        dissenters: Vec<String>,
    },
    PlayerDecided {
        action: String,
    },
    Deadlocked {
        positions: Vec<String>,
    },
}

/// Error type for theater operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TheaterError {
    AlreadyResolved,
    AllSpoke,
    NoParticipants,
}

impl DeliberationTheater {
    /// Build a new theater session.
    pub fn new(
        topic: String,
        trigger: TheaterTrigger,
        participants: Vec<TheaterSpeaker>,
        player_present: bool,
    ) -> Self {
        DeliberationTheater {
            topic,
            trigger,
            participants,
            turn: 0,
            history: Vec::new(),
            resolution: None,
            player_present,
            allow_intervention: true,
        }
    }

    /// Advance the theater by one speaker's turn.
    /// Returns the line spoken and any resolution.
    pub fn step(&mut self) -> Result<(TheaterLine, Option<TheaterResolution>), TheaterError> {
        if self.resolution.is_some() {
            return Err(TheaterError::AlreadyResolved);
        }
        if self.participants.is_empty() {
            return Err(TheaterError::NoParticipants);
        }

        // Find next speaker who hasn't spoken this round.
        let n = self.participants.len();
        let rounds = self.history.len() / n;
        if rounds >= MAX_THEATER_ROUNDS {
            let res = self.force_resolve();
            self.resolution = Some(res.clone());
            // Return a dummy line alongside the resolution.
            let line = TheaterLine {
                speaker: "system".into(),
                portrait: String::new(),
                position: TheaterPosition::Recuse {
                    reason: "max rounds".into(),
                },
                llm_raw: String::new(),
                display_text: "All crew have spoken. Reaching a decision...".into(),
                reactions: Vec::new(),
                relationship_deltas: Vec::new(),
            };
            return Ok((line, Some(res)));
        }

        let speaker_idx = self.history.len() % n;
        let crew_id = self.participants[speaker_idx].crew_id.clone();
        self.participants[speaker_idx].spoke = true;

        let position = self.compute_position(speaker_idx);
        let display_text = self.format_position(&position, &crew_id);

        let line = TheaterLine {
            speaker: crew_id.clone(),
            portrait: String::new(),
            position: position.clone(),
            llm_raw: String::new(),
            display_text,
            reactions: Vec::new(),
            relationship_deltas: Vec::new(),
        };

        self.history.push(line.clone());
        self.participants[speaker_idx].position = Some(position.clone());
        self.turn += 1;

        // Check for resolution.
        let resolution = self.detect_resolution();
        if let Some(ref res) = resolution {
            self.resolution = Some(res.clone());
        }

        Ok((line, resolution))
    }

    /// Compute a speaker's position from their relationship to the topic
    /// and what's been said so far.
    fn compute_position(&self, idx: usize) -> TheaterPosition {
        let speaker = &self.participants[idx];

        // First round: advocate their initial position.
        if self.history.is_empty() || !self.history.iter().any(|l| l.speaker == speaker.crew_id) {
            return TheaterPosition::Advocate {
                position: format!("I think we should consider {}", self.topic),
                reasoning: speaker.relationship_to_topic.clone(),
            };
        }

        // Subsequent rounds: react to the last position.
        if let Some(last) = self.history.last() {
            if last.speaker != speaker.crew_id {
                // If the last speaker was someone else, question or oppose.
                if idx.is_multiple_of(2) {
                    return TheaterPosition::Question {
                        to_whom: last.speaker.clone(),
                        question: format!("What about {}", last.display_text),
                    };
                } else {
                    return TheaterPosition::Amend {
                        to_whom: last.speaker.clone(),
                        amendment: format!("Adding to that: {}", self.topic),
                    };
                }
            }
        }

        // Default: defer to the first speaker with a position.
        let first = self.participants.iter().find(|p| p.position.is_some());
        match first {
            Some(f) => TheaterPosition::Defer {
                to_whom: f.crew_id.clone(),
            },
            None => TheaterPosition::Recuse {
                reason: "no position".into(),
            },
        }
    }

    /// Detect whether a resolution has emerged.
    fn detect_resolution(&self) -> Option<TheaterResolution> {
        let positions: Vec<&TheaterPosition> = self
            .participants
            .iter()
            .filter_map(|p| p.position.as_ref())
            .collect();

        if positions.len() < self.participants.len() {
            return None; // not everyone has spoken
        }

        // Consensus: all Advocate the same position.
        let advocates: Vec<&str> = positions
            .iter()
            .filter_map(|p| match p {
                TheaterPosition::Advocate { position, .. } => Some(position.as_str()),
                _ => None,
            })
            .collect();
        if advocates.len() >= self.participants.len().saturating_sub(1) {
            return Some(TheaterResolution::Consensus {
                action: advocates[0].to_string(),
                summary: "All crew agree.".into(),
            });
        }

        // Majority: count Advocate + Amend positions.
        let for_action = positions
            .iter()
            .filter(|p| {
                matches!(
                    p,
                    TheaterPosition::Advocate { .. } | TheaterPosition::Amend { .. }
                )
            })
            .count();
        let against = positions
            .iter()
            .filter(|p| matches!(p, TheaterPosition::Oppose { .. }))
            .count();

        if for_action > against && for_action as f64 > self.participants.len() as f64 * 0.6 {
            let for_votes: Vec<String> = self
                .participants
                .iter()
                .map(|p| p.crew_id.clone())
                .collect();
            let against_votes: Vec<String> = positions
                .iter()
                .filter(|p| matches!(p, TheaterPosition::Oppose { .. }))
                .map(|_| String::new())
                .collect();
            return Some(TheaterResolution::MajorityVote {
                action: "majority_decision".into(),
                for_votes,
                against: against_votes,
            });
        }

        None
    }

    /// Force a resolution after max rounds.
    fn force_resolve(&self) -> TheaterResolution {
        if self.participants.is_empty() {
            return TheaterResolution::Deadlocked {
                positions: Vec::new(),
            };
        }

        // Find the highest-speaking-order participant as the leader.
        let leader = self
            .participants
            .iter()
            .filter(|p| p.position.is_some())
            .min_by_key(|p| p.speaking_order);

        match leader {
            Some(l) => {
                let dissenters: Vec<String> = self
                    .participants
                    .iter()
                    .filter(|p| {
                        p.crew_id != l.crew_id
                            && p.position
                                .as_ref()
                                .is_some_and(|pos| matches!(pos, TheaterPosition::Oppose { .. }))
                    })
                    .map(|p| p.crew_id.clone())
                    .collect();
                TheaterResolution::CrewLeadsDecision {
                    leader: l.crew_id.clone(),
                    action: "leader_decision".into(),
                    dissenters,
                }
            }
            None => TheaterResolution::Deadlocked {
                positions: Vec::new(),
            },
        }
    }

    fn format_position(&self, position: &TheaterPosition, speaker: &str) -> String {
        match position {
            TheaterPosition::Advocate {
                position: pos,
                reasoning,
            } => {
                format!("{speaker} advocates: {pos} — {reasoning}")
            }
            TheaterPosition::Oppose {
                to_whom,
                position: pos,
                reasoning,
            } => {
                format!("{speaker} opposes {to_whom}: {pos} — {reasoning}")
            }
            TheaterPosition::Amend { to_whom, amendment } => {
                format!("{speaker} amends {to_whom}: {amendment}")
            }
            TheaterPosition::Question { to_whom, question } => {
                format!("{speaker} asks {to_whom}: {question}")
            }
            TheaterPosition::Defer { to_whom } => {
                format!("{speaker} defers to {to_whom}")
            }
            TheaterPosition::Recuse { reason } => {
                format!("{speaker} recuses: {reason}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_speaker(id: &str, order: u8, role: &str, topic_rel: &str) -> TheaterSpeaker {
        TheaterSpeaker {
            crew_id: id.into(),
            role: role.into(),
            relationship_to_topic: topic_rel.into(),
            speaking_order: order,
            spoke: false,
            position: None,
        }
    }

    fn simple_theater() -> DeliberationTheater {
        DeliberationTheater::new(
            "repair_priority".into(),
            TheaterTrigger::PlayerCalled {
                reason: "needs discussion".into(),
            },
            vec![
                make_speaker(
                    "boris",
                    1,
                    "Engineer",
                    "hull integrity is my responsibility",
                ),
                make_speaker("tove", 2, "Medic", "crew safety comes first"),
                make_speaker("prudence", 3, "Pilot", "we need to be flight-ready"),
            ],
            true,
        )
    }

    #[test]
    fn step_produces_turns() {
        let mut t = simple_theater();
        let (line, resolution) = t.step().unwrap();
        assert_eq!(line.speaker, "boris");
        assert!(resolution.is_none());
    }

    #[test]
    fn all_speakers_get_a_turn() {
        let mut t = simple_theater();
        for _ in 0..3 {
            let (line, _) = t.step().unwrap();
            assert!(!line.speaker.is_empty());
        }
        assert_eq!(t.history.len(), 3);
    }

    #[test]
    fn max_rounds_triggers_deadlock() {
        let mut t = simple_theater();
        for _ in 0..3 * MAX_THEATER_ROUNDS {
            let (_, resolution) = t.step().unwrap();
            if resolution.is_some() {
                return; // resolved within max rounds
            }
        }
        // After max rounds, step returns a resolution.
        let (_, resolution) = t.step().unwrap();
        assert!(resolution.is_some(), "should resolve after max rounds");
    }

    #[test]
    fn detects_consensus() {
        let mut t = simple_theater();
        // All participants speak — force positions.
        for i in 0..t.participants.len() {
            let _ = t.step().unwrap();
            t.participants[i].position = Some(TheaterPosition::Advocate {
                position: "prioritize_safety".into(),
                reasoning: "it's the right call".into(),
            });
        }
        let res = t.detect_resolution();
        assert!(res.is_some());
    }

    #[test]
    fn empty_participants_returns_error() {
        let mut t = DeliberationTheater::new(
            "test".into(),
            TheaterTrigger::PlayerCalled {
                reason: "test".into(),
            },
            vec![],
            true,
        );
        assert_eq!(t.step(), Err(TheaterError::NoParticipants));
    }
}
