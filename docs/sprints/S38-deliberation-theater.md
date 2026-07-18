# S38 — Deliberation Theater

**Spec:** §6 (contract system), §16 (dialogue UX), §18 (LLM agency model) ·
**Wave 8 (LLM gameplay) · Depends on:** S15 (agency), S16 (dialogue), S33 (crew dynamics)

## Outcome

The crew deliberates as a group, and the player watches. When a big decision
faces the ship, the comms panel lights up with every crew member's voice.
Each speaks in turn. Each has access to what the previous speaker said. The
player sees who agrees, who defers, who pushes back. The deliberation IS the
content — the player is the audience, watching their created characters
navigate a situation through the rules the player gave them.

The player can intervene at any moment: "No, we're doing it my way." This is
a manual override that short-circuits the remaining deliberation. The crew
reacts. The relationships shift. The story advances.

This is not crew as automation. This is crew as ensemble cast.

## Context

- S33 (crew dynamics) handles co-deliberation between specific crew pairs
  triggered by events. This sprint handles deliberately-initiated FULL CREW
  deliberation — the "crew meeting" where everyone has a voice.
- The player creates their characters. Their contracts define their voices.
  The theater is where those voices interact. Two characters the same player
  created, with different contracts, can disagree. The player designed the
  tension. The theater plays it out.
- The research surface: what patterns of social influence emerge in LLM-simulated
  group decisions? How does knowing a crewmate's reasoning affect the next
  speaker? Does sequential deliberation produce conformity, polarization,
  or independent reasoning?
- The theater is triggered by: the player calling a crew meeting, a major
  dilemma (S36) affecting the whole crew, or a crisis where no single crew
  member's contract covers the situation.

## Freeze first

### Theater setup (`contract/theater.rs`)

```rust
pub struct DeliberationTheater {
    pub topic: String,                       // what they're deliberating about
    pub trigger: TheaterTrigger,
    pub participants: Vec<TheaterSpeaker>,
    pub turn: usize,
    pub history: Vec<TheaterLine>,
    pub resolution: Option<TheaterResolution>,
    pub player_present: bool,                // is the player watching?
    pub allow_intervention: bool,
}

pub enum TheaterTrigger {
    PlayerCalled { reason: String },
    MajorDilemma { dilemma_id: String },
    ShipCrisis { crisis_type: String },
    MissionChoice { options: Vec<String> },
    CrewIssue { between: Vec<String>, issue: String },
}

pub struct TheaterSpeaker {
    pub crew_id: String,
    pub role: CrewRole,
    pub relationship_to_topic: String,       // why they care about this
    pub speaking_order: u8,
    pub spoke: bool,
    pub position: Option<TheaterPosition>,
}

pub enum TheaterPosition {
    Advocate { position: String, reasoning: String },
    Oppose { to_whom: String, position: String, reasoning: String },
    Amend { to_whom: String, amendment: String },
    Question { to_whom: String, question: String },
    Defer { to_whom: String },
    Recuse { reason: String },
}

pub struct TheaterLine {
    pub speaker: String,
    pub portrait: String,
    pub position: TheaterPosition,
    pub llm_raw: String,
    pub display_text: String,
    pub reactions: Vec<(String, ReactionType)>,  // how other crew reacted
    pub relationship_deltas: Vec<(String, i64)>,
}

pub enum ReactionType {
    Nod,                  // silent agreement
    Frown,                // silent disagreement
    Surprise,             // didn't expect that
    Relief,               // someone said what they were thinking
    Tension,              // this made things worse
    Breakthrough,         // this changed someone's mind
}

pub enum TheaterResolution {
    Consensus { action: String, summary: String },
    MajorityVote { action: String, for_votes: Vec<String>, against: Vec<String> },
    CrewLeadsDecision { leader: String, action: String, dissenters: Vec<String> },
    PlayerDecided { action: String },        // player cut in
    Deadlocked { positions: Vec<String> },   // no resolution — situation escalates
}
```

### Trigger conditions

`fn should_convene_theater(event: &GameEvent, crew: &[SoulFile]) -> Option<TheaterTrigger>`
decides whether an event merits full-crew deliberation instead of individual
contract evaluation.

## Deliverables

