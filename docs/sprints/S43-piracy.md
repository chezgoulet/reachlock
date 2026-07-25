# S43 — Piracy

**Spec:** §22 (combat system), §21 (faction engine) · **Wave 9 (Living Galaxy) ·
Depends on:** S11 (factions), S19 (space combat), S20 (landed combat), S42 (career)

## Outcome

Piracy is a valid, supported life path — not a deviant playstyle the game
tolerates, but a career with its own mechanics, progression, and havens.
Disable ships, board them, seize cargo or the ship itself. Smuggle contraband
through faction blockades. Build a reputation in the criminal underworld.
Factions issue bounties; bounty hunters track you. Pirate stations in the
Reach offer safe harbor, black market upgrades, and fence services. The
consequences are real — high notoriety means kill-on-sight in lawful space —
but the rewards match the risk. Piracy is a choice the game respects.

## Context

- S19's subsystem targeting already lets players disable specific ship systems.
  S20 provides landed/onboard combat for boarding actions. S11 provides
  faction reputation tracking. S42 provides the Criminal career track. This
  sprint wires them together and adds the piracy-specific mechanics.
- The key design principle: piracy is HIGH RISK, HIGH REWARD. Disabling a
  trader and seizing 50 tons of refined ferrite should feel like a heist.
  Getting caught by a Compact patrol with a hold full of contraband should
  feel like a disaster. Both outcomes are valid gameplay.
- Pirate havens in the Reach are the counterbalance. Without them, pirates
  have nowhere to dock, repair, sell, or recruit. With them, piracy has an
  ecosystem — stations that WANT your stolen goods, crew who admire your
  notoriety, upgrades you can't get anywhere else.

## Freeze first

### Piracy state (`career/piracy.rs`)

```rust
pub struct PiracyState {
    pub notoriety: PiracyNotoriety,
    pub active_bounties: Vec<Bounty>,
    pub contraband_knowledge: Vec<ContrabandRoute>,
    pub ships_captured: u32,
    pub cargo_seized_value: u64,
    pub pirate_reputation: HashMap<String, i64>,  // per-pirate-faction reputation
    pub current_havens_known: Vec<String>,        // station IDs of discovered havens
}

pub struct PiracyNotoriety {
    pub level: NotorietyLevel,
    pub value: u64,                      // 0 to u64::MAX — accumulated notoriety
    pub decay_per_tick: u64,            // notoriety slowly decays over time
    pub last_crime_tick: u64,
}

pub enum NotorietyLevel {
    Clean,           // no record
    Suspicious,      // minor offenses — smugglers
    Wanted,          // active bounty in one faction
    Hunted,          // bounties in multiple factions
    Infamous,        // known across the galaxy — kill-on-sight in lawful space
}

pub struct Bounty {
    pub bounty_id: String,
    pub issuer_faction: String,
    pub amount: u64,
    pub crime: String,                   // "destruction of Compact freighter 'Venture'"
    pub issued_at_tick: u64,
    pub expires_at_tick: Option<u64>,
    pub dead_or_alive: bool,             // alive = bonus
    pub claimed_by: Option<String>,      // bounty hunter ID
}

pub struct ContrabandRoute {
    pub good_id: String,
    pub source_faction: String,          // where it's legal/produced
    pub destination_faction: String,     // where it's illegal/in demand
    pub price_multiplier: f64,           // how much more it's worth smuggled
    pub risk_level: u8,                  // 1-10 — patrol density
}
```

### Ship capture mechanics

```rust
pub struct BoardingAction {
    pub target_ship_id: String,
    pub target_ship_class: HullClass,
    pub target_hull_state: CombatVessel, // from S19
    pub defender_crew_count: u32,
    pub defender_crew_quality: u8,       // 1-10
    pub breach_point: SubsystemKind,     // which system was disabled for boarding
    pub resistance_level: u8,            // 0 = surrendered, 10 = fight to the death
}

pub enum BoardingResult {
    Surrender { demands_met: Vec<String> },
    CrewFight { outcome: CombatOutcome },
    Scuttled,                            // enemy self-destructed
    Escaped,                             // enemy escaped during boarding prep
}
```

