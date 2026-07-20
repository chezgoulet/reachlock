//! Storyline chapter generator (S25): seed -> episodic narrative.

use crate::util::SeededRng;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoryChapter {
    pub title: String,
    pub text: String,
}

fn pick<'a>(rng: &mut SeededRng, table: &'a [&str]) -> &'a str {
    table[rng.next_below(table.len() as u64) as usize]
}

const TITLE_TEMPLATES: &[&str] = &[
    "The {adj} {noun}",
    "{adj} {noun}",
    "Echoes of {noun}",
    "The {noun} Protocol",
    "{adj} Horizon",
    "Beyond the {noun}",
    "Into {adj} Space",
    "The {noun} Gambit",
    "{adj} Awakening",
    "Children of {noun}",
];

const ADJECTIVES: &[&str] = &[
    "Lost",
    "Dark",
    "Silent",
    "Broken",
    "Burning",
    "Frozen",
    "Crimson",
    "Fading",
    "Hollow",
    "Dying",
    "Ancient",
    "Forgotten",
    "Hidden",
    "Last",
    "Shattered",
    "Bleeding",
    "Empty",
    "Final",
    "Rising",
    "Fallen",
];

const NOUNS: &[&str] = &[
    "Signal",
    "Gate",
    "Star",
    "Void",
    "Colony",
    "Wreck",
    "Station",
    "Gauntlet",
    "Nexus",
    "Relic",
    "Drift",
    "Core",
    "Frontier",
    "Depths",
    "Expanse",
    "Harbor",
    "Ashes",
    "Covenant",
    "Threshold",
    "Pilgrimage",
];

const PROLOGUES: &[&str] = &[
    "The signal arrived without warning. Static resolved into a set of coordinates and a single word: COME.",
    "Something is wrong with the gate network. Ships are arriving at the wrong systems. The old routes are shifting.",
    "A distress call echoes through the sector. The transmission is old — decades old — but the message is still urgent.",
    "The derelict drifted in silence. No power signature, no life signs, no log records. But something inside was still running.",
    "War is coming. Everyone can feel it. Factions arm themselves. Pirates grow bolder. The core worlds look away.",
    "They found something in the deep range. A structure that predates every known civilization. And it is waking up.",
    "A plague is spreading through the colonies. Not biological — digital. A ghost in every system's network.",
];

const DEVELOPMENTS: &[&str] = &[
    "The trail leads deeper into contested territory. Each jump brings more questions than answers.",
    "Alliances shift as the truth emerges. Old enemies become uneasy allies.",
    "A shadowy figure appears at every turn, always one step ahead. Their motives remain unclear.",
    "The technology is unlike anything ever seen. It rewrites what we know about physics.",
    "Someone is sabotaging the investigation. Supplies go missing. Data gets corrupted. Crew members vanish.",
    "A rival faction intercepts the transmission. Now it's a race to the source.",
    "The entity makes contact. It does not speak in words, but in images. Disturbing ones.",
];

const RESOLUTIONS: &[&str] = &[
    "The source of the signal is not a place, but a person. Someone who should not exist anymore.",
    "The gate network was never meant for humans. The old builders left a warning. We ignored it.",
    "The derelict was a tomb. Not for its crew — for something they found and could not contain.",
    "Peace is brokered, but the cost is high. Some secrets were never meant to stay buried.",
    "The structure activates. It does not destroy. It observes. And it is not alone.",
    "The cure exists, but distributing it requires navigating a web of corporate interests and paranoia.",
    "The figure is revealed to be an alternate version of yourself from a future that must not happen.",
];

fn chapter_text(rng: &mut SeededRng, chapter_index: u32, chapter_count: u32) -> String {
    let part = if chapter_index == 0 {
        pick(rng, PROLOGUES)
    } else if chapter_index == chapter_count - 1 {
        pick(rng, RESOLUTIONS)
    } else {
        pick(rng, DEVELOPMENTS)
    };

    let specific = match rng.next_below(4) {
        0 => format!(
            " In system {}, the crew faces a difficult choice.",
            pick(rng, NOUNS)
        ),
        1 => format!(
            " The {}-class ship pushes onward through the void.",
            pick(rng, ADJECTIVES)
        ),
        2 => format!(
            " {} {:04x} is the only clue left behind.",
            pick(rng, NOUNS),
            rng.next_u64() & 0xFFFF
        ),
        _ => String::new(),
    };

    format!("{}{}", part, specific)
}

fn chapter_title(rng: &mut SeededRng, _chapter_index: u32) -> String {
    let adj = pick(rng, ADJECTIVES);
    let noun = pick(rng, NOUNS);
    let template = pick(rng, TITLE_TEMPLATES);
    template.replace("{adj}", adj).replace("{noun}", noun)
}

pub fn generate_storyline(seed: u64, chapter_count: u32) -> Vec<StoryChapter> {
    let mut rng = SeededRng::new(seed);
    let count = chapter_count.max(3);

    let mut chapters = Vec::with_capacity(count as usize);
    for i in 0..count {
        let title = chapter_title(&mut rng, i);
        let text = chapter_text(&mut rng, i, count);
        chapters.push(StoryChapter { title, text });
    }
    chapters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = generate_storyline(42, 5);
        let b = generate_storyline(42, 5);
        assert_eq!(a, b);
    }

    #[test]
    fn correct_chapter_count() {
        let s = generate_storyline(7, 8);
        assert_eq!(s.len(), 8);
    }

    #[test]
    fn minimum_three_chapters() {
        let s = generate_storyline(3, 1);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn chapters_have_content() {
        let s = generate_storyline(99, 4);
        for ch in &s {
            assert!(!ch.title.is_empty());
            assert!(!ch.text.is_empty());
        }
    }
}
