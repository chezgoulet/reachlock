# S42 — Unified Career Progression

**Spec:** §14 (three-mode gameplay), §21 (faction engine) · **Wave 9 (Living Galaxy) ·
Depends on:** S11 (faction engine)

## Outcome

Players don't just fly a ship — they build a life. Join a faction and rise
through its ranks. Specialize in a career path: military, trade, exploration,
science, political, criminal, freelance. Each path has ranks, perks, and
unlocks. Progress is earned through actions that match the path, not through
XP grinding. A life path is a series of choices the game notices and rewards.
The framework is unified — one progression system with parallel tracks that
share the same data structures. Players can switch tracks but it costs
standing. A pirate who goes legit. An explorer who joins a faction. A trader
who becomes a smuggler. These are stories the progression system tells.

## Context

- S11 already has multi-axis reputation per faction and internal divisions.
  This sprint builds ON TOP of reputation — a career track is a long-term
  commitment with structured rewards, not just a number that goes up.
- The player creates their own character, background, starter attributes.
  Career progression is how those choices play out over time. A character
  who starts as a "former Compact officer" has a head start on the Military
  track. A character who started as a "Reach scavenger" has criminal
  contacts.
- The Loup-Garou is the demo/tutorial. The career system works for any crew
  the player creates. NPC crew members have their own mini-careers (their
  contracts can advance based on what they do).
- Multiple career tracks are possible simultaneously — a player can be both
  an Explorer (independent) and a Compact Science Division member. But
  conflicting tracks (Compact Military + Reach Pirate) are mechanically
  incompatible — advancing one damages the other.

## Freeze first

### Career system types (`career/mod.rs`)

```rust
pub struct CareerPath {
    pub id: String,
    pub path_type: PathType,
    pub name: String,
    pub description: String,
    pub faction_id: Option<String>,    // None = independent (freelance, criminal)
    pub ranks: Vec<CareerRank>,
    pub progression_criteria: Vec<ProgressionCriterion>,
    pub perks: Vec<CareerPerk>,
    pub conflicting_paths: Vec<String>, // path IDs that can't be advanced simultaneously
}

pub enum PathType {
    Military,
    Trade,
    Exploration,
    Science,
    Political,
    Criminal,
    Freelance,
}

pub struct CareerRank {
    pub rank: u8,
    pub title: String,                   // "Lieutenant", "Master Trader", "Senior Xenobiologist"
    pub required_criteria: Vec<ProgressionRequirement>,
    pub rank_perks: Vec<String>,         // perk IDs unlocked at this rank
    pub faction_standing_bonus: i64,     // reputation bump when achieving this rank
}

pub struct ProgressionCriterion {
    pub criterion_type: ProgressionCriterionType,
    pub target: String,
    pub threshold: u64,
    pub weight: f64,                     // how much this counts toward progression
}

pub enum ProgressionCriterionType {
    CombatVictories,
    TradeVolume,
    SystemsDiscovered,
    SpeciesScanned,
    MissionsCompleted,
    FactionReputationGained,
    CrewTrustBuilt,
    ArtifactsRecovered,
    ShipsCaptured,
    ContrabandSmuggled,
    BountiesCollected,
    ResearchPointsEarned,
    StoryMissionsCompleted,
}

pub struct ProgressionRequirement {
    pub criterion_type: ProgressionCriterionType,
    pub threshold: u64,
}

pub struct CareerPerk {
    pub id: String,
    pub name: String,
    pub description: String,
    pub perk_type: PerkType,
    pub magnitude: f64,
}

pub enum PerkType {
    StationDiscount { faction_id: String, pct: f64 },
    RestrictedAreaAccess { area_id: String },
    UniqueShipComponent { item_id: String },
    ExclusiveContract { contract_id: String },
    CrewRecruitUnlock { crew_id: String },
    MissionBonus { mission_type: String, bonus_pct: f64 },
    ScannerBoost { pct: f64 },
    CombatBonus { damage_type: String, pct: f64 },
    TradeBonus { good_category: String, pct: f64 },
    DiplomaticImmunity { faction_id: String },
    BountyPass { faction_id: String },    // crimes forgiven in this faction
}

pub struct PlayerCareer {
    pub player_id: String,
    pub active_paths: Vec<ActiveCareerPath>,
    pub completed_paths: Vec<CompletedPath>,
    pub total_prestige: u64,              // aggregate across all paths
}

pub struct ActiveCareerPath {
    pub path_id: String,
    pub current_rank: u8,
    pub progress: HashMap<ProgressionCriterionType, u64>,
    pub joined_at_tick: u64,
    pub last_advanced_at_tick: u64,
}

pub struct CompletedPath {
    pub path_id: String,
    pub final_rank: u8,
    pub completed_at_tick: u64,
    pub reason: CompletionReason,
}

pub enum CompletionReason {
    ReachedMaxRank,
    Resigned,             // player chose to leave
    Expelled,             // faction kicked the player out
    Defected,             // player joined a conflicting path
}
```

