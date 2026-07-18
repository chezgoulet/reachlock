# S40 — Trope Engine: Templates

**Spec:** New (procedural narrative encounters) · **Wave 9 (Living Galaxy) ·
Depends on:** S04 (system generator), S36 (dilemma generator)

## Outcome

The galaxy is filled with science fiction encounters — derelict ships,
distress beacons, abandoned stations, Predecessor artifacts, smuggler caches —
each assembled from authored templates with procedural fill. Every player's
universe has different specifics, but the beats are recognizable. Deep space
is weirder. The Reach is wilder. The gate network is safer but never boring.
The trope engine makes exploration surprising.

## Context

- Like Stellaris leans on sci-fi tropes for encounters and flavor, ReachLock's
  procedural galaxy needs a library of recognizable situations that play out
  differently each time. The template is authored (quality control). The fill
  is procedural (infinite variety).
- The trope engine is NOT the dilemma generator (S36) — tropes are narrative
  encounters with authored structure. Dilemmas are moral/strategic choice
  points with no correct answer. A trope CAN produce a dilemma. A dilemma
  CAN be framed as a trope. They're complementary layers.
- Tropes fire on: system entry, deep space transit, asteroid belt scan,
  distress beacon detection, anomaly scan, abandoned station approach,
  long-duration silence (background timer).
- Frequency scales with location: frontier systems (S21) have high trope
  frequency. The Reach is trope-dense. Authored gate systems have lower
  frequency (but higher quality — authored encounters, S41).
- This sprint covers the TEMPLATE engine. S41 covers fully scripted
  encounters. The template engine is the procedural layer; scripted
  encounters are the authored set pieces that use the same data structures.

## Freeze first

### Trope template system (`generator/trope.rs`)

```rust
pub struct TropeTemplate {
    pub id: String,
    pub trope_type: TropeType,
    pub title_template: String,          // "Derelict {ship_class} near {planet_name}"
    pub narrative_template: String,      // multi-paragraph with {slots}
    pub slots: Vec<TropeSlot>,
    pub branches: Vec<TropeBranch>,
    pub base_frequency: f64,             // 0.0-1.0 — how often this trope fires
    pub location_types: Vec<LocationType>, // where this trope can appear
    pub min_threat_level: u8,
    pub max_threat_level: u8,
    pub dilemma_chance: f64,             // probability this trope produces a dilemma
}

pub enum TropeType {
    DerelictShip,
    DistressBeacon,
    AnomalousSignal,
    AbandonedStation,
    PredecessorArtifact,
    SmugglerCache,
    RefugeeConvoy,
    ScienceOutpost,
    PirateAmbush,
    TradeOpportunity,
    WeirdSpacePhenomenon,
    ColonyGoneWrong,
    AIShip,
}

pub struct TropeSlot {
    pub slot_name: String,               // "{ship_class}", "{secret_type}", etc.
    pub slot_kind: SlotKind,
    pub constraints: Vec<SlotConstraint>, // e.g., "must be from a hostile faction"
}

pub enum SlotKind {
    ShipClass,
    Faction,
    Item,
    Species,           // from S39 ecosystem
    CrewRole,
    PlanetName,
    StationName,
    SecretType,
    FateDescription,
    ClueItem,
    Number { min: u32, max: u32 },
    Text { options: Vec<String> },  // authored text options, one picked by RNG
}

pub struct SlotConstraint {
    pub field: String,       // constraint on the chosen value
    pub value: String,       // e.g., "faction_relation: hostile"
}

pub struct TropeBranch {
    pub label: String,
    pub condition: Option<String>,       // e.g., "player_has_item: medical_supplies"
    pub action: TropeAction,
    pub consequences: Vec<TropeConsequence>,
}

pub enum TropeAction {
    GiveItem { item_seed: ItemSeed },
    StartCombat { enemies: Vec<EnemyClass>, difficulty: u8 },
    TriggerDilemma { dilemma_type: DilemmaType },
    TriggerEcosystemEvent { event: EcosystemEventType },
    ModifyReputation { faction: String, delta: i64 },
    UnlockMission { mission_template_id: String },
    TextOnly { text: String },
}

pub struct TropeConsequence {
    pub kind: TropeConsequenceKind,
    pub target: String,
    pub magnitude: i64,
}

pub enum TropeConsequenceKind {
    FactionReputation,
    CrewTrust,
    Credits,
    CargoSpace,
    ShipDamage,
    CrewInjury,
    EcosystemImpact,
    MissionProgress,
}
```

