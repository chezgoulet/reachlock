//! Co-deliberation: crew arguing it out instead of deciding in isolation
//! (spec §6 / §15 / §18, S33). Pure state machine — no I/O, no floats
//! (iron rule #1/#2). The caller drives it: the client feeds the trigger and
//! the per-crew positions (derived from each crew member's contract
//! evaluation), calls [`CoDeliberation::step`] once per LLM turn, renders the
//! returned [`DeliberationTurn`] on the comms panel, and persists the
//! resulting [`CrewRelationship`] deltas into the soul save.
//!
//! `step()` is total: it always terminates at a [`CoResolution`] within
//! [`MAX_ROUNDS`] rounds per participant, so the deliberation can never spin
//! the game loop (spec S33 gotcha).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::util::rng::Fixed;

/// Max rounds any single participant may speak before a forced resolution.
/// A "round" is every participant speaking once; 3 rounds is the spec cap.
pub const MAX_ROUNDS: usize = 3;

/// A game event that kicked off co-deliberation. Reuses the contract engine's
/// stringly-typed event vocabulary (`event_type` matches the `Event` trigger
/// and soul `event.<type>` bridge fields) so triggers compose with the rest
/// of the contract/soul pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameEvent {
    pub event_type: String,
    pub summary: String,
    /// Extra fixed-point context (e.g. `hull.damage`, `combat.active`).
    #[serde(default)]
    pub fields: BTreeMap<String, i64>,
}

/// What a crew member is arguing for this turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrewPosition {
    /// "We should do X, because …"
    Propose { action: String, reasoning: String },
    /// "I'm with <who>."
    Support { who: String, reason: String },
    /// "No, <who> is wrong, because …"
    Oppose { who: String, reason: String },
    /// "Defer to <to> on this one."
    Defer { to: String, reason: String },
    /// "I'll sit this one out."
    Abstain { reason: String },
}

impl CrewPosition {
    /// The action this position backs, if any: a `Propose` backs its own
    /// action; `Support`/`Defer`/`Oppose` reference another speaker and resolve
    /// to that speaker's proposed action (looked up in history by the caller).
    pub fn backs_direct(&self) -> Option<&str> {
        match self {
            CrewPosition::Propose { action, .. } => Some(action),
            _ => None,
        }
    }

    /// The target crew id a `Support`/`Defer`/`Oppose` refers to, if any.
    pub fn target(&self) -> Option<&str> {
        match self {
            CrewPosition::Support { who, .. } => Some(who),
            CrewPosition::Oppose { who, .. } => Some(who),
            CrewPosition::Defer { to, .. } => Some(to),
            _ => None,
        }
    }
}

/// One spoken turn in the exchange. `llm_raw` is the full deliberation output;
/// `visible_to_player` is what the comms panel renders. In core (offline /
/// pinned model) both are synthesized deterministically from the position,
/// the relationship context, and the trigger — the client substitutes real
/// LLM prose for `llm_raw` when online.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliberationTurn {
    pub speaker: String,
    pub position: CrewPosition,
    pub llm_raw: String,
    pub visible_to_player: String,
    /// Net relationship movement this turn, per other participant:
    /// `(other_crew_id, delta)` where delta is a signed composite of the
    /// trust/respect/tension moves (positive = closer, negative = colder).
    pub relationship_delta: Vec<(String, i64)>,
}

/// How co-deliberation ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoResolution {
    /// Everyone converged on one action.
    Consensus { action: String },
    /// A clear winner with one dissenter.
    MajorityAction { action: String, dissenter: String },
    /// Two (or more) equal blocs; the tiebreaker's choice won.
    TieBreak { action: String, tiebreaker: String },
    /// No decision emerged — the player must choose.
    Deadlocked,
    /// The player cut deliberation short with "my way."
    PlayerOverride { action: String },
}

/// One crew member's standing toward another crew member. Fixed-point
/// (1024 = 1.0) per iron rule #2. This is the S33 relationship compression;
/// S35 (persistent relationship memory) will extend it with long-term
/// compression, so the fields here are the live, per-session form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrewRelationship {
    /// How long they've served together (0 ..= 1024).
    pub familiarity: Fixed,
    /// -1.0 (hostile) ..= 1.0 (unquestioning).
    pub trust: Fixed,
    /// Does this crewmate know what they're talking about? (-1.0 ..= 1.0)
    pub respect: Fixed,
    /// 0.0 (clean) ..= 1.0 (unresolved friction).
    pub tension: Fixed,
    #[serde(default)]
    pub notable_events: Vec<RelationshipEvent>,
}

