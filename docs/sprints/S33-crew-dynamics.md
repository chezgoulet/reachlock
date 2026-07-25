# S33 — Crew Dynamics

**Spec:** §6 (contract system), §15 (soul system), §18 (LLM agency) ·
**Wave 8 (LLM gameplay) · Depends on:** S13 (souls), S15 (agency), S16 (dialogue)

## Outcome

The crew don't operate in isolation. When two crew members' contracts hit the
LLM edge simultaneously — or when one's deliberation feeds into another's —
they interact. They argue, defer, compromise, persuade. The comms panel shows
the exchange in real time. The outcome shifts crew relationships, which shift
future behavior, which generates new stories. This is not crew as automation.
This is crew as drama.

## Context

- Every crew member has a soul file (S13), a contract (S06/S16), and can
  enter deliberation when their rule tree reaches an LLM edge.
- The contract engine evaluates rules independently per contract. This sprint
  adds a *co-deliberation* layer: when two crew members' contracts are
  triggered by the same event, or when one's deliberation output becomes
  context for another's, the system sequences them through a conversation
  rather than resolving them in isolation.
- The player creates their own characters, backgrounds, and attributes.
  The Loup-Garou is the demo/tutorial. This means contracts are CHARACTER
  CREATION — the rules the player writes ARE the crew member's personality.
  Crew dynamics emerge from how the player's own characters interact through
  their contracts. Two characters the same player created, with different
  contracts, can be at odds. That's the player designing dramatic tension.

## Freeze first

### Co-deliberation protocol (`contract/co_deliberation.rs`)

```rust
pub struct CoDeliberation {
    pub participants: Vec<CrewDeliberant>,
    pub trigger_event: GameEvent,
    pub turn: usize,                          // whose turn to speak/act
    pub history: Vec<DeliberationTurn>,       // what's been said so far
    pub resolution: Option<CoResolution>,
}

pub struct CrewDeliberant {
    pub crew_id: String,                      // soul ID
    pub relationship_state: RelationshipState, // history with each other participant
    pub initial_position: CrewPosition,        // what they want before deliberation
    pub current_position: CrewPosition,        // shifts as they hear others
}

pub enum CrewPosition {
    Propose { action: String, reasoning: String },
    Support { who: String, reason: String },
    Oppose { who: String, reason: String },
    Defer { to: String, reason: String },
    Abstain { reason: String },
}

pub struct DeliberationTurn {
    pub speaker: String,
    pub position: CrewPosition,
    pub llm_raw: String,                       // full deliberation output
    pub visible_to_player: String,             // what the comms panel shows
    pub relationship_delta: Vec<(String, i64)>, // who moved how much
}

pub enum CoResolution {
    Consensus { action: String },
    MajorityAction { action: String, dissenter: String },
    TieBreak { action: String, tiebreaker: String },
    Deadlocked,                                // no decision — player must choose
    PlayerOverride { action: String },         // player cut off deliberation
}
```

### Relationship compression (extends S35 types)

Relationship state between crew members is a compressed representation of
their interaction history. Used by the co-deliberation LLM to produce
conversationally coherent behavior.

```rust
pub struct CrewRelationship {
    pub familiarity: Fixed,       // how long they've served together
    pub trust: Fixed,             // -1.0 to 1.0
    pub respect: Fixed,           // does this crewmate know what they're talking about?
    pub tension: Fixed,           // 0.0 to 1.0 — unresolved conflicts
    pub notable_events: Vec<RelationshipEvent>,
}

pub struct RelationshipEvent {
    pub event_type: RelationshipEventType, // SavedMyLife, UnderminedMyDecision, etc.
    pub timestamp: u64,
    pub weight: Fixed,
}
```

## Deliverables

### 1. Co-deliberation engine (`core/src/contract/co_deliberation.rs`)

- [ ] `CoDeliberation::step()` — pure state transition. Given the current
      turn, each participant's relationship state, and the trigger event:
      produces the next `DeliberationTurn` OR a `CoResolution`. Never
      produces an infinite loop — max 3 rounds per participant, then
      forced resolution.
