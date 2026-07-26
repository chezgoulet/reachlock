//! Soul-backed dialogue (S16): one surface (S07's dialogue panel) serving
//! authored graphs, condition-gated choices, and the unscripted edge —
//! free input assembled into a bounded context and routed through the LLM
//! proxy, with the deliberation state IN the panel and clean supersession
//! when the player walks away mid-think.
//!
//! Latency rule (v1 M10, ported as principle): the authored beat renders
//! instantly; the generated line arrives as a follow-up beat; moving on
//! supersedes the in-flight call.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use reachlock_core::dialogue::{
    assemble, deflection_line, shape_line, voice_prompt, ChoiceEffect, DialogueTurn,
    MAX_UTTERANCE_CHARS,
};
use reachlock_core::soul::types::Mood;
use reachlock_core::soul::SoulEvent;

use crate::focus_stack::FocusStack;
use crate::net::{NetMode, NetOutbox};
use crate::settings::{InputAction, Settings};
use crate::systems::contract::ShipLog;
use crate::systems::crew::CrewFigure;
use crate::systems::interaction::{ActivePanel, Npc};
use crate::systems::soul::SoulRegistry;
use crate::systems::ticker::UniverseTicker;

/// The live conversation, if any. One at a time — one surface.
#[derive(Resource, Default)]
pub struct DialogueSession {
    pub active: Option<ActiveDialogue>,
    /// Call-id generator for dialogue LLM calls (`dlg-<n>`).
    counter: u64,
}

pub struct ActiveDialogue {
    /// The panel entity this session belongs to (supersession key).
    pub entity: Entity,
    pub soul_id: String,
    /// Current graph node; `None` after an edge/ending beat.
    pub node_id: Option<String>,
    pub history: Vec<DialogueTurn>,
    /// The NPC's current beat (authored, deflected, or generated).
    pub npc_line: String,
    /// `Some(buffer)` while the player is typing a free-input utterance.
    pub typing: Option<String>,
    /// In-flight LLM dialogue call, if any.
    pub call_id: Option<String>,
    pub thinking: bool,
}

impl DialogueSession {
    /// True while the player is typing — movement and interaction keys are
    /// suppressed so WASD spells words instead of walking.
    pub fn typing(&self) -> bool {
        self.active.as_ref().is_some_and(|a| a.typing.is_some())
    }
}

/// Resolve a panel entity to a soul id: crew figures carry their id; station
/// NPCs match by lowercased name (authored NPCs and souls share names).
fn soul_for(
    entity: Entity,
    crew: &Query<&CrewFigure>,
    npcs: &Query<&Npc>,
    souls: &SoulRegistry,
) -> Option<String> {
    if let Ok(fig) = crew.get(entity) {
        if souls.files.contains_key(&fig.0) {
            return Some(fig.0.clone());
        }
    }
    if let Ok(npc) = npcs.get(entity) {
        let id = npc.name.to_lowercase();
        if souls.files.contains_key(&id) {
            return Some(id);
        }
    }
    None
}

/// Keep the session in step with the open panel: opening `Dialogue(e)` on a
/// soul-backed figure starts (or reuses) a session at the graph's start
/// node; anything else closes it — and closing with a call in flight is the
/// supersession beat ("…loses the thread").
pub fn sync_dialogue_session(
    panel: Res<ActivePanel>,
    crew: Query<&CrewFigure>,
    npcs: Query<&Npc>,
    souls: Res<SoulRegistry>,
    mut session: ResMut<DialogueSession>,
    mut log: ResMut<ShipLog>,
) {
    let target = match &*panel {
        ActivePanel::Dialogue(e) => soul_for(*e, &crew, &npcs, &souls).map(|id| (*e, id)),
        _ => None,
    };
    match (&mut session.active, target) {
        (Some(active), Some((entity, _))) if active.entity == entity => {}
        (active @ Some(_), target) => {
            // Panel moved on: supersede.
            let old = active.take().expect("matched Some");
            if old.call_id.is_some() {
                let name = souls
                    .files
                    .get(&old.soul_id)
                    .map(|f| f.name.clone())
                    .unwrap_or(old.soul_id.clone());
                log.log(format!("{name} loses the thread."));
            }
            if let Some((entity, soul_id)) = target {
                start_session(&mut session, entity, soul_id, &souls);
            }
        }
        (None, Some((entity, soul_id))) => start_session(&mut session, entity, soul_id, &souls),
        (None, None) => {}
    }
}