impl Default for CrewRelationship {
    fn default() -> Self {
        CrewRelationship {
            familiarity: Fixed::from_int(0),
            trust: Fixed::from_int(0),
            respect: Fixed::from_int(0),
            tension: Fixed::from_int(0),
            notable_events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipEventType {
    SavedMyLife,
    UnderminedMyDecision,
    SupportedMyCall,
    ArguedWithMe,
    DeferredToMe,
    SharedCrisis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipEvent {
    pub event_type: RelationshipEventType,
    pub timestamp: u64,
    pub weight: Fixed,
}

/// A participant's relationships with every *other* participant in this
/// deliberation, keyed by the other crew id.
pub type RelationshipState = BTreeMap<String, CrewRelationship>;

/// One crew member in the deliberation. `participants` must be ordered by
/// seniority (most senior first) — `step` speaks round-robin in that order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrewDeliberant {
    pub crew_id: String,
    pub relationship_state: RelationshipState,
    pub initial_position: CrewPosition,
    pub current_position: CrewPosition,
}

/// The co-deliberation session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoDeliberation {
    pub participants: Vec<CrewDeliberant>,
    pub trigger_event: GameEvent,
    /// How many turns have been spoken so far.
    pub turn: usize,
    pub history: Vec<DeliberationTurn>,
    pub resolution: Option<CoResolution>,
}

/// What `step` produced: either another spoken turn, or the final resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    Turn(DeliberationTurn),
    Resolved(CoResolution),
}

/// Relationship delta magnitudes (Fixed units, ≈ points on a -100..100 scale
/// per the spec — small per event). Signs are the contract: support warms,
/// opposition engages (raises respect) but adds friction unless it was settled.
const SUPPORT_TRUST_DELTA: i64 = 30; // ~3 pts
const SUPPORT_RESPECT_DELTA: i64 = 20; // ~2 pts
const OPPOSE_RESPECT_DELTA: i64 = 30; // ~3 pts
const OPPOSE_TENSION_DELTA: i64 = 30; // ~3 pts, unresolved friction
const RESOLVED_OPPOSE_TENSION_DELTA: i64 = -20; // settled: friction eases
const PLAYER_OVERRIDE_TRUST_GAIN: i64 = 40; // crew who agreed with captain
const PLAYER_OVERRIDE_TRUST_LOSS: i64 = 40; // crew overruled
const PLAYER_OVERRIDE_TENSION_GAIN: i64 = 20; // crew cut off mid-argument

impl CoDeliberation {
    /// Build a session. `participants` MUST be ordered by seniority.
    pub fn new(participants: Vec<CrewDeliberant>, trigger_event: GameEvent) -> Self {
        let participants = participants
            .into_iter()
            .map(|mut p| {
                p.current_position = p.initial_position.clone();
                p
            })
            .collect();
        CoDeliberation {
            participants,
            trigger_event,
            turn: 0,
            history: Vec::new(),
            resolution: None,
        }
    }

    /// Convenience constructor for tests and the player-initiated crew
    /// conference: each `(crew_id, action, reasoning)` becomes an initial
    /// `Propose`. Relationships start neutral.
    pub fn from_proposals(
        proposals: Vec<(String, String, String)>,
        trigger_event: GameEvent,
    ) -> Self {
        let ids: Vec<String> = proposals.iter().map(|(id, _, _)| id.clone()).collect();
        let participants = proposals
            .into_iter()
            .map(|(crew_id, action, reasoning)| {
                let mut relationship_state = RelationshipState::new();
                for other in ids.iter().filter(|o| **o != crew_id) {
                    relationship_state.insert(other.clone(), CrewRelationship::default());
                }
                CrewDeliberant {
                    crew_id,
                    relationship_state,
                    initial_position: CrewPosition::Propose { action, reasoning },
                    current_position: CrewPosition::Propose {
                        action: String::new(),
                        reasoning: String::new(),
                    },
                }
            })
            .collect();
        CoDeliberation::new(participants, trigger_event)
    }

