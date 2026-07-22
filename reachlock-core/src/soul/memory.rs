//! Persistent relationship memory (S35): per-crew-pair memory store with
//! event tracking, trust trajectory, and time-based fading. All gameplay
//! values are fixed-point (iron rule #2).

use serde::{Deserialize, Serialize};

use crate::util::rng::Fixed;

/// Maximum significant events stored per relationship before oldest are
/// collapsed into a single "Early history" event (spec S35 gotcha).
pub const MAX_SIGNIFICANT_EVENTS: usize = 200;

/// Maximum player-noted events per relationship (never fade).
pub const MAX_PLAYER_NOTED: usize = 50;

/// Maximum trust trajectory inflection points before oldest are collapsed.
pub const MAX_TRAJECTORY_POINTS: usize = 20;

/// One crew-to-crew relationship's full memory. Keyed by sorted participant
/// pair `(id_a, id_b)` in the soul store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipMemory {
    pub participants: (String, String),
    pub first_interaction: u64,
    pub interaction_count: u64,
    pub conversation_count: u64,
    pub crisis_count: u64,
    pub significant_events: Vec<SignificantEvent>,
    pub compressed_context: Option<String>,
    pub compression_version: u32,
    pub last_compression_tick: u64,
    pub trust_trajectory: TrustTrajectory,
}

/// One event that shaped a relationship.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignificantEvent {
    pub tick: u64,
    pub event_type: SignificantEventType,
    pub summary: String,
    pub weight: Fixed,
    pub fading: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignificantEventType {
    // Positive
    SavedMyLife,
    FollowedMyAdvice,
    ShowedTrust,
    DefendedMe,
    SharedSuccess,
    // Negative
    OverruledMe,
    EndangeredMe,
    BrokeTrust,
    AbandonedMission,
    WasWrong { consequences: String },
    // Neutral
    FirstMet,
    Reunited,
    SharedSilence,
    ObservedFromAfar,
    // Player-custom (never fades)
    PlayerNoted { note: String },
}

/// How trust has changed over the relationship's lifetime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrustTrajectory {
    /// Inflection points: (tick, trust_value). Up to `MAX_TRAJECTORY_POINTS`.
    pub points: Vec<(u64, Fixed)>,
    pub trend: TrustTrend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustTrend {
    Rising { rate: Fixed },
    Falling { rate: Fixed },
    Stable,
    Volatile { amplitude: Fixed },
}

impl RelationshipMemory {
    /// Build a new relationship memory for a first interaction.
    pub fn new(a: String, b: String, tick: u64) -> Self {
        let mut participants = (a, b);
        if participants.0 > participants.1 {
            participants = (participants.1.clone(), participants.0.clone());
        }
        RelationshipMemory {
            participants,
            first_interaction: tick,
            interaction_count: 1,
            conversation_count: 0,
            crisis_count: 0,
            significant_events: Vec::new(),
            compressed_context: None,
            compression_version: 0,
            last_compression_tick: tick,
            trust_trajectory: TrustTrajectory {
                points: Vec::new(),
                trend: TrustTrend::Stable,
            },
        }
    }

    /// Record a significant event. If over cap, collapses 10 oldest into one.
    /// Player-noted events never fade but count toward a separate cap.
    pub fn record_event(&mut self, event: SignificantEvent, tick: u64) {
        let is_player_noted = matches!(&event.event_type, SignificantEventType::PlayerNoted { .. });
        if is_player_noted {
            let count = self
                .significant_events
                .iter()
                .filter(|e| matches!(&e.event_type, SignificantEventType::PlayerNoted { .. }))
                .count();
            if count >= MAX_PLAYER_NOTED {
                return; // cap reached
            }
        }
        self.significant_events.push(event);
        if self.significant_events.len() > MAX_SIGNIFICANT_EVENTS {
            self.collapse_oldest(tick);
        }
    }

