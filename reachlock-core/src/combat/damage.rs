//! Damage model (S19 freeze, spec §22): hull HP, shield absorption by
//! weapon type, and per-subsystem states (engines / weapons / sensors /
//! drive: Nominal / Damaged / Disabled). Pure integer math — the client's
//! collision handlers call [`apply_hit`] and render whatever it returns.

use serde::{Deserialize, Serialize};

use crate::item::{ItemStats, StatKey};

/// Fixed-point scale shared with the rest of the crate.
const ONE: i64 = 1024;

/// The four targetable subsystems (spec §22 subsystem targeting): engines
/// (disable escape), weapons (neutralize threat), sensors (blind them),
/// drive (prevent jump).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubsystemKind {
    Engines,
    Weapons,
    Sensors,
    Drive,
}

impl SubsystemKind {
    pub const ALL: [SubsystemKind; 4] = [
        SubsystemKind::Engines,
        SubsystemKind::Weapons,
        SubsystemKind::Sensors,
        SubsystemKind::Drive,
    ];

    fn index(self) -> usize {
        match self {
            SubsystemKind::Engines => 0,
            SubsystemKind::Weapons => 1,
            SubsystemKind::Sensors => 2,
            SubsystemKind::Drive => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SubsystemKind::Engines => "engines",
            SubsystemKind::Weapons => "weapons",
            SubsystemKind::Sensors => "sensors",
            SubsystemKind::Drive => "drive",
        }
    }
}

/// Health tier of one subsystem, derived from its damage pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubsystemState {
    Nominal,
    Damaged,
    Disabled,
}

/// Energy is soaked by shields; kinetic mass punches partway through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponKind {
    Kinetic,
    Energy,
}

/// The stats a fired weapon carries into `apply_hit`, in whole units.
/// Built from S05 item stats ([`WeaponStats::from_item_stats`]) so gear
/// tiers ARE combat power — no parallel balance table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeaponStats {
    pub kind: WeaponKind,
    /// Hit points removed per hit.
    pub damage: i64,
    /// Reach in item-stat units (the client scales to world units for
    /// projectile lifetime; the AI's `weapon_range` senses reuse it).
    pub range: i64,
    /// Shots per time unit; the client derives cooldown = 1/fire_rate.
    pub fire_rate: i64,
}

impl WeaponStats {
    /// Read `Damage`/`Range`/`FireRate` out of an S05 stat map (fixed-point
    /// 1/1024 → whole units). Missing keys fall back to a floor of 1 so a
    /// malformed item never divides by zero downstream.
    pub fn from_item_stats(stats: &ItemStats, kind: WeaponKind) -> Self {
        let whole = |key: StatKey| stats.0.get(&key).copied().unwrap_or(ONE) / ONE;
        WeaponStats {
            kind,
            damage: whole(StatKey::Damage).max(1),
            range: whole(StatKey::Range).max(1),
            fire_rate: whole(StatKey::FireRate).max(1),
        }
    }
}

/// One ship's combat state: hull, shield, and the four subsystem pools.
/// Both sides of a dogfight carry one (spec §22 symmetry: what disables
/// them disables you).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatVessel {
    pub hull: i64,
    pub hull_max: i64,
    pub shield: i64,
    pub shield_max: i64,
    /// Per-subsystem damage pools (order = `SubsystemKind::ALL`).
    pub sub_hp: [i64; 4],
    pub sub_max: i64,
}

impl CombatVessel {
    /// A fresh vessel: full hull/shield, all subsystems nominal. Subsystem
    /// pools are a quarter of the hull each — small enough that focused
    /// fire disables before it destroys (the spec's tactical choice).
    pub fn new(hull_max: i64, shield_max: i64) -> Self {
        let sub_max = (hull_max / 4).max(1);
        CombatVessel {
            hull: hull_max,
            hull_max,
            shield: shield_max,
            shield_max,
            sub_hp: [sub_max; 4],
            sub_max,
        }
    }

    pub fn state(&self, kind: SubsystemKind) -> SubsystemState {
        let hp = self.sub_hp[kind.index()];
        if hp <= 0 {
            SubsystemState::Disabled
        } else if hp * 2 <= self.sub_max {
            SubsystemState::Damaged
        } else {
            SubsystemState::Nominal
        }
    }

    pub fn destroyed(&self) -> bool {
        self.hull <= 0
    }