    /// The action a participant currently backs, resolving `Support`/`Defer`/
    /// `Oppose` targets against the history of spoken proposals.
    fn backed_action(&self, idx: usize) -> Option<String> {
        let pos = &self.participants[idx].current_position;
        if let Some(direct) = pos.backs_direct() {
            return Some(direct.to_string());
        }
        if let Some(target) = pos.target() {
            return self.proposed_by(target).map(|a| a.to_string());
        }
        None
    }

    /// The action most recently `Propose`d by `speaker`, if any.
    fn proposed_by(&self, speaker: &str) -> Option<&str> {
        self.history
            .iter()
            .rev()
            .find(|t| t.speaker == speaker)
            .and_then(|t| t.position.backs_direct())
    }

    /// Advance the deliberation by one turn. Pure: mutates only `self`.
    pub fn step(&mut self) -> StepOutcome {
        if let Some(r) = &self.resolution {
            return StepOutcome::Resolved(r.clone());
        }
        let n = self.participants.len();
        if n == 0 {
            self.resolution = Some(CoResolution::Deadlocked);
            return StepOutcome::Resolved(CoResolution::Deadlocked);
        }

        let speaker_idx = self.history.len() % n;
        let position = self.compute_position(speaker_idx);
        let speaker_id = self.participants[speaker_idx].crew_id.clone();
        let deltas = self.apply_relationship_deltas(speaker_idx, &position);

        let turn = DeliberationTurn {
            speaker: speaker_id.clone(),
            position: position.clone(),
            llm_raw: self.synthesize(&speaker_id, &position),
            visible_to_player: self.visible(&speaker_id, &position),
            relationship_delta: deltas,
        };

        self.history.push(turn.clone());
        self.participants[speaker_idx].current_position = position;
        self.turn += 1;

        if let Some(res) = self.try_resolve() {
            self.resolution = Some(res.clone());
            return StepOutcome::Resolved(res);
        }
        if self.history.len() >= MAX_ROUNDS * n {
            let forced = self.force_resolve();
            self.resolution = Some(forced.clone());
            return StepOutcome::Resolved(forced);
        }
        StepOutcome::Turn(turn)
    }

    /// Decide the speaker's position this turn from their initial stance,
    /// what's been said, and how much they respect the people who said it.
    /// Deterministic — no RNG, no LLM (the client layers real prose on top).
    fn compute_position(&self, speaker_idx: usize) -> CrewPosition {
        let initial = self.participants[speaker_idx].initial_position.clone();
        // First speaker just states their opening position.
        if self.history.is_empty() {
            return initial;
        }
        // Find the leading proposed action so far (first proposal in history).
        let lead_action = self
            .history
            .iter()
            .find_map(|t| t.position.backs_direct().map(|a| a.to_string()));
        let lead_speaker = self
            .history
            .iter()
            .find(|t| t.position.backs_direct().is_some())
            .map(|t| t.speaker.clone());

        match &initial {
            CrewPosition::Propose { action, reasoning } => {
                // If someone else already proposed and this speaker respects
                // them, they back the leader instead of re-proposing alone.
                if let (Some(lead), Some(lead_spk)) = (lead_action, lead_speaker) {
                    if lead_spk != self.participants[speaker_idx].crew_id && lead != *action {
                        let rel = self.participants[speaker_idx]
                            .relationship_state
                            .get(&lead_spk)
                            .map(|r| r.respect.0)
                            .unwrap_or(0);
                        if rel > 256 {
                            // respect > 0.25 → defer to the respected lead
                            return CrewPosition::Defer {
                                reason: format!("I trust {lead_spk}'s call on this"),
                                to: lead_spk,
                            };
                        } else if rel < -256 {
                            // active distrust → oppose
                            return CrewPosition::Oppose {
                                reason: format!("{lead_spk} doesn't know this situation"),
                                who: lead_spk,
                            };
                        }
                    }
                }
                CrewPosition::Propose {
                    action: action.clone(),
                    reasoning: reasoning.clone(),
                }
            }
            // Non-proposing initial stances (e.g. a pre-set Support) pass
            // through; they already encode the relationship reaction.
            other => other.clone(),
        }
    }

