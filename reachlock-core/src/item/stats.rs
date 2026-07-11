//! Stat bands and rarity rolls (spec §16, "Stat ranges"): tier sets the
//! band, the seed picks the tradeoff point inside it. Every value is
//! fixed-point (`util::rng::Fixed`, 1/1024) — no floats reach `ItemStats`.

use std::collections::BTreeMap;

use super::types::{ItemFamily, ItemStats, Rarity, StatKey};
use crate::util::rng::{Fixed, SeededRng};

/// Valid tier range (spec §16: "tier: u8, 1-10, determines stat range").
pub const TIER_MIN: u8 = 1;
pub const TIER_MAX: u8 = 10;

struct StatRange {
    key: StatKey,
    /// Band at tier 1, in whole units (pre-fixed-point scaling).
    base_lo: i64,
    base_hi: i64,
    /// Added to both ends of the band per tier above 1.
    growth: i64,
}

const fn r(key: StatKey, base_lo: i64, base_hi: i64, growth: i64) -> StatRange {
    StatRange {
        key,
        base_lo,
        base_hi,
        growth,
    }
}

/// Which stats apply to a family, and their tier-1 band + per-tier growth.
/// Every family includes `Weight` — every item has mass (spec: "weight is
/// fixed-point too" — index rule 2 overrides the spec's `f32`).
fn family_stat_ranges(family: ItemFamily) -> &'static [StatRange] {
    match family {
        ItemFamily::EnergyWeapon => &[
            r(StatKey::Damage, 8, 14, 6),
            r(StatKey::Range, 40, 70, 15),
            r(StatKey::FireRate, 2, 5, 1),
            r(StatKey::Weight, 3, 6, 1),
        ],
        ItemFamily::KineticWeapon => &[
            r(StatKey::Damage, 12, 20, 8),
            r(StatKey::Range, 30, 55, 12),
            r(StatKey::FireRate, 3, 7, 1),
            r(StatKey::Weight, 5, 9, 2),
        ],
        ItemFamily::MissileWeapon => &[
            r(StatKey::Damage, 20, 35, 14),
            r(StatKey::Range, 80, 140, 30),
            r(StatKey::FireRate, 1, 2, 1),
            r(StatKey::Weight, 6, 10, 2),
        ],
        ItemFamily::MeleeWeapon => &[
            r(StatKey::Damage, 6, 10, 4),
            r(StatKey::FireRate, 4, 8, 2),
            r(StatKey::Weight, 2, 4, 1),
        ],
        ItemFamily::BoardingWeapon => &[
            r(StatKey::Damage, 10, 18, 6),
            r(StatKey::Weight, 3, 6, 1),
        ],
        ItemFamily::Armor => &[
            r(StatKey::ShieldHp, 20, 40, 12),
            r(StatKey::Weight, 8, 14, 3),
        ],
        ItemFamily::Shield => &[
            r(StatKey::ShieldHp, 30, 60, 18),
            r(StatKey::Recharge, 2, 5, 1),
            r(StatKey::Weight, 4, 8, 1),
        ],
        ItemFamily::Engine => &[
            r(StatKey::Thrust, 20, 40, 10),
            r(StatKey::Turn, 5, 12, 2),
            r(StatKey::Weight, 10, 18, 3),
        ],
        ItemFamily::Sensor => &[
            r(StatKey::SensorRange, 50, 100, 20),
            r(StatKey::Weight, 1, 3, 1),
        ],
        ItemFamily::MiningTool => &[
            r(StatKey::MiningRate, 5, 12, 3),
            r(StatKey::Weight, 4, 8, 1),
        ],
        ItemFamily::RepairTool => &[
            r(StatKey::RepairRate, 5, 12, 3),
            r(StatKey::Weight, 3, 6, 1),
        ],
        ItemFamily::Cybernetic => &[r(StatKey::Weight, 1, 2, 0)],
        ItemFamily::Augmentation => &[r(StatKey::Weight, 1, 2, 0)],
        ItemFamily::Spacesuit => &[
            r(StatKey::ShieldHp, 10, 20, 5),
            r(StatKey::Weight, 6, 12, 2),
        ],
        ItemFamily::Consumable => &[r(StatKey::Weight, 1, 3, 0)],
        ItemFamily::Component => &[r(StatKey::Weight, 2, 6, 1)],
        ItemFamily::Implant => &[r(StatKey::Weight, 1, 2, 0)],
        ItemFamily::Cosmetic => &[r(StatKey::Weight, 0, 1, 0)],
    }
}

/// The inclusive-lo/exclusive-hi band (in whole units, pre-scaling) for a
/// given family/stat/tier — the same band `roll_stats` samples from.
/// Public so property tests (and, later, UI tooltips) can check "is this
/// stat in band" without re-deriving the table.
pub fn stat_band(family: ItemFamily, key: StatKey, tier: u8) -> Option<(i64, i64)> {
    let tier = tier.clamp(TIER_MIN, TIER_MAX) as i64;
    family_stat_ranges(family).iter().find(|range| range.key == key).map(|range| {
        let bonus = range.growth * (tier - 1);
        (range.base_lo + bonus, range.base_hi + bonus)
    })
}

