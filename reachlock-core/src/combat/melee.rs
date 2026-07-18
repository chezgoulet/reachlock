//! Melee hit resolution (S20, spec §22): the pure integer geometry and timing
//! that decides whether a swing lands and for how much. The state machine in
//! [`super::humanoid`] says *what* a combatant does; these three functions say
//! *whether it connects* — arc + range, block/parry reduction, and dodge
//! i-frames. No floats: the arc test runs on the crate's integer trig table so
//! a swing lands bit-identically on every target (the §22 fairness promise).

use crate::util::trig::{icos, isin};

/// Turns in a full circle for the [`icos`]/[`isin`] table (16-bit angle).
const TURNS: i64 = 65536;

/// Is `target` inside the attacker's melee arc — both within `range` and
/// within `arc_half_degrees` of `facing_angle`?
///
/// `facing_angle` is in turns/65536 (the [`icos`]/[`isin`] convention);
/// `arc_half_degrees` is the half-angle of the cone (so a 90° frontal swing
/// passes `45`). Range and coordinates are fixed-point world units. All
/// intermediate products run in `i128` so large world coordinates never
/// overflow the squared comparison.
pub fn in_melee_arc(
    attacker: (i64, i64),
    facing_angle: u16,
    target: (i64, i64),
    range: i64,
    arc_half_degrees: u16,
) -> bool {
    let dx = (target.0 - attacker.0) as i128;
    let dy = (target.1 - attacker.1) as i128;
    let dist_sq = dx * dx + dy * dy;
    let range = range.max(0) as i128;
    if dist_sq > range * range {
        return false;
    }
    if dx == 0 && dy == 0 {
        return true; // sitting on the attacker: trivially in-arc
    }

    // Facing unit vector, scaled by the trig table (cos/sin × 32768).
    let fx = icos(facing_angle) as i128;
    let fy = isin(facing_angle) as i128;
    // dot = 32768 * |d| * cos(theta), where theta is the angle between facing
    // and the target direction.
    let dot = fx * dx + fy * dy;

    // Threshold: cos(theta) >= cos(arc_half) ⇔ dot >= c * |d|, with
    // c = 32768 * cos(arc_half). The shared 32768 scale cancels when we
    // square, so this stays integer-only.
    let arc_turn = ((arc_half_degrees as i64 % 360) * TURNS / 360) as u16;
    let c = icos(arc_turn) as i128; // 32768 * cos(arc_half)
    if c >= 0 {
        // Frontal cone (arc_half <= 90°): must be in front and past the
        // cosine threshold.
        dot >= 0 && dot * dot >= c * c * dist_sq
    } else {
        // Reflex cone (arc_half > 90°): anything not-behind qualifies, plus
        // the shallow wrap behind up to the threshold.
        dot >= 0 || dot * dot <= c * c * dist_sq
    }
}

/// Fraction of a blocked hit that still gets through (a raised guard soaks
/// three-quarters). Numerator / denominator so it stays integer.
const BLOCK_THROUGH_NUM: i64 = 1;
const BLOCK_THROUGH_DEN: i64 = 4;

/// Reduce incoming `damage` for a guarding defender.
///
/// A hit inside the parry window (`block_elapsed < parry_ticks`) is negated
/// entirely — the reward for a well-timed guard. A hit that lands later while
/// the block is still up is soaked to a quarter of its value (never below 1 so
/// chip damage still registers). `damage <= 0` in, `0` out.
pub fn block_reduce(damage: i64, block_elapsed: u32, parry_ticks: u32) -> i64 {
    if damage <= 0 {
        return 0;
    }
    if block_elapsed < parry_ticks {
        return 0; // parried
    }
    (damage * BLOCK_THROUGH_NUM / BLOCK_THROUGH_DEN).max(1)
}

/// Is the defender invulnerable this tick? True while dodge i-frames remain.
pub fn is_dodging(i_frame_remaining: u32) -> bool {
    i_frame_remaining > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Facing +X (0 turns) so a target to the east is dead ahead.
    const EAST: u16 = 0;

    #[test]
    fn in_melee_arc_hits() {
        // Target 100 units due east, attacker facing east, 5-unit reach isn't
        // enough — but 200 is, and it's straight ahead.
        assert!(in_melee_arc((0, 0), EAST, (100, 0), 200, 45));
        assert!(
            !in_melee_arc((0, 0), EAST, (100, 0), 50, 45),
            "out of range"
        );
    }

    #[test]
    fn in_melee_arc_misses() {
        // Target directly behind (west) is outside a 90°-wide frontal cone.
        assert!(!in_melee_arc((0, 0), EAST, (-100, 0), 200, 45));
        // Off to the side beyond the half-angle also misses.
        assert!(!in_melee_arc((0, 0), EAST, (0, 100), 200, 30));
        // ...but a wide enough cone catches the flank.
        assert!(in_melee_arc((0, 0), EAST, (0, 100), 200, 90));
    }

    #[test]
    fn block_reduces_damage() {
        // Blocked (past the parry window) but still guarding: partial soak.
        let out = block_reduce(1024, 10, 4);
        assert!(out > 0, "chip damage still registers");
        assert!(out < 1024, "the guard soaked most of it");
        assert_eq!(out, 256);
    }

    #[test]
    fn parry_negates() {
        // Inside the first `parry_ticks` ticks the hit is erased entirely.
        assert_eq!(block_reduce(1024, 0, 4), 0);
        assert_eq!(block_reduce(1024, 3, 4), 0);
        // One tick past the window it's back to a normal block.
        assert_eq!(block_reduce(1024, 4, 4), 256);
    }

    #[test]
    fn dodge_i_frames() {
        assert!(is_dodging(8));
        assert!(is_dodging(1));
        assert!(!is_dodging(0));
    }

    #[test]
    fn arc_is_frame_rate_agnostic_geometry() {
        // Pure geometry — identical result regardless of when it's evaluated.
        for _ in 0..3 {
            assert!(in_melee_arc((5, 5), EAST, (105, 5), 200, 45));
        }
    }
}