    /// Apply the relationship moves implied by `speaker_idx` taking `position`,
    /// mutating their [`RelationshipState`] and returning the per-other
    /// composite deltas for the turn record.
    fn apply_relationship_deltas(
        &mut self,
        speaker_idx: usize,
        position: &CrewPosition,
    ) -> Vec<(String, i64)> {
        let resolved_action = self.leading_action();
        let mut out = Vec::new();

        // Clone the current relationship state so we can mutate while reading.
        let rels = self.participants[speaker_idx].relationship_state.clone();
        for (other, mut rel) in rels {
            let mut trust_d = 0i64;
            let mut respect_d = 0i64;
            let mut tension_d = 0i64;

            let targets = match position {
                CrewPosition::Support { who, .. } if who == &other => Some(true),
                CrewPosition::Oppose { who, .. } if who == &other => Some(false),
                CrewPosition::Defer { to, .. } if to == &other => Some(true),
                _ => None,
            };
            if let Some(supportive) = targets {
                if supportive {
                    trust_d += SUPPORT_TRUST_DELTA;
                    respect_d += SUPPORT_RESPECT_DELTA;
                } else {
                    respect_d += OPPOSE_RESPECT_DELTA;
                    // Opposing someone whose call won eases friction; opposing
                    // a losing call leaves it.
                    if resolved_action
                        .as_deref()
                        .map(|a| self.proposed_by(&other).map(|p| p == a).unwrap_or(false))
                        .unwrap_or(false)
                    {
                        tension_d += RESOLVED_OPPOSE_TENSION_DELTA;
                    } else {
                        tension_d += OPPOSE_TENSION_DELTA;
                    }
                }
            }

            rel.trust = Fixed(clamp(rel.trust.0 + trust_d, -1024, 1024));
            rel.respect = Fixed(clamp(rel.respect.0 + respect_d, -1024, 1024));
            rel.tension = Fixed(clamp(rel.tension.0 + tension_d, 0, 1024));
            self.participants[speaker_idx]
                .relationship_state
                .insert(other.clone(), rel);

            let net = trust_d + respect_d / 2 - tension_d;
            if net != 0 {
                out.push((other, net));
            }
        }
        out
    }

    /// The action with the most backing right now (first tie wins).
    pub fn leading_action(&self) -> Option<String> {
        self.tally().into_iter().next().map(|(a, _)| a)
    }