/// Roll fixed-point stats for a family at a tier. Same `(rng state, family,
/// tier)` always produces the same stats; a fresh `SeededRng` per item seed
/// upstream is what gives two same-tier items different tradeoffs.
pub fn roll_stats(rng: &mut SeededRng, family: ItemFamily, tier: u8) -> ItemStats {
    let mut map = BTreeMap::new();
    for range in family_stat_ranges(family) {
        let (lo, hi) = stat_band(family, range.key, tier).expect("range came from the same table");
        let span = (hi - lo).max(1) as u64;
        let whole = lo + rng.next_below(span) as i64;
        map.insert(range.key, Fixed::from_int(whole).0);
    }
    ItemStats(map)
}

/// Roll rarity. Higher tier skews the distribution toward rarer outcomes,
/// but every tier can (rarely) roll Legendary and every tier can (commonly)
/// roll Common — tier sets the band, not a guarantee (mirrors stat rolls).
pub fn roll_rarity(rng: &mut SeededRng, tier: u8) -> Rarity {
    let tier = tier.clamp(TIER_MIN, TIER_MAX) as u64;
    let roll = rng.next_below(1000) + tier * 20;
    match roll {
        0..=549 => Rarity::Common,
        550..=799 => Rarity::Uncommon,
        800..=929 => Rarity::Rare,
        930..=984 => Rarity::Epic,
        _ => Rarity::Legendary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let mut a = SeededRng::new(42);
        let mut b = SeededRng::new(42);
        assert_eq!(
            roll_stats(&mut a, ItemFamily::KineticWeapon, 4),
            roll_stats(&mut b, ItemFamily::KineticWeapon, 4)
        );
    }

    #[test]
    fn higher_tier_raises_the_band() {
        let (lo1, hi1) = stat_band(ItemFamily::EnergyWeapon, StatKey::Damage, 1).unwrap();
        let (lo8, hi8) = stat_band(ItemFamily::EnergyWeapon, StatKey::Damage, 8).unwrap();
        assert!(lo8 > lo1 && hi8 > hi1);
    }

    #[test]
    fn every_family_rolls_weight() {
        for family in ItemFamily::ALL {
            let mut rng = SeededRng::new(1);
            let stats = roll_stats(&mut rng, family, 5);
            assert!(
                stats.0.contains_key(&StatKey::Weight),
                "{family:?} missing weight"
            );
        }
    }

    /// Property test (S05 deliverable): 500 seeded tier-4 kinetic weapons
    /// all stay in the tier-4 band, and at least two distinct stat profiles
    /// exist (seed variance actually moves the tradeoff point).
    #[test]
    fn five_hundred_tier_4_items_stay_in_band_and_vary() {
        let family = ItemFamily::KineticWeapon;
        let tier = 4;
        let mut profiles = std::collections::HashSet::new();
        for seed in 0..500u64 {
            let mut rng = SeededRng::new(seed ^ 0xABCD_EF01);
            let stats = roll_stats(&mut rng, family, tier);
            for (key, value) in &stats.0 {
                let (lo, hi) = stat_band(family, *key, tier).unwrap();
                let (lo, hi) = (Fixed::from_int(lo).0, Fixed::from_int(hi).0);
                assert!(
                    (lo..hi).contains(value),
                    "seed {seed}: {key:?}={value} out of band [{lo}, {hi})"
                );
            }
            profiles.insert(stats.0.into_iter().collect::<Vec<_>>());
        }
        assert!(
            profiles.len() >= 2,
            "500 seeds produced only {} distinct stat profile(s)",
            profiles.len()
        );
    }

    #[test]
    fn rarity_is_deterministic() {
        let mut a = SeededRng::new(99);
        let mut b = SeededRng::new(99);
        assert_eq!(roll_rarity(&mut a, 5), roll_rarity(&mut b, 5));
    }

    #[test]
    fn rarity_distribution_covers_multiple_tiers() {
        let mut seen = std::collections::HashSet::new();
        for seed in 0..300u64 {
            let mut rng = SeededRng::new(seed ^ 0x1234);
            seen.insert(roll_rarity(&mut rng, 3));
        }
        assert!(seen.len() >= 2, "expected rarity variance across 300 seeds");
    }

    #[test]
    fn tier_clamped_to_valid_range() {
        // Out-of-range tiers don't panic; they clamp.
        let (lo, hi) = stat_band(ItemFamily::EnergyWeapon, StatKey::Damage, 0).unwrap();
        let (lo1, hi1) = stat_band(ItemFamily::EnergyWeapon, StatKey::Damage, 1).unwrap();
        assert_eq!((lo, hi), (lo1, hi1));
        let (lo, hi) = stat_band(ItemFamily::EnergyWeapon, StatKey::Damage, 255).unwrap();
        let (lo10, hi10) = stat_band(ItemFamily::EnergyWeapon, StatKey::Damage, 10).unwrap();
        assert_eq!((lo, hi), (lo10, hi10));
    }
}