    /// Collapse the 10 oldest events into a single "Early history" event.
    fn collapse_oldest(&mut self, tick: u64) {
        let mut rest: Vec<SignificantEvent> = self.significant_events.drain(..).collect();
        if rest.len() <= 10 {
            return;
        }
        let oldest: Vec<SignificantEvent> = rest.drain(..10).collect();
        let summary = format!(
            "Early history: {} events (tick {}–{})",
            oldest.len(),
            oldest.first().map(|e| e.tick).unwrap_or(tick),
            oldest.last().map(|e| e.tick).unwrap_or(tick),
        );
        let total_weight: i64 = oldest.iter().map(|e| e.weight.0).sum();
        rest.insert(
            0,
            SignificantEvent {
                tick,
                event_type: SignificantEventType::ObservedFromAfar,
                summary,
                weight: Fixed(total_weight.clamp(0, 1024)),
                fading: true,
            },
        );
        self.significant_events = rest;
    }

    /// Apply time-based fading: events older than 20 game-hours decay
    /// linearly from current weight to 0 over 60 game-hours.
    pub fn apply_fading(&mut self, current_tick: u64, ticks_per_hour: u64) {
        let fade_start = 20 * ticks_per_hour;
        let fade_duration = 60 * ticks_per_hour;
        for event in &mut self.significant_events {
            if matches!(&event.event_type, SignificantEventType::PlayerNoted { .. }) {
                continue; // player-noted never fades
            }
            let age = current_tick.saturating_sub(event.tick);
            if age < fade_start {
                event.fading = false;
                continue;
            }
            event.fading = true;
            if age >= fade_start + fade_duration {
                event.weight = Fixed(0);
            } else {
                let decay = (age - fade_start) as i64;
                let duration = fade_duration as i64;
                let remaining = event.weight.0 * (duration - decay) / duration;
                event.weight = Fixed(remaining.max(0));
            }
        }
    }

    /// Append a trust trajectory point and maintain cap. If over 20 points,
    /// collapse oldest 10 into a single average point.
    pub fn record_trust(&mut self, tick: u64, trust_value: Fixed) {
        self.trust_trajectory.points.push((tick, trust_value));
        if self.trust_trajectory.points.len() > MAX_TRAJECTORY_POINTS {
            let mut rest: Vec<(u64, Fixed)> = self.trust_trajectory.points.drain(..).collect();
            let oldest: Vec<(u64, Fixed)> = rest.drain(..10).collect();
            let avg: i64 = oldest.iter().map(|(_, v)| v.0).sum::<i64>() / oldest.len() as i64;
            rest.insert(0, (oldest[0].0, Fixed(avg)));
            self.trust_trajectory.points = rest;
        }
        self.compute_trend();
    }

