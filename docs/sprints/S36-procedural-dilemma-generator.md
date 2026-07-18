# S36 — Procedural Dilemma Generator

**Spec:** §5 (generator system), §18 (LLM agency model) · **Wave 8 (LLM gameplay) ·
Depends on:** S04 (system generator), S15 (agency model), S16 (dialogue)

## Outcome

The universe creates situations designed to land on the LLM edge. Not every
encounter — but the generator knows which seeds produce ambiguity. A derelict
ship with a failing life support AI that must choose who lives. A station
where the governor's policy is hurting the population but keeping the faction
stable. A crew member who discovers another crew member's secret. The generator
produces the setup. The crew's contracts deliberate. The outcome has persistent
consequences — faction relations, station population, crew trust. The generator
is the game master.

## Context

- Generators already produce systems, stations, encounters, and items. They
  produce VARIATION but not SITUATION. This sprint adds a new generator
  layer that takes a seed + system context and produces a `Dilemma` — a
  structured situation with no single correct resolution, designed to trigger
  crew deliberation.
- The spec's LLM philosophy: "LLMs fire at deterministic-tree leaf nodes."
  Dilemmas produce those leaf nodes. They're the spice, not the meal. Most
  encounters still resolve through deterministic rules. Dilemmas are the
  moments where rules can't suffice.
- The research surface: what dilemma structures reliably trigger interesting
  deliberation? Which produce trivial resolutions? Which produce the widest
  variance across LLM models? The generator is a data-gathering instrument.
- Players create their own crew and attributes. A dilemma that hits a
  pacifist medic differently from a mercenary gunner IS the system working.
  The same dilemma, different crews, different outcomes. The research
  compares across player types.

## Freeze first

### Dilemma type hierarchy (`generator/dilemma.rs`)

```rust
pub struct Dilemma {
    pub id: String,                      // seed-derived
    pub dilemma_type: DilemmaType,
    pub setup: DilemmaSetup,             // the situation, as the player sees it
    pub participants: Vec<DilemmaParticipant>,
    pub stakes: Vec<DilemmaStake>,        // what's at risk for each choice
    pub choices: Vec<DilemmaChoice>,      // possible resolutions (at least 2, no "correct" one)
    pub seed: u64,
    pub complexity: DilemmaComplexity,
}

pub enum DilemmaType {
    // Life-or-death tradeoffs
    Triage { max_can_save: u8, candidates: Vec<String> },
    Sacrifice { who: String, for_what: String },
    Abandonment { what: String, consequence: String },

    // Moral/social tradeoffs
    LoyaltyConflict { between: (String, String), issue: String },
    SecretRevealed { who: String, secret: String, affected: Vec<String> },
    UnjustLaw { law: String, victims: Vec<String> },

    // Resource tradeoffs
    Allocation { resource: String, claimants: Vec<String>, amount: u32 },
    Investment { options: Vec<(String, u32)>, budget: u32 },

    // Tactical/strategic tradeoffs
    FogOfWar { known: Vec<String>, unknown: Vec<String> },
    RetreatOrStand { odds: u8, stakes_on_ground: String },

    // Crew interpersonal
    CrewSecret { who: String, secret: String, discovered_by: String },
    MutinyBrewing { dissenters: Vec<String>, grievance: String },
    OutsiderAppeal { outsider: String, offer: String, cost: String },

    // Faction/outer world
    BlockadeChoice { blockaded: String, needed_goods: Vec<String> },
    DefectorAppeal { defector: String, information: String },
    PredecessorEnigma { artifact: String, risk: String, potential: String },
}

pub struct DilemmaSetup {
    pub title: String,                   // one-line summary
    pub narrative: String,               // 2-3 paragraph scene-setting
    pub urgency: DilemmaUrgency,         // how long they have to decide
}

pub enum DilemmaUrgency {
    Immediate,           // must decide now (seconds)
    Pressing,            // decision within the hour
    Looming,             // decision within the session
    Background,          // resolves over multiple sessions
}

pub struct DilemmaChoice {
    pub label: String,
    pub description: String,
    pub consequences: Vec<DilemmaConsequence>,
    pub alignment_tags: Vec<String>,     // "pacifist", "pragmatic", "reckless", etc.
}

pub struct DilemmaConsequence {
    pub kind: ConsequenceKind,
    pub target: String,                  // who or what is affected
    pub magnitude: u8,                   // 0-10 severity
    pub description_template: String,
}

pub enum ConsequenceKind {
    CrewTrustChanged,
    FactionReputationChanged,
    PopulationChanged,
    ResourceGained,
    ResourceLost,
    CrewMemberQuits,
    NewMissionUnlocked,
    StoryArcProgressed,
    Nothing,
}

pub enum DilemmaComplexity {
    Simple,     // 2 choices, clear tradeoffs
    Nuanced,    // 3-4 choices, mixed consequences
    Wicked,     // no good option, all choices hurt someone
}
```