    /// Fixed-point hull fraction (0..=ONE) for behavior senses.
    pub fn hull_frac(&self) -> i64 {
        (self.hull.max(0) * ONE) / self.hull_max.max(1)
    }

    /// Fixed-point shield fraction (0..=ONE).
    pub fn shield_frac(&self) -> i64 {
        (self.shield.max(0) * ONE) / self.shield_max.max(1)
    }

    /// Recharge the shield by `amount`, clamped to max. Integer units —
    /// the client accumulates fractional ticks itself.
    pub fn recharge_shield(&mut self, amount: i64) {
        self.shield = (self.shield + amount.max(0)).min(self.shield_max);
    }
}

/// What one hit did — the client renders it (shield shimmer vs hull spark
/// vs subsystem callout) and the log narrates it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageResult {
    pub shield_absorbed: i64,
    pub hull_damage: i64,
    /// The targeted subsystem and its state AFTER this hit, when the hit
    /// reached it (shields must be down for precision damage).
    pub subsystem: Option<(SubsystemKind, SubsystemState)>,
    pub destroyed: bool,
}

/// Fraction of kinetic damage that bleeds through a live shield (energy is
/// fully absorbed until the shield collapses).
const KINETIC_BLEED_NUM: i64 = 1;
const KINETIC_BLEED_DEN: i64 = 2;

/// Fraction of post-shield damage diverted into a targeted subsystem pool.
const SUBSYSTEM_SHARE_NUM: i64 = 1;
const SUBSYSTEM_SHARE_DEN: i64 = 2;

