# S80 — Crew as an Open-World System

**Spec:** New (crew recruitment, retention, relationships, loss) ·
**Wave D (character & open world) · Depends on:** S78 (creation flow — ship & crew step), S75 (player soul/identity)

**Closes:** X9 (crew is a fixed party — no recruit, hire, injure, lose, or death)

## Outcome

Crew is no longer a fixed party of six named lore characters. The `CrewRole` enum becomes open (data-driven or replaced by a `role: String` field), and a `CrewRoster` resource supports adding, removing, injuring, and losing crew members. New crew are recruited at stations through a hiring UI that surfaces procedurally generated souls (`generate_soul`) or authored soul files. Crew members form relationships with the player's soul — these persist across sessions, compress via `RelationshipMemory`, and affect trust deltas in co-deliberation (`contract/co_deliberation.rs`). Crew can refuse orders, leave, or mutiny when breaking points in the soul runtime are triggered by the player's choices. The full loop: recruit a procedurally generated crew member → build trust through co-deliberation → hit a breaking point → lose them.

## Context

- **Crew is a fixed party (X9).** `CrewRole` is a closed 5-variant enum. `CrewRoster::default_crew()` inserts six named members unconditionally (S77 made it data-driven, but the concept of "add/remove crew" doesn't exist). There is no hire, fire, recruit, injure, or death path. Duty rooms map to lore spaces on the authored ship.
- **S75 gave the player a soul.** The player character is a `SoulFile` with `relationship_graph`, `RelationshipMemory`, breaking points, and secrets. NPCs can form persistent, compressing relationships *with the player*. This sprint uses that machinery.
- **Co-deliberation exists** (`contract/co_deliberation.rs`). Trust deltas are modelled. The player can interject at a relationship cost. Crew members have moods and histories. This sprint wires crew members *as participants* in deliberation — they're not just names on a roster, they're emotional entities whose trust in the player evolves.
- **Soul runtime has breaking points** (`soul/runtime.rs`). The data model supports "I will leave if you do X" thresholds. This sprint surfaces them: crew members can issue ultimatums, refuse orders, abandon ship, or mutiny when a breaking point is crossed.
- **`generate_soul(seed, species)`** (`generator/soul.rs:128`) produces entire personalities — the raw material for procedurally generated crew. Every station visit can surface unique recruitable souls.
- **S78's origin step** assigns starting crew via `CrewAssignment`. This sprint makes crew *dynamic* — you can recruit beyond the starting package, and you can lose crew members the origin gave you.
- **Offline-first:** recruitment, relationships, breaking points, and trust all work with no server. Online adds crew sync for multiplayer (future), but the system is local.

## Freeze first

### New `CrewRole` design — data-driven

Replace the closed enum with a string-based role system. Crew roles become data, not variants.

```rust
// reachlock-client/src/systems/crew.rs
/// A role a crew member fills on the ship. Data-driven — defined by the
/// ship's interior (duty stations have expected roles) and authored content
/// (origins assign starting roles).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CrewRole {
    pub id: String,    // "pilot", "engineer", "doctor", "marine", "scientist", …
    pub name: String,  // "Pilot", "Chief Engineer", "Ship's Doctor", …
    pub description: String,
    /// Duty station entity id on the ship interior this role is assigned to.
    /// None = roving / unassigned.
    pub duty_station: Option<String>,
}

/// The old closed enum is removed. Roles are looked up by string id from
/// a CrewRoleRegistry resource populated by authored content and ship
/// interior definitions.
#[derive(Resource, Default)]
pub struct CrewRoleRegistry {
    pub roles: HashMap<String, CrewRole>,
}
```

Migration: all existing `CrewRole::Pilot` → `CrewRole { id: "pilot", … }`. The old enum's 5 variants become the 5 default roles in `CrewRoleRegistry`.

### Crew recruitment flow type

```rust
/// A recruitable crew member encountered at a station.
#[derive(Clone)]
pub struct RecruitableCrew {
    /// The procedurally generated or authored soul.
    pub soul: SoulFile,
    /// The role they're willing to take.
    pub preferred_role: String,
    /// Salary demand per pay period (in credits).
    pub salary_demand: u64,
    /// A short hook for the hiring UI ("Former Compact engineer, looking
    /// for a quiet berth.")
    pub hook: String,
    /// How they were generated (for persistence — if recruited, the seed
    /// or authored id is stored).
    pub source: CrewSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CrewSource {
    /// Procedural: seed + species regenerate the soul.
    Procedural { seed: Seed, species: String },
    /// Authored: a soul file id in the content index.
    Authored { soul_id: String },
}
```

### Crew → player relationship model

```rust
// reachlock-client/src/systems/crew.rs
/// A crew member on the player's roster.
#[derive(Clone)]
pub struct CrewMember {
    pub id: String,
    pub soul: SoulFile,
    pub role: CrewRole,
    pub salary: u64,
    /// Ticks since last paid. If this exceeds the crew member's patience,
    /// they demand payment or leave.
    pub unpaid_ticks: u64,
    /// Current health state.
    pub health: CrewHealth,
    /// Breaking point thresholds that are currently active. Mirrors
    /// the soul's breaking points but tracks per-crew-member state.
    pub active_breaking_points: Vec<BreakingPointState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrewHealth {
    Healthy,
    Injured,    // reduced duty efficiency
    Critical,   // incapacitated, needs medical care
    Dead,
}

/// Tracks a single breaking point's state.
#[derive(Debug, Clone)]
pub struct BreakingPointState {
    pub condition: String,     // "player_kills_civilian", "player_abandons_crew", …
    pub threshold: u32,        // How many violations before triggering
    pub current: u32,          // Current count
    pub triggered: bool,       // Has this already fired?
    pub consequence: BreakingPointConsequence,
}

#[derive(Debug, Clone)]
pub enum BreakingPointConsequence {
    /// Crew member voices disapproval (trust delta, no loss).
    Warning,
    /// Crew member refuses a specific order.
    RefuseOrder,
    /// Crew member leaves the ship at next station.
    LeaveAtStation,
    /// Crew member abandons ship immediately (in a lifepod or EVA).
    AbandonShip,
    /// Crew member attempts to take control (mutiny).
    Mutiny,
}
```

## Deliverables

### 1. `CrewRole` — closed enum → data-driven (`crew.rs`, `crew_role_registry.rs`)

- [ ] Define `CrewRole` struct with `id`, `name`, `description`, `duty_station: Option<String>`.
- [ ] Define `CrewRoleRegistry`: `HashMap<String, CrewRole>` resource.
- [ ] Populate with 5 default roles: pilot, engineer, doctor, marine, scientist.
- [ ] Remove the old 5-variant `CrewRole` enum. Update all `match` sites to use role id strings.
- [ ] Duty station assignment: ship interior definitions reference role ids. `deck_of()` / `deck_zero_g()` (S77 now uses live `ShipInterior`) queries duty stations by expected role string.
- [ ] Add a content consumer for role definitions (optional — roles can also be hardcoded defaults for now, but the system is data-ready).
- [ ] Test: `CrewRoleRegistry` has 5 default roles. A `CrewRole` with id `"pilot"` matches the ship interior's pilot station.

**Gate:** `cargo test -p reachlock-client crew::role_registry_defaults`. No closed enum remains in the codebase.

### 2. `CrewRoster` refactor — add/remove support (`crew.rs`)

- [ ] Refactor `CrewRoster` from a fixed `Vec<CrewMember>` (or `[CrewMember; 6]`) to a dynamic `HashMap<String, CrewMember>` keyed by crew member id.
- [ ] Add methods:
      ```rust
      impl CrewRoster {
          pub fn add(&mut self, member: CrewMember);
          pub fn remove(&mut self, id: &str) -> Option<CrewMember>;
          pub fn get(&self, id: &str) -> Option<&CrewMember>;
          pub fn get_mut(&mut self, id: &str) -> Option<&mut CrewMember>;
          pub fn iter(&self) -> impl Iterator<Item = &CrewMember>;
          pub fn count(&self) -> usize;
          pub fn by_role(&self, role_id: &str) -> Vec<&CrewMember>;
          pub fn set_health(&mut self, id: &str, health: CrewHealth);
          /// Total salary demand per pay period.
          pub fn total_salary(&self) -> u64;
      }
      ```
- [ ] Remove `default_crew()` — S77 made crew data-driven. The Loup-Garou veteran origin provides its 6 crew as `CrewAssignment` entries. On game start (S78 Launch), these are loaded into the roster.
- [ ] Persistence: `SaveFile` includes a `crew_roster: Vec<CrewMember>` field (or equivalent) for save/reload.
- [ ] Save/reload test: add crew → save → reload → roster matches.

**Gate:** `cargo test -p reachlock-client crew::roster_add_remove_save_cycle`.

### 3. Procedural crew generation (`generate_soul` integration)

- [ ] At station interaction points (docking, bar, hiring hall), generate a pool of `RecruitableCrew` entries:
      ```rust
      fn generate_recruitable_crew(
          station_souls: &[SoulFile],  // authored NPCs at this station
          station_seed: Seed,
          local_species: &[String],
      ) -> Vec<RecruitableCrew> {
          // 1. Some fraction use authored souls from the station's population
          // 2. Some fraction use generate_soul(seed, species) for each
          //    species present on the station
          // 3. Each gets a preferred role weighted by their personality
          //    (e.g., high discipline → marine, high curiosity → scientist)
          // 4. Salary demand based on their skills and the local economy
      }
      ```
- [ ] Each generated crew member has a `hook` line drawn from their personality/backstory (e.g., "Disgraced scientist, looking for data"). Use the soul's `backstory` field or generate a short hook from traits.
- [ ] Station influence: high-tech stations generate more engineers/scientists; military stations generate more marines; trade hubs generate more pilots.
- [ ] Test: generate 10 recruitable crew from a known seed → assert each has a non-empty soul, valid role, positive salary. Same seed → same 10 crew (deterministic).

**Gate:** `cargo test -p reachlock-client crew::recruitable_generation_deterministic`.

### 4. Hiring UI at stations (`interaction.rs`)

- [ ] At docking, if the station has a hiring hall or bar, surface a "Hire Crew" option (interaction prompt per S70).
- [ ] Recruitment panel shows a scrollable list of `RecruitableCrew` entries. Each entry shows:
  - Character portrait (species-specific generated sprite from `CharacterLookConfig`)
  - Name, species, preferred role
  - Salary demand
  - Hook line
  - A personality summary (2-3 tags: "Brave, Loyal, Curious")
- [ ] Selecting a crew member opens a detail view: full personality description, skills, breaking points (summarised as "Won't stand for: piracy, abandoning crew"), current mood.
- [ ] **Hire** button: deducts first month's salary from credits, adds `CrewMember` to `CrewRoster`. If insufficient credits, button is disabled with "Insufficient funds" tooltip.
- [ ] **Dismiss** button on the detail view: removes from the recruitable pool (they're not interested if rejected once).
- [ ] Keyboard/gamepad per S70: ↑/↓ navigate list, Enter opens detail, H hires, D dismisses, Esc closes.
- [ ] **Randomize** button on the list: rerolls the recruitable pool at this station (uses a different seed offset). Only available if the player has a reason to wait (e.g., "Wait for new arrivals" — costs time, advances the universe tick).

**Gate:** Dock at a station → "Hire Crew" appears → list shows 3-5 recruitable NPCs → hire one → crew appears on roster with correct role. Credits deducted.

### 5. Crew relationships with the player's soul (`compression.rs`, `co_deliberation.rs`)

- [ ] When a crew member is hired, create an entry in the player's `SoulFile.relationship_graph` for that crew member's soul id. Initial trust: neutral (or slightly positive for voluntary hires, slightly negative for forced assignments).
- [ ] `RelationshipMemory` compression runs on the crew ↔ player relationship (same as NPC ↔ NPC). Trust deltas from co-deliberation (`contract/co_deliberation.rs`) affect this relationship.
- [ ] Crew members have moods that change based on:
  - Recent co-deliberation outcomes (were they listened to?)
  - Pay status (unpaid crew get progressively unhappier)
  - Health status (injured crew are unhappy)
  - Breaking point proximity (nearing a threshold → tense)
- [ ] Mood affects crew performance: unhappy crew give worse skill checks, refuse orders more often, and are more likely to trigger breaking points.
- [ ] The player can interact with crew members between missions (a "talk to crew" option on the ship) that triggers a mini-dialogue and a small positive trust delta — the relationship-building loop.
- [ ] Persistence: relationship state compresses and saves as part of `SoulFile` (existing `RelationshipMemory` machinery from S35).

**Gate:** Hire a crew member → player's soul has a relationship entry for them. Run a deliberation with the crew member → trust delta applied. Save → reload → trust level persists.

### 6. Breaking points — crew can refuse, leave, or mutiny (`runtime.rs`, `crew.rs`)

- [ ] Every crew member has active breaking points derived from their soul's `breaking_points`. Each breaking point has a condition, threshold, current count, and consequence.
- [ ] Breaking point triggers are checked when the player takes an action that matches a condition. For example:
  - `player_kills_civilian` — after combat, check crew who have "Won't tolerate harming innocents"
  - `player_abandons_crew` — after jumping away with a crew member left behind
  - `player_breaks_contract` — after violating a contract term
  - `player_unpaid_crew` — triggers when `unpaid_ticks > patience`
- [ ] Consequences escalate:
  1. **Warning:** crew member voices disapproval in a dialogue popup. Trust delta negative. Player has a chance to apologise / explain (small trust recovery).
  2. **Refuse order:** the crew member refuses a specific command (e.g., "I won't fire on that station"). The player can override (trust penalty) or accept.
  3. **Leave at station:** next time the ship docks, the crew member disembarks and is removed from the roster. A log entry explains their departure.
  4. **Abandon ship:** the crew member takes a lifepod immediately. Emergency. Roster loss + potential reputation hit.
  5. **Mutiny:** the crew member attempts to take control. Resolved as a skill contest (crew's will vs captain's authority). Mutiny failure: crew member is subdued (can be dismissed or jailed). Mutiny success: player loses control (game over or forced to negotiate).
- [ ] Breaking points are visible in the crew detail view (as "Hard lines: …") so the player knows what will cause a crew member to leave.
- [ ] Co-deliberation trust can heal breaking point counters: high trust crew are more forgiving (threshold effectively increases with trust level).

**Gate:** Hire a crew member with `player_kills_civilian` breaking point. Attack a civilian ship → crew member issues warning. Attack again → crew member leaves at next station. Full loop: recruit → build trust → hit breaking point → lose them.

### 7. Salary system

- [ ] Each crew member has a salary demand and an `unpaid_ticks` counter.
- [ ] A pay period fires every N ticks (configurable, default 1000 ticks ≈ ~10 minutes gameplay).
- [ ] When pay period fires: deduct each crew member's salary from player credits. If insufficient funds, crew members go unpaid (`unpaid_ticks` increments).
- [ ] Unpaid crew: each missed payment increases unhappiness and proximity to a `player_unpaid_crew` breaking point (or a general "unpaid" morale mechanic).
- [ ] Crew with `unpaid_ticks > patience_threshold` demand payment: a dialogue popup "I haven't been paid in [N] cycles. Pay up or I walk."
- [ ] Player can pay immediately (clears debt, restores mood) or refuse (crew leaves or mutinies).
- [ ] UI: crew list in the ship shows pay status (green = paid, yellow = due, red = overdue).

**Gate:** Hire a crew member → wait for pay period → credits deducted. Let pay lapse → crew demands payment. Refuse → crew leaves.

### 8. Crew injury and death

- [ ] Combat, accidents, and environmental hazards can injure or kill crew members.
- [ ] `CrewHealth::Injured`: reduced duty efficiency (skill checks at disadvantage). Heals over time (with medical facilities on ship) or with medical supply consumption.
- [ ] `CrewHealth::Critical`: incapacitated. Cannot perform duties. Requires immediate medical attention (station medbay or ship's doctor). If untreated, degrades to Dead.
- [ ] `CrewHealth::Dead`: crew member is removed from the roster. A log entry records their death. Their soul's relationship with the player ends (or enters a "deceased" state with permanent emotional weight).
- [ ] On crew death: surviving crew may react (breaking point triggers if they were close with the deceased).
- [ ] UI: crew health shown in crew list (icon + status text). Medical bay on ship can treat injuries (consumes medical supplies).

**Gate:** Enter combat → crew member injured → health status changes. Dock at station with medbay → crew healed. Crew member dies → removed from roster, log entry created.

### 9. Crew positioning on interior (`interior.rs`)

- [ ] `deck_of()` / `deck_zero_g()` (S77 now uses live ship interior) assigns crew to duty stations by role id.
- [ ] When a crew member is hired, they are assigned to an unoccupied duty station matching their role. If no station matches, they are "roving" (no station assignment).
- [ ] When a crew member leaves or dies, their duty station becomes vacant.
- [ ] Crew positions are rendered on the interior map (small character sprites at their duty station, or a status panel listing locations).
- [ ] Player can reassign crew to different stations (e.g., move the engineer to security during combat) via the interior interaction UI.

**Gate:** Open interior view → hired crew member appears at their duty station. Reassign → sprite moves. Crew member leaves → station vacant.

## Acceptance gates

```
# Role registry
cargo test -p reachlock-client crew::role_registry_defaults

# Roster add/remove/save
cargo test -p reachlock-client crew::roster_add_remove_save_cycle

# Procedural generation
cargo test -p reachlock-client crew::recruitable_generation_deterministic

# Relationship integration
cargo test -p reachlock-client crew::relationship_on_hire
cargo test -p reachlock-client crew::trust_delta_from_deliberation

# Breaking points
cargo test -p reachlock-client crew::breaking_point_warning
cargo test -p reachlock-client crew::breaking_point_leave
cargo test -p reachlock-client crew::breaking_point_mutiny

# Salary
cargo test -p reachlock-client crew::salary_deduction
cargo test -p reachlock-client crew::unpaid_crew_demands_payment

# Injury/death
cargo test -p reachlock-client crew::injury_healing_cycle
cargo test -p reachlock-client crew::death_removes_from_roster

make check
```

Manual:
1. Dock at a station with a hiring hall → "Hire Crew" → browse recruitable NPCs → check personality and hard lines → hire → crew appears on roster, assigned to a duty station
2. Open ship interior view → crew member visible at station
3. Trigger a breaking point (e.g., attack civilian ship with a principled crew member) → crew member voices warning in dialogue
4. Trigger the same breaking point again → crew member leaves at next station → roster updated, log entry created
5. Enter combat → crew member injured → health status changes → bring to station medbay → healed
6. Let salary lapse → crew demands payment → pay → mood restores. Let it lapse again → refuse → crew leaves
7. Save → reload → all crew members, relationships, health, pay status intact

## Non-goals

- Multiplayer crew sync (remote players see each other's crew — deferred to Wave E)
- Full crew skill system (skills exist as soul traits but no skill-check minigame — the co-deliberation system uses trust, not skill rolls)
- Crew promotion / rank progression (career progression handles the player; crew progression is a future sprint)
- Funeral / memorial UI for dead crew (log entry exists; visual ceremony is future)
- Pet / non-humanoid crew (the species system handles non-human; non-sapient crew are future)
- Crew cosmetic customization (uniform editor, appearance customization beyond what `generate_soul` provides)
- Crew quarters customization (duty stations are functional, not decorative)
- Rival crew / enemy crew boarding parties (space combat boarding is S19 — can trigger crew injury/death when it ships)

## Gotchas

- `CrewRole` migration from closed enum to data-driven string: every `match crew.role { CrewRole::Pilot => … }` must become `if crew.role.id == "pilot" { … }`. Use a `match_crew_role!` macro or helper function that maps role ids to behaviour, so the 5 default roles are handled exhaustively and unknown roles get a default handler. `#[non_exhaustive]` on the old enum mitigates compile errors but the real change is mechanical — budget for the grep/audit.
- Breaking points reference `SoulFile.breaking_points` which is part of the core soul data model. The trigger conditions (`player_kills_civilian`, etc.) must match what the soul runtime's breaking point system expects. Coordinate with S35 (persistent relationship memory) and S15 (LLM agency) — those sprints defined the breaking point data shape. If the shape has changed, reconcile in this sprint.
- `SaveFile` crew serialization: `CrewMember` contains a full `SoulFile`. The soul serialization is already wire-shape tested (iron rule #4). Ensure the `crew_roster` field is properly embedded in `SaveFile` and serializes as a `Vec<CrewMember>`. Add a save-format migration if the set of crew fields changed.
- `generate_recruitable_crew` must be deterministic for the same station seed + universe tick. The station seed is known; the universe tick adds temporal variation (different crew available over time). Use `seeded_rng(station_seed ^ tick_offset)` so the same station at the same time produces the same recruitable pool. This avoids desync between offline and online modes.
- Hiring UI depends on S70's widget kit (scrollable list, buttons, text fields). If S70 hasn't shipped, build a minimal text-based hiring list (following the existing `if ui.open { return }` pattern) that can be migrated to widgets later. Document in the PR: "S70-dependent — list is text-based until S70's scrollable list widget is available."
- `CrewPositioningSystem` (interior.rs) needs the crew roster and the ship interior. The interior is now the live ship (S77 fix). Ensure the system queries `CrewRoster` resource and `ShipInterior` (from the player's current ship), not the old `loup_garou_interior()` hardcode — S77 already fixed this, but verify.
- Mutiny consequence: if the player loses a mutiny, what happens? Options: game over (load last save), player knocked out and wakes up in a lifepod, or player forced to negotiate. For this sprint, implement mutiny as a skill contest. If the player wins, the mutineer is subdued (dismissed or jailed). If the player loses, the ship is taken — game over with a log entry explaining the mutiny's success. The player reloads from last save. Document this as a known limitation: "mutiny loss = game over; full mutiny resolution (negotiation, compromise) is future work."
