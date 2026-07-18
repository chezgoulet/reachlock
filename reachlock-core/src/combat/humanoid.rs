//! Humanoid (landed) combat state machine (S20, spec §22 "Landed Combat").
//!
//! The space-combat brain in [`super::behavior`] flies ships; this one walks
//! people. Same shape — a pure `(state, senses) → intent` transition, no LLM,
//! no wall clock, no floats — different states: Idle / Patrol / Chase / Attack
//! / Flee / Downed. Enemies and the crew companion share it; only their
//! [`HostileArchetype`] data differs, so balancing is a content edit and never
//! a recompile (S20 gotcha).
//!
//! Timing is all integer ticks. An attack committed at `startup` telegraphs,
//! lands during `active`, and is punishable during `recovery` — the client
//! reads `sub_timer` against the chosen [`AttackWindow`] to know which phase
//! a swing is in. i-frames, parry, and block resolution live in
//! [`super::melee`]; this module decides *what* the humanoid does, that one
//! resolves *whether it lands*.

use serde::{Deserialize, Serialize};

use crate::generator::FixedVec2;

/// One fixed-point unit (1.0 == 1024), the crate-wide scale.
pub const ONE: i64 = 1024;

// ---------------------------------------------------------------------
// Timing / stat windows — the data an archetype tunes.
// ---------------------------------------------------------------------

/// A melee (or ranged) swing as three tick phases plus its reach and bite.
/// `startup` is the telegraph, `active` is when the arc can connect, and
/// `recovery` is the punishable tail. All ticks run at the fixed 10 Hz
/// combat rate so feel is frame-rate independent (S20 gotcha).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackWindow {
    pub startup_ticks: u32,
    pub active_ticks: u32,
    pub recovery_ticks: u32,
    /// Hit points removed on a clean connect (fixed-point i64).
    pub damage: i64,
    /// Reach in world units (fixed-point i64).
    pub range: i64,
}

impl AttackWindow {
    /// Total swing length in ticks — the span the state machine spends in
    /// [`HumanoidState::Attack`] before returning to Chase.
    pub fn total_ticks(&self) -> u32 {
        self.startup_ticks + self.active_ticks + self.recovery_ticks
    }

    /// True while `elapsed` falls inside the active window — the only phase
    /// during which [`super::melee::in_melee_arc`] can connect.
    pub fn is_active(&self, elapsed: u32) -> bool {
        elapsed >= self.startup_ticks && elapsed < self.startup_ticks + self.active_ticks
    }
}

/// A guard: how long the block soaks, its cooldown, and the parry window at
/// the very front of it (a hit inside `parry_ticks` is negated entirely —
/// see [`super::melee::block_reduce`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockWindow {
    pub active_ticks: u32,
    pub cooldown_ticks: u32,
    pub parry_ticks: u32,
}

/// A dodge roll: invulnerable for `i_frame_ticks`, committed for
/// `recovery_ticks`, covering `distance` world units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DodgeWindow {
    pub i_frame_ticks: u32,
    pub recovery_ticks: u32,
    pub distance: i64,
}

// ---------------------------------------------------------------------
// Archetype — the data-driven enemy (or companion) class.
// ---------------------------------------------------------------------

/// A humanoid combatant class, authored in `content/combat/*.ron`. Enemies
/// and the crew companion both instantiate one; the state machine is generic
/// over it. Keeping every number here means the balance pass is a data edit
/// (S20 gotcha: "no recompile to balance").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostileArchetype {
    pub id: String,
    pub display_name: String,
    /// Max hit points (fixed-point i64).
    pub hp: i64,
    /// Move speed per tick (fixed-point i64, ONE == 1024).
    pub speed: i64,
    pub light_attack: AttackWindow,
    pub heavy_attack: AttackWindow,
    pub block: BlockWindow,
    pub dodge: DodgeWindow,
    /// Target inside this radius (world units) wakes Idle/Patrol into Chase.
    pub chase_radius: i64,
    /// Target beyond this radius (world units) ends the fight → Patrol.
    pub disengage_radius: i64,
    /// Hull fraction (0..=ONE) below which the humanoid breaks and flees.
    /// Set to 0 for a fearless archetype (security bots never run).
    pub flee_hp_frac: i64,
}

// ---------------------------------------------------------------------
// State machine.
// ---------------------------------------------------------------------

/// The six landed-combat states. Wire-stable (serde snake_case) so saves and
/// the determinism manifest can carry them, mirroring [`super::BehaviorState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanoidState {
    Idle,
    Patrol,
    Chase,
    Attack,
    Flee,
    /// HP hit zero. Idles forever; revive is the companion system's job, not
    /// a transition this pure function owns.
    Downed,
}

