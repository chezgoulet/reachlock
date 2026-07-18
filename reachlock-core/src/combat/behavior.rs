//! Enemy behavior tree (spec §22): Patrol / Engage / Evade / Retreat /
//! RequestReinforcements as a pure state transition. `enemy_step` is the
//! whole brain — same senses, same state, same decision, on every target.
//! No LLM, no wall clock, no floats: fairness is determinism.
//!
//! The client feeds fixed-point senses each decision tick and flies the
//! returned [`Intent`] through rapier, capped by the enemy's OWN
//! `HullHandling` (S19 gotcha: AI that ignores physics feels like cheating).

use serde::{Deserialize, Serialize};

use crate::generator::FixedVec2;
use crate::util::rng::Fixed;

/// One fixed-point unit (1.0 == 1024), the crate-wide scale.
pub const ONE: i64 = 1024;

/// Hull fraction (0..=ONE) below which an engaged enemy starts evading.
pub const EVADE_HULL: i64 = 400;
/// Hull fraction below which the enemy runs for good.
pub const RETREAT_HULL: i64 = 180;
/// Hull fraction below which a lone enemy calls for backup first.
pub const REINFORCE_HULL: i64 = 512;
/// Shield fraction at which an evading enemy turns back into the fight.
pub const REJOIN_SHIELD: i64 = 512;

/// The five behavior states the spec names. Wire-stable (serde snake_case)
/// so saves and the determinism manifest can carry them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorState {
    Patrol,
    Engage,
    Evade,
    Retreat,
    RequestReinforcements,
}

/// What the enemy ship knows this decision tick. All fixed-point / integer;
/// the client converts world floats down to whole units before calling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Senses {
    /// Vector from me to the player, whole world units (fixed-point).
    pub to_player: FixedVec2,
    /// Distance to the player, whole world units.
    pub dist_to_player: i64,
    /// My hull fraction, 0..=ONE.
    pub hull_frac: i64,
    /// My shield fraction, 0..=ONE.
    pub shield_frac: i64,
    /// Living allies in the encounter (excluding me).
    pub ally_count: u32,
    /// True once this wing has already radioed for backup — keeps
    /// RequestReinforcements a one-shot instead of an oscillation.
    pub reinforcements_called: bool,
    /// Patrol leg direction (seeded by the client per enemy).
    pub patrol_dir: FixedVec2,
    /// My weapon's reach, whole world units.
    pub weapon_range: i64,
    /// Sensor pickup range — inside this, Patrol becomes Engage.
    pub engage_range: i64,
    /// Beyond this the fight is over and everyone goes back to Patrol.
    pub disengage_range: i64,
    /// Subsystem gates (from my own `CombatVessel`): a ship with disabled
    /// engines can't run, with disabled weapons can't shoot.
    pub engines_disabled: bool,
    pub weapons_disabled: bool,
}

/// What the enemy wants to do this tick. The client caps thrust/turn by the
/// enemy's `HullHandling` — the tree asks, physics answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intent {
    /// Desired heading (unnormalized; direction only). Zero = hold.
    pub heading: FixedVec2,
    /// Desired throttle, 0..=ONE of the hull's available thrust.
    pub throttle: Fixed,
    /// Pull the trigger this tick (already gated by range + weapon state).
    pub fire: bool,
    /// Broadcast a reinforcement request (client spawns the backup wing).
    pub request_reinforcements: bool,
}

impl Intent {
    fn hold() -> Self {
        Intent {
            heading: FixedVec2 {
                x: Fixed(0),
                y: Fixed(0),
            },
            throttle: Fixed(0),
            fire: false,
            request_reinforcements: false,
        }
    }
}

/// Negate a vector (flee = away from the player).
fn away(v: FixedVec2) -> FixedVec2 {
    FixedVec2 {
        x: Fixed(-v.x.0),
        y: Fixed(-v.y.0),
    }
}

/// Perpendicular (90° CCW) — the evade corkscrew's plane direction.
fn perp(v: FixedVec2) -> FixedVec2 {
    FixedVec2 {
        x: Fixed(-v.y.0),
        y: Fixed(v.x.0),
    }
}