## Deliverables

### 1. Ship disabling and capture (`client/src/systems/piracy.rs` + core)

- [ ] Boarding preparation: after disabling a target ship's engines AND
      weapons via subsystem targeting (S19), the player can initiate
      boarding. Approach within 50 units, match velocity, and trigger
      "Board" from the interaction system. A 3-second boarding prep
      timer runs (grappling, cutting through hull).
- [ ] During boarding prep: the target can self-destruct (high crew morale +
      military ship), surrender (low hull + civilian ship + offered terms),
      or fight (anything else). Surrender conditions: "Drop cargo and we
      let you live" = target releases some cargo. "Surrender the ship" =
      very low probability unless crew morale is broken.
- [ ] Crew fight: transitions to OnBoard mode (S20) on the target ship's
      interior. Player + selected boarding crew vs defender crew. If player
      wins: ship is captured, cargo is lootable, crew can be taken prisoner
      or released.
- [ ] Ship capture: captured ship becomes player property. Can be added to a
      personal fleet (fly it, crew it) or sold at a pirate haven. Sale
      value = hull value × condition. A captured Compact destroyer is worth
      a fortune — IF you can get it to a haven without being scanned.
- [ ] Cargo seizure: after boarding, the cargo transfer UI shows target
      ship's manifest. Player selects what to take. Transfer time = total
      tonnage / cargo transfer rate. Player can abandon the transfer if a
      patrol arrives.
- [ ] Escape: after a successful raid, the player jumps out. If in lawful
      space, patrols respond to distress calls. Response time = system
      security level. High-security systems: 30 seconds. Reach: never.

### 2. Notoriety system (`core/src/career/piracy.rs`)

- [ ] `record_crime(player_id, crime_type, faction, severity) -> PiracyState` —
      records a criminal act. Severity: minor (smuggling), moderate
      (cargo theft), major (ship destruction), capital (murder of faction
      personnel). Notoriety increases by severity.
