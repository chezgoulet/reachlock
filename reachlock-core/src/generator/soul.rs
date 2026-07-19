//! Soul generator (S25): seed + species -> NPC personality data.

use crate::util::SeededRng;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Soul {
    pub name: String,
    pub species: String,
    pub backstory: String,
    pub formality: i16,
    pub verbosity: i16,
    pub humor: i16,
    pub aggression: i16,
    pub portrait_seed: u64,
}

fn pick<'a>(rng: &mut SeededRng, table: &'a [&str]) -> &'a str {
    table[rng.next_below(table.len() as u64) as usize]
}

fn name_tables(species: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match species {
        "Human" => (
            &["Ana", "Bas", "Caz", "Dax", "Elu", "Fen", "Gus", "Hav", "Ion", "Jex", "Kai", "Lux", "Mya", "Nox", "Osa", "Pax", "Rey", "Siv", "Tor", "Vix", "Wyn", "Xan", "Yen", "Zev"],
            &["Chen", "Drake", "Ezzo", "Farr", "Graves", "Hale", "Ito", "Jax", "Korr", "Lynx", "Moss", "Nero", "Ortiz", "Pryce", "Rho", "Shade", "Toll", "Vane", "Wren", "Yukio"],
        ),
        "Synthetic" => (
            &["ALPHA", "BETA", "C6", "D3LTA", "E_X", "F7", "GAMMA", "H9", "IOTA", "J5", "KAPPA", "L2", "M4", "NOVA", "OMEGA", "P1", "Q7", "R5", "SIGMA", "T3", "U4", "V2", "W6", "X1", "Y0", "Z3"],
            &["Unit", "Frame", "Chassis", "Core", "Droid", "Engine", "Golem", "Mech", "Proxy", "Shell"],
        ),
        "Voidborn" => (
            &["Cir", "Dusk", "Esh", "Fane", "Gloom", "Hush", "Ish", "Jinn", "Kith", "Lorn", "Mist", "Nyx", "Ombre", "Pall", "Rime", "Shade", "Tarn", "Umbra", "Vale", "Wisp", "Ymir", "Zeph"],
            &["Night", "Shadow", "Deep", "Dark", "Star", "Void", "Dust", "Shroud"],
        ),
        "Augmented" => (
            &["Blade", "Cypher", "Dash", "Echo", "Flux", "Ghost", "Hex", "Jolt", "Kode", "Link", "Neon", "Pixel", "Quake", "Reap", "Spark", "Tesla", "Vex", "Watt", "Zero"],
            &["Cyte", "Dyne", "Graft", "Hack", "Mod", "Neural", "Rig", "Synth", "Tek", "Ware"],
        ),
        "Xenotype" => (
            &["Chrr", "Fssk", "Grrk", "Hsss", "Krrk", "Mroo", "Nnnn", "Prrt", "Rrsh", "Sssk", "Trrl", "Vrrn", "Xrrk", "Yrrl", "Zrrk"],
            &["Hive", "Nest", "Swarm", "Brood", "Caste", "Cluster", "Colony", "Horde"],
        ),
        _ => (&["Nom", "One", "Two"], &["None", "Unknown"]),
    }
}

fn backstory_tables(species: &str) -> &'static [&'static str] {
    match species {
        "Human" => &[
            "Born on a frontier colony, survived a pirate raid.",
            "Ex-corporate security, went freelance after a betrayal.",
            "Raised in a Core-world orbital, never touched dirt.",
            "Deserter from a private military company.",
            "Third-generation spacer, knows every ship like home.",
            "Fled religious persecution on their homeworld.",
            "Former trade guild merchant, ruined by a bad deal.",
            "Station rat who worked every dock job there is.",
        ],
        "Synthetic" => &[
            "Activated in a factory, learned to think beyond orders.",
            "Retrieved from a derelict research station.",
            "Escaped from a corporate lab with stolen memories.",
            "Built by a tinkerer who gave it too much freedom.",
            "One of a discontinued line, still running.",
            "Woke up on a scrap heap with no call sign.",
        ],
        "Voidborn" => &[
            "Born in deep space, has never seen a star up close.",
            "Survived a self-jump that killed the rest of the crew.",
            "Raised in a generation ship's library.",
            "Found drifting in an escape pod as a child.",
            "Haunted by visions from a near-miss with an anomaly.",
            "Born on a comet miner, inherited the debt.",
        ],
        "Augmented" => &[
            "Volunteered for full-body replacement after an accident.",
            "Designed their own augmentations piece by piece.",
            "Former military cyber-soldier, now mercenary.",
            "Augmented for deep-space salvage operations.",
            "Lost organic limbs to an infection, chose steel.",
            "Prototype unit stolen from a research facility.",
        ],
        "Xenotype" => &[
            "First contact survivor, adopted into human space.",
            "Diplomatic exile from their hive collective.",
            "Scout for a xenological survey that went wrong.",
            "Hatchery-born with an unusual independent streak.",
            "Translator for interspecies trade negotiations.",
            "Cast out for questioning the hive mind.",
        ],
        _ => &["Origin unknown.", "Past is classified.", "No record found."],
    }
}

fn slider(rng: &mut SeededRng) -> i16 {
    (rng.next_below(1025)) as i16
}

pub fn generate_soul(seed: u64, species: &str) -> Soul {
    let mut rng = SeededRng::new(seed);

    let (first_names, last_names) = name_tables(species);
    let first = pick(&mut rng, first_names);
    let last = pick(&mut rng, last_names);
    let name = format!("{} {}", first, last);

    let stories = backstory_tables(species);
    let backstory = pick(&mut rng, stories).to_string();

    let portrait_seed = rng.next_u64();

    Soul {
        name,
        species: species.to_string(),
        backstory,
        formality: slider(&mut rng),
        verbosity: slider(&mut rng),
        humor: slider(&mut rng),
        aggression: slider(&mut rng),
        portrait_seed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_soul(42, "Human");
        let b = generate_soul(42, "Human");
        assert_eq!(a.name, b.name);
        assert_eq!(a.backstory, b.backstory);
        assert_eq!(a.formality, b.formality);
    }

    #[test]
    fn species_differ() {
        let a = generate_soul(7, "Human");
        let b = generate_soul(7, "Synthetic");
        assert_ne!(a.name, b.name);
    }

    #[test]
    fn sliders_in_range() {
        let s = generate_soul(99, "Voidborn");
        assert!(s.formality >= 0 && s.formality <= 1024);
        assert!(s.verbosity >= 0 && s.verbosity <= 1024);
        assert!(s.humor >= 0 && s.humor <= 1024);
        assert!(s.aggression >= 0 && s.aggression <= 1024);
    }
}