/// The whole behavior tree as one pure transition: `(state, senses) →
/// (next state, intent)`. Every state has an exit condition — the no-trap
/// property test at the foot of this file enforces it.
pub fn enemy_step(state: BehaviorState, senses: &Senses) -> (BehaviorState, Intent) {
    let in_gun_range = senses.dist_to_player <= senses.weapon_range && !senses.weapons_disabled;
    match state {
        BehaviorState::Patrol => {
            if senses.dist_to_player <= senses.engage_range {
                return (BehaviorState::Engage, chase_intent(senses, in_gun_range));
            }
            (
                BehaviorState::Patrol,
                Intent {
                    heading: senses.patrol_dir,
                    throttle: Fixed(ONE * 2 / 5),
                    ..Intent::hold()
                },
            )
        }
        BehaviorState::Engage => {
            if senses.hull_frac < RETREAT_HULL {
                return (BehaviorState::Retreat, flee_intent(senses));
            }
            if senses.hull_frac < REINFORCE_HULL
                && senses.ally_count == 0
                && !senses.reinforcements_called
            {
                let mut intent = flee_intent(senses);
                intent.request_reinforcements = true;
                return (BehaviorState::RequestReinforcements, intent);
            }
            if senses.hull_frac < EVADE_HULL && senses.shield_frac == 0 {
                return (BehaviorState::Evade, evade_intent(senses));
            }
            if senses.dist_to_player > senses.disengage_range {
                return (BehaviorState::Patrol, Intent::hold());
            }
            (BehaviorState::Engage, chase_intent(senses, in_gun_range))
        }
        BehaviorState::RequestReinforcements => {
            // Transient: the call went out last tick; pick the fight back up
            // (or start weaving if the hull is already ragged).
            if senses.hull_frac < EVADE_HULL {
                (BehaviorState::Evade, evade_intent(senses))
            } else {
                (BehaviorState::Engage, chase_intent(senses, in_gun_range))
            }
        }
        BehaviorState::Evade => {
            if senses.hull_frac < RETREAT_HULL {
                return (BehaviorState::Retreat, flee_intent(senses));
            }
            // The fight ending outranks rejoining it.
            if senses.dist_to_player > senses.disengage_range {
                return (BehaviorState::Patrol, Intent::hold());
            }
            if senses.shield_frac >= REJOIN_SHIELD {
                return (BehaviorState::Engage, chase_intent(senses, in_gun_range));
            }
            (BehaviorState::Evade, evade_intent(senses))
        }
        BehaviorState::Retreat => {
            if senses.dist_to_player > senses.disengage_range {
                return (BehaviorState::Patrol, Intent::hold());
            }
            (BehaviorState::Retreat, flee_intent(senses))
        }
    }
}

fn chase_intent(senses: &Senses, fire: bool) -> Intent {
    Intent {
        heading: senses.to_player,
        throttle: Fixed(ONE * 4 / 5),
        fire,
        request_reinforcements: false,
    }
}

fn evade_intent(senses: &Senses) -> Intent {
    Intent {
        heading: perp(senses.to_player),
        throttle: Fixed(ONE * 9 / 10),
        fire: false,
        request_reinforcements: false,
    }
}

