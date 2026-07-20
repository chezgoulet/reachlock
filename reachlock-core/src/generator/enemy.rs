//! Enemy archetype generator (S25): seed + class -> enemy stats.

use crate::util::SeededRng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnemyArchetype {
    pub name: String,
    pub hull: i64,
    pub shield: i64,
    pub speed: i16,
    pub attack_pattern: String,
}

fn pick<'a>(rng: &mut SeededRng, table: &'a [&str]) -> &'a str {
    table[rng.next_below(table.len() as u64) as usize]
}

fn name_table(class: &str) -> &'static [&'static str] {
    match class {
        "drone" => &[
            "Shiv", "Stinger", "Needle", "Spark", "Gnat", "Pincer", "Fang", "Razor", "Splinter",
            "Barb",
        ],
        "fighter" => &[
            "Reaper", "Viper", "Mantis", "Scorpion", "Wasp", "Hornet", "Raven", "Hawk", "Falcon",
            "Osprey",
        ],
        "cruiser" => &[
            "Leviathan",
            "Behemoth",
            "Colossus",
            "Titan",
            "Goliath",
            "Hydra",
            "Kraken",
            "Juggernaut",
            "Overlord",
            "Dreadnought",
        ],
        "bomber" => &[
            "Hammer",
            "Boulder",
            "Anvil",
            "Crusher",
            "Meteor",
            "Comet",
            "Thumper",
            "Ram",
            "Pounder",
            "Battering",
        ],
        "carrier" => &[
            "Hive",
            "Nest",
            "Swarm",
            "Brood",
            "Arkship",
            "Cradle",
            "Mother",
            "Harbor",
            "Bastion",
            "Sanctuary",
        ],
        _ => &["Marauder", "Raider", "Bandit", "Outlaw", "Predator"],
    }
}

fn attack_patterns(class: &str) -> &'static [&'static str] {
    match class {
        "drone" => &[
            "swarm_rush",
            "hit_and_run",
            "flanking_pincer",
            "kamikaze_dive",
            "harassment_circle",
        ],
        "fighter" => &[
            "boom_and_zoom",
            "energy_siphon",
            "missile_barrage",
            "jousting_pass",
            "shield_buster",
        ],
        "cruiser" => &[
            "broadside_cannon",
            "turret_covering",
            "ion_cannon_snipe",
            "point_defense_wall",
            "railgun_salvo",
        ],
        "bomber" => &[
            "torpedo_run",
            "carpet_bomb",
            "plasma_drop",
            "orbital_strike",
            "cluster_munitions",
        ],
        "carrier" => &[
            "drone_spam",
            "boarding_pod",
            "tractor_trap",
            "mine_layer",
            "repair_squadron",
        ],
        _ => &[
            "wild_fire",
            "unchained_aggression",
            "erratic_pursuit",
            "ambush_tactic",
        ],
    }
}

fn stat_bands(class: &str) -> (i64, i64, i64, i64, i64, i64, i16, i16) {
    match class {
        "drone" => (80, 200, 10, 80, 300, 600, 60, 120),
        "fighter" => (300, 800, 100, 400, 200, 500, 40, 100),
        "cruiser" => (2000, 5000, 1000, 3000, 50, 150, 10, 40),
        "bomber" => (800, 2000, 200, 800, 100, 300, 20, 60),
        "carrier" => (3000, 6000, 1500, 4000, 40, 100, 5, 20),
        _ => (400, 1200, 100, 500, 150, 400, 30, 80),
    }
}

pub fn generate_enemy(seed: u64, class: &str) -> EnemyArchetype {
    let mut rng = SeededRng::new(seed);

    let name = pick(&mut rng, name_table(class)).to_string();
    let attack_pattern = pick(&mut rng, attack_patterns(class)).to_string();

    let (hull_lo, hull_hi, shield_lo, shield_hi, speed_lo, speed_hi, _, _) = stat_bands(class);

    let hull = hull_lo + rng.next_below((hull_hi - hull_lo + 1) as u64) as i64;
    let shield = shield_lo + rng.next_below((shield_hi - shield_lo + 1) as u64) as i64;
    let speed = (speed_lo as i16) + rng.next_below((speed_hi - speed_lo + 1) as u64) as i16;

    EnemyArchetype {
        name,
        hull,
        shield,
        speed,
        attack_pattern,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_enemy(42, "drone");
        let b = generate_enemy(42, "drone");
        assert_eq!(a.name, b.name);
        assert_eq!(a.hull, b.hull);
        assert_eq!(a.shield, b.shield);
        assert_eq!(a.speed, b.speed);
    }

    #[test]
    fn classes_differ() {
        let a = generate_enemy(7, "drone");
        let b = generate_enemy(7, "cruiser");
        assert_ne!(a.hull, b.hull);
        assert!(b.hull > a.hull);
    }

    #[test]
    fn stats_positive() {
        let e = generate_enemy(99, "fighter");
        assert!(e.hull > 0);
        assert!(e.shield >= 0);
        assert!(e.speed > 0);
    }
}