/// What the humanoid perceives this decision tick. All fixed-point / integer;
/// the client converts world floats down to whole units before calling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanoidSenses {
    /// Vector from me to my target (fixed-point world units).
    pub to_target: FixedVec2,
    /// Distance to my target (world units).
    pub dist_to_target: i64,
    /// My hit-point fraction, 0..=ONE.
    pub hp_frac: i64,
    /// My weapon is off cooldown and ready to swing.
    pub weapon_ready: bool,
    /// The target is inside my current weapon's reach (client-computed).
    pub target_in_range: bool,
    /// The target is mid-telegraph — a read for dodge/block timing.
    pub target_telegraphing: bool,
    /// I took damage since the last tick.
    pub under_attack: bool,
    /// Living allies nearby (excluding me).
    pub ally_count: u32,
    /// Absolute patrol waypoints (world units), cycled by `waypoint_index`.
    pub patrol_waypoints: [(i64, i64); 4],
    /// Which waypoint is current; the client advances it on arrival (it owns
    /// my Transform, this function doesn't).
    pub waypoint_index: u32,
}

/// What the humanoid wants to do this tick. The client applies it against the
/// entity's Transform, capped by the archetype's own `speed`.
///
/// `Walk(x, y)` is dual-purpose by state, and the client is its only reader:
/// in Chase/Attack/Flee it is a **direction** (`to_target` or its negation);
/// in Patrol it is the **absolute waypoint coordinate** to steer toward (this
/// function has no own-position sense, so it can't hand back a patrol vector).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanoidIntent {
    Idle,
    Walk(i64, i64),
    LightAttack(i64, i64),
    HeavyAttack(i64, i64),
    Dodge(i64, i64),
    Block,
    Die,
}

/// A wounded humanoid commits to heavy swings — below half HP the desperation
/// blow comes out. HP is monotone non-increasing within a fight, so this can
/// only flip light→heavy at a swing boundary, never oscillate.
fn use_heavy(senses: &HumanoidSenses) -> bool {
    senses.hp_frac * 2 < ONE
}

/// The whole landed brain as one pure transition: `(state, sub_timer,
/// senses) → intent`, mutating `state`/`sub_timer` in place. Every non-Downed
/// state has an exit — the no-trap property test at the foot of this file
/// enforces it, exactly as space combat does.
pub fn humanoid_step(
    state: &mut HumanoidState,
    sub_timer: &mut u32,
    senses: &HumanoidSenses,
    archetype: &HostileArchetype,
) -> HumanoidIntent {
    // Death dominates every other state.
    if senses.hp_frac <= 0 {
        if *state != HumanoidState::Downed {
            *state = HumanoidState::Downed;
            *sub_timer = 0;
            return HumanoidIntent::Die;
        }
        return HumanoidIntent::Idle;
    }

    let toward = (senses.to_target.x.0, senses.to_target.y.0);
    let away = (-senses.to_target.x.0, -senses.to_target.y.0);
    let waypoint = || senses.patrol_waypoints[(senses.waypoint_index as usize) % 4];

    match *state {
        HumanoidState::Downed => HumanoidIntent::Idle,

        HumanoidState::Idle | HumanoidState::Patrol => {
            if senses.dist_to_target <= archetype.chase_radius {
                *state = HumanoidState::Chase;
                *sub_timer = 0;
                return HumanoidIntent::Walk(toward.0, toward.1);
            }
            *state = HumanoidState::Patrol;
            let wp = waypoint();
            HumanoidIntent::Walk(wp.0, wp.1)
        }

        HumanoidState::Chase => {
            if senses.hp_frac < archetype.flee_hp_frac {
                *state = HumanoidState::Flee;
                return HumanoidIntent::Walk(away.0, away.1);
            }
            if senses.dist_to_target > archetype.disengage_radius {
                *state = HumanoidState::Patrol;
                *sub_timer = 0;
                let wp = waypoint();
                return HumanoidIntent::Walk(wp.0, wp.1);
            }
            if senses.target_in_range && senses.weapon_ready {
                *state = HumanoidState::Attack;
                *sub_timer = 0;
                return swing_intent(senses, toward);
            }
            HumanoidIntent::Walk(toward.0, toward.1)
        }

        HumanoidState::Attack => {
            let win = if use_heavy(senses) {
                &archetype.heavy_attack
            } else {
                &archetype.light_attack
            };
            *sub_timer = sub_timer.saturating_add(1);
            if *sub_timer >= win.total_ticks() {
                // Swing done. Break for the exit if the wound is mortal,
                // otherwise fold back into the chase and re-decide.
                *sub_timer = 0;
                if senses.hp_frac < archetype.flee_hp_frac {
                    *state = HumanoidState::Flee;
                    return HumanoidIntent::Walk(away.0, away.1);
                }
                *state = HumanoidState::Chase;
                return HumanoidIntent::Walk(toward.0, toward.1);
            }
            // Committed swing — rooted, facing the target.
            swing_intent(senses, toward)
        }

        HumanoidState::Flee => {
            if senses.dist_to_target > archetype.disengage_radius {
                *state = HumanoidState::Patrol;
                *sub_timer = 0;
                let wp = waypoint();
                return HumanoidIntent::Walk(wp.0, wp.1);
            }
            HumanoidIntent::Walk(away.0, away.1)
        }
    }
}