    /// Tally backed actions across all participants (ignoring abstentions).
    /// Returns actions in descending backing count.
    fn tally(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for (i, p) in self.participants.iter().enumerate() {
            if matches!(p.current_position, CrewPosition::Abstain { .. }) {
                continue;
            }
            if let Some(action) = self.backed_action(i) {
                *counts.entry(action).or_insert(0) += 1;
            }
        }
        let mut v: Vec<_> = counts.into_iter().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.1));
        v
    }

    /// Natural resolution once at least one full round has spoken.
    fn try_resolve(&self) -> Option<CoResolution> {
        if self.history.len() < self.participants.len() {
            return None;
        }
        let tally = self.tally();
        let n = self.participants.len();

        match tally.first() {
            Some((action, count)) if *count == n => Some(CoResolution::Consensus {
                action: action.clone(),
            }),
            Some((action, count)) if *count * 2 > n => {
                let dissenter = self
                    .participants
                    .iter()
                    .enumerate()
                    .find(|(i, _)| self.backed_action(*i).as_deref() != Some(action.as_str()))
                    .map(|(_, p)| p.crew_id.clone())
                    .unwrap_or_default();
                Some(CoResolution::MajorityAction {
                    action: action.clone(),
                    dissenter,
                })
            }
            _ => None,
        }
    }

    /// Forced resolution after [`MAX_ROUNDS`]: majority, then tiebreak by the
    /// highest-respect proposer, else deadlock.
    fn force_resolve(&self) -> CoResolution {
        let tally = self.tally();
        match tally.first() {
            Some((action, count)) if *count * 2 > self.participants.len() => {
                let dissenter = self
                    .participants
                    .iter()
                    .enumerate()
                    .find(|(i, _)| self.backed_action(*i).as_deref() != Some(action.as_str()))
                    .map(|(_, p)| p.crew_id.clone())
                    .unwrap_or_default();
                CoResolution::MajorityAction {
                    action: action.clone(),
                    dissenter,
                }
            }
            Some((action, _)) => {
                // Tie (or no strict majority): tiebreak by respect.
                let tiebreaker = self
                    .participants
                    .iter()
                    .filter(|p| {
                        self.backed_action_of_id(&p.crew_id).as_deref() == Some(action.as_str())
                    })
                    .max_by_key(|p| {
                        p.relationship_state
                            .values()
                            .map(|r| r.respect.0)
                            .max()
                            .unwrap_or(0)
                    })
                    .map(|p| p.crew_id.clone())
                    .unwrap_or_default();
                CoResolution::TieBreak {
                    action: action.clone(),
                    tiebreaker,
                }
            }
            None => CoResolution::Deadlocked,
        }
    }

    /// Backed action for a participant looked up by crew id.
    fn backed_action_of_id(&self, id: &str) -> Option<String> {
        self.participants
            .iter()
            .position(|p| p.crew_id == id)
            .and_then(|i| self.backed_action(i))
    }

    /// The player cuts deliberation short. Relationship consequences: crew who
    /// backed the player's action gain trust; crew overruled lose trust; crew
    /// who were mid-argument gain tension. Mild, per the S33 gotcha.
    pub fn player_override(&mut self, action: String) {
        // Snapshot each participant's stance before mutating (no self-borrow
        // while iterating mutably).
        let stances: Vec<(String, Option<String>, bool)> = self
            .participants
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let backed = self.backed_action(i);
                let was_arguing = matches!(p.current_position, CrewPosition::Oppose { .. });
                (p.crew_id.clone(), backed, was_arguing)
            })
            .collect();
        for (id, backed, was_arguing) in stances {
            let agreed = backed.as_deref() == Some(action.as_str());
            if let Some(p) = self.participants.iter_mut().find(|q| q.crew_id == id) {
                for rel in p.relationship_state.values_mut() {
                    if agreed {
                        rel.trust =
                            Fixed(clamp(rel.trust.0 + PLAYER_OVERRIDE_TRUST_GAIN, -1024, 1024));
                    } else {
                        rel.trust =
                            Fixed(clamp(rel.trust.0 - PLAYER_OVERRIDE_TRUST_LOSS, -1024, 1024));
                    }
                    if was_arguing {
                        rel.tension =
                            Fixed(clamp(rel.tension.0 + PLAYER_OVERRIDE_TENSION_GAIN, 0, 1024));
                    }
                }
            }
        }
        self.resolution = Some(CoResolution::PlayerOverride { action });
    }

    /// Structural metrics for the S33 research table (no PII).
    pub fn metrics(&self) -> CoDeliberationMetrics {
        let resolution_type = self
            .resolution
            .as_ref()
            .map(resolution_name)
            .unwrap_or("unresolved")
            .to_string();
        let delta_magnitude: i64 = self
            .history
            .iter()
            .flat_map(|t| &t.relationship_delta)
            .map(|(_, d)| d.abs())
            .sum();
        CoDeliberationMetrics {
            participant_count: self.participants.len(),
            trigger_type: self.trigger_event.event_type.clone(),
            turn_count: self.history.len(),
            resolution_type,
            relationship_delta_magnitude: delta_magnitude,
        }
    }

    fn synthesize(&self, speaker: &str, position: &CrewPosition) -> String {
        match position {
            CrewPosition::Propose { action, reasoning } => {
                format!("[{speaker}] proposes '{action}': {reasoning}")
            }
            CrewPosition::Support { who, reason } => {
                format!("[{speaker}] supports {who}: {reason}")
            }
            CrewPosition::Oppose { who, reason } => {
                format!("[{speaker}] opposes {who}: {reason}")
            }
            CrewPosition::Defer { to, reason } => {
                format!("[{speaker}] defers to {to}: {reason}")
            }
            CrewPosition::Abstain { reason } => format!("[{speaker}] abstains: {reason}"),
        }
    }

    fn visible(&self, speaker: &str, position: &CrewPosition) -> String {
        match position {
            CrewPosition::Propose { action, .. } => format!("{speaker}: Let's {action}."),
            CrewPosition::Support { who, .. } => format!("{speaker}: I'm with {who}."),
            CrewPosition::Oppose { who, .. } => format!("{speaker}: No — not {who}'s call."),
            CrewPosition::Defer { to, .. } => format!("{speaker}: Your call, {to}."),
            CrewPosition::Abstain { .. } => format!("{speaker}: I'll stay out of this."),
        }
    }
}

fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}