fn start_session(
    session: &mut DialogueSession,
    entity: Entity,
    soul_id: String,
    souls: &SoulRegistry,
) {
    let file = &souls.files[&soul_id];
    let (node_id, npc_line) = match &file.dialogue {
        Some(graph) => match graph.node(&graph.start) {
            Some(node) => (Some(node.id.clone()), node.line.clone()),
            None => (None, file.identity.public_bio.clone()),
        },
        None => (None, file.identity.public_bio.clone()),
    };
    session.active = Some(ActiveDialogue {
        entity,
        soul_id,
        node_id,
        history: Vec::new(),
        npc_line,
        typing: None,
        call_id: None,
        thinking: false,
    });
}

/// Drive the open conversation: number keys pick visible choices, `9`
/// opens the free-input edge (where the node allows it), typing mode
/// captures characters until Enter submits or Esc cancels.
#[allow(clippy::too_many_arguments)]
pub fn dialogue_input(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Res<Settings>,
    mut chars: bevy::ecs::message::MessageReader<KeyboardInput>,
    mut session: ResMut<DialogueSession>,
    mut souls: ResMut<SoulRegistry>,
    mut panel: ResMut<ActivePanel>,
    mut outbox: ResMut<NetOutbox>,
    mode: Res<NetMode>,
    ticker: Res<UniverseTicker>,
    mut log: ResMut<ShipLog>,
    focus_stack: Res<FocusStack>,
) {
    // One deref so the field borrows split on the plain struct.
    let session = &mut *session;
    let counter = &mut session.counter;
    let Some(active) = &mut session.active else {
        chars.clear();
        return;
    };

    // While typing, dialogue owns input; otherwise bail if a modal has focus.
    if active.typing.is_none() && focus_stack.top_captures_input() {
        chars.clear();
        return;
    }

    // ── typing mode ──
    if let Some(buffer) = &mut active.typing {
        for input in chars.read() {
            if input.state != ButtonState::Pressed {
                continue;
            }
            match &input.logical_key {
                Key::Character(text) => {
                    if buffer.chars().count() < MAX_UTTERANCE_CHARS {
                        buffer.push_str(text);
                    }
                }
                Key::Space => {
                    if buffer.chars().count() < MAX_UTTERANCE_CHARS {
                        buffer.push(' ');
                    }
                }
                Key::Backspace => {
                    buffer.pop();
                }
                _ => {}
            }
        }
        if keys.just_pressed(settings.key(InputAction::Pause)) {
            active.typing = None;
            return;
        }
        if keys.just_pressed(settings.key(InputAction::EditorConfirm)) {
            let utterance = active.typing.take().unwrap_or_default();
            if !utterance.trim().is_empty() {
                submit_utterance(
                    counter,
                    active,
                    &mut souls,
                    &mut outbox,
                    &mode,
                    &ticker,
                    &mut log,
                    utterance,
                );
            }
        }
        return;
    }
    chars.clear();

    if active.thinking {
        return; // the NPC is considering; Esc/walk-away still supersedes
    }

    // ── choice mode ──
    let Some(file) = souls.files.get(&active.soul_id) else {
        return;
    };
    let node = active
        .node_id
        .as_ref()
        .and_then(|id| file.dialogue.as_ref().and_then(|g| g.node(id)))
        .cloned();
    let Some(node) = node else {
        // Edge-of-graph beat: only the free-input edge remains.
        if keys.just_pressed(settings.key(InputAction::QuickSave)) {
            active.typing = Some(String::new());
        }
        return;
    };
    let state = souls
        .states
        .get(&active.soul_id)
        .cloned()
        .unwrap_or_else(|| reachlock_core::soul::SoulState::from_file(file));
    let graph = file.dialogue.as_ref().expect("node implies graph");
    let visible = graph.visible_choices(&node, &state);

    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
    ];
    for (slot, (_, choice)) in visible.iter().enumerate().take(digits.len()) {
        if !keys.just_pressed(digits[slot]) {
            continue;
        }
        let choice = (*choice).clone();
        // Apply the choice's effects through the S13 pipeline.
        let mut deltas: Vec<(String, i64, i64)> = Vec::new();
        for effect in &choice.effects {
            match effect {
                ChoiceEffect::SoulEvent {
                    event_type,
                    emotional_weight,
                    summary,
                } => {
                    let event = SoulEvent {
                        event_type: event_type.clone(),
                        player_involved: true,
                        emotional_weight: *emotional_weight,
                        timestamp: ticker.state.tick_no,
                        summary: summary.clone(),
                        fields: Default::default(),
                        relationship_deltas: std::mem::take(&mut deltas),
                    };
                    for output in souls.apply(&active.soul_id, &event) {
                        crate::systems::soul::log_soul_output(&mut log, &output);
                    }
                }
                ChoiceEffect::RelationshipDelta { trust, familiarity } => {
                    deltas.push(("player".into(), *trust, *familiarity));
                }
            }
        }
        if !deltas.is_empty() {
            // Pure relationship moves with no wrapping event.
            let event = SoulEvent {
                event_type: "conversation".into(),
                player_involved: true,
                emotional_weight: 128,
                timestamp: ticker.state.tick_no,
                summary: format!("Chose: {}", choice.label),
                fields: Default::default(),
                relationship_deltas: deltas,
            };
            for output in souls.apply(&active.soul_id, &event) {
                crate::systems::soul::log_soul_output(&mut log, &output);
            }
        }
        active.history.push(DialogueTurn {
            speaker: "player".into(),
            line: choice.label.clone(),
        });
        match &choice.next {
            Some(next_id) => {
                if let Some(next) = souls.files[&active.soul_id]
                    .dialogue
                    .as_ref()
                    .and_then(|g| g.node(next_id))
                {
                    active.history.push(DialogueTurn {
                        speaker: active.soul_id.clone(),
                        line: next.line.clone(),
                    });
                    active.node_id = Some(next.id.clone());
                    active.npc_line = next.line.clone();
                }
            }
            None => {
                *panel = ActivePanel::None; // conversation ends
            }
        }
        return;
    }

    if node.llm_edge && keys.just_pressed(settings.key(InputAction::QuickSave)) {
        active.typing = Some(String::new());
    }
}