### Trope instance (generated from template)

```rust
pub struct TropeInstance {
    pub template_id: String,
    pub seed: u64,
    pub trope_type: TropeType,
    pub title: String,                   // filled from template
    pub narrative: String,               // filled from template
    pub filled_slots: HashMap<String, String>,
    pub branches: Vec<TropeBranch>,
    pub location: LocationType,
    pub resolved: bool,
    pub player_choice: Option<String>,
}
```

## Deliverables

### 1. Trope template catalog (`content/tropes/`)

- [ ] 12-15 authored trope templates as `.ron` files. Minimum coverage:
      - DerelictShip: "A {ship_class} drifts near {planet_name}. Scans
        show {life_signs}. The ship's log indicates {fate_description}.
        A {clue_item} was left in the captain's quarters."
      - DistressBeacon: "A {faction} {ship_class} is broadcasting a
        distress signal. {crew_role} reports: '{distress_message}'."
      - AnomalousSignal: "Sensors detect {signal_type} from {location}.
        {science_crew} suggests it may be {origin_theory}."
      - AbandonedStation: "A {station_type} orbits {planet_name}.
        Abandoned for {years}. Records indicate it was a {original_purpose}.
        The last log entry: '{last_log}'."
      - PredecessorArtifact: "A Predecessor structure on {planet_name}'s
        {biome}. {crew_role} detects {energy_signature}. It appears to be
        a {artifact_function}."
      - SmugglerCache: "A hidden cargo container in {asteroid_field}.
        Marked with {faction} insignia. Contains {item_type}. The manifest
        says destination was {station_name}."
      - RefugeeConvoy: "A convoy of {ship_count} civilian ships fleeing
        {faction} space. They request {need}. They offer {offer}."
      - ScienceOutpost: "A {faction} research station studying
        {research_topic}. They've made a breakthrough: {discovery}.
        But there's a problem: {complication}."
      - PirateAmbush: "Multiple {ship_class} ships decloak from
        {asteroid_field}. {faction} raiders. They demand {demand}."
      - WeirdSpacePhenomenon: "{phenomenon_type} detected in {system_region}.
        It's {size}. {crew_role} says: '{crew_reaction}'. Ship systems
        are {system_effect}."
      - ColonyGoneWrong: "A {faction} colony on {planet_name} has gone
        silent. Last transmission: '{last_message}'. The cause appears
        to be {cause}."
      - AIShip: "An automated ship of {origin} design. Its AI identifies
        itself as {ai_name}. It claims its mission was {mission}. It
        has been operating for {years}. It requests {request}."
- [ ] Each template defines: title template, narrative template, slot list
      with constraints, branches with conditions and consequences.
- [ ] Templates are validated by the content pipeline (S01). Schema in
      `content/schemas/trope_template.schema.json`.
- [ ] Template balance: no trope type appears more than 2 templates.
      Templates are authored — quality over quantity.

### 2. Trope slot filler (`core/src/generator/trope.rs`)

- [ ] `fill_trope_slots(template, seed, game_state) -> HashMap<String, String>`
      — for each slot, resolves the value from the game state using the
      seed for randomization. `ShipClass` → picks from available hull classes
      weighted by system threat. `Faction` → picks from factions present
      in the system. `Species` → picks from the planet's ecosystem.
      `SecretType` → picks from a table of authored secrets.
- [ ] `instantiate_trope(template, seed, game_state) -> TropeInstance` —
      fills all slots, evaluates branch conditions against the player's
      current state, produces a ready-to-present trope instance.
- [ ] Deduplication: the same trope template + same seed cannot fire twice
      for the same player. Track `(template_id, player_id, seed)` in a
      `resolved_tropes` set per player.
- [ ] Determinism: same seed + same game state = same trope instance.
      Add `trope_instantiation` to determinism manifest.

### 3. Trope triggering (`client/src/systems/trope.rs`)

