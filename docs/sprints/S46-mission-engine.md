# S46 — Mission Engine

**Spec:** §14 (three-mode gameplay), §21 (faction engine), §10 (content pipeline) ·
**Wave 9 (Living Galaxy) · Depends on:** S10 (economy), S11 (factions), S40 (trope engine),
S42 (career)

## Outcome

Every station has a mission board. Every mission is context-aware — generated
from the current economy state, faction politics, player reputation, and ship
capability. A war zone generates combat missions. A blockade generates
smuggling missions. A newly discovered ecosystem generates survey missions.
Mission briefings are flavored by the trope engine, turning "deliver 50
ferrite" into "The Compact garrison at Kessel Forge is critically low on
ferrite. Their patrol craft are grounded. If you can break through the Veil
blockade with 50 tons, they'll pay hazard rates." Not every mission is
available to every captain — missions require ship capability, career rank,
or faction standing. The mission engine makes the player's ship and choices
matter.

## Context

- Mission generation is a pure function: `(seed, system_state, player_state) ->
  Vec<Mission>`. Deterministic from the inputs. Same seed + same state = same
  missions. Offline parity is preserved.
- Missions draw from the trope engine (S40) for narrative framing. The mission
  board isn't just a task list — it's worldbuilding. Each mission tells you
  something about the universe.
- Ship capability gating means the mission board reflects the player's ship.
  A player in a shuttle sees different missions than a player in a freighter.
  This makes ship choices matter — your ship defines what opportunities are
  available to you.
- Career path filtering (S42) means the board shows missions relevant to
  your path. A Compact Navy officer sees military missions. A freelance
  explorer sees survey missions. A pirate sees criminal missions.
- Mission chains provide narrative arcs. Completing a mission can unlock
  a follow-up. The chain is seeded — the player follows a thread through
  multiple missions that tell a connected story.

## Freeze first

### Mission types (`generator/mission.rs`)

```rust
pub struct Mission {
    pub id: String,                    // seed-derived
    pub mission_type: MissionType,
    pub title: String,                 // generated from trope template
    pub briefing: String,              // narrative generated from trope engine
    pub issuer: MissionIssuer,
    pub objectives: Vec<MissionObjective>,
    pub requirements: MissionRequirements,
    pub rewards: MissionRewards,
    pub expires_at_tick: Option<u64>,
    pub chain: Option<MissionChain>,
    pub tags: Vec<MissionTag>,
}

pub enum MissionType {
    Transport,       // deliver goods from A to B
    Combat,          // destroy specific targets
    Exploration,     // scan systems, discover species, map anomalies
    Diplomacy,       // deliver messages, negotiate with NPCs
    Investigation,   // find clues, solve mysteries
    Mining,          // extract specific quantity of minerals
    Salvage,         // recover items from derelicts or debris
    Smuggling,       // deliver contraband past blockades
    Bounty,          // eliminate specific pirate/NPC
    Escort,          // protect a convoy through dangerous space
    Survey,          // catalog an entire ecosystem or planet
    Construction,    // deliver materials for station construction
    Rescue,          // rescue crew from a disabled ship or station
}

pub enum MissionIssuer {
    Faction { faction_id: String, division_id: Option<String> },
    Station { station_id: String },
    NPC { npc_id: String },
    Personal,         // generated for the player's career path
    DistressBeacon,   // dynamic mission from a trope encounter
}

pub struct MissionObjective {
    pub objective_type: ObjectiveType,
    pub target: String,               // what: good_id, npc_id, system_id, etc.
    pub quantity: Option<u64>,        // how many/much
    pub destination: Option<String>,  // where to deliver/complete
    pub optional: bool,               // bonus objective?
}

pub enum ObjectiveType {
    Deliver,
    Destroy,
    Scan,
    TalkTo,
    Retrieve,
    Extract,
    Protect,
    Escort,
    Reach,          // reach a location
}

pub struct MissionRequirements {
    pub min_cargo_space: Option<u64>,
    pub min_crew_count: Option<u32>,
    pub required_crew_roles: Vec<CrewRole>,
    pub required_ship_upgrades: Vec<String>,     // widget IDs from S45
    pub min_career_rank: Option<(String, u8)>,   // (path_id, rank)
    pub min_faction_reputation: Option<(String, i64)>,
    pub required_items: Vec<String>,
    pub max_notoriety: Option<NotorietyLevel>,   // some missions won't hire criminals
}

pub struct MissionRewards {
    pub credits: u64,
    pub reputation_gains: Vec<(String, i64)>,    // (faction_id, delta)
    pub items: Vec<String>,                      // item IDs rewarded
    pub career_progress: Vec<(ProgressionCriterionType, u64)>,
    pub unlock: Option<String>,                  // encounter/station/path unlocked
}

pub struct MissionChain {
    pub chain_id: String,
    pub position: u8,              // 1-based — which mission in the chain
    pub total_missions: u8,
    pub next_mission_seed: u64,    // seed for the follow-up
    pub chain_title: String,       // overarching narrative arc name
}

pub enum MissionTag {
    HighRisk,
    HighReward,
    TimeSensitive,
    Repeatable,
    StoryCritical,
    Coop(Vec<String>),             // requires specific other players
    FactionSecret,
    Beginner,
    Expert,
}
```