### 1. Theater sequencing engine (`core/src/contract/theater.rs`)

- [ ] `DeliberationTheater::step() -> Result<(TheaterLine, Option<TheaterResolution>), TheaterError>`
      — pure state transition. Advances one speaker. Returns their line
      and any resolution that emerges. Works through participants in
      `speaking_order`. Speakers who have already spoken can react to
      later speakers (the `reactions` field updates retroactively).
- [ ] Speaking order: determined by role seniority, relationship to the topic,
      and crew dynamics. The captain-equivalent speaks last (authority).
      The crew member most affected by the topic speaks first (stake).
      S33's relationship state influences order — tense crew pairs are
      separated; trusted pairs are adjacent.
- [ ] Each speaker's LLM call receives: the topic, the trigger, their role,
      their relationship to the topic, the full history of what's been
      said so far, the relationship state with every other speaker, and
      their own contract's predispositions. The system prompt instructs
      them to respond AS that character, in their voice, with their
      personality.
- [ ] Resolution emergence: after each speaker, evaluate whether a resolution
      has emerged. Consensus = all speakers who've taken a position agree.
      Majority = 60%+ agree on an action. CrewLeadsDecision = the senior
      crew member makes a call after hearing everyone. Deadlocked = all
      speakers have spoken, no resolution emerged.
- [ ] Maximum rounds: 2 rounds per speaker. If no resolution after 2 full
      rounds, `Deadlocked`. The player must decide or the situation
      escalates (crisis worsens, dilemma auto-resolves, opportunity passes).
- [ ] Test: set up a 3-person crew, fire a known topic, verify each speaker
      gets the correct context, verify consensus detection, verify deadlock
      after 2 rounds with no agreement.

### 2. Comms panel theater mode (`client/src/systems/comms.rs` expansion)

- [ ] Theater view: the comms panel enters a dedicated theater mode when
      deliberation theater is active. Full-screen panel (or wide overlay)
      with each speaker's portrait, name, role badge, and dialogue area.
- [ ] Speech animation: each line renders with a typewriter effect
      (character-at-a-time, adjustable speed). Between speakers, a brief
      pause (1-2s). The pause IS the drama — the crew is thinking.
- [ ] Reaction overlay: when a speaker says something and another crew
      member reacts (Nod, Frown, Surprise, etc.), the reacting crew
      member's portrait shows a small animation overlay. Silent reactions
      that the player reads as body language.
- [ ] Relationship bar: at the bottom of the theater view, a horizontal bar
      showing each participant's current sentiment. Green bar = positive
      trajectory. Red = negative. Shifts in real time as deliberation
      progresses. The player sees the room temperature changing.
- [ ] Intervention prompt: always visible during theater. "[ENTER] Make a
      decision [ESC] Let them continue." ENTER opens the action picker.
      ESC defers to the next speaker. The player chooses when to step in.
- [ ] After resolution: a summary card. "Consensus: abandon the cargo to
      save fuel. Boris proposed it. Tove agreed. Tib opposed but deferred.
      Crew sentiment: relieved. Relationship changes: Boris gained respect
      from Tove. Tib lost trust in Boris." The card is logged to the ship
      log (S37).

### 3. Theater triggers

- [ ] Player-initiated: from the comms panel or ship console, "Call crew
      meeting" opens a topic picker. Options: current situation, upcoming
      mission, crew interpersonal issue (select the crew pair), faction
      decision, or open topic (player types a prompt). The theater
      convenes with the selected topic.
- [ ] Event-initiated: S36's major dilemmas auto-convene the theater if
      the dilemma affects 3+ crew members. Crises where 3+ crew are
      involved (boarding action, multi-compartment fire, evacuation).
      Mission choices offered by NPCs or the faction system.
- [ ] Crew-initiated: a crew member with high trust + a pending grievance
      can request a crew meeting. "Tove has requested a crew meeting to
      discuss repair priorities." The request appears in the comms panel.
      The player can accept or defer.
- [ ] Frequency: crew-initiated meetings have a cooldown (once per 2 hours
      of play). Player-initiated meetings have no cooldown. Theater is
      always available — the limit is how often the player wants to use it.

### 4. Player intervention dynamics