/// The unscripted edge: assemble the bounded context (secret-safe, by
/// construction) and either route it through the proxy (online) or answer
/// with the soul's authored deflection (offline / Classic) — never a hang.
#[allow(clippy::too_many_arguments)]
fn submit_utterance(
    counter: &mut u64,
    active: &mut ActiveDialogue,
    souls: &mut SoulRegistry,
    outbox: &mut NetOutbox,
    mode: &NetMode,
    ticker: &UniverseTicker,
    log: &mut ShipLog,
    utterance: String,
) {
    let Some(file) = souls.files.get(&active.soul_id) else {
        return;
    };
    let state = souls
        .states
        .get(&active.soul_id)
        .cloned()
        .unwrap_or_else(|| reachlock_core::soul::SoulState::from_file(file));

    active.history.push(DialogueTurn {
        speaker: "player".into(),
        line: utterance.clone(),
    });

    if matches!(mode, NetMode::Online { .. }) {
        let context = assemble(file, &state, &active.history, &utterance);
        *counter += 1;
        let call_id = format!("dlg-{counter}");
        outbox.push(reachlock_core::network::ClientMessage::LlmCall {
            call_id: call_id.clone(),
            contract_id: format!("dialogue:{}", active.soul_id),
            context: serde_json::json!({ "dialogue": context }),
            // S16B wire revision: the soul's voice is the TRUE system
            // prompt now, not a payload hint the wrapper buries.
            system_prompt: Some(voice_prompt(file, &state)),
            timeout_ms: None,
            max_tokens: None,
        });
        active.call_id = Some(call_id);
        active.thinking = true;
    } else {
        // Offline / Classic: the authored deflection, in their own voice.
        let line = deflection_line(file, ticker.state.tick_no)
            .unwrap_or("…")
            .to_string();
        let name = file.name.clone();
        active.history.push(DialogueTurn {
            speaker: active.soul_id.clone(),
            line: line.clone(),
        });
        active.npc_line = line;
        log.log(format!("{name} deflects (no inference offline)."));
        record_exchange(souls, &active.soul_id, ticker.state.tick_no, &utterance);
    }
}