- [ ] Turn sequencing: round-robin by crew seniority. Each participant sees
      the full history of what's been said. Their deliberation prompt includes
      relationship context ("Tove has respected your calls 7 times in the
      past"), the trigger event, what other crew have already said, and their
      own contract's predispositions.
- [ ] Relationship deltas: each turn computes how the speaker's position
      affects their relationship with every other participant. Supporting
      someone increases trust. Opposing someone who was right increases
      respect but decreases tension. Opposing someone who was wrong increases
      respect. The delta is stored and fed into the persistent relationship
      system (S35).
- [ ] `CoResolution::Deadlocked` — when no resolution emerges, the player
      is prompted to decide. The comms panel shows the crew is at an impasse.
      The player's choice is a manual action override. The crew reacts.
- [ ] `CoResolution::PlayerOverride` — the player can interrupt deliberation
      at any turn with "No, we're doing it my way." This IS the manual
      override signal that short-circuits the remaining deliberation.
      Relationship consequences are computed: crew who agreed with the player
      gain trust; crew who were overruled lose trust; crew who were arguing
      and got cut off gain tension.

### 2. Comms panel rendering (`client/src/systems/comms.rs` expansion)

- [ ] Deliberation mode: when co-deliberation is active, the comms panel
      enters a multi-participant mode. Each turn, a new line appears with the
      speaker's name, portrait chip, and what they said. Turns animate in
      with a small delay (400ms) — the deliberation IS the content, not a
      loading screen to skip through.
- [ ] Player intervention: a prompt at the bottom of the comms panel during
      deliberation: `[ENTER] Decide  [ESC] Let them work it out`. ESC defers
      to the next turn. ENTER opens a quick action picker (the standard
      contract action options filtered to the situation).
- [ ] Deliberation log: the full exchange is logged to the ship log (S37
      consumes this). The player-visible comms panel shows a conversation;
      the log shows the underlying deliberation traces for those who want
      to dig deeper.

### 3. Triggers for co-deliberation

- [ ] Simultaneous edge hits: when two crew members' contracts both reach
      their LLM edge on the same event, the system enters co-deliberation.
      Example: hull breach during combat. Boris's contract wants to repair
      weapons. Tove's contract wants to repair med bay. Both hit the edge at
      once. They talk it out.
- [ ] Cascaded deliberation: when one crew member's LLM output becomes
      relevant to another's contract evaluation, the system chains them.
      Example: Tib's combat contract fires → LLM decides to retreat →
      Boris's damage control contract evaluates the retreat order as a new
      context → Boris deliberates whether to comply or keep repairing.
- [ ] Player-initiated: the player can call a "crew conference" from the
      comms panel. Select a topic (current situation, upcoming mission choice,
      interpersonal issue). The co-deliberation system runs. This is the
      "hold a crew meeting" mechanic.
- [ ] Offline: all triggers function without a server. Co-deliberation uses
      the same offline/online agency path as single-crew deliberation (S15).
      Offline = stub/pinned model; online = tiered LLM proxy.

### 4. Relationship persistence hook

- [ ] After co-deliberation resolves, the relationship deltas are written to
      the crew relationship store. This feeds S35's persistent memory.
      An argument today changes how they interact tomorrow.
- [ ] Visual indicators: the crew roster screen shows relationship status
      between crew pairs. Not a number — a word. "Tense." "Respectful."
      "Close." "Avoiding." Updated after significant deliberation events.

### 5. Metrics collection

- [ ] `co_deliberation_events` table: records each co-deliberation session
      with participant count, trigger type, turn count, resolution type,
      relationship delta magnitudes. No PII — just structural data about
      how the system behaves.
- [ ] Research question tracked: what triggers most often produce
      `Deadlocked`? Which crew role pairs produce the widest position
      divergence? How often does `PlayerOverride` occur vs letting
      deliberation resolve naturally?

## Acceptance gates

```
cargo test -p reachlock-core contract::co_deliberation
# step() terminates, resolution types all reachable, relationship deltas correct sign
make check
```

Manual: trigger a hull breach with Boris and Tove aboard → comms panel
shows their exchange → watch Tove defer to Boris after he cites his
engineering experience → crew roster updates their relationship → ship
log records the full exchange.

## Non-goals

- Scale beyond 5 concurrent deliberants (crew max is 7; 3-4 in practice
  for co-deliberation. Simple round-robin is sufficient.)
- LLM-vs-LLM combat decisions (enemy AI is behavior trees, not contracts)
- Real-time audio of deliberation (voice is S29 P2P proximity — deliberation
  is text on the comms panel, rendered as crew comm chatter)
- Deliberation replays as shareable content (that's S37's captain's log)

## Gotchas

- The deliberation system must never block the game loop. Co-deliberation
  uses the existing async LLM path (S14) with visible deliberation state
  (S16). The comms panel animates one turn at a time; between turns, the
  game continues.
- PlayerOverride must feel responsive, not punitive. If the player cuts off
  deliberation after 2 turns, the relationship consequences should be mild —
  "captain made a call." If they cut it off at turn 1 repeatedly across many
  events, the crew gains a "captain doesn't listen" relationship modifier.
- The loop is: event → Step 1 → render → wait for LLM → Step 2 → render →
  wait for LLM → resolution. Each step is a separate LLM call. This is slow
  by design — the deliberateness IS the mechanic. The player watches.
- Relationship deltas are small per event (1-5 points on a -100 to 100 scale).
  A crew that's been through 50 events together has earned their dynamic;
  no single deliberation should swing a relationship wildly.
