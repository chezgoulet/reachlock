//! Seeded encounter generation (S19, spec §22): `threat_level` → patrol
//! wings anchored near the gate and asteroid fields. Pure function of the
//! system — same seed, same ambush, on every target (manifest-pinned).

use serde::{Deserialize, Serialize};

use super::damage::{WeaponKind, WeaponStats};
use crate::generator::hull::HullClass;
use crate::generator::system::GeneratedSystem;
use crate::generator::FixedVec2;
use crate::item::{stats::roll_stats, ItemFamily};
use crate::util::rng::{Fixed, SeededRng};
use crate::util::trig::{icos, isin};

/// Spec §22 enemy roles shipping in S19: interceptor (fast, light) and
/// bomber (slow, heavy). Capital ships/bosses are an explicit non-goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyClass {
    Interceptor,
    Bomber,
}

impl EnemyClass {
    /// The hull the flight scene renders and rapier flies. Interceptors are
    /// shuttle-frame skirmishers; bombers fly converted freighter hulls.
    pub fn hull_class(self) -> HullClass {
        match self {
            EnemyClass::Interceptor => HullClass::Shuttle,
            EnemyClass::Bomber => HullClass::Freighter,
        }
    }

    /// Combat pools by class and threat tier. Interceptors die fast and
    /// shield-tank; bombers are hull sponges.
    pub fn vessel(self, tier: u8) -> super::CombatVessel {
        let t = tier.clamp(1, 10) as i64;
        match self {
            EnemyClass::Interceptor => super::CombatVessel::new(180 + 30 * t, 40 + 15 * t),
            EnemyClass::Bomber => super::CombatVessel::new(520 + 90 * t, 20 + 8 * t),
        }
    }

    /// The class's gun, rolled from S05 stat bands at the threat tier so
    /// gear math and enemy math are the same table. Interceptors run energy
    /// repeaters; bombers throw slow kinetic torpedoes.
    pub fn weapon(self, seed: u64, tier: u8) -> WeaponStats {
        let (family, kind) = match self {
            EnemyClass::Interceptor => (ItemFamily::EnergyWeapon, WeaponKind::Energy),
            EnemyClass::Bomber => (ItemFamily::MissileWeapon, WeaponKind::Kinetic),
        };
        let mut rng = SeededRng::new(seed ^ 0x57E4_60D5);
        let stats = roll_stats(&mut rng, family, tier.clamp(1, 10));
        WeaponStats::from_item_stats(&stats, kind)
    }

    pub fn label(self) -> &'static str {
        match self {
            EnemyClass::Interceptor => "interceptor",
            EnemyClass::Bomber => "bomber",
        }
    }
}

/// One enemy ship to spawn: where, what, and its personal seed (hull
/// geometry, weapon roll, patrol heading all derive from it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounterSpawn {
    pub position: FixedVec2,
    pub class: EnemyClass,
    pub seed: u64,
    /// Wing index — ships sharing a wing count as allies for behavior
    /// senses and reinforcement bookkeeping.
    pub wing: u32,
}

/// How far from its anchor (gate / asteroid field) a wing scatters.
const WING_SCATTER: i64 = 320;

