# S41 — Trope Engine: Scripted Encounters

**Spec:** New (authored narrative encounters) · **Wave 9 (Living Galaxy) ·
Depends on:** S40 (trope templates)

## Outcome

Content authors write complete, structured encounters — dialogue trees, custom
conditions, scripted consequences, unique rewards. These are the set pieces
that template tropes can't capture: a multi-branch negotiation with a
Predecessor AI, a rescue mission with timed phases, a faction betrayal that
unfolds over three interactions. Authored encounters use the same trope data
structures but replace procedural fill with hand-written specifics. They can
reference generated content — "use the ecosystem from {system_id}" — to
stay grounded in the procedural galaxy. The template engine fills space;
scripted encounters create landmarks.

## Context

- S40's template engine produces variety through procedural fill. This sprint
  produces INTENT through authored design. A template trope is "a derelict
  ship with a secret." A scripted encounter is "The Ghost of Kessel Station
  — a three-scene encounter where a long-dead station AI tries to convince
  the crew to complete its final mission, and the player's choices determine
  whether the AI finds peace, becomes a recurring crew presence, or turns
  hostile."
- Scripted encounters can use everything the procedural systems produce:
  species from ecosystems (S39), items from the item generator (S05),
  faction state from the faction engine (S11), crew relationship memory (S35),
  career progression (S42). They're authored content that WIRES INTO the
  procedural systems.
- The content editor (S25) is the authoring tool. Scripted encounters are
  `.ron` files validated by the content pipeline. Authors write them;
  players encounter them.

## Freeze first

### Scripted encounter type (`content/scripted_encounter.rs` or `generator/trope_scripted.rs`)

```rust
pub struct ScriptedEncounter {
    pub id: String,
    pub title: String,
    pub encounter_type: ScriptedEncounterType,
    pub trigger: EncounterTrigger,
    pub prerequisites: Vec<EncounterPrerequisite>,
    pub scenes: Vec<EncounterScene>,
    pub on_complete: Vec<EncounterOutcome>,
    pub repeatable: bool,
    pub cooldown_ticks: Option<u64>,
}

pub enum ScriptedEncounterType {
    StoryBeat,          // advances a faction or personal storyline
    UniqueLocation,     // tied to a specific system/station
    FactionEvent,       // triggers when faction state meets criteria
    PlayerMilestone,    // triggers when player reaches career rank
    CommunityGoal,      // triggers for all players in a universe
}

pub enum EncounterTrigger {
    OnSystemEntry { system_id: String },
    OnStationDock { station_id: String },
    OnFactionReputation { faction: String, threshold: i64, direction: Direction },
    OnCareerRank { path_type: PathType, rank: u8 },
    OnItemAcquired { item_type: ItemType },
    OnCrewMilestone { crew_id: String, milestone_type: String },
    OnTropeResolved { template_id: String },          // fires after a template trope
    OnDilemmaResolved { dilemma_type: DilemmaType },
    OnTimerElapsed { ticks: u64 },
    Manual,             // fired by an admin or content publish event
}

pub struct EncounterPrerequisite {
    pub condition_type: PrerequisiteType,
    pub params: HashMap<String, String>,
}

pub enum PrerequisiteType {
    FactionReputationRange,
    CareerRankMinimum,
    ShipHasUpgrade,
    CrewHasRole,
    ItemInInventory,
    SystemDiscovered,
    EcosystemScanned,
    StoryArcActive,
    PlayerLevelMinimum,
    UniverseTier,
}

pub struct EncounterScene {
    pub scene_id: String,
    pub narrative: String,            // can reference generated content: {ecosystem:system_id}
    pub speaker: Option<String>,      // crew member, NPC name, "narrator", "station_ai"
    pub choices: Vec<EncounterChoice>,
    pub time_pressure: Option<u64>,   // ticks before auto-resolution
}

pub struct EncounterChoice {
    pub label: String,
    pub condition: Option<String>,    // when this choice is available
    pub outcome_scene: String,        // scene_id to transition to
    pub immediate_consequences: Vec<EncounterConsequence>,
    pub narrative_response: String,   // what happens when the player chooses this
}

pub struct EncounterConsequence {
    pub consequence_type: ConsequenceType,
    pub target: String,
    pub params: HashMap<String, serde_json::Value>,
}

pub enum ConsequenceType {
    GiveItem,
    RemoveItem,
    ModifyReputation,
    ModifyCredits,
    ModifyCrewTrust,
    StartCombat,
    TriggerDilemma,
    TriggerTrope,
    EcosystemEvent,
    UnlockMission,
    CompleteMission,
    UnlockStation,
    ModifyCareerProgress,
    ModifyShipUpgrade,
    AddCrewMember,
    RemoveCrewMember,
    SetStoryFlag,
    BroadcastUniverseEvent,
    Custom { function: String },  // calls a named handler function
}

pub struct EncounterOutcome {
    pub condition: String,          // which ending was reached
    pub summary: String,            // ship log entry
    pub permanent_effects: Vec<EncounterConsequence>,
    pub unlocks: Vec<String>,       // encounter IDs unlocked by this outcome
}
```