    /// Compute TrustTrend from last 10 trajectory points via integer math.
    fn compute_trend(&mut self) {
        let pts = &self.trust_trajectory.points;
        if pts.len() < 2 {
            self.trust_trajectory.trend = TrustTrend::Stable;
            return;
        }
        let recent: Vec<&(u64, Fixed)> = pts.iter().rev().take(10).collect();
        let changes: Vec<i64> = recent.windows(2).map(|w| w[0].1 .0 - w[1].1 .0).collect();
        let sum: i64 = changes.iter().sum();
        let abs_sum: i64 = changes.iter().map(|c| c.abs()).sum();
        let n = changes.len() as i64;

        if sum.abs() * 3 < abs_sum {
            self.trust_trajectory.trend = TrustTrend::Volatile {
                amplitude: Fixed(abs_sum / n),
            };
        } else if sum > 10 * n {
            self.trust_trajectory.trend = TrustTrend::Rising {
                rate: Fixed(sum / n),
            };
        } else if sum < -10 * n {
            self.trust_trajectory.trend = TrustTrend::Falling {
                rate: Fixed(sum.abs() / n),
            };
        } else {
            self.trust_trajectory.trend = TrustTrend::Stable;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(tick: u64, etype: SignificantEventType, weight: i64) -> SignificantEvent {
        SignificantEvent {
            tick,
            event_type: etype,
            summary: "test event".into(),
            weight: Fixed(weight),
            fading: false,
        }
    }

    #[test]
    fn new_memory_starts_empty() {
        let m = RelationshipMemory::new("a".into(), "b".into(), 100);
        assert_eq!(m.interaction_count, 1);
        assert!(m.significant_events.is_empty());
        assert_eq!(m.participants.0, "a");
        assert_eq!(m.participants.1, "b");
    }

    #[test]
    fn record_event_appends() {
        let mut m = RelationshipMemory::new("a".into(), "b".into(), 100);
        m.record_event(event(150, SignificantEventType::SavedMyLife, 800), 150);
        assert_eq!(m.significant_events.len(), 1);
    }

    #[test]
    fn cap_at_200_collapses_oldest() {
        let mut m = RelationshipMemory::new("a".into(), "b".into(), 100);
        // Add 210 events — should trigger collapse at 201st.
        for i in 0..210 {
            m.record_event(
                event(150 + i as u64, SignificantEventType::ObservedFromAfar, 100),
                150 + i as u64,
            );
        }
        // Should have collapsed: 200 cap, but collapse removes 10, so 190 + 1
        // merged = 191 events remaining after the 201st was added.
        // After 210 events, multiple collapses may have happened.
        assert!(
            m.significant_events.len() <= MAX_SIGNIFICANT_EVENTS,
            "events: {}",
            m.significant_events.len()
        );
    }

    #[test]
    fn fading_decays_over_time() {
        let mut m = RelationshipMemory::new("a".into(), "b".into(), 100);
        m.record_event(event(100, SignificantEventType::SavedMyLife, 800), 100);
        let ticks_per_hour = 1000;
        let fade_start = 20 * ticks_per_hour;
        let fade_end = fade_start + 60 * ticks_per_hour;

        // Before fade start: not fading.
        m.apply_fading(fade_start - 1, ticks_per_hour);
        assert!(!m.significant_events[0].fading);

        // After fade start: fading but not zero.
        m.apply_fading(fade_start + 30 * ticks_per_hour, ticks_per_hour);
        assert!(m.significant_events[0].fading);
        assert!(m.significant_events[0].weight.0 > 0);
        assert!(m.significant_events[0].weight.0 < 800);

        // After fade end: weight is 0.
        m.apply_fading(fade_end + 1, ticks_per_hour);
        assert_eq!(m.significant_events[0].weight.0, 0);
    }

    #[test]
    fn player_noted_never_fades() {
        let mut m = RelationshipMemory::new("a".into(), "b".into(), 100);
        m.record_event(
            event(
                100,
                SignificantEventType::PlayerNoted {
                    note: "stays forever".into(),
                },
                800,
            ),
            100,
        );
        m.apply_fading(1_000_000, 1000);
        assert!(m.significant_events[0].weight.0 > 0);
    }

    #[test]
    fn trajectory_tracks_trend() {
        let mut m = RelationshipMemory::new("a".into(), "b".into(), 100);
        // Rising trust
        for i in 0..15 {
            m.record_trust(100 + i as u64, Fixed(100 + i as i64 * 50));
        }
        assert!(matches!(
            m.trust_trajectory.trend,
            TrustTrend::Rising { .. }
        ));

        // Reset and test falling
        let mut m2 = RelationshipMemory::new("a".into(), "b".into(), 100);
        for i in 0..15 {
            m2.record_trust(100 + i as u64, Fixed(1024 - i as i64 * 50));
        }
        assert!(matches!(
            m2.trust_trajectory.trend,
            TrustTrend::Falling { .. }
        ));
    }

    #[test]
    fn sorted_pair_key() {
        let m = RelationshipMemory::new("z".into(), "a".into(), 100);
        assert_eq!(m.participants.0, "a");
        assert_eq!(m.participants.1, "z");
    }
}