fn flee_intent(senses: &Senses) -> Intent {
    Intent {
        // Engines disabled: the tree still ASKS to run; the client's physics
        // (subsystem-gated thrust) is what strands it. Intent stays honest.
        heading: away(senses.to_player),
        throttle: Fixed(if senses.engines_disabled { 0 } else { ONE }),
        fire: false,
        request_reinforcements: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn senses() -> Senses {
        Senses {
            to_player: FixedVec2 {
                x: Fixed(100 * 1024),
                y: Fixed(0),
            },
            dist_to_player: 100,
            hull_frac: ONE,
            shield_frac: ONE,
            ally_count: 1,
            reinforcements_called: false,
            patrol_dir: FixedVec2 {
                x: Fixed(0),
                y: Fixed(1024),
            },
            weapon_range: 300,
            engage_range: 600,
            disengage_range: 1500,
            engines_disabled: false,
            weapons_disabled: false,
        }
    }

    #[test]
    fn patrol_engages_in_sensor_range() {
        let (next, intent) = enemy_step(BehaviorState::Patrol, &senses());
        assert_eq!(next, BehaviorState::Engage);
        assert!(intent.fire, "player is inside weapon range");
    }

    #[test]
    fn patrol_holds_leg_when_alone() {
        let mut s = senses();
        s.dist_to_player = 2000;
        let (next, intent) = enemy_step(BehaviorState::Patrol, &s);
        assert_eq!(next, BehaviorState::Patrol);
        assert_eq!(intent.heading, s.patrol_dir);
        assert!(!intent.fire);
    }

    #[test]
    fn engage_fires_only_in_range_and_with_working_guns() {
        let mut s = senses();
        s.dist_to_player = 400; // inside engage, outside weapon range
        let (_, intent) = enemy_step(BehaviorState::Engage, &s);
        assert!(!intent.fire);
        s.dist_to_player = 200;
        s.weapons_disabled = true;
        let (_, intent) = enemy_step(BehaviorState::Engage, &s);
        assert!(!intent.fire, "disabled weapons stay silent");
    }

    #[test]
    fn shredded_hull_evades_then_retreats() {
        let mut s = senses();
        s.hull_frac = EVADE_HULL - 1;
        s.shield_frac = 0;
        s.ally_count = 2;
        let (next, _) = enemy_step(BehaviorState::Engage, &s);
        assert_eq!(next, BehaviorState::Evade);
        s.hull_frac = RETREAT_HULL - 1;
        let (next, intent) = enemy_step(BehaviorState::Evade, &s);
        assert_eq!(next, BehaviorState::Retreat);
        assert_eq!(intent.heading.x.0, -s.to_player.x.0, "runs away");
    }

    #[test]
    fn lone_wounded_enemy_calls_for_backup_once() {
        let mut s = senses();
        s.hull_frac = REINFORCE_HULL - 1;
        s.ally_count = 0;
        let (next, intent) = enemy_step(BehaviorState::Engage, &s);
        assert_eq!(next, BehaviorState::RequestReinforcements);
        assert!(intent.request_reinforcements);
        // Once called, the same senses re-engage instead of re-requesting.
        s.reinforcements_called = true;
        let (next, intent) = enemy_step(BehaviorState::Engage, &s);
        assert_eq!(next, BehaviorState::Engage);
        assert!(!intent.request_reinforcements);
    }

    #[test]
    fn request_state_is_transient() {
        let s = senses();
        let (next, _) = enemy_step(BehaviorState::RequestReinforcements, &s);
        assert_ne!(next, BehaviorState::RequestReinforcements);
    }

    #[test]
    fn everyone_goes_home_when_the_player_leaves() {
        let mut s = senses();
        s.dist_to_player = s.disengage_range + 1;
        for state in [
            BehaviorState::Engage,
            BehaviorState::Evade,
            BehaviorState::Retreat,
        ] {
            let (next, _) = enemy_step(state, &s);
            assert_eq!(next, BehaviorState::Patrol, "{state:?} must disengage");
        }
    }

    /// The brief's no-trap property: from EVERY state there exists a senses
    /// battery that exits to a different state. A state you can never leave
    /// is a stuck enemy orbiting forever.
    #[test]
    fn no_state_can_trap() {
        let all = [
            BehaviorState::Patrol,
            BehaviorState::Engage,
            BehaviorState::Evade,
            BehaviorState::Retreat,
            BehaviorState::RequestReinforcements,
        ];
        // A small battery of extreme senses: near/far, healthy/dying,
        // shields up/down, alone/escorted.
        let mut battery = Vec::new();
        for dist in [50, 100, 700, 5000] {
            for hull in [ONE, EVADE_HULL - 1, RETREAT_HULL - 1] {
                for shield in [0, ONE] {
                    for allies in [0, 2] {
                        let mut s = senses();
                        s.dist_to_player = dist;
                        s.hull_frac = hull;
                        s.shield_frac = shield;
                        s.ally_count = allies;
                        battery.push(s);
                    }
                }
            }
        }
        for state in all {
            let escapes = battery.iter().any(|s| enemy_step(state, s).0 != state);
            assert!(escapes, "{state:?} has no exit across the senses battery");
        }
    }

    /// Determinism: the same (state, senses) always produces the same
    /// decision — the §22 fairness promise in one assert.
    #[test]
    fn step_is_deterministic() {
        let s = senses();
        for state in [
            BehaviorState::Patrol,
            BehaviorState::Engage,
            BehaviorState::Evade,
            BehaviorState::Retreat,
            BehaviorState::RequestReinforcements,
        ] {
            assert_eq!(enemy_step(state, &s), enemy_step(state, &s));
        }
    }
}
