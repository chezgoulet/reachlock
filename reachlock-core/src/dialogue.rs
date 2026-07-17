//! Dialogue & deliberation context (S16, spec §15 soul→dialogue, §6 UX).
//!
//! Three jobs, all pure:
//! - [`DialogueContext`]: the exact, size-bounded context assembled for an
//!   NPC inference call. Secret-safety is enforced HERE, at assembly — the
//!   model is never handed what the player hasn't earned — not by asking
//!   the prompt nicely.
//! - The dialogue graph: authored nodes with choices, conditions (reusing
//!   [`crate::contract::types::Condition`] over soul/reputation bridge
//!   fields — one predicate language), soul-event effects, and `llm_edge`
//!   nodes where the script hands over to inference.
//! - Voice shaping: the system-prompt template renders personality + the
//!   CURRENT mood into instructions, and [`shape_line`] post-processes the
//!   reply (strip meta, cap length). Terse Boris must read terse — the
//!   template is what's tested, not the model.

use serde::{Deserialize, Serialize};

use crate::contract::engine::{condition_holds, EvalContext};
use crate::contract::types::Condition;
use crate::soul::runtime::{inject_soul_fields, SoulState};
use crate::soul::types::{SoulFile, SpeakingStyle};

/// Hard byte budget for an assembled context (serialized JSON). The
/// assembler trims memories/history to fit; it never exceeds this.
pub const CONTEXT_BUDGET_BYTES: usize = 4096;
/// Memories included, best emotional weight first.
pub const TOP_K_MEMORIES: usize = 5;
/// Exchange turns of history included, most recent last.
pub const LAST_N_TURNS: usize = 6;
/// Client-side cap on a free-input utterance (the S14 rule: cap before it
/// ever reaches the wire).
pub const MAX_UTTERANCE_CHARS: usize = 240;
/// Spoken-line cap after post-processing.
pub const MAX_LINE_CHARS: usize = 280;

/// One prior exchange turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueTurn {
    /// "player" or the soul id.
    pub speaker: String,
    pub line: String,
}

/// The exact context an NPC inference call receives — nothing more. Public
/// identity only; unrevealed secret *content* never appears (tested).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueContext {
    pub soul_id: String,
    pub name: String,
    pub role: String,
    pub public_bio: String,
    pub personality: String,
    pub mood: String,
    /// 0..=1024.
    pub mood_intensity: i64,
    /// Top-K memory summaries by emotional weight.
    pub memories: Vec<String>,
    /// Standing with the speaker: (trust, familiarity), fixed-point.
    pub relationship_with_speaker: Option<(i64, i64)>,
    pub goals: Vec<String>,
    /// Last N turns, oldest first.
    pub history: Vec<DialogueTurn>,
    /// What the player just said or chose.
    pub input: String,
}

/// Assemble the bounded context for one inference call. Deterministic:
/// same soul + history + input = same context, byte for byte.
pub fn assemble(
    file: &SoulFile,
    state: &SoulState,
    history: &[DialogueTurn],
    input: &str,
) -> DialogueContext {
    let mut memories: Vec<&crate::soul::types::Memory> = state.memories.iter().collect();
    // Best weight first; ties newest-first so the context stays fresh.
    memories.sort_by_key(|m| (-m.emotional_weight, u64::MAX - m.timestamp));

    let mut input: String = input.chars().take(MAX_UTTERANCE_CHARS).collect();
    let mut ctx = DialogueContext {
        soul_id: file.id.clone(),
        name: file.name.clone(),
        role: file.identity.role.clone(),
        public_bio: file.identity.public_bio.clone(),
        personality: personality_summary(file, state),
        mood: state.mood.as_str().to_string(),
        mood_intensity: state.intensity,
        memories: memories
            .iter()
            .take(TOP_K_MEMORIES)
            .map(|m| m.summary.clone())
            .collect(),
        relationship_with_speaker: state
            .relationship("player")
            .map(|r| (r.trust, r.familiarity)),
        goals: state.goals.iter().map(|g| g.description.clone()).collect(),
        history: history
            .iter()
            .rev()
            .take(LAST_N_TURNS)
            .rev()
            .cloned()
            .collect(),
        input: std::mem::take(&mut input),
    };

    // Enforce the byte budget by shedding bulk in reverse order of value:
    // history first, then memories, then goals. The identity block always
    // fits (authored fields are short by schema).
    while context_bytes(&ctx) > CONTEXT_BUDGET_BYTES {
        if !ctx.history.is_empty() {
            ctx.history.remove(0);
        } else if !ctx.memories.is_empty() {
            ctx.memories.pop();
        } else if !ctx.goals.is_empty() {
            ctx.goals.pop();
        } else {
            // Last resort: truncate the bio (identity stays, prose shrinks).
            let keep = ctx.public_bio.chars().take(120).collect();
            if ctx.public_bio == keep {
                break;
            }
            ctx.public_bio = keep;
        }
    }
    ctx
}