### Dilemma generator type

```rust
/// Pure function: seed + game state → Option<Dilemma>.
/// Returns None if the seed doesn't produce a dilemma (most seeds don't).
pub fn generate_dilemma(
    seed: u64,
    system: &GeneratedSystem,
    crew: &[SoulFile],
    relationship_memories: &[RelationshipMemory],  // from S35
    player_reputation: &FactionReputationMap,
) -> Option<Dilemma>;
```

The generator is deterministic from the seed. The same seed + same game state =
same dilemma. This means a shared seed between offline and online produces the
same dilemma, which means the same crew deliberation, which means the same
outcome. Offline parity is preserved.

## Deliverables

### 1. Dilemma generation (`core/src/generator/dilemma.rs`)

- [ ] `generate_dilemma()` — takes a seed + game state, returns an `Option<Dilemma>`.
      Uses a `SeededRng` derived from the seed. The generation pipeline:
      1. Determine if this seed produces a dilemma (probability: ~15% for
         Wicked seeds, ~30% for Nuanced, ~50% for Simple — systems with
         higher threat levels have higher dilemma probability).
      2. Select `DilemmaType` from the system's context (system biome,
         faction presence, crew composition, threat level, gate proximity).
      3. Populate the dilemma's participants, stakes, and choices.
      4. Each choice gets consequences generated from the dilemma type's
         consequence template. Consequences are weighted so no single choice
         is obviously "correct."
- [ ] Dilemma complexity distribution by system: frontier systems (S21) are more
      likely to produce dilemmas, and more likely to produce complex ones.
      Safe, authored systems produce fewer dilemmas. The frontier is where
      the LLM shines — the gate network is where the authored story is.
- [ ] Integration with S35: when generating a dilemma involving crew members,
      the generator reads `RelationshipMemory` to determine which crew pairs
      have tension, which have trust, and produces dilemmas that leverage
      those dynamics. "The generator notices that Boris and Tove haven't
      resolved their Veil argument yet. It produces a dilemma where they
      must cooperate." This is the system "reading" the story and creating
      new chapters.
- [ ] Determinism: add `dilemma_generation` to the determinism manifest.
      Test with a fixed seed + fixed game state → deterministic output.
      Bump manifest version.

### 2. Dilemma presentation (`client/src/systems/dilemma.rs`)

- [ ] Trigger: when the system generator or universe tick produces a dilemma,
      the client displays it. Not immediately — the dilemma enters the
      crew comms panel as if a crew member discovered it. "Captain, I'm
      picking up a distress signal from a derelict. Life support AI is
      requesting a decision. It says... it can only sustain 3 of 5
      compartments." The narrative is delivered by the crew, not a UI popup.
- [ ] Deliberation interface: the dilemma is fed into the co-deliberation
      system (S33). The crew discusses. The comms panel shows the exchange.
      The player can intervene (manual override) or let the crew decide.
      The outcome is one of the dilemma's choices, selected by the crew's
      deliberation or the player.
- [ ] Consequences panel: after resolution, a summary panel shows what
      happened. "The crew chose to save the med bay, engineering, and crew
      quarters. 12 station residents died in the cargo hold. The station
      AI sent a final message: 'You did what you could.' Faction reputation
      with the local administration: -3 (they wanted everyone saved)."
- [ ] Background dilemmas: dilemmas with `Looming` or `Background` urgency
      don't demand immediate resolution. They appear in the ship log as
      open items. The player can open them at any time from the log. The
      universe keeps ticking — a `Looming` dilemma that's ignored for
      too long auto-resolves (the generator picks the "do nothing" outcome).