/// Apply one weapon hit to a vessel (S19 frozen contract).
///
/// Order: shield absorbs by weapon type → remainder hits the hull → when
/// the shield is down and a subsystem is targeted, half the hull damage is
/// diverted into that subsystem's pool instead. Deterministic, saturating,
/// never panics on degenerate vessels.
pub fn apply_hit(
    vessel: &mut CombatVessel,
    weapon: &WeaponStats,
    target: Option<SubsystemKind>,
) -> DamageResult {
    let damage = weapon.damage.max(0);

    // --- shield layer ---
    let (shield_absorbed, mut through) = if vessel.shield > 0 {
        match weapon.kind {
            WeaponKind::Energy => {
                // Fully soaked until the shield collapses; excess spills.
                let absorbed = damage.min(vessel.shield);
                (absorbed, damage - absorbed)
            }
            WeaponKind::Kinetic => {
                // Mass bleeds through: half the hit ignores the shield.
                let bleed = damage * KINETIC_BLEED_NUM / KINETIC_BLEED_DEN;
                let absorbed = (damage - bleed).min(vessel.shield);
                (absorbed, bleed + (damage - bleed - absorbed))
            }
        }
    } else {
        (0, damage)
    };
    vessel.shield = (vessel.shield - shield_absorbed).max(0);

    // --- subsystem layer (precision fire needs the shield down) ---
    let mut subsystem = None;
    if let Some(kind) = target {
        if vessel.shield == 0 && through > 0 {
            let diverted = through * SUBSYSTEM_SHARE_NUM / SUBSYSTEM_SHARE_DEN;
            through -= diverted;
            let i = kind.index();
            vessel.sub_hp[i] = (vessel.sub_hp[i] - diverted).max(0);
            subsystem = Some((kind, vessel.state(kind)));
        }
    }

    // --- hull layer ---
    vessel.hull = (vessel.hull - through).max(0);

    DamageResult {
        shield_absorbed,
        hull_damage: through,
        subsystem,
        destroyed: vessel.destroyed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(damage: i64) -> WeaponStats {
        WeaponStats {
            kind: WeaponKind::Energy,
            damage,
            range: 100,
            fire_rate: 3,
        }
    }

    fn kinetic(damage: i64) -> WeaponStats {
        WeaponStats {
            kind: WeaponKind::Kinetic,
            damage,
            range: 60,
            fire_rate: 4,
        }
    }

    #[test]
    fn energy_is_fully_absorbed_by_a_live_shield() {
        let mut v = CombatVessel::new(400, 100);
        let r = apply_hit(&mut v, &energy(30), None);
        assert_eq!(r.shield_absorbed, 30);
        assert_eq!(r.hull_damage, 0);
        assert_eq!(v.hull, 400);
        assert_eq!(v.shield, 70);
    }

    #[test]
    fn energy_overkill_spills_to_hull() {
        let mut v = CombatVessel::new(400, 20);
        let r = apply_hit(&mut v, &energy(50), None);
        assert_eq!(r.shield_absorbed, 20);
        assert_eq!(r.hull_damage, 30);
        assert_eq!(v.shield, 0);
        assert_eq!(v.hull, 370);
    }

    #[test]
    fn kinetic_bleeds_half_through_the_shield() {
        let mut v = CombatVessel::new(400, 100);
        let r = apply_hit(&mut v, &kinetic(40), None);
        assert_eq!(r.shield_absorbed, 20);
        assert_eq!(r.hull_damage, 20);
        assert_eq!(v.shield, 80);
        assert_eq!(v.hull, 380);
    }

    #[test]
    fn subsystem_needs_the_shield_down() {
        let mut v = CombatVessel::new(400, 100);
        // Shield up: the targeted hit never reaches the engines.
        let r = apply_hit(&mut v, &energy(30), Some(SubsystemKind::Engines));
        assert_eq!(r.subsystem, None);
        v.shield = 0;
        let r = apply_hit(&mut v, &energy(30), Some(SubsystemKind::Engines));
        let (kind, _) = r.subsystem.expect("shield is down; precision fire lands");
        assert_eq!(kind, SubsystemKind::Engines);
        // Half diverted to the pool, half to the hull.
        assert_eq!(r.hull_damage, 15);
        assert_eq!(v.sub_hp[0], v.sub_max - 15);
    }

    #[test]
    fn focused_fire_walks_a_subsystem_to_disabled() {
        let mut v = CombatVessel::new(400, 0); // sub_max = 100
        let gun = kinetic(80); // 40 diverted per hit
        assert_eq!(v.state(SubsystemKind::Weapons), SubsystemState::Nominal);
        apply_hit(&mut v, &gun, Some(SubsystemKind::Weapons));
        // 100 - 40 = 60 > 50 → still nominal.
        assert_eq!(v.state(SubsystemKind::Weapons), SubsystemState::Nominal);
        apply_hit(&mut v, &gun, Some(SubsystemKind::Weapons));
        // 20 <= 50 → damaged.
        assert_eq!(v.state(SubsystemKind::Weapons), SubsystemState::Damaged);
        apply_hit(&mut v, &gun, Some(SubsystemKind::Weapons));
        assert_eq!(v.state(SubsystemKind::Weapons), SubsystemState::Disabled);
        // The rest of the ship survived the surgery.
        assert!(!v.destroyed());
    }

    #[test]
    fn destruction_clamps_at_zero() {
        let mut v = CombatVessel::new(50, 0);
        let r = apply_hit(&mut v, &kinetic(999), None);
        assert!(r.destroyed);
        assert_eq!(v.hull, 0, "never negative");
    }

    #[test]
    fn fractions_feed_behavior_senses() {
        let mut v = CombatVessel::new(400, 100);
        assert_eq!(v.hull_frac(), 1024);
        assert_eq!(v.shield_frac(), 1024);
        v.hull = 100;
        v.shield = 0;
        assert_eq!(v.hull_frac(), 256);
        assert_eq!(v.shield_frac(), 0);
    }

    #[test]
    fn shield_recharge_clamps_to_max() {
        let mut v = CombatVessel::new(400, 100);
        v.shield = 90;
        v.recharge_shield(50);
        assert_eq!(v.shield, 100);
        v.recharge_shield(-10); // hostile input is a no-op, not a drain
        assert_eq!(v.shield, 100);
    }

    #[test]
    fn weapon_stats_read_item_fixed_point() {
        use std::collections::BTreeMap;
        let mut m = BTreeMap::new();
        m.insert(StatKey::Damage, 26 * ONE);
        m.insert(StatKey::Range, 70 * ONE);
        m.insert(StatKey::FireRate, 3 * ONE);
        let w = WeaponStats::from_item_stats(&ItemStats(m), WeaponKind::Energy);
        assert_eq!((w.damage, w.range, w.fire_rate), (26, 70, 3));
        // Missing keys floor at 1 — no zero cooldowns downstream.
        let w = WeaponStats::from_item_stats(&ItemStats(BTreeMap::new()), WeaponKind::Kinetic);
        assert_eq!((w.damage, w.range, w.fire_rate), (1, 1, 1));
    }
}