/// An unscripted exchange writes a memory and warms familiarity a notch —
/// the S16 "the exchange writes memories and moves the relationship" beat.
pub fn record_exchange(souls: &mut SoulRegistry, soul_id: &str, tick: u64, utterance: &str) {
    let event = SoulEvent {
        event_type: "conversation".into(),
        player_involved: true,
        emotional_weight: 192,
        timestamp: tick,
        summary: format!("The captain said: {utterance}"),
        fields: Default::default(),
        relationship_deltas: vec![("player".into(), 0, 8)],
    };
    souls.apply(soul_id, &event);
}

/// A dialogue LLM reply landed (called from `network::poll_network`).
/// Shapes the line in the soul's voice pipeline and applies the exchange.
pub fn resolve_dialogue_response(
    session: &mut DialogueSession,
    souls: &mut SoulRegistry,
    ticker: &UniverseTicker,
    call_id: &str,
    raw_line: &str,
) -> bool {
    let Some(active) = &mut session.active else {
        return false;
    };
    if active.call_id.as_deref() != Some(call_id) {
        return false; // superseded or foreign call — ignore quietly
    }
    let name = souls
        .files
        .get(&active.soul_id)
        .map(|f| f.name.clone())
        .unwrap_or_default();
    let line = shape_line(raw_line, &name);
    active.npc_line = line.clone();
    active.history.push(DialogueTurn {
        speaker: active.soul_id.clone(),
        line,
    });
    active.call_id = None;
    active.thinking = false;
    let last_player_line = active
        .history
        .iter()
        .rev()
        .find(|t| t.speaker == "player")
        .map(|t| t.line.clone())
        .unwrap_or_default();
    let soul_id = active.soul_id.clone();
    record_exchange(souls, &soul_id, ticker.state.tick_no, &last_player_line);
    true
}

/// A dialogue LLM call failed: authored deflection instead — never a hang.
pub fn resolve_dialogue_failure(
    session: &mut DialogueSession,
    souls: &SoulRegistry,
    ticker: &UniverseTicker,
    call_id: &str,
    log: &mut ShipLog,
    reason: &str,
) -> bool {
    let Some(active) = &mut session.active else {
        return false;
    };
    if active.call_id.as_deref() != Some(call_id) {
        return false;
    }
    let file = souls.files.get(&active.soul_id);
    let line = file
        .and_then(|f| deflection_line(f, ticker.state.tick_no))
        .unwrap_or("…")
        .to_string();
    active.npc_line = line.clone();
    active.history.push(DialogueTurn {
        speaker: active.soul_id.clone(),
        line,
    });
    active.call_id = None;
    active.thinking = false;
    log.log(format!(
        "{}: the thought doesn't land ({reason}) — they deflect.",
        file.map(|f| f.name.as_str()).unwrap_or("crew")
    ));
    true
}

/// UI marker for the dialogue portrait sprite (T10b).
#[derive(Component)]
pub struct DialoguePortrait;

/// Paint and return a portrait [`Image`] for the given speaker's look,
/// suitable for display in the dialogue panel.
pub fn paint_dialogue_portrait(look: &crate::pixel::Look) -> Image {
    crate::pixel::paint_portrait(look).into_image()
}

fn mood_glyph(mood: &Mood) -> (&'static str, &'static str) {
    match mood {
        Mood::Stable => ("◆", "Stable"),
        Mood::Happy => ("★", "Happy"),
        Mood::Tense => ("⚡", "Tense"),
        Mood::Grieving => ("▼", "Grieving"),
        Mood::Suspicious => ("◈", "Suspicious"),
        Mood::Grateful => ("★", "Grateful"),
        Mood::Anxious => ("⬡", "Anxious"),
        Mood::Protective => ("▲", "Protective"),
        Mood::Defensive => ("◈", "Defensive"),
        Mood::Focused => ("■", "Focused"),
        Mood::Withdrawn => ("▼", "Withdrawn"),
    }
}