### Mission generation input

```rust
pub struct MissionGenerationContext {
    pub seed: u64,
    pub system_state: SystemState,          // economy, factions, events
    pub player_state: PlayerMissionState,   // ship, career, reputation
    pub tick: u64,
}
```

## Deliverables

### 1. Mission generator (`core/src/generator/mission.rs`)

- [ ] `generate_missions(context) -> Vec<Mission>` — produces 5-15 missions
      per station/system. Quantity scales with station size and system
      activity. Frontier stations have fewer missions (less civilization).
      Capital stations have many.
- [ ] Mission type distribution weighted by system state:
      - War zone: +50% Combat, +30% Smuggling, +20% Rescue
      - Blockade: +40% Smuggling, +30% Transport, +20% Combat
      - Frontier: +40% Exploration, +30% Survey, +20% Salvage
      - Trade hub: +50% Transport, +30% Construction
      - Research hub: +50% Survey, +30% Investigation
      - Pirate haven: criminal mission types only
- [ ] Narrative generation: each mission's `title` and `briefing` are
      generated from the trope engine (S40) with mission-specific
      templates. "Transport" → "The {faction} {division} needs {good}
      delivered to {station}. {urgency_context} {complication_context}."
      Fills from the economy state, faction relations, and system context.
- [ ] Requirement calculation: ship capability requirements are derived
      from the mission objectives. A transport mission for 50 cargo space
      requires `min_cargo_space: 50`. An exploration mission requiring
      deep space scanning requires `required_ship_upgrades: ["deep_space_scanner"]`.
- [ ] Mission selection: after generating, filter missions the player
      CAN do. Show a "Mission Board" with all available missions and a
      grayed-out section: "Missions Beyond Your Capability" — visible
      requirements with red X marks. "Requires Cargo Space: 50 (You have:
      40)." This is aspirational — it tells the player what to upgrade.
- [ ] Determinism: add `mission_generation` to the determinism manifest.
      Test with fixed seed + fixed game state → identical mission set.

### 2. Mission board UI (`client/src/systems/mission_board.rs`)

- [ ] Accessible at every station (InteractKind::MissionBoard). Also
      accessible from the ship's comms panel (remote mission board —
      shows missions for the current system).
- [ ] Board view: mission cards showing type icon, title, issuer faction
      badge, reward summary, and time remaining (if TimeSensitive).
      Click a card → mission detail view.
- [ ] Detail view: full briefing narrative, objective checklist, rewards
      breakdown, requirements list (with checkmarks for those met, X for
      those unmet). "Accept" button (if requirements met) or "Upgrade
      Required" button (links to ship upgrade panel).
- [ ] Active missions: a separate tab showing accepted missions. Progress
      bars per objective. Abandon button (costs faction reputation).
- [ ] Mission chain indicator: if the mission is part of a chain, shows
      "Part 3 of 5: The Kessel Conspiracy."
- [ ] Career-filtered view: toggle to show only missions for your career
      path(s). Toggle to show all.

### 3. Mission chains (`core/src/generator/mission_chain.rs`)

- [ ] Chain generation: when generating a mission that spawns a chain,
      the `next_mission_seed` is deterministic from the current mission seed
      + chain position. The next mission is generated when the current one
      is completed. Chains are 2-5 missions long.