- [ ] Cutting off deliberation at different points produces different
      relationship consequences. Intervening before anyone has spoken =
      "captain doesn't trust the crew to think" = significant trust loss.
      Intervening after 3 speakers have made good points and there's no
      resolution = "captain made a timely decision" = neutral or slight
      trust gain from the crew who agreed with the decision.
- [ ] The player's decision can reference what was said. "Do what Boris
      proposed" = Boris gains influence. "Tove's concern is valid but
      we're going another way" = Tove's trust changes minimally (player
      acknowledged her). "Ignore everything and do this" = all crew lose
      trust (player didn't listen).
- [ ] Player silence: if the player lets the theater run to its natural
      resolution without intervening, the crew gains trust in each other
      and a small trust in the captain ("the captain trusts us to decide").

### 5. Theater replay & log

- [ ] The full theater transcript is saved to the ship log (S37). Players
      can re-read past crew meetings. The transcript includes every line,
      every reaction, and the resolution.
- [ ] Replay mode: the theater UI can replay a past deliberation at the
      same speed it originally played. This is for the player's own
      enjoyment — "watch what happened in last session's big meeting."
- [ ] The transcript feeds into the captain's log generation (S37). The
      LLM that writes the captain's log sees the full theater transcript
      and can reference specific lines. "Tove was right. Boris knew it.
      Tib wouldn't admit it. We spent 4 minutes arguing about fuel
      reserves while the Kessel blockade tightened."

### 6. Metrics collection

- [ ] `theater_events` table: trigger type, participant count, turn count,
      resolution type, player intervention timing (if any), relationship
      delta distribution, deliberation duration.
- [ ] Research questions: what triggers produce `Deadlocked` most often?
      How does player intervention timing correlate with relationship
      outcomes? Does theater participation reduce individual crew
      deliberation frequency (because issues get resolved in meetings)?
      What crew sizes produce the richest theater dynamics?
- [ ] Social influence tracking: does Speaker 1's position predict
      Speaker 2's position more often when they have high trust? Does
      a dissenting voice early in the round produce more diverse
      opinions, or does it get drowned out?

## Acceptance gates

```
cargo test -p reachlock-core contract::theater::
# step sequencing correct, resolution detection, deadlock after max rounds
make check
```

Manual: call a crew meeting → select "upcoming mission" → watch 4 crew
members deliberate → see Tove advocate for caution, Boris push for aggression
→ Tib defers to Boris → deadlocked after 2 rounds → player decides → summary
card shows relationship changes → ship log records the transcript.

## Non-goals

- Real-time voice acting of theater lines. Text only in the comms panel.
  Voice is S29, and S29 is proximity P2P chat — not theater playback.
- Theater with NPCs who aren't crew (station NPCs, faction leaders). Crew
  theater only. NPC interactions are dialogue (S16).
- Shared theater replays. The transcript is in the ship log and shareable
  via S37 if the player opts in, but there's no dedicated "watch someone
  else's crew meeting" feature.
- Theater-scale voting/parliamentary procedure. Simple consensus/majority/
  leader-decides/deadlock covers the fiction. No Robert's Rules.

## Gotchas

- The theater LLM calls are sequential — each speaker waits for the
  previous speaker's LLM response. 4 crew members × 2-10 seconds per
  call = up to 80 seconds of deliberation. This is SLOW by design. The
  deliberation IS the content. If this feels too slow in testing, reduce
  the default LLM timeout for theater calls (theater is fine with faster,
  shorter responses — it's a conversation, not a tactical decision).
- Speakers must have access to the full history of what's been said.
  The context window for speaker 4 includes speakers 1-3's full
  deliberation output. This means the LLM call gets larger as the round
  progresses. The `DialogueContext` builder (S16) already handles
  size-bounded assembly — theater lines are another source with the
  same bounds.
- Crew who Recuse themselves ("I don't have a position on this") still
  count toward the round. They've had their turn. They can still react
  silently (ReactionType) to later speakers.
- The player-intervention timing scoring ("cut off too early" vs "timely
  decision") is heuristic. Define the thresholds: <2 speakers = too
  early; 2+ speakers + no emerging consensus = timely; after deadlock
  declared = necessary. Test these thresholds.