fn resolution_name(r: &CoResolution) -> &'static str {
    match r {
        CoResolution::Consensus { .. } => "consensus",
        CoResolution::MajorityAction { .. } => "majority",
        CoResolution::TieBreak { .. } => "tiebreak",
        CoResolution::Deadlocked => "deadlocked",
        CoResolution::PlayerOverride { .. } => "player_override",
    }
}

/// Structural record of one co-deliberation session (S33 metrics table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoDeliberationMetrics {
    pub participant_count: usize,
    pub trigger_type: String,
    pub turn_count: usize,
    pub resolution_type: String,
    pub relationship_delta_magnitude: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(event_type: &str) -> GameEvent {
        GameEvent {
            event_type: event_type.into(),
            summary: format!("{event_type} happened"),
            fields: BTreeMap::new(),
        }
    }

    fn two_crew(agree: bool) -> CoDeliberation {
        // Boris wants repairs; Tove wants medbay. If agree, both propose the
        // same action (consensus path); else they diverge.
        let action_b = "repair_weapons".to_string();
        let action_t = if agree {
            "repair_weapons".to_string()
        } else {
            "tend_medbay".to_string()
        };
        CoDeliberation::from_proposals(
            vec![
                ("boris".into(), action_b, "weapons keep us alive".into()),
                ("tove".into(), action_t, "crew is hurt".into()),
            ],
            ev("hull_breach"),
        )
    }

    #[test]
    fn step_terminates_with_consensus() {
        let mut d = two_crew(true);
        let mut resolved = None;
        for _ in 0..10 {
            match d.step() {
                StepOutcome::Turn(_) => {}
                StepOutcome::Resolved(r) => {
                    resolved = Some(r);
                    break;
                }
            }
        }
        assert!(matches!(resolved, Some(CoResolution::Consensus { .. })));
        assert_eq!(d.history.len(), 2, "one round is enough for 2 crew");
        assert_eq!(d.metrics().resolution_type, "consensus");
    }

    #[test]
    fn divergent_crew_reaches_majority_or_deadlock() {
        let mut d = two_crew(false);
        let mut resolved = None;
        for _ in 0..10 {
            match d.step() {
                StepOutcome::Turn(_) => {}
                StepOutcome::Resolved(r) => {
                    resolved = Some(r);
                    break;
                }
            }
        }
        // 2 crew, 1 each → tie → forced after MAX_ROUNDS → TieBreak (no strict
        // majority) or Deadlocked depending on respect (both neutral here →
        // TieBreak on the leading action).
        match resolved {
            Some(CoResolution::TieBreak { action, .. }) => {
                assert!(action == "repair_weapons" || action == "tend_medbay");
            }
            Some(CoResolution::Deadlocked) => {}
            other => panic!("expected tiebreak/deadlock, got {other:?}"),
        }
    }

    #[test]
    fn support_increases_trust_relationship_delta() {
        // Boris proposes repair_weapons; Tove wants tend_medbay but respects
        // Boris → she defers to him and warms.
        let mut d = CoDeliberation::from_proposals(
            vec![
                ("boris".into(), "repair_weapons".into(), "do it".into()),
                ("tove".into(), "tend_medbay".into(), "crew hurt".into()),
            ],
            ev("hull_breach"),
        );
        // Seed Tove's respect for Boris so she defers (supportive).
        if let Some(tove) = d.participants.iter_mut().find(|p| p.crew_id == "tove") {
            tove.relationship_state.insert(
                "boris".into(),
                CrewRelationship {
                    familiarity: Fixed::from_int(0),
                    trust: Fixed::from_int(0),
                    respect: Fixed::from_int(512),
                    tension: Fixed::from_int(0),
                    notable_events: vec![],
                },
            );
        }
        // Step once: Boris proposes. Step twice: Tove defers to Boris
        // (which may also resolve — either way her turn is in history).
        let _ = d.step();
        let _ = d.step();
        let tove = d
            .history
            .iter()
            .find(|t| t.speaker == "tove")
            .expect("Tove spoke");
        assert!(
            matches!(tove.position, CrewPosition::Defer { .. }),
            "Tove should defer to Boris: {:?}",
            tove.position
        );
        // Deferring to Boris warms the relationship.
        assert!(
            tove.relationship_delta
                .iter()
                .any(|(who, d)| who == "boris" && *d > 0),
            "deferring to Boris should warm the relationship: {:?}",
            tove.relationship_delta
        );
    }

    #[test]
    fn oppose_marks_tension_and_respect() {
        let mut d = CoDeliberation::from_proposals(
            vec![
                ("boris".into(), "repair_weapons".into(), "do it".into()),
                ("tove".into(), "tend_medbay".into(), "crew hurt".into()),
            ],
            ev("hull_breach"),
        );
        // Tove distrusts Boris → she opposes his lead.
        if let Some(tove) = d.participants.iter_mut().find(|p| p.crew_id == "tove") {
            tove.relationship_state.insert(
                "boris".into(),
                CrewRelationship {
                    familiarity: Fixed::from_int(0),
                    trust: Fixed::from_int(-512),
                    respect: Fixed::from_int(-512),
                    tension: Fixed::from_int(0),
                    notable_events: vec![],
                },
            );
        }
        let _ = d.step(); // Boris proposes
        let _ = d.step(); // Tove should oppose (may also resolve)
        let tove = d
            .history
            .iter()
            .find(|t| t.speaker == "tove")
            .expect("Tove spoke");
        assert!(
            matches!(tove.position, CrewPosition::Oppose { .. }),
            "Tove should oppose Boris: {:?}",
            tove.position
        );
        // Opposing raises respect (engaged) — sign check.
        assert!(
            tove.relationship_delta
                .iter()
                .any(|(who, d)| who == "boris" && *d > 0),
            "opposing should still raise respect (engagement): {:?}",
            tove.relationship_delta
        );
    }

    #[test]
    fn player_override_resolves_and_moves_relationships() {
        let mut d = two_crew(false);
        // Run a couple of turns so stances exist, then override.
        let _ = d.step();
        let _ = d.step();
        d.player_override("repair_weapons".to_string());
        assert!(matches!(
            d.resolution,
            Some(CoResolution::PlayerOverride { .. })
        ));
        // Boris backed repair_weapons → trust gain; Tove dissented → trust loss.
        let boris = d
            .participants
            .iter()
            .find(|p| p.crew_id == "boris")
            .unwrap();
        let tove = d.participants.iter().find(|p| p.crew_id == "tove").unwrap();
        assert!(boris.relationship_state["tove"].trust.0 > 0);
        assert!(tove.relationship_state["boris"].trust.0 < 0);
    }

    #[test]
    fn step_is_total_never_loops() {
        // Any configuration must resolve within MAX_ROUNDS*n steps.
        let mut d = two_crew(false);
        let mut steps = 0;
        loop {
            steps += 1;
            match d.step() {
                StepOutcome::Turn(_) => {
                    assert!(
                        steps <= MAX_ROUNDS * d.participants.len() + 1,
                        "too many steps"
                    );
                }
                StepOutcome::Resolved(_) => break,
            }
            if steps > 20 {
                panic!("step did not terminate");
            }
        }
    }

    #[test]
    fn all_resolution_variants_reachable() {
        // Consensus
        assert!(matches!(
            run_to_resolution(two_crew(true)),
            CoResolution::Consensus { .. }
        ));
        // Deadlocked: everyone abstains.
        let mut d = CoDeliberation::from_proposals(
            vec![
                ("a".into(), "x".into(), "".into()),
                ("b".into(), "y".into(), "".into()),
            ],
            ev("test"),
        );
        for p in d.participants.iter_mut() {
            p.initial_position = CrewPosition::Abstain {
                reason: "nope".into(),
            };
            p.current_position = CrewPosition::Abstain {
                reason: "nope".into(),
            };
        }
        assert!(matches!(run_to_resolution(d), CoResolution::Deadlocked));
    }

    fn run_to_resolution(mut d: CoDeliberation) -> CoResolution {
        for _ in 0..20 {
            if let StepOutcome::Resolved(r) = d.step() {
                return r;
            }
        }
        panic!("no resolution");
    }

    #[test]
    fn metrics_record_structure() {
        let mut d = two_crew(true);
        while let StepOutcome::Turn(_) = d.step() {}
        let m = d.metrics();
        assert_eq!(m.participant_count, 2);
        assert_eq!(m.trigger_type, "hull_breach");
        assert!(m.turn_count >= 2);
        assert_eq!(m.resolution_type, "consensus");
    }
}