- [ ] Chain narrative coherence: the chain has a `chain_title` and each
      mission references the previous. "After delivering the ferrite,
      Commander Voss has another job for you..." The narrative builds
      across missions.
- [ ] Chain rewards escalate: later missions in a chain have higher rewards.
      The final mission has a significant reward — unique item, faction
      standing boost, career progression milestone.
- [ ] Chain branching: at certain points, the player can choose how to
      proceed. "Investigate the smugglers' source (Exploration path) or
      report to faction command (Military path)." The choice determines
      the next mission in the chain.
- [ ] Authored chains: scripted encounters (S41) can define mission chains
      with fully authored briefings and specific rewards. The generator
      fills in between authored missions.

### 4. Player reputation impact

- [ ] Completing missions for a faction improves reputation (S11). Failing
      or abandoning damages it. Some missions are reputation-gated: "Must
      have Compact reputation 20+ to receive this mission."
- [ ] Mission issuer visibility: some missions are only visible at certain
      reputation thresholds. A faction's internal division (S11) offers
      secret missions at high standing. "Compact Intelligence Division —
      Classified: Infiltrate ISC trade network."
- [ ] Mission consequences in the universe: completing a Transport mission
      moves goods from A to B. This affects the economy (S10/S44). A
      blockade-running mission that succeeds changes prices at the
      blockaded station. Missions have persistent effects.

### 5. Co-op missions

- [ ] `MissionTag::Coop(vec![other_player_id])` — missions that require
      multiple players. Generated when two players in the same system
      have complementary capabilities. "You need cargo space; another
      player needs combat escort." Co-op missions show on both players'
      boards.
- [ ] Co-op acceptance: both players must accept. Mission progress is
      shared. Rewards are split. Failing affects both players' reputation.
- [ ] Co-op missions are Phase 1 of multiplayer gameplay beyond presence
      (S23). They're generated, not authored. The generator matches
      players by proximity and complementary ship capabilities.

### 6. Metrics collection

- [ ] `mission_events` table: mission generation counts by type, acceptance
      rate, completion rate, abandon rate, chain completion rate, average
      time per objective, co-op participation rate.
- [ ] Research questions: what mission types have the highest completion
      rate? What types are most commonly abandoned? What ship capabilities
      correlate with mission type preference? Do mission chains improve
      retention? Do co-op missions increase session length?

## Acceptance gates

```
cargo test -p reachlock-core generator::mission::
# generation deterministic, requirement calculation, chain generation,
# type distribution weighted correctly
make check
```

Manual: dock at a station → open mission board → see available missions →
filter by career → accept a transport mission → cargo hold fills → deliver
to destination → complete → accept chain follow-up → complete chain →
view reputation change → check mission board at a different station →
different missions based on that station's context.

## Non-goals

- Player-authored missions (mod support is S22/S25 — modders can write
      mission templates as `.ron` files).
- Dynamic mission difficulty scaling (level scaling). Mission difficulty is
      set by the generator based on the objective, not adjusted to the
      player's ship. A hard mission is hard regardless of who takes it.
- Real-time mission updates. Missions are generated on station entry and
      cached for the session. Economy changes between generation and
      completion don't retroactively update the mission briefing.
- Full quest journal with branching dialogue trees. That's scripted
      encounters (S41). Missions are objective-based; the narrative is
      the trope-flavored briefing.

## Gotchas

- Mission objectives that require the player to "destroy 3 pirate ships"
      need a way to count completions. The combat system (S19) must emit
      events that the mission tracker can consume. Each system that
      produces completable actions (combat, trade, scan, talk) needs an
      event emission hook. This is the same kind of cross-cutting concern
      as career progression (S42) — coordinate the hooks.
- Mission generation is called every time the player enters a station.
      Generation must be fast (<50ms for a board of 10 missions). Cache
      the mission board per station per tick — if the player re-enters
      the same station on the same tick, reuse the cached board.
- Chain narrative coherence depends on the trope engine producing
      sequentially consistent text. The chain template must include a
      "previous mission summary" slot that feeds into the next mission's
      briefing. Test chain narrative quality with automated generation
      of 3-mission chains.
- Co-op mission matching must not be intrusive. Players shouldn't see
      co-op missions for other players in their system unless those
      missions are relevant to their capabilities. The matching algorithm
      is: find overlapping mission types where the two players'
      capabilities complement each other. No match → no co-op missions.
