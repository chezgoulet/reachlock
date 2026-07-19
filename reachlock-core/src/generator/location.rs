//! Location generator (S25): seed + size -> location data.

use crate::util::SeededRng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Location {
    pub name: String,
    pub room_count: u32,
    pub enemy_count: u32,
}

fn pick<'a>(rng: &mut SeededRng, table: &'a [&str]) -> &'a str {
    table[rng.next_below(table.len() as u64) as usize]
}

const PREFIXES: &[&str] = &[
    "Abandoned", "Forgotten", "Ancient", "Derelict", "Hidden", "Lost",
    "Cursed", "Burning", "Sunken", "Floating", "Submerged", "Cratered",
];

const SUFFIXES: &[&str] = &[
    "Station", "Outpost", "Bunker", "Vault", "Temple", "Tower",
    "Caverns", "Depths", "Ruins", "Facility", "Colony", "Refinery",
    "Mine", "Dock", "Archive", "Laboratory",
];

fn size_params(size: &str) -> (u32, u32, u32, u32) {
    match size {
        "small" => (3, 6, 2, 5),
        "medium" => (6, 12, 5, 12),
        "large" => (12, 20, 10, 25),
        "huge" => (20, 35, 20, 50),
        _ => (4, 8, 3, 8),
    }
}

pub fn generate_location(seed: u64, size: &str) -> Location {
    let mut rng = SeededRng::new(seed);

    let prefix = pick(&mut rng, PREFIXES);
    let suffix = pick(&mut rng, SUFFIXES);
    let name = format!("{} {}", prefix, suffix);

    let (room_lo, room_hi, enemy_lo, enemy_hi) = size_params(size);
    let room_count = room_lo + rng.next_below((room_hi - room_lo + 1) as u64) as u32;
    let enemy_count = enemy_lo + rng.next_below((enemy_hi - enemy_lo + 1) as u64) as u32;

    Location { name, room_count, enemy_count }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_location(42, "medium");
        let b = generate_location(42, "medium");
        assert_eq!(a.name, b.name);
        assert_eq!(a.room_count, b.room_count);
        assert_eq!(a.enemy_count, b.enemy_count);
    }

    #[test]
    fn sizes_differ() {
        let a = generate_location(7, "small");
        let b = generate_location(7, "large");
        assert!(a.room_count <= b.room_count);
    }

    #[test]
    fn counts_within_bounds() {
        let loc = generate_location(99, "medium");
        assert!((6..=12).contains(&loc.room_count));
        assert!((5..=12).contains(&loc.enemy_count));
    }
}