/// Seeded encounters for a system (S19 deliverable): wing count and
/// composition scale with `threat_level` (0 = safe lanes, no spawns).
/// Anchors alternate between the gate and the asteroid fields — patrols
/// guard the places worth visiting.
pub fn generate_encounters(system_seed: u64, system: &GeneratedSystem) -> Vec<EncounterSpawn> {
    let threat = system.threat_level as u64;
    if threat == 0 {
        return Vec::new();
    }
    let mut rng = SeededRng::new(system_seed ^ 0xC0BB_A75E);
    let wing_count = threat.div_ceil(2); // threat 1-2 → 1 wing … 9-10 → 5
    let wing_size = 1 + (threat / 4) as usize; // 1..=3 ships per wing

    let mut anchors: Vec<FixedVec2> = vec![system.gate_position];
    anchors.extend(system.asteroid_fields.iter().map(|f| f.center));

    let mut spawns = Vec::new();
    for wing in 0..wing_count {
        let anchor = anchors[(wing as usize) % anchors.len()];
        for slot in 0..wing_size {
            let seed = rng.next_u64();
            // Bombers appear from threat 4 up, one per wing, trailing the
            // interceptor screen.
            let class = if threat >= 4 && slot == wing_size - 1 && wing_size > 1 {
                EnemyClass::Bomber
            } else {
                EnemyClass::Interceptor
            };
            let radius = rng.next_below(WING_SCATTER as u64) as i64;
            let turn = rng.next_below(65536) as u16;
            let offset_x = Fixed(Fixed::from_int(radius).0 * icos(turn) as i64 / 32768);
            let offset_y = Fixed(Fixed::from_int(radius).0 * isin(turn) as i64 / 32768);
            spawns.push(EncounterSpawn {
                position: FixedVec2 {
                    x: Fixed(anchor.x.0 + offset_x.0),
                    y: Fixed(anchor.y.0 + offset_y.0),
                },
                class,
                seed,
                wing: wing as u32,
            });
        }
    }
    spawns
}

/// Threat tier for a system — the single place client spawning and enemy
/// weapon rolls read it from.
pub fn threat_tier(system: &GeneratedSystem) -> u8 {
    system.threat_level.clamp(1, 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::system::{generate_system, Fidelity};
    use crate::seed::types::Biome;

    fn system(seed: u64) -> GeneratedSystem {
        generate_system(seed, Biome::Frontier, Fidelity::Full)
    }

    #[test]
    fn encounters_are_deterministic() {
        let sys = system(42);
        assert_eq!(generate_encounters(42, &sys), generate_encounters(42, &sys));
    }

    #[test]
    fn threat_scales_the_fleet() {
        let sys = system(42);
        let mut low = sys.clone();
        low.threat_level = 1;
        let mut high = sys.clone();
        high.threat_level = 9;
        let few = generate_encounters(42, &low);
        let many = generate_encounters(42, &high);
        assert!(!few.is_empty());
        assert!(
            many.len() > few.len(),
            "threat 9 ({}) must outnumber threat 1 ({})",
            many.len(),
            few.len()
        );
        // High threat brings bombers; low threat is interceptors only.
        assert!(few.iter().all(|s| s.class == EnemyClass::Interceptor));
        assert!(many.iter().any(|s| s.class == EnemyClass::Bomber));
    }

    #[test]
    fn zero_threat_spawns_nothing() {
        let mut sys = system(42);
        sys.threat_level = 0;
        assert!(generate_encounters(42, &sys).is_empty());
    }

    #[test]
    fn wings_scatter_near_their_anchor() {
        let mut sys = system(42);
        sys.threat_level = 2; // one wing, anchored at the gate
        for spawn in generate_encounters(42, &sys) {
            let dx = (spawn.position.x.0 - sys.gate_position.x.0).abs();
            let dy = (spawn.position.y.0 - sys.gate_position.y.0).abs();
            let max = Fixed::from_int(WING_SCATTER).0;
            assert!(dx <= max && dy <= max, "wing strays from the gate");
        }
    }

    #[test]
    fn class_tables_are_distinct_and_tier_scaled() {
        let i1 = EnemyClass::Interceptor.vessel(1);
        let i9 = EnemyClass::Interceptor.vessel(9);
        let b1 = EnemyClass::Bomber.vessel(1);
        assert!(i9.hull_max > i1.hull_max);
        assert!(b1.hull_max > i1.hull_max, "bombers are hull sponges");
        assert!(i1.shield_max > b1.shield_max, "interceptors shield-tank");

        let wi = EnemyClass::Interceptor.weapon(7, 4);
        let wb = EnemyClass::Bomber.weapon(7, 4);
        assert_eq!(wi.kind, WeaponKind::Energy);
        assert_eq!(wb.kind, WeaponKind::Kinetic);
        assert!(wb.damage > wi.damage, "torpedoes hit harder");
        assert!(wb.fire_rate <= wi.fire_rate, "and fire slower");
        // Same seed, same roll — enemy guns are deterministic.
        assert_eq!(wi, EnemyClass::Interceptor.weapon(7, 4));
    }
}