pub fn panel_text(session: &DialogueSession, souls: &SoulRegistry) -> Option<String> {
    let active = session.active.as_ref()?;
    let file = souls.files.get(&active.soul_id)?;
    // If state is None (not yet initialized), return early — the panel
    // renders empty until init_souls finishes. This is correct: state
    // is initialized at startup before any interaction can open.
    let state = souls.states.get(&active.soul_id)?;
    let (mood_glyph_char, mood_name) = mood_glyph(&state.mood);
    let mut lines = vec![
        format!("{} — {}", file.name, file.identity.role),
        format!("    {mood_glyph_char} {mood_name}"),
        String::new(),
    ];
    if !active.history.is_empty() {
        lines.push("    ── Conversation ──".into());
        for turn in active.history.iter().rev().take(5) {
            if turn.speaker == "player" {
                lines.push(format!("    Player: {}", turn.line));
            } else {
                lines.push(format!("    {}: \"{}\"", file.name, turn.line));
            }
        }
        lines.push("    ──────────────────".into());
        lines.push(String::new());
    }
    lines.push(format!("“{}”", active.npc_line));
    lines.push(String::new());
    if active.thinking {
        lines.push(format!(
            "{} is considering… ({})",
            file.name,
            state.mood.as_str()
        ));
        lines.push("(walk away to let it go)".into());
        return Some(lines.join("\n"));
    }
    if let Some(buffer) = &active.typing {
        lines.push(format!("say: {buffer}▌"));
        lines.push("(Enter send · Esc cancel)".into());
        return Some(lines.join("\n"));
    }
    let node = active
        .node_id
        .as_ref()
        .and_then(|id| file.dialogue.as_ref().and_then(|g| g.node(id)));
    if let Some(node) = node {
        let graph = file.dialogue.as_ref().expect("node implies graph");
        for (slot, (_, choice)) in graph
            .visible_choices(node, state)
            .iter()
            .enumerate()
            .take(5)
        {
            lines.push(format!("{}. {}", slot + 1, choice.label));
        }
        if node.llm_edge {
            lines.push("9. say something else…".into());
        }
    } else {
        lines.push("9. say something else…".into());
    }
    Some(lines.join("\n"))
}

/// Get the [`Look`] for the current dialogue speaker, if available.
pub fn speaker_look(session: &DialogueSession, souls: &SoulRegistry) -> Option<crate::pixel::Look> {
    let active = session.active.as_ref()?;
    let file = souls.files.get(&active.soul_id)?;
    file.look.clone().map(crate::pixel::Look::from).or_else(|| {
        Some(crate::pixel::Look::seeded(
            active
                .soul_id
                .bytes()
                .fold(42u64, |a, b| a.wrapping_mul(31) + b as u64),
        ))
    })
}

/// Update the dialogue portrait sprite whenever the session or panel changes.
pub fn update_dialogue_portrait(
    panel: Res<crate::systems::interaction::ActivePanel>,
    session: Res<DialogueSession>,
    souls: Res<SoulRegistry>,
    mut images: ResMut<Assets<Image>>,
    mut portrait: Query<&mut ImageNode, With<DialoguePortrait>>,
) {
    let Ok(mut node) = portrait.single_mut() else {
        return;
    };
    let needs_portrait = matches!(
        &*panel,
        crate::systems::interaction::ActivePanel::Dialogue(_)
    ) && session.active.is_some();
    if !needs_portrait {
        node.image = Handle::default();
        return;
    }
    let look = match speaker_look(&session, &souls) {
        Some(l) => l,
        None => {
            node.image = Handle::default();
            return;
        }
    };
    let image = paint_dialogue_portrait(&look);
    let previous = std::mem::replace(&mut node.image, images.add(image));
    if previous != Handle::default() {
        images.remove(&previous);
    }
}
