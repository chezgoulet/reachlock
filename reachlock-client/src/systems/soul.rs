//! Client soul integration (S13). Authored souls arrive through the content
//! pipeline (`ContentPayload::Soul`); live state is a plain map keyed by
//! soul id that rides in the save. Crew roster members link to souls by
//! their stable ids (`tove`, `prudence`, `risc`, `keene`, `bardo`,
//! `boris`; the player is `tib`).
//!
//! S13 delivers events, not consequences: mood shifts and soul breaks land
//! in the ship's log; S15/S16 give them teeth and words.

use std::collections::BTreeMap;

use bevy::prelude::*;

use reachlock_core::soul::{apply_event, SoulEvent, SoulFile, SoulOutput, SoulState};

use crate::systems::content_index::ContentIndex;
use crate::systems::contract::ShipLog;
use crate::systems::ship::ShipSystems;
use crate::systems::ticker::UniverseTicker;

/// Every authored soul plus its live state. Files are immutable content;
/// states persist in the save (`inventory::SaveFile::souls`).
#[derive(Resource)]
pub struct SoulRegistry {
    pub files: BTreeMap<String, SoulFile>,
    pub states: BTreeMap<String, SoulState>,
    /// The authored mutation arcs (embedded content), scanned after every
    /// applied event — S16B closes the S13 gap where these never ran.
    pub mutations: Vec<reachlock_core::soul::SoulMutation>,
}

impl Default for SoulRegistry {
    fn default() -> Self {
        SoulRegistry {
            files: BTreeMap::new(),
            states: BTreeMap::new(),
            mutations: reachlock_core::soul::load_soul_mutations(),
        }
    }
}

impl SoulRegistry {
    /// Apply one event to one soul, then scan that soul's authored mutation
    /// arcs with the event's fields in context (fired-once — the narrative
    /// intersects the procedural world here, spec §15). Returns the outputs
    /// plus a log line per fired mutation. Empty when the soul isn't loaded
    /// — offline-safe, never a panic.
    pub fn apply(&mut self, soul_id: &str, event: &SoulEvent) -> Vec<SoulOutput> {
        let Some(file) = self.files.get(soul_id) else {
            return Vec::new();
        };
        let current = self
            .states
            .get(soul_id)
            .cloned()
            .unwrap_or_else(|| SoulState::from_file(file));
        let (mut next, outputs) = apply_event(file, &current, event);

        // Mutation scan: the event's fields (plus its `event.<type>` flag)
        // are the extra context the authored triggers gate on.
        let mut extra = event.fields.clone();
        extra.insert(format!("event.{}", event.event_type), 1);
        for mutation in self.mutations.iter().filter(|m| m.soul_id == soul_id) {
            let (mutated, fired) = reachlock_core::soul::apply_mutation(&next, mutation, &extra);
            if fired {
                info!("soul mutation fired: {} on {soul_id}", mutation.id);
                next = mutated;
            }
        }
        self.states.insert(soul_id.to_string(), next);
        outputs
    }
}

/// Fill the registry from the content index (Startup, chained after
/// `load_content_index` and before `inventory::load_save`, which restores
/// saved states over the fresh ones). The resource itself is an
/// `init_resource` so it exists from the first frame regardless.
pub fn init_souls(content: Res<ContentIndex>, mut registry: ResMut<SoulRegistry>) {
    for file in &content.files {
        if let reachlock_core::content::ContentPayload::Soul(soul) = &file.payload {
            registry
                .states
                .insert(soul.id.clone(), SoulState::from_file(soul));
            registry.files.insert(soul.id.clone(), (**soul).clone());
        }
    }
    if !registry.files.is_empty() {
        info!("souls: loaded {} authored soul(s)", registry.files.len());
    }
}

/// Feed ship damage into the crew's souls: when the hull takes a hit, every
/// soul aboard experiences a `ship_damage` event carrying the current damage
/// fraction (`ship.damage`, fixed-point). Mood shifts and breaks land in the
/// ship's log — the visible half of the S13 acceptance gate.
pub fn soul_ship_damage_events(
    systems: Res<ShipSystems>,
    ticker: Res<UniverseTicker>,
    roster: Res<crate::systems::crew::CrewRoster>,
    mut registry: ResMut<SoulRegistry>,
    mut log: ResMut<ShipLog>,
    mut prev_hp: Local<Option<i64>>,
) {
    let hp = systems.hull_hp.0;
    let last = prev_hp.replace(hp).unwrap_or(hp);
    if hp >= last {
        return; // repairs and no-ops don't traumatize anyone
    }
    let damage = 1024 - hp; // fraction of the hull gone, fixed-point
    let mut event = SoulEvent {
        event_type: "ship_damage".into(),
        player_involved: true,
        emotional_weight: (damage / 2).clamp(64, 1024),
        timestamp: ticker.state.tick_no,
        summary: format!("The hull took a hit ({}% damage).", damage * 100 / 1024),
        fields: BTreeMap::new(),
        relationship_deltas: Vec::new(),
    };
    event.fields.insert("ship.damage".into(), damage);

    let ids: Vec<String> = roster.members.iter().map(|m| m.id.clone()).collect();
    for id in ids {
        for output in registry.apply(&id, &event) {
            log_soul_output(&mut log, &output);
        }
    }
}