fn context_bytes(ctx: &DialogueContext) -> usize {
    serde_json::to_string(ctx).map(|s| s.len()).unwrap_or(0)
}

/// The live personality line: traits come from the *state* (mutations move
/// them), style and values from the file.
fn personality_summary(file: &SoulFile, state: &SoulState) -> String {
    format!(
        "traits: {}; values: {}; speaking style: {}",
        state.traits.join(", "),
        file.personality.values.join(", "),
        style_word(file.personality.speaking_style),
    )
}

fn style_word(style: SpeakingStyle) -> &'static str {
    match style {
        SpeakingStyle::Terse => "terse",
        SpeakingStyle::Elaborate => "elaborate",
        SpeakingStyle::Technical => "technical",
        SpeakingStyle::Lyrical => "lyrical",
        SpeakingStyle::Sarcastic => "sarcastic",
        SpeakingStyle::Blunt => "blunt",
        SpeakingStyle::Formal => "formal",
        SpeakingStyle::Wry => "wry",
        SpeakingStyle::Warm => "warm",
    }
}

/// Voice-shaping system prompt: personality, quirks, values, and the
/// CURRENT mood become instructions. This is what the S16 test pins.
pub fn voice_prompt(file: &SoulFile, state: &SoulState) -> String {
    let style = style_word(file.personality.speaking_style);
    let style_directive = match file.personality.speaking_style {
        SpeakingStyle::Terse => "Answer in one or two short sentences. Never elaborate.",
        SpeakingStyle::Formal => {
            "Use full names and complete, precise sentences. Never use contractions or nicknames."
        }
        SpeakingStyle::Blunt => "Say the thing directly. Do not soften it.",
        SpeakingStyle::Wry => "Dry, understated; humor arrives sideways.",
        SpeakingStyle::Lyrical => "Let the language breathe; imagery is welcome.",
        SpeakingStyle::Warm => "Kind, present, unhurried.",
        SpeakingStyle::Technical => "Precise terminology; numbers where they exist.",
        SpeakingStyle::Sarcastic => "The compliment is the insult.",
        SpeakingStyle::Elaborate => "Full, rounded answers.",
    };
    format!(
        "You are {name}, {role}. {bio}\n\
         Speaking style: {style}. {style_directive}\n\
         Quirks: {quirks}.\n\
         You value: {values}.\n\
         Your current mood is {mood} (intensity {pct}%). Let it color the reply.\n\
         Respond IN CHARACTER with a single spoken line. No narration, no \
         stage directions, no quotation marks around the whole line.",
        name = file.name,
        role = file.identity.role,
        bio = file.identity.public_bio,
        quirks = file.personality.quirks.join("; "),
        values = file.personality.values.join(", "),
        mood = state.mood.as_str(),
        pct = state.intensity * 100 / 1024,
    )
}