- [ ] Trigger conditions: on system entry, roll per-trope-type probability
      against the base frequency scaled by location. Frontier systems: +50%
      frequency. The Reach: +100%. Authored gate systems: -50%. Deep space
      transit: guaranteed one trope per jump.
- [ ] Presentation: trope appears as a comms panel notification. "Captain,
      I'm detecting a {trope_type_summary}." Player can investigate
      (opens the trope) or defer (trope goes to the ship log as pending).
      Deferred tropes expire after N ticks (the situation resolves without
      the player).
- [ ] Trope UI: narrative text with filled slots, rendered as a comms
      message from the crew member best suited to the situation. Branch
      options appear as action buttons. The player chooses. Consequences
      apply immediately.
- [ ] Offline: tropes work offline. The generator fills slots from the
      local game state. Dilemmas are resolved locally. No server needed.

### 4. Frequency scaling engine

- [ ] `trope_frequency(location, system_params) -> f64` — computes the
      probability of a trope firing at a given location. Factors: distance
      from gate network (frontier = higher), system threat level, player's
      exploration career rank (higher rank = rarer tropes found), time
      since last trope (cooldown increases probability).
- [ ] Minimum spacing: at least 5 minutes of play between tropes (except
      deep space transit, which guarantees one). Tropes are seasoning, not
      the meal. Too many and they become noise.
- [ ] Escalation: a trope can chain into another trope. "The smuggler cache
      contained coordinates to a Predecessor artifact." The second trope is
      seeded from the first — the player follows a thread.

### 5. Authoring tool integration (S25)

- [ ] The Content Editor Suite (S25) includes a "Trope Template Editor"
      tab. Visual editor for template structure: slot definitions, branch
      trees, condition builder. Preview: "Generate 10 sample instances
      from this template" → shows filled text with different seeds.
- [ ] Slot constraint testing: the editor highlights slots that never resolve
      to a valid value given the constraints (e.g., "ship_class: Capital"
      when no faction in the system fields capital ships).

### 6. Metrics collection

- [ ] `trope_events` table: template ID, trope type, location type, frequency
      modifier, player choice, consequences applied, time to resolve.
- [ ] Research questions: which trope types have the highest investigation
      rate vs defer rate? What location types produce the most varied
      outcomes? Do tropes increase session length? What templates are
      most/least engaging?

## Acceptance gates

```
cargo test -p reachlock-core generator::trope::
# slot filling deterministic, instantiation complete, dedup working
reachlock content validate content/tropes/*.ron
make check
```

Manual: transit to a frontier system → trope fires on entry → read the
narrative → choose a branch → consequences apply → deferred trope appears
in log → expires after N ticks → new trope in deep space transit.

## Non-goals

- Fully scripted authored encounters with custom logic (S41 covers that).
  This sprint is the template engine only.
- Trope voice acting or animated cutscenes. Text-only in the comms panel.
- Player-authored tropes (mod support is S22/S25 — modders can write
  `.ron` templates, which works with zero extra code).
- Trope chains beyond two hops (one trope → one follow-up). Longer chains
  are authored encounters (S41).

## Gotchas

- Slot filling can fail to produce a valid value given constraints. E.g.,
  "ship_class: Capital" in a system with no capital-ship-capable faction.
  The filler must have a fallback for every slot kind. Fallback values
  are generic: "unknown ship", "mysterious faction", etc. Log the fallback
  at debug level.
- Template narrative quality depends on slot placement in the text. A slot
  in the middle of a sentence must produce grammatically correct results.
  Slot values have grammatical metadata (singular/plural, definite/indefinite
  article). "A {faction:indefinite} ship" → "A Compact ship." "The
  {crew_role:definite}" → "The engineer."
- The `TropeBranch.condition` field is a string DSL that must be parsed
  and evaluated. Keep the DSL minimal: `player_has_item:{id}`,
  `faction_reputation:{faction}:{op}:{value}`, `crew_has_role:{role}`,
  `ship_has_upgrade:{id}`, `always`. Document the DSL. Test every operator.
- Frequency scaling must not produce tropes faster than the player can
  resolve them. The 5-minute minimum spacing is enforced at the trigger
  level. If a trope is active (unresolved), no new tropes fire.