- [ ] Notoriety decay: notoriety decays slowly over time (1% per hour of play).
      Faster if the player lays low (stays docked, doesn't commit crimes).
      Decay stops at the threshold of the next `NotorietyLevel` — you can't
      decay from Wanted to Suspicious without actively working it off.
- [ ] Notoriety effects: Clean → no effect. Suspicious → lawful stations
      scan your cargo (chance of contraband detection). Wanted → denied
      docking in that faction's stations. Hunted → patrols actively
      intercept you. Infamous → kill-on-sight in all lawful space. Bounty
      hunters spawn as encounters.
- [ ] Bounty issuance: when notoriety crosses a faction-specific threshold,
      that faction issues a bounty. Bounty amount = notoriety × faction's
      bounty multiplier. Bounties are visible to bounty hunter NPCs and
      other players (S23 MMO presence).
- [ ] Bounty collection: bounty hunters (NPCs with combat contracts) spawn
      in systems where the player is Wanted or higher. Frequency increases
      with notoriety level. Bounty hunters are tough — they're tracking YOU,
      not random encounters.

### 3. Contraband and smuggling

- [ ] Contraband goods: certain S44 luxury/weapon/tech goods are marked as
      contraband in specific faction territories. "Compact law prohibits
      Predecessor technology." "ISC tariffs make luxury goods contraband
      if untaxed."
- [ ] Hidden cargo compartments: a ship upgrade (S45) that conceals cargo
      from scans. Base model hides 20% of capacity. Upgraded versions hide
      more. Without hidden compartments, scanned contraband is detected
      and confiscated + notoriety gain.
- [ ] Smuggling routes: discovered through pirate NPCs, black market contacts,
      or analyzing trade data. A route tells you: "Buy X on Station A
      (legal), sell X on Station B (contraband, 3x markup, high patrol
      density)." Routes expire as faction enforcement changes.
- [ ] Scan evasion: when entering a lawful station while carrying contraband,
      a scan check runs. Probability of detection = (contraband volume /
      total cargo) × (1 - hidden compartment pct) × faction security level.
      Detected → contraband confiscated, notoriety gained, denied docking.

### 4. Pirate havens

- [ ] 3-5 authored pirate stations in Reach space (S07 station authoring).
      Each has: a pirate faction controller, a black market vendor, a fence
      (buys stolen goods at 60% value), a shipyard (installs criminal-grade
      upgrades), a bounty board (pirate missions), and a cantina (recruit
      criminal crew).
- [ ] Haven docking: no questions asked. No contraband scanned. No notoriety
      check. Pirates are welcome. Non-pirates can dock but pay a "docking
      fee" (protection money). Pirate crew members lower the fee.
- [ ] Haven services: black market upgrades (S45) not available at lawful
      stations. Stolen cargo sold for 60% value (lawful stations buy at
      30% or refuse). Bounty board missions (S46): piracy-specific missions
      — "Raid this convoy," "Liberate this pirate from Compact prison,"
      "Destroy this bounty hunter ship."
- [ ] Haven reputation: separate from faction reputation. Rising reputation
      unlocks: better prices at the fence, access to more dangerous pirate
      missions, introduction to other havens, criminal crew recruits.

### 5. Bounty hunter gameplay (counter-piracy)

- [ ] Players can become bounty hunters (Criminal career track → Bounty Hunter
      path). Track wanted players (S23 presence + S43 bounties). Intercept
      them. Disable and capture. Return to the issuing faction for bounty
      payout (credits + reputation).
- [ ] Bounty hunter tools: tracking scanner (detects high-notoriety players
      in the same system), restraint devices (prevent captured player from
      fighting during transport), bounty claim system (server verifies).
- [ ] PvP bounty hunting: the first direct PvP gameplay. Both players are
      in the same system. The hunter engages. Combat resolves through
      normal space combat (S19) + boarding (S20). Winner claims the bounty
      or escapes. Loser faces the consequences.

### 6. Metrics collection

- [ ] `piracy_events` table: crimes committed, ships captured, cargo seized
      value, notoriety progression, bounty issuances, bounty claims.
- [ ] Research questions: what percentage of players engage in piracy? What
      notoriety level do pirates stabilize at? Does piracy increase or
      decrease session length? What's the average pirate career duration?
      Do bounty hunters effectively police pirates?

## Acceptance gates

```
cargo test -p reachlock-core career::piracy::
# notoriety accumulation/decay, bounty issuance, contraband detection
make check
```

Manual: commit a crime in Compact space → notoriety increases → try docking
at Compact station → denied → travel to Reach → find a pirate haven → dock
successfully → sell stolen cargo → buy black market upgrade → take a pirate
mission → bounty hunters appear → evade or fight them.

## Non-goals

- Player-run pirate factions / guilds. Future MMO social feature.
- Piracy against other players' stations/colonies. Colonization is Phase 3+.
- Pirate "base building" or territory control. You're a ship-based pirate,
  not a warlord.
- Full prison system for captured players. Captured pirates lose their ship
  and some cargo, respawn at a haven at a cost. Detailed prison mechanics
  are Phase 4.
- NPC pirates as a player faction you can lead. NPC pirates are encounters.
  Player pirates are independent.

## Gotchas

- The boarding transition from space combat (S19) to onboard combat (S20)
  must be seamless. The target ship's interior layout is generated from
  its hull class + seed. If the target is an NPC ship, its interior is
  generated on demand from the NPC's seed. The boarding scene must be
  ready within 2 seconds (pre-generate during boarding prep timer).
- Notoriety decay versus player activity: a player who logs off for a week
  should return to the same notoriety level. Decay only happens during
  active play. Time-off is not a get-out-of-jail-free card.
- Bounty hunters must scale with the player's combat capability. A
  Infamous pirate in a corvette should face corvette-class hunters, not
  shuttles. The S19 threat system (combat difficulty from threat level)
  should scale bounty hunter spawns.
- PvP bounty hunting requires server authority for the bounty claim.
  The server verifies: was the target player genuinely Wanted by the
  issuer faction at the time of capture? Did combat actually occur?
  Simple server-side checks prevent fraudulent claims.