/// One line in the ship's log per soul output — the event, not the
/// consequence.
pub fn log_soul_output(log: &mut ShipLog, output: &SoulOutput) {
    match output {
        SoulOutput::MoodShift {
            soul_id,
            to,
            intensity,
            ..
        } => log.log(format!(
            "{soul_id}: mood shifts to {} ({}%)",
            to.as_str(),
            intensity * 100 / 1024
        )),
        SoulOutput::SecretUnlocked { soul_id, secret_id } => {
            log.log(format!("{soul_id} opens up about {secret_id}."))
        }
        SoulOutput::SoulBreak {
            soul_id, reaction, ..
        } => log.log(format!("{soul_id}: a line was crossed ({reaction:?}).")),
    }
}

/// Inspect text for one crew member: public bio, visible mood, standing
/// with the player. Secrets stay hidden until unlocked. Pure — the order
/// panel renders it.
pub fn inspect_text(registry: &SoulRegistry, soul_id: &str) -> Option<String> {
    let file = registry.files.get(soul_id)?;
    let state = registry.states.get(soul_id)?;
    let mut lines = vec![
        format!("{} — {}", file.name, file.identity.role),
        file.identity.public_bio.clone(),
        format!(
            "mood: {} ({}%)",
            state.mood.as_str(),
            state.intensity * 100 / 1024
        ),
    ];
    if let Some(rel) = state.relationship("player") {
        lines.push(format!(
            "trust: {}%   familiarity: {}%",
            rel.trust * 100 / 1024,
            rel.familiarity * 100 / 1024
        ));
    }
    for secret_id in &state.unlocked_secrets {
        if let Some(secret) = file.secrets.iter().find(|s| s.id == *secret_id) {
            lines.push(format!("· {}", secret.content));
        }
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::soul::types::*;

    fn registry_with(soul: SoulFile) -> SoulRegistry {
        let mut r = SoulRegistry::default();
        r.states
            .insert(soul.id.clone(), SoulState::from_file(&soul));
        r.files.insert(soul.id.clone(), soul);
        r
    }

    fn minimal_soul(id: &str) -> SoulFile {
        SoulFile {
            id: id.into(),
            name: id.to_uppercase(),
            species: Species::Robot,
            portrait_id: String::new(),
            identity: Identity {
                origin: "test".into(),
                faction_affiliation: "crew".into(),
                role: "EVA".into(),
                public_bio: "A test soul.".into(),
            },
            personality: Personality {
                traits: vec![],
                values: vec![],
                speaking_style: SpeakingStyle::Formal,
                quirks: vec![],
            },
            emotional_state: EmotionalState {
                dominant_mood: Mood::Stable,
                intensity: 256,
                triggers: vec![],
            },
            memory_tree: vec![],
            relationship_graph: vec![Relationship {
                target_id: "player".into(),
                trust: 512,
                familiarity: 256,
                history: vec![],
            }],
            goals: vec![],
            breaking_points: vec![],
            contracts: vec![],
            backstory: String::new(),
            secrets: vec![Secret {
                id: "hidden".into(),
                reveal_condition: reachlock_core::contract::types::Condition::Compare {
                    field: "trust.player".into(),
                    op: reachlock_core::contract::types::Comparison::Gt,
                    value: 1000,
                },
                content: "You should not see this yet.".into(),
            }],
            dialogue: None,
            deflections: vec![],
        }
    }

    #[test]
    fn inspect_shows_bio_mood_standing_but_no_locked_secrets() {
        let registry = registry_with(minimal_soul("boris"));
        let text = inspect_text(&registry, "boris").expect("soul loaded");
        assert!(text.contains("A test soul."));
        assert!(text.contains("mood: stable"));
        assert!(text.contains("trust: 50%"));
        assert!(
            !text.contains("You should not see this yet."),
            "locked secrets stay hidden"
        );
    }

    #[test]
    fn inspect_is_none_for_unknown_souls() {
        let registry = SoulRegistry::default();
        assert!(inspect_text(&registry, "nobody").is_none());
    }

    #[test]
    fn authored_mutation_arcs_fire_through_apply() {
        // S16B: the boris_the_mark_earned arc (loup_garou_souls.ron) fires
        // when trust is earned during a crisis — through the same apply()
        // every game event uses, fired once.
        let mut registry = registry_with(minimal_soul("boris"));
        let trust_event = |t: u64| SoulEvent {
            event_type: "player_showed_trust_during_crisis".into(),
            player_involved: true,
            emotional_weight: 800,
            timestamp: t,
            summary: "Held the line together.".into(),
            fields: BTreeMap::new(),
            relationship_deltas: vec![("player".into(), 400, 100)],
        };
        // First event raises trust past the 819 gate AND carries the flag —
        // the mutation fires in the same apply.
        registry.apply("boris", &trust_event(1));
        let state = &registry.states["boris"];
        assert!(
            state.fired_mutations.contains("boris_the_mark_earned"),
            "the authored arc fired"
        );
        assert!(state.unlocked_secrets.contains("the_mark"));
        assert!(state.traits.contains(&"Devoted".to_string()));
        // Fired-once: a second crisis doesn't re-run it.
        let before = state.clone();
        registry.apply("boris", &trust_event(2));
        assert_eq!(
            registry.states["boris"].fired_mutations,
            before.fired_mutations
        );
    }

    #[test]
    fn apply_on_missing_soul_is_a_quiet_no_op() {
        let mut registry = SoulRegistry::default();
        let event = SoulEvent {
            event_type: "x".into(),
            player_involved: false,
            emotional_weight: 0,
            timestamp: 0,
            summary: String::new(),
            fields: BTreeMap::new(),
            relationship_deltas: vec![],
        };
        assert!(registry.apply("nobody", &event).is_empty());
    }
}
