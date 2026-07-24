//! Piracy system (S43): ship capture, contraband, notoriety, bounty system,
//! and boarding mechanics. Pure functions over `PiracyState` — no I/O,
//! deterministic, wasm-safe (iron rule #1).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::combat::damage::{CombatVessel, SubsystemKind};
use crate::generator::hull::HullClass;
use crate::util::Fixed;

/// Player's piracy state — tracks notoriety, bounties, contraband knowledge,
/// captured ships, and haven reputation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PiracyState {
    pub notoriety: PiracyNotoriety,
    pub active_bounties: Vec<Bounty>,
    pub contraband_knowledge: Vec<ContrabandRoute>,
    pub ships_captured: u32,
    pub cargo_seized_value: u64,
    pub pirate_reputation: HashMap<String, i64>,
    pub current_havens_known: Vec<String>,
}

impl Default for PiracyState {
    fn default() -> Self {
        PiracyState {
            notoriety: PiracyNotoriety {
                level: NotorietyLevel::Clean,
                value: 0,
                decay_per_tick: 1,
                last_crime_tick: 0,
            },
            active_bounties: vec![],
            contraband_knowledge: vec![],
            ships_captured: 0,
            cargo_seized_value: 0,
            pirate_reputation: HashMap::new(),
            current_havens_known: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PiracyNotoriety {
    pub level: NotorietyLevel,
    pub value: u64,
    pub decay_per_tick: u64,
    pub last_crime_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotorietyLevel {
    Clean,
    Suspicious,
    Wanted,
    Hunted,
    Infamous,
}

impl NotorietyLevel {
    pub fn threshold(self) -> u64 {
        match self {
            NotorietyLevel::Clean => 0,
            NotorietyLevel::Suspicious => 100,
            NotorietyLevel::Wanted => 500,
            NotorietyLevel::Hunted => 2000,
            NotorietyLevel::Infamous => 5000,
        }
    }

    pub fn from_value(v: u64) -> Self {
        if v >= NotorietyLevel::Infamous.threshold() {
            NotorietyLevel::Infamous
        } else if v >= NotorietyLevel::Hunted.threshold() {
            NotorietyLevel::Hunted
        } else if v >= NotorietyLevel::Wanted.threshold() {
            NotorietyLevel::Wanted
        } else if v >= NotorietyLevel::Suspicious.threshold() {
            NotorietyLevel::Suspicious
        } else {
            NotorietyLevel::Clean
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bounty {
    pub bounty_id: String,
    pub issuer_faction: String,
    pub amount: u64,
    pub crime: String,
    pub issued_at_tick: u64,
    pub expires_at_tick: Option<u64>,
    pub dead_or_alive: bool,
    pub claimed_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContrabandRoute {
    pub good_id: String,
    pub source_faction: String,
    pub destination_faction: String,
    pub price_multiplier: Fixed,
    pub risk_level: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardingAction {
    pub target_ship_id: String,
    pub target_ship_class: HullClass,
    pub target_hull_state: CombatVessel,
    pub defender_crew_count: u32,
    pub defender_crew_quality: u8,
    pub breach_point: SubsystemKind,
    pub resistance_level: u8,
}

/// Outcome of a boarding attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardingResult {
    Surrender { demands_met: Vec<String> },
    CrewFight { outcome: CombatOutcome },
    Scuttled,
    Escaped,
}

/// Result of a crew fight — defined here for independence from S19/S20.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatOutcome {
    Victory,
    Defeat,
    Retreat,
    Stalemate,
}

/// Record a crime committed by the player. Updates notoriety value and level.
/// Pure: returns the new state without mutating the input.
pub fn record_crime(
    mut state: PiracyState,
    crime_type: &str,
    faction: &str,
    severity: u8,
    tick: u64,
) -> PiracyState {
    state.notoriety.value += severity as u64 * 10;
    state.notoriety.last_crime_tick = tick;
    state.notoriety.level = NotorietyLevel::from_value(state.notoriety.value);

    // Issue a bounty if notoriety crosses the Hunted threshold.
    if state.notoriety.level >= NotorietyLevel::Hunted
        && !state
            .active_bounties
            .iter()
            .any(|b| b.issuer_faction == faction)
    {
        state.active_bounties.push(Bounty {
            bounty_id: format!("bounty-{}-{}", faction, tick),
            issuer_faction: faction.to_string(),
            amount: state.notoriety.value * 2,
            crime: crime_type.to_string(),
            issued_at_tick: tick,
            expires_at_tick: None,
            dead_or_alive: state.notoriety.level >= NotorietyLevel::Infamous,
            claimed_by: None,
        });
    }

    state
}

/// Apply notoriety decay (only during active play — 1% per tick).
pub fn decay_notoriety(mut state: PiracyState, current_tick: u64) -> PiracyState {
    let elapsed = current_tick.saturating_sub(state.notoriety.last_crime_tick);
    let decay = state.notoriety.decay_per_tick * elapsed;
    state.notoriety.value = state.notoriety.value.saturating_sub(decay);
    state.notoriety.level = NotorietyLevel::from_value(state.notoriety.value);
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crime_increases_notoriety() {
        let state = PiracyState::default();
        let state = record_crime(state, "smuggling", "compact", 3, 100);
        assert_eq!(state.notoriety.value, 30);
        assert_eq!(state.notoriety.level, NotorietyLevel::Clean);
    }

    #[test]
    fn bounty_issued_at_hunted() {
        let mut state = PiracyState::default();
        state.notoriety.value = 2500;
        state.notoriety.level = NotorietyLevel::Hunted;
        let state = record_crime(state, "piracy", "compact", 100, 200);
        assert!(state.active_bounties.iter().any(|b| b.issuer_faction == "compact"));
        assert!(state.active_bounties[0].dead_or_alive == false);
    }

    #[test]
    fn infamous_bounty_is_dead_or_alive() {
        let mut state = PiracyState::default();
        state.notoriety.value = 6000;
        state.notoriety.level = NotorietyLevel::Infamous;
        let state = record_crime(state, "massacre", "isc", 200, 300);
        assert!(state.active_bounties[0].dead_or_alive);
    }

    #[test]
    fn decay_reduces_notoriety() {
        let mut state = PiracyState::default();
        state.notoriety.value = 500;
        state.notoriety.last_crime_tick = 100;
        let state = decay_notoriety(state, 200);
        assert!(state.notoriety.value < 500);
    }

    #[test]
    fn notoriety_level_from_value() {
        assert_eq!(NotorietyLevel::from_value(0), NotorietyLevel::Clean);
        assert_eq!(NotorietyLevel::from_value(150), NotorietyLevel::Suspicious);
        assert_eq!(NotorietyLevel::from_value(600), NotorietyLevel::Wanted);
        assert_eq!(NotorietyLevel::from_value(2500), NotorietyLevel::Hunted);
        assert_eq!(NotorietyLevel::from_value(6000), NotorietyLevel::Infamous);
    }
}