### 3. Dilemma outcomes as persistent consequences

- [ ] Every consequence in a dilemma choice is applied to the game state.
      Crew trust changes (S35 memory). Faction reputation changes (S11).
      Station population changes (economy engine). Resource changes
      (inventory). New missions unlocked (storyline engine).
- [ ] Consequences are logged to the ship log (S37). The event log records
      the dilemma ID, the choice made, and the consequences applied.
- [ ] Some consequences are delayed. "The administration will remember this."
      The faction reputation penalty applies immediately, but a follow-up
      event 10 ticks later: "The administration has denied your docking
      request at Sorrow Station." The dilemma's consequences ripple.

### 4. Transient crew effects

- [ ] After a dilemma is resolved, the crew who participated in the
      deliberation carry the effects for a session. A crew member who
      advocated for a choice that lost gains a temporary "disheartened"
      modifier. A crew member who was right gains "validated." These
      affect their deliberation tone and contract evaluation speed.
- [ ] Crew who were affected by the dilemma's consequences (e.g., the
      medic who watched patients die because the crew chose to save
      engineering) have a persistent "scar" — a relationship memory
      event with very high weight. This feeds into S35 and future dilemmas.

### 5. Metrics collection

- [ ] `dilemma_events` table: dilemma type, complexity, crew composition,
      choice made, deliberation duration, crew relationship deltas,
      outcome distribution.
- [ ] Research questions: which dilemma types most reliably trigger
      crew deliberation (vs deterministic resolution)? What crew
      compositions correlate with which choice types? How does dilemma
      complexity affect player intervention rate? Do dilemmas with
      crew personal dynamics (CrewSecret, MutinyBrewing) produce more
      relationship change than external dilemmas?
- [ ] Model comparison: if the same dilemma seed + game state is
      encountered by players in different universe tiers (Classic vs
      Spectrum), how do outcomes differ? Classic = deterministic defaults
      only. Spectrum = LLM-influenced choices. This IS the research
      instrument.

## Acceptance gates

```
cargo test -p reachlock-core generator::dilemma::
# generation is deterministic, dilemma types all reachable,
# consequence weighting doesn't produce single "correct" choice
cargo test -p reachlock-core determinism::  # dilemma golden entries
make check
```

Manual: fly to a frontier system → encounter a derelict ship dilemma →
crew deliberates → comms panel shows exchange → choose a resolution →
see consequences applied to faction reputation → ship log records the
encounter → 5 ticks later, a follow-up event references the dilemma's
outcome.

## Non-goals

- Authored dilemmas (that's the content pipeline, S01/NPC dialogue).
  This sprint is PROCEDURAL dilemmas. Authored dilemmas use the same
  data structures but are hand-written in `.ron` files.
- Dilemma chains (dilemma A's outcome produces dilemma B). The generator
  is stateless per seed. Chained dilemmas can emerge naturally from the
  consequence system (delayed consequences create new situations), but
  the generator doesn't explicitly chain them.
- LLM-generated dilemmas. The generator is deterministic in core.
  The LLM is the resolver, not the creator.
- Every encounter being a dilemma. Most encounters are deterministic.
  Dilemma probability is calibrated to ~1 per 2-3 hours of play.
  They're special because they're rare.

## Gotchas

- The generator must NOT produce dilemmas that have an obviously correct
  choice. A triage dilemma where "save the med bay" saves 15 people and
  "save the cargo hold" saves 0 is not a dilemma — it's a UI for the
  correct answer. Every dilemma must be tested for choice balance:
  run the consequence scoring against a standard utility function and
  ensure no single choice dominates.
- Dilemma generation uses `RelationshipMemory` (S35) to tailor dilemmas
  to the crew. If S35 isn't merged, fall back to generic dilemmas that
  don't reference crew relationships. The generator must detect which
  inputs are available.
- The same seed + same game state must produce the identical dilemma.
  This means the game state input to the generator must be a DETERMINISTIC
  snapshot — no floating-point timestamps, no non-deterministic ordering.
  Use the tick clock and sorted collections.
- Background dilemmas that auto-resolve must apply the "do nothing" outcome.
  This is a valid choice with consequences. The player can't ignore a
  dilemma without cost. The "do nothing" outcome is often the worst one —
  inertia has a price.