## Deliverables

### 1. Career path definitions (`content/careers/`)

- [ ] 8-12 authored career paths as `.ron` files. Minimum paths per type:
      - Military: Compact Navy, ISC Defense Force, Reach Militia
      - Trade: Compact Merchant Guild, ISC Free Traders, Independent Hauler
      - Exploration: Compact Survey Corps, Deep Space Cartographers, Frontier Scout
      - Science: Compact Xenobiology Division, Independent Researcher
      - Political: Compact Internal Affairs, ISC Diplomatic Corps
      - Criminal: Reach Pirates, Smuggler's Network, Bounty Hunter's Guild
      - Freelance: Renegade Operator (no faction — earn through deeds alone)
- [ ] Each path defines: 5-10 ranks with escalating requirements, 3-8 perks
      unlocked at specific ranks, progression criteria with weights,
      conflicting paths.
- [ ] Validated by the content pipeline. Schema in
      `content/schemas/career_path.schema.json`.

### 2. Progression engine (`core/src/career/mod.rs`)

- [ ] `record_progress(player_career, action_type, target, magnitude) -> PlayerCareer` —
      pure function. Called after any significant player action that matches
      a career criterion. "Player won combat against 3 pirate ships" →
      records progress on all paths that track CombatVictories, weighted
      by the path's weight for that criterion.
- [ ] `check_rank_advancement(player_career, path) -> Option<u8>` — checks if
      all requirements for the next rank are met. Returns the new rank
      number or None.
- [ ] `advance_rank(player_career, path) -> (PlayerCareer, Vec<CareerPerk>)` —
      promotes the player to the next rank. Unlocks any perks at that rank.
      Applies faction standing bonus. Returns the updated career and the
      list of unlocked perks.
- [ ] `join_path(player_career, path_id, game_state) -> Result<PlayerCareer, JoinError>` —
      validates no conflicting paths are active, checks prerequisites (some
      paths require minimum reputation or a sponsor), adds the path at rank 0.
- [ ] `leave_path(player_career, path_id, reason) -> (PlayerCareer, Vec<CareerConsequence>)` —
      removes the path, records the completion. Consequences depend on
      reason: resigning from a faction military track loses standing.
      Defection to a conflicting faction causes a reputation crash.

### 3. Career panel UI (`client/src/systems/career.rs`)

- [ ] Accessible from the player menu (tab alongside inventory, crew, ship).
      Shows: active career paths with current rank, progress bars per
      criterion, next rank requirements, unlocked perks. Available paths
      (discoverable — you learn about paths through faction interaction
      and NPCs) with join requirements.
- [ ] Path detail view: rank progression tree showing all ranks with their
      titles and perks. "Rank 3: Lieutenant → unlocks: Compact Shipyard
      Access, 10% Military Component Discount." Grayed-out future ranks
      with previews of what's coming.