/// Post-process a model line: strip a leading "Name:" echo, surrounding
/// quotes, and stage directions in asterisks; collapse whitespace; cap
/// length at a sentence boundary where possible.
pub fn shape_line(raw: &str, name: &str) -> String {
    let mut line = raw.trim();
    // Leading speaker echo ("Boris: …").
    if let Some(rest) = line.strip_prefix(&format!("{name}:")) {
        line = rest.trim_start();
    }
    // Whole-line quotes.
    if line.len() >= 2 && line.starts_with('"') && line.ends_with('"') {
        line = &line[1..line.len() - 1];
    }
    // Drop *stage directions*.
    let mut cleaned = String::with_capacity(line.len());
    let mut in_stars = false;
    for c in line.chars() {
        match c {
            '*' => in_stars = !in_stars,
            _ if in_stars => {}
            _ => cleaned.push(c),
        }
    }
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_LINE_CHARS {
        return collapsed;
    }
    // Cap at the last sentence end inside the budget, else hard cut with an
    // ellipsis.
    let capped: String = collapsed.chars().take(MAX_LINE_CHARS).collect();
    match capped.rfind(['.', '!', '?']) {
        Some(i) if i > MAX_LINE_CHARS / 2 => capped[..=i].to_string(),
        _ => format!("{}…", capped.trim_end()),
    }
}

/// Deterministic offline deflection: with no inference available the soul
/// answers the unscripted edge in its own authored voice — never a hang,
/// never lorem ipsum. `salt` (e.g. the universe tick) varies the pick.
pub fn deflection_line(file: &SoulFile, salt: u64) -> Option<&str> {
    if file.deflections.is_empty() {
        return None;
    }
    Some(file.deflections[(salt % file.deflections.len() as u64) as usize].as_str())
}

// ───────────────────────── dialogue graph ─────────────────────────

/// A soul-event effect a choice applies (kept narrow: the S13 event
/// pipeline does the actual work).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceEffect {
    /// Fire a soul event of this type at the current tick (fields empty;
    /// `player_involved` true; weight as given).
    SoulEvent {
        event_type: String,
        emotional_weight: i64,
        summary: String,
    },
    /// Move the relationship with the player.
    RelationshipDelta { trust: i64, familiarity: i64 },
}

/// One player choice on a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueChoice {
    pub label: String,
    /// Gate over the soul bridge fields (`trust.player`, `mood.*`, …).
    /// Reuses the contract condition language — never a second DSL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<Condition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<ChoiceEffect>,
    /// Next node id; `None` ends the conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// One authored node: what the NPC says, and where it can go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueNode {
    pub id: String,
    /// The NPC's line for this beat.
    pub line: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<DialogueChoice>,
    /// The unscripted edge: this node also offers "say something else",
    /// routing free input through `assemble` → inference.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub llm_edge: bool,
}

/// An authored conversation, attached to a soul file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueGraph {
    /// Node the conversation opens on.
    pub start: String,
    pub nodes: Vec<DialogueNode>,
}

impl DialogueGraph {
    pub fn node(&self, id: &str) -> Option<&DialogueNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Structural validation: the start node exists, every `next` resolves,
    /// no duplicate ids. Returns human-readable problems.
    pub fn validate(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.node(&self.start).is_none() {
            problems.push(format!("start node {:?} does not exist", self.start));
        }
        let mut seen = std::collections::BTreeSet::new();
        for n in &self.nodes {
            if !seen.insert(&n.id) {
                problems.push(format!("duplicate node id {:?}", n.id));
            }
            for (i, c) in n.choices.iter().enumerate() {
                if let Some(next) = &c.next {
                    if self.node(next).is_none() {
                        problems.push(format!(
                            "node {:?} choice #{i} -> unknown node {next:?}",
                            n.id
                        ));
                    }
                }
            }
        }
        problems
    }

