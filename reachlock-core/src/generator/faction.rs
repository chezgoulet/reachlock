//! Faction generator (S25): seed -> faction data.

use crate::util::SeededRng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Faction {
    pub name: String,
    pub doctrine: String,
    pub goal: String,
}

fn pick<'a>(rng: &mut SeededRng, table: &'a [&str]) -> &'a str {
    table[rng.next_below(table.len() as u64) as usize]
}

const PREFIXES: &[&str] = &[
    "Iron", "Crimson", "Azure", "Obsidian", "Golden", "Shadow", "Solar", "Lunar", "Void",
    "Crystal", "Storm", "Ember", "Frost", "Ash", "Thorn", "Bone",
];

const SUFFIXES: &[&str] = &[
    "Legion",
    "Collective",
    "Syndicate",
    "Alliance",
    "Covenant",
    "Federation",
    "Order",
    "Clan",
    "Nexus",
    "Dominion",
    "Guild",
    "Council",
    "Concord",
    "Hegemony",
];

const DOCTRINES: &[&str] = &[
    "Expansion through trade and diplomacy",
    "Military supremacy above all",
    "Self-sufficiency and isolation",
    "Knowledge is the only currency",
    "Purity of the human genome",
    "Technological transcendence",
    "Free market, no governance",
    "Collective ownership of all assets",
    "Ancestral traditions guide all decisions",
    "Might makes right in every dispute",
    "Peaceful coexistence with all species",
    "Corporate profit justifies any action",
    "Faith in a higher power unites us",
    "Meritocracy through combat trials",
    "Scientific progress at any cost",
    "Balance between nature and industry",
];

const GOALS: &[&str] = &[
    "Control all trade routes in the sector",
    "Eliminate rival factions from the system",
    "Discover ancient precursor technology",
    "Establish a new homeworld for their people",
    "Achieve economic dominance over all others",
    "Unlock the secrets of self-generated jump",
    "Build the largest fleet in known space",
    "Secure a monopoly on a rare resource",
    "Restore a lost artificial intelligence",
    "Convert all colonies to their ideology",
    "Terraform a dead world into a paradise",
    "Assassinate the leader of a rival faction",
    "Hack the galactic network for intelligence",
    "Protect the sector from an external threat",
    "Reverse-engineer xenotype technology",
    "Install a puppet government on a core world",
];

pub fn generate_faction(seed: u64) -> Faction {
    let mut rng = SeededRng::new(seed);

    let prefix = pick(&mut rng, PREFIXES);
    let suffix = pick(&mut rng, SUFFIXES);
    let name = format!("{} {}", prefix, suffix);

    let doctrine = pick(&mut rng, DOCTRINES).to_string();
    let goal = pick(&mut rng, GOALS).to_string();

    Faction {
        name,
        doctrine,
        goal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_faction(42);
        let b = generate_faction(42);
        assert_eq!(a.name, b.name);
        assert_eq!(a.doctrine, b.doctrine);
        assert_eq!(a.goal, b.goal);
    }

    #[test]
    fn not_all_seeds_are_identical() {
        let mut seen = Vec::new();
        for seed in 0..50 {
            seen.push(generate_faction(seed));
        }
        // At least one pair should differ across 50 seeds.
        let some_differ = seen.windows(2).any(|w| w[0].name != w[1].name);
        assert!(some_differ, "all 50 seeds produced identical names");
    }

    #[test]
    fn fields_not_empty() {
        let f = generate_faction(99);
        assert!(!f.name.is_empty());
        assert!(!f.doctrine.is_empty());
        assert!(!f.goal.is_empty());
    }
}