- [ ] Prestige tracker: total prestige score across all paths. Prestige
      affects: NPC reactions ("I've heard of you — you're the one who
      mapped the Kessel Expanse"), faction diplomacy (high-prestige
      players get audience with faction leaders), and exclusive encounters
      (scripted encounters gated by prestige).
- [ ] Path switching UI: shows what you lose, what you gain, and the
      standing cost. "Leaving Compact Navy will demote you to Reserve
      status. You will lose access to Compact Shipyards. Your Compact
      reputation will drop by 30. Are you sure?" Confirmation dialog.

### 4. Crew career progression

- [ ] Crew members have their own mini-careers. Their career progress is
      tied to contract evaluations. "Boris has successfully repaired the
      ship under fire 47 times. His Engineering career advances to
      Senior Engineer. Perk: 15% faster repair speed."
- [ ] Crew perks affect ship systems: faster repair, better sensor readings,
      higher trade prices, combat bonuses. The ship gets better because
      the crew gets better.
- [ ] Crew career visibility: the crew roster shows each member's career
      path and rank. Part of their character sheet. "Tib: Independent
      Trader, Rank 4 — Master Negotiator. Perk: 10% better sell prices."

### 5. Career-tied content gating

- [ ] Scripted encounters (S41) can require career ranks as prerequisites.
      "Must be Compact Military Rank 5+ to receive 'alexanders_gambit'."
- [ ] Stations can have career-gated areas. "Compact Officer's Lounge —
      requires Compact Military Rank 3+." Accessed through the interaction
      system.
- [ ] Mission board (S46) filters missions by career path. Military path
      players see more combat missions. Explorer path players see more
      survey missions.
- [ ] Ship components and upgrades (S45) can be career-gated. "Predecessor
      Drive Core — requires Explorer Rank 6."

### 6. Metrics collection

- [ ] `career_progression_events` table: path joins, rank advancements,
      path leaves, prestige milestones, perk unlocks.
- [ ] Research questions: what career paths have the highest advancement
      rate? What criterion types are most/least grindy? Do players with
      active career paths have longer retention? What percentage of
      players join vs stay freelance? What paths are most commonly
      paired (Explorer + Science)?

## Acceptance gates

```
cargo test -p reachlock-core career::
# progression recording, rank advancement, path joining/leaving,
# conflict detection, perk unlocking
reachlock content validate content/careers/*.ron
make check
```

Manual: join the Compact Navy → complete combat missions → rank up to
Lieutenant → unlock shipyard access perk → dock at a Compact shipyard →
verify perk applies → resign commission → verify standing loss and
perk removal → join Explorer path → scan species → rank up in Explorer →
verify both paths show in the career panel.

## Non-goals

- NPC career tracking beyond the player's own crew. Faction leaders and
  station NPCs have reputations (S11) but not career ranks.
- Career leaderboards / competition between players. This is personal
  progression, not a competitive ranking system.
- Career respec / full reset. You can leave a path but you can't erase
  history. The completed path stays on your record.
- Automated career path recommendation. The system doesn't suggest paths;
  players discover them through gameplay.

## Gotchas

- The `ProgressionCriterionType` enum must cover all actions the game can
  produce. Every combat system, trade system, scan system, and mission
  system needs to call `record_progress` with the correct criterion type.
  This is a cross-cutting concern — each system that produces a trackable
  action needs a one-line call. Document the integration point in each
  system's module docs.
- Prestige calculation must be bounded. A player who completes all paths
  has a prestige cap, not infinite. Each path contributes a fixed maximum.
  Path contributions are additive but diminishing (first path = full value,
  second = 50%, third = 25%, etc.).
- Path conflicts are symmetric. If Military conflicts with Criminal, then
  Criminal conflicts with Military. Validate symmetry in the content
  loader. Asymmetric conflicts → validation error.
- Crew career perks apply to ship systems via a modifier stack. Multiple
  crew members with overlapping perks (both have repair speed bonuses)
  should stack additively, not multiplicatively. Document the stacking.