    /// The choices visible right now: condition-gated against the soul's
    /// bridge fields.
    pub fn visible_choices<'g>(
        &'g self,
        node: &'g DialogueNode,
        state: &SoulState,
    ) -> Vec<(usize, &'g DialogueChoice)> {
        let mut ctx = EvalContext::default();
        inject_soul_fields(&mut ctx, state);
        node.choices
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.condition
                    .as_ref()
                    .map(|cond| condition_holds(cond, &ctx))
                    .unwrap_or(true)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::types::Comparison;
    use crate::soul::types::*;

    fn boris_like() -> (SoulFile, SoulState) {
        let file = SoulFile {
            id: "boris".into(),
            name: "Boris".into(),
            species: Species::Robot,
            portrait_id: String::new(),
            identity: Identity {
                origin: "EVA".into(),
                faction_affiliation: "crew".into(),
                role: "EVA robot".into(),
                public_bio: "First out, last back.".into(),
            },
            personality: Personality {
                traits: vec!["Consistent".into()],
                values: vec!["CrewSafety".into()],
                speaking_style: SpeakingStyle::Terse,
                quirks: vec!["Full names, always".into()],
            },
            emotional_state: EmotionalState {
                dominant_mood: Mood::Stable,
                intensity: 300,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![Relationship {
                target_id: "player".into(),
                trust: 512,
                familiarity: 512,
                history: vec![],
            }],
            goals: vec![Goal {
                id: "sweep".into(),
                priority: GoalPriority::Constant,
                description: "Complete the sweep.".into(),
            }],
            breaking_points: vec![],
            contracts: vec![],
            backstory: "Nobody sees this.".into(),
            secrets: vec![Secret {
                id: "the_mark".into(),
                reveal_condition: Condition::Compare {
                    field: "trust.player".into(),
                    op: Comparison::Gt,
                    value: 1000,
                },
                content: "THE-SECRET-CONTENT-SENTINEL".into(),
            }],
            dialogue: None,
            deflections: vec!["The sweep is incomplete. Excuse me.".into()],
        };
        let state = crate::soul::SoulState::from_file(&file);
        (file, state)
    }

    #[test]
    fn assemble_is_bounded_whatever_the_history() {
        let (file, mut state) = boris_like();
        // Flood memories and history far past any budget.
        for t in 0..500u64 {
            state.memories.push(Memory {
                id: format!("m{t}"),
                event_type: "chatter".into(),
                player_involved: true,
                emotional_weight: (t % 1024) as i64,
                timestamp: t,
                summary: format!("a fairly long memory summary line number {t} with padding"),
            });
        }
        let history: Vec<DialogueTurn> = (0..200)
            .map(|i| DialogueTurn {
                speaker: "player".into(),
                line: format!("a long conversational line number {i} {}", "x".repeat(80)),
            })
            .collect();
        let ctx = assemble(&file, &state, &history, &"y".repeat(2000));
        assert!(
            context_bytes(&ctx) <= CONTEXT_BUDGET_BYTES,
            "context stayed inside the budget"
        );
        assert!(ctx.memories.len() <= TOP_K_MEMORIES);
        assert!(ctx.history.len() <= LAST_N_TURNS);
        assert!(ctx.input.chars().count() <= MAX_UTTERANCE_CHARS);
        // Determinism: same inputs, same bytes.
        let again = assemble(&file, &state, &history, &"y".repeat(2000));
        assert_eq!(ctx, again);
    }

    #[test]
    fn top_k_memories_are_the_heaviest() {
        let (file, mut state) = boris_like();
        for (w, id) in [(1000, "big"), (10, "small"), (900, "alsobig")] {
            state.memories.push(Memory {
                id: id.into(),
                event_type: "e".into(),
                player_involved: true,
                emotional_weight: w,
                timestamp: 1,
                summary: id.into(),
            });
        }
        let ctx = assemble(&file, &state, &[], "hey");
        assert!(ctx.memories.contains(&"big".to_string()));
        assert!(ctx.memories.contains(&"alsobig".to_string()));
    }

    /// The gotcha that matters: unrevealed secret content never enters the
    /// context — nor does the backstory (author-facing, per spec §15).
    #[test]
    fn secrets_and_backstory_never_enter_the_context() {
        let (file, state) = boris_like();
        let ctx = assemble(&file, &state, &[], "what's that mark on your arm?");
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(
            !json.contains("THE-SECRET-CONTENT-SENTINEL"),
            "unrevealed secret leaked into the context"
        );
        assert!(!json.contains("Nobody sees this."), "backstory leaked");
        // And the voice prompt is equally clean.
        let prompt = voice_prompt(&file, &state);
        assert!(!prompt.contains("THE-SECRET-CONTENT-SENTINEL"));
        assert!(!prompt.contains("Nobody sees this."));
    }

    #[test]
    fn voice_prompt_renders_style_quirks_and_mood() {
        let (file, mut state) = boris_like();
        state.mood = Mood::Defensive;
        state.intensity = 717;
        let prompt = voice_prompt(&file, &state);
        assert!(prompt.contains("terse"), "style word present");
        assert!(
            prompt.contains("one or two short sentences"),
            "terse directive present"
        );
        assert!(prompt.contains("Full names, always"), "quirks present");
        assert!(prompt.contains("defensive"), "CURRENT mood present");
        assert!(prompt.contains("70%"), "intensity present");
    }

    #[test]
    fn shape_line_strips_meta_and_caps() {
        assert_eq!(
            shape_line("Boris: \"*adjusts servos* Understood.\"", "Boris"),
            "Understood."
        );
        let long = "word ".repeat(200);
        let shaped = shape_line(&long, "Boris");
        assert!(shaped.chars().count() <= MAX_LINE_CHARS + 1); // +ellipsis
                                                               // Sentence-boundary cap when one exists.
        let sentences = format!("{} Second sentence never fits.", "a".repeat(200));
        let shaped = shape_line(&format!("{sentences}{}", "b".repeat(200)), "Boris");
        assert!(shaped.chars().count() <= MAX_LINE_CHARS + 1);
    }

    #[test]
    fn graph_validates_and_gates_choices() {
        let graph = DialogueGraph {
            start: "open".into(),
            nodes: vec![
                DialogueNode {
                    id: "open".into(),
                    line: "Captain Thibodeaux.".into(),
                    choices: vec![
                        DialogueChoice {
                            label: "Ask about the mark".into(),
                            condition: None,
                            effects: vec![ChoiceEffect::SoulEvent {
                                event_type: "asked_about_mark".into(),
                                emotional_weight: 400,
                                summary: "The captain asked about the mark.".into(),
                            }],
                            next: Some("deflect".into()),
                        },
                        DialogueChoice {
                            label: "Only the trusted see this".into(),
                            condition: Some(Condition::Compare {
                                field: "trust.player".into(),
                                op: Comparison::Gt,
                                value: 900,
                            }),
                            effects: vec![],
                            next: None,
                        },
                    ],
                    llm_edge: true,
                },
                DialogueNode {
                    id: "deflect".into(),
                    line: "The mark predates the current operational period.".into(),
                    choices: vec![],
                    llm_edge: true,
                },
            ],
        };
        assert!(graph.validate().is_empty());
        let (_, state) = boris_like(); // trust 512
        let open = graph.node("open").unwrap();
        let visible = graph.visible_choices(open, &state);
        assert_eq!(visible.len(), 1, "the trust-gated choice stays hidden");
        assert_eq!(visible[0].1.label, "Ask about the mark");

        // Broken graphs name their problems.
        let broken = DialogueGraph {
            start: "nope".into(),
            nodes: vec![DialogueNode {
                id: "a".into(),
                line: String::new(),
                choices: vec![DialogueChoice {
                    label: "x".into(),
                    condition: None,
                    effects: vec![],
                    next: Some("missing".into()),
                }],
                llm_edge: false,
            }],
        };
        let problems = broken.validate();
        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn graph_round_trips_ron() {
        let graph = DialogueGraph {
            start: "open".into(),
            nodes: vec![DialogueNode {
                id: "open".into(),
                line: "Hm.".into(),
                choices: vec![],
                llm_edge: true,
            }],
        };
        let text = ron::to_string(&graph).unwrap();
        let back: DialogueGraph = ron::from_str(&text).unwrap();
        assert_eq!(graph, back);
    }
}