## Deliverables

### 1. Scripted encounter engine (`core/src/content/scripted_encounter.rs`)

- [ ] `evaluate_scripted_encounter(encounter, game_state) -> EncounterEvaluation` —
      resolves prerequisites, generates rendered narrative text (fills
      `{ecosystem:system_id}` references with actual content from the game
      state), and produces a ready-to-present encounter.
- [ ] Scene transition: `advance_scene(encounter, current_scene, player_choice) -> next_scene`
      — pure state transition. Each choice maps to the next scene. Auto-resolution
      picks the first choice with no conditions (or a default fallback).
- [ ] Consequence application: `apply_consequences(consequences, game_state) -> GameState`
      — pure function. Applies all consequences to a copy of the game state.
      Returns the modified state. The caller (client or server) commits the
      changes.
- [ ] `{reference}` resolution: narrative text can reference generated
      content by ID. `{ecosystem:aethon_prime}` → renders as a summary of
      Aethon Prime's ecosystem. `{ship_class:seed_0x4A7B}` → renders as the
      ship class name. `{crew:boris}` → renders as Boris's full name and role.
      The resolver is a registry of reference handlers, extensible per
      content type.
- [ ] Determinism: same encounter + same game state = same rendered text
      and consequence outcomes. The reference resolver is deterministic.

### 2. Encounter catalog (`content/encounters/`)

- [ ] 5-8 authored encounters as `.ron` files. At minimum:
      - `ghost_of_kessel.ron`: The AI encounter described above. 3 scenes,
        4 endings. Integrates with the ecosystem system (the station AI
        was monitoring Kessel's ecosystem for 200 years — it knows things
        the player can use for discovery).
      - `alexanders_gambit_opening.ron`: First meeting with Alexander if the
        player fits his criteria. Multi-branch. Choices affect the Compact
        storyline arc.
      - `first_predecessor.ron`: The player's first Predecessor ruin (if
        not the tutorial one). Sets the tone. References the dilemma
        generator for the ruin's central puzzle.
      - `reach_ambush.ron`: A Reach pirate ambush that becomes a negotiation.
        The pirate leader has a soul file. Can end in combat, a deal, or
        a new crew recruit (the pirate joins you).
      - `colony_crisis.ron`: A colony's ecosystem is collapsing (S39 event).
        Player can help, exploit, or ignore. Three paths. Consequences
        affect the colony's existence.
- [ ] Each encounter has: trigger, prerequisites, scenes with choices,
      consequences, outcomes. Validated by content pipeline. Schema in
      `content/schemas/scripted_encounter.schema.json`.
- [ ] Encounters are discoverable: a scripted encounter's prerequisites
      can be shown as a "mystery to solve" in the mission log. "Something
      is waiting at Kessel Station. Requires: Ecosystem Scanned (Aethon)."

### 3. Encounter presentation