/// The attack intent for the current swing choice, facing the target.
fn swing_intent(senses: &HumanoidSenses, toward: (i64, i64)) -> HumanoidIntent {
    if use_heavy(senses) {
        HumanoidIntent::HeavyAttack(toward.0, toward.1)
    } else {
        HumanoidIntent::LightAttack(toward.0, toward.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::rng::{Fixed, SeededRng};

    fn window(startup: u32, active: u32, recovery: u32, damage: i64, range: i64) -> AttackWindow {
        AttackWindow {
            startup_ticks: startup,
            active_ticks: active,
            recovery_ticks: recovery,
            damage,
            range,
        }
    }

    fn archetype() -> HostileArchetype {
        HostileArchetype {
            id: "test_raider".into(),
            display_name: "Test Raider".into(),
            hp: 8192,
            speed: 256,
            light_attack: window(8, 4, 12, 1024, 2048),
            heavy_attack: window(16, 6, 20, 2048, 2560),
            block: BlockWindow {
                active_ticks: 20,
                cooldown_ticks: 30,
                parry_ticks: 4,
            },
            dodge: DodgeWindow {
                i_frame_ticks: 8,
                recovery_ticks: 12,
                distance: 3072,
            },
            chase_radius: 8192,
            disengage_radius: 16000,
            flee_hp_frac: 256,
        }
    }

    fn senses() -> HumanoidSenses {
        HumanoidSenses {
            to_target: FixedVec2 {
                x: Fixed(100 * 1024),
                y: Fixed(0),
            },
            dist_to_target: 100 * 1024,
            hp_frac: ONE,
            weapon_ready: true,
            target_in_range: false,
            target_telegraphing: false,
            under_attack: false,
            ally_count: 0,
            patrol_waypoints: [(0, 0), (10, 0), (10, 10), (0, 10)],
            waypoint_index: 0,
        }
    }

    #[test]
    fn chase_enters_attack_range() {
        let mut state = HumanoidState::Idle;
        let mut timer = 0;
        let mut s = senses();
        s.dist_to_target = archetype().chase_radius - 1; // inside chase radius
        let intent = humanoid_step(&mut state, &mut timer, &s, &archetype());
        assert_eq!(state, HumanoidState::Chase);
        assert!(matches!(intent, HumanoidIntent::Walk(_, _)));
    }

    #[test]
    fn attack_phases_progress() {
        let arch = archetype();
        let mut state = HumanoidState::Chase;
        let mut timer = 0;
        let mut s = senses();
        s.dist_to_target = 1024;
        s.target_in_range = true;
        // Chase → Attack, sub_timer resets to 0 and we swing on entry.
        let intent = humanoid_step(&mut state, &mut timer, &s, &arch);
        assert_eq!(state, HumanoidState::Attack);
        assert_eq!(timer, 0);
        assert!(matches!(intent, HumanoidIntent::LightAttack(_, _)));

        let win = arch.light_attack;
        // Walk the whole swing: startup → active → recovery.
        let mut saw_active = false;
        for _ in 0..win.total_ticks() {
            let before = timer;
            let intent = humanoid_step(&mut state, &mut timer, &s, &arch);
            if state == HumanoidState::Attack {
                assert_eq!(timer, before + 1, "sub_timer advances each tick");
                if win.is_active(timer) {
                    saw_active = true;
                    assert!(matches!(intent, HumanoidIntent::LightAttack(_, _)));
                }
            }
        }
        assert!(saw_active, "the swing passed through its active window");
        // After the full window the swing ends and we re-decide as Chase.
        assert_eq!(state, HumanoidState::Chase);
        assert_eq!(timer, 0);
    }

    #[test]
    fn flee_at_low_hp() {
        let arch = archetype();
        let mut state = HumanoidState::Chase;
        let mut timer = 0;
        let mut s = senses();
        s.dist_to_target = 1024;
        s.hp_frac = arch.flee_hp_frac - 1;
        let intent = humanoid_step(&mut state, &mut timer, &s, &arch);
        assert_eq!(state, HumanoidState::Flee);
        // Runs directly away from the target.
        assert_eq!(intent, HumanoidIntent::Walk(-s.to_target.x.0, -s.to_target.y.0));
    }

    #[test]
    fn patrol_loop() {
        let arch = archetype();
        let s0 = senses();
        // Target far outside chase radius — pure patrol.
        let mut s = s0;
        s.dist_to_target = arch.chase_radius + 1_000_000;
        for i in 0..4u32 {
            let mut state = HumanoidState::Idle;
            let mut timer = 0;
            s.waypoint_index = i;
            let intent = humanoid_step(&mut state, &mut timer, &s, &arch);
            assert_eq!(state, HumanoidState::Patrol);
            let wp = s.patrol_waypoints[i as usize];
            assert_eq!(intent, HumanoidIntent::Walk(wp.0, wp.1));
        }
        // The index wraps modulo four — waypoint 4 is waypoint 0 again.
        let mut state = HumanoidState::Patrol;
        let mut timer = 0;
        s.waypoint_index = 4;
        let intent = humanoid_step(&mut state, &mut timer, &s, &arch);
        let wp = s.patrol_waypoints[0];
        assert_eq!(intent, HumanoidIntent::Walk(wp.0, wp.1));
    }

    #[test]
    fn deterministic_patrol_path() {
        let arch = archetype();
        // Seeded waypoints, exactly the client's per-enemy patrol roll.
        let path = |seed: u64| {
            let mut rng = SeededRng::new(seed);
            let mut wps = [(0i64, 0i64); 4];
            for wp in &mut wps {
                *wp = (
                    (rng.next_below(20_000) as i64) - 10_000,
                    (rng.next_below(20_000) as i64) - 10_000,
                );
            }
            let mut s = senses();
            s.patrol_waypoints = wps;
            s.dist_to_target = arch.chase_radius + 1_000_000;
            let mut out = Vec::new();
            for i in 0..8u32 {
                s.waypoint_index = i;
                let mut state = HumanoidState::Patrol;
                let mut timer = 0;
                out.push(humanoid_step(&mut state, &mut timer, &s, &arch));
            }
            out
        };
        assert_eq!(path(0xABCD), path(0xABCD), "same seed, same patrol");
    }

    #[test]
    fn downed_idles_forever() {
        let arch = archetype();
        let mut state = HumanoidState::Chase;
        let mut timer = 0;
        let mut s = senses();
        s.hp_frac = 0;
        // First zero-HP tick reports Die and latches Downed.
        assert_eq!(humanoid_step(&mut state, &mut timer, &s, &arch), HumanoidIntent::Die);
        assert_eq!(state, HumanoidState::Downed);
        // Thereafter it idles regardless of what the world does.
        s.hp_frac = ONE; // even a phantom heal doesn't self-revive
        s.dist_to_target = 10;
        assert_eq!(humanoid_step(&mut state, &mut timer, &s, &arch), HumanoidIntent::Idle);
        assert_eq!(state, HumanoidState::Downed);
    }

    #[test]
    fn no_state_can_trap() {
        let arch = archetype();
        let states = [
            HumanoidState::Idle,
            HumanoidState::Patrol,
            HumanoidState::Chase,
            HumanoidState::Attack,
            HumanoidState::Flee,
        ];
        let mut battery = Vec::new();
        for dist in [10i64, 1024, arch.chase_radius + 1, arch.disengage_radius + 1] {
            for hp in [ONE, arch.flee_hp_frac - 1] {
                for in_range in [false, true] {
                    let mut s = senses();
                    s.dist_to_target = dist;
                    s.hp_frac = hp.max(1);
                    s.target_in_range = in_range;
                    battery.push(s);
                }
            }
        }
        for state in states {
            let escapes = battery.iter().any(|s| {
                // Attack needs several ticks to run its window down; give it
                // the full swing before judging it stuck.
                let mut st = state;
                let mut timer = 0;
                for _ in 0..arch.heavy_attack.total_ticks() + 1 {
                    humanoid_step(&mut st, &mut timer, s, &arch);
                    if st != state {
                        return true;
                    }
                }
                false
            });
            assert!(escapes, "{state:?} has no exit across the senses battery");
        }
    }

    #[test]
    fn step_is_deterministic() {
        let arch = archetype();
        let s = senses();
        for state in [HumanoidState::Idle, HumanoidState::Chase, HumanoidState::Flee] {
            let mut a = state;
            let mut ta = 0;
            let mut b = state;
            let mut tb = 0;
            assert_eq!(
                humanoid_step(&mut a, &mut ta, &s, &arch),
                humanoid_step(&mut b, &mut tb, &s, &arch)
            );
            assert_eq!(a, b);
        }
    }
}