- [ ] Encounter UI: full-screen narrative panel (not the comms panel — this
      is a bigger production). Speaker portrait on the left, narrative text
      center, choice buttons at the bottom. Scene transitions with a brief
      fade (200ms).
- [ ] Time pressure: if the scene has `time_pressure`, a subtle timer bar
      appears at the top. When it expires, the default choice fires.
      "The station AI is waiting for your response..."
- [ ] Reference tooltips: hover over a `{reference}` in the narrative text
      to see the referenced content. Hover over "{ecosystem:aethon_prime}"
      → tooltip shows a summary of Aethon Prime's ecosystem.
- [ ] Crew involvement: if a crew member is relevant (their role, their
      background, their relationship memory), they can appear as a speaker
      in the scene. "Boris interjects: 'Captain, this AI's power signature
      matches Predecessor tech. I've seen this before.'"

### 4. Encounter state persistence

- [ ] `encounter_state` per player: which scripted encounters have been
      triggered, which are in progress (current scene), which are completed
      (which ending). Saved with the player's save data.
- [ ] In-progress encounters survive save/load and disconnect/reconnect.
      The player returns to the scene they left at.
- [ ] Encounters with `Manual` trigger can be fired by an admin (S26)
      for community events. "The Veil escalation has begun. All players
      in Aethon system receive 'alexanders_gambit_opening'."

### 5. Content editor integration (S25)

- [ ] Scripted Encounter Editor in the Content Editor Suite. Scene editor
      with text areas, choice builder, consequence configurator. Reference
      picker: browse generated content by ID and insert `{references}`.
      Preview: run the encounter from scene 1 with a sample game state.
- [ ] Validation: check that all scene transitions have valid target scenes,
      all consequences reference valid targets, all prerequisites are
      evaluable, no unreachable scenes, all endings are reachable.

### 6. Metrics collection

- [ ] `scripted_encounter_events` table: encounter ID, trigger type,
      scenes reached, player choice per scene, ending reached, time spent
      per scene, completion rate.
- [ ] Research questions: what choice distributions reveal about player
      preferences? Do time-pressured scenes produce different choice
      patterns? What encounter types have the highest completion rate?
      What prerequisites produce the most exclusive encounters?

## Acceptance gates

```
cargo test -p reachlock-core content::scripted_encounter::
# scene transitions correct, consequence application, reference resolution
reachlock content validate content/encounters/*.ron
make check
```

Manual: meet the prerequisites for an encounter → trigger it → play through
all scenes → reach an ending → see permanent consequences in game state →
replay from save → choose differently → reach a different ending → verify
ship log records both outcomes correctly.

## Non-goals

- Encounter branching that modifies the encounter structure itself (dynamic
  scene generation). Scenes are authored, not generated.
- Multiplayer encounters (multiple players in the same scripted encounter
  simultaneously). Future MMO feature.
- Voice-acted encounters. Text only.
- "Random encounter" mixing of scripted and template tropes. Scripted
  encounters have specific triggers; template tropes are random. They
  don't blend mid-encounter.
- Encounter versioning / hotfix while players are mid-encounter. If an
  encounter is updated, players in progress finish on the old version.

## Gotchas

- `{reference}` resolution must be lazy — the narrative text is rendered
  when the scene is presented, not when the encounter is loaded. This
  ensures ecosystem changes between load and presentation are reflected.
- Prerequisites can become unsatisfied between the encounter being
  triggered and the next scene. Example: player loses required item
  between scene 1 and scene 2. Handle gracefully: the choice that
  requires the item is grayed out with an explanation. "You no longer
  have the artifact."
- Custom consequence functions (`ConsequenceType::Custom { function }`)
  are a plugin point. They call named functions registered at startup.
  Document the function signature. Limit to pre-registered functions
  (no arbitrary code execution). The registry is a `HashMap<String, fn>`
  populated at init.
- Encounter IDs must be globally unique. Encounters can reference each
  other (unlocks, prerequisites). Validate at content load time that
  all referenced encounter IDs exist. Circular dependency → validation
  error.
