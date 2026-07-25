# S37 — Captain's Log

**Spec:** §6 (contract evaluation logging), §16 (dialogue UX), §18 (LLM agency) ·
**Wave 8 (LLM gameplay) · Depends on:** S15 (agency), S16 (dialogue), S33 (crew dynamics)

## Outcome

After every session, the LLM reads the deliberation traces, game events, crew
relationship changes, and dilemma outcomes — and writes the captain's log. Not
a bullet list. Not a combat log. A narrative. "Day 147. Docked at Sorrow
Station with three tons of refined ferrite and a crew that won't look at each
other. The Veil escalation reached Aethon. Alexander sent a communiqué. Boris
intercepted it before I did. I don't know what to do with that."

The log is private. It's the player's story of their universe. It's shareable
if the player wants, but the default is: this is for you. The LLM's job is to
find the narrative thread through the chaos of game events. What story is the
universe telling through your crew?

## Context

- Every contract deliberation, co-deliberation, dilemma resolution, faction
  interaction, crisis event, jump transit, combat outcome, and crew
  relationship change is already logged. The raw data exists. This sprint
  reads it and produces narrative.
- This is NOT a summary algorithm. It's an LLM call. The log entry is a
  creative act. The LLM has access to the raw event data, relationship
  trajectories, and previous log entries.
- The research surface: how do LLM-written narratives compare to player-written
  accounts? What details does the LLM surface? How coherent are multi-session
  narratives? Does the log improve player retention?
- Players create their own characters. The log is about THEIR crew, THEIR
  choices, THEIR universe. The Loup-Garou demo has its own log entries
  for the tutorial sessions — after that, it's the player's story.

## Freeze first

### Log entry structure (`agency/log.rs`)

```rust
pub struct LogSession {
    pub session_id: String,
    pub start_tick: u64,
    pub end_tick: u64,
    pub raw_events: Vec<LoggableEvent>,
    pub relationship_changes: Vec<RelationshipDelta>,
    pub crew_mood_snapshot: CrewMoodSnapshot,
    pub key_moments: Vec<LogMoment>,
    pub previous_entry_summary: Option<String>,
    pub generated_entry: Option<LogEntry>,
}

pub struct LogEntry {
    pub session_id: String,
    pub title: String,
    pub narrative: String,
    pub narrator_voice: NarratorVoice,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub model_used: String,
    pub approved: bool,
}

pub enum NarratorVoice {
    Captain,              // "I docked at Sorrow Station."
    ShipLog,              // "The ship docked at Sorrow Station."
    CrewMember(String),   // "Boris's log: The captain docked."
    Omniscient,           // "A ship docked at Sorrow Station."
}

pub struct LogMoment {
    pub tick: u64,
    pub moment_type: LogMomentType,
    pub summary: String,
    pub significance: u8,    // 0-10
}

pub enum LogMomentType {
    CrewDeliberation,
    DilemmaResolved,
    CombatOutcome,
    FactionMilestone,
    CrewMilestone,
    Discovery,
    Loss,
    Triumph,
    PlayerChoice,
}
```

### Log generation request

```rust
pub struct LogGenerationRequest {
    pub session_events: Vec<LoggableEvent>,
    pub relationship_changes: Vec<RelationshipDelta>,
    pub key_moments: Vec<LogMoment>,
    pub previous_entry: Option<String>,
    pub narrator: NarratorVoice,
    pub style_hints: Vec<String>,  // "concise", "dramatic", "personal"
    pub max_words: u32,
}
```

## Deliverables

### 1. Key moment detection (`agency/log.rs`)

- [ ] `detect_key_moments(session_events, relationship_changes) -> Vec<LogMoment>`
      scans the session and identifies significant moments. Pure function
      (no LLM). Rules: any crew deliberation lasting >5s with a non-trivial
      outcome; any dilemma resolution; any faction reputation threshold
      crossing; any crew relationship trust delta exceeding threshold; any
      combat reaching <20% hull; any system discovery; any manual player
      override; any recurring pattern (same crew pair argued 3+ times).
- [ ] `score_significance(session, moment) -> u8` — assigns 0-10 significance
      weighted by crew involvement, consequence persistence, event rarity,
      and relationship change magnitude.
- [ ] Test: feed a session with known events → verify correct key moments
      detected with expected significance scores.

### 2. Log entry generation (`agency/log_generation.rs`)

- [ ] `generate_log_entry(request) -> Result<LogEntry, LogGenError>` — LLM
      call through the existing proxy, same tier as crew deliberation. System
      prompt defines narrator voice and style.
- [ ] Narrative continuity: previous entry summary included in context. LLM
      instructed to maintain continuity — reference ongoing threads,
      acknowledge character arcs, note resolved tensions.
- [ ] Style control: player sets style hints. "Concise" = ~100 words.
      "Detailed" = ~400 words. "Dramatic" = novel-style. "Technical" =
      dry ship's log. "Personal" = emotional first-person.
- [ ] Regeneration: player can regenerate with different style/narrator.
      Old entry kept as draft. Player picks the canonical entry.
- [ ] Offline fallback: if no LLM available, template-generated summary.
      "Session 47: 3 combat, 1 dilemma, morale stable. Key moment: Boris
      and Tove argued about repair priorities." Deterministic text generation
      from the event data — functional but not narrative.

### 3. Log browser UI (`client/src/systems/captains_log.rs`)

- [ ] Accessible from main menu (dedicated button) and ship's onboard
      consoles. Journal UI: scrollable entries, newest first, entry title
      as heading.
- [ ] Entry view: narrative text. Navigation arrows for prev/next.
      "Context" toggle: shows key moments that fed this entry. Transparency
      matters — player sees what the LLM worked from.
- [ ] Session summary: stats block above the narrative. "Systems visited: 3.
      Combat: 5. Dilemmas: 1. Credits: 12,400. Relationship changes: 7."
- [ ] Narrator voice selector: dropdown. Changing narrator regenerates entry.
- [ ] Export: copy to clipboard. Export full log to text file. Share entry
      as formatted snippet (opt-in per entry).

### 4. Shared entries (`server` + `client`)

- [ ] "Share this entry" button, opt-in per entry. Shares narrative text +
      session summary (no PII, no other player names, no undiscovered seeds).
- [ ] Server endpoint for shared entries. Stored in `shared_log_entries`
      table. Community feed — chronological, same pattern as S34's contract
      library browser.
- [ ] Filter by: narrator voice, style, content tags. Sort by: newest,
      longest, "most key moments."
- [ ] No ratings. No comments. Just stories. This is an archive, not a
      social network.
- [ ] Moderation: same as S23 chat and S34 contract stories (length cap,
      rate limit, report mechanism).

### 5. Auto-log on session end

- [ ] On game exit / disconnect: capture session events, detect key moments,
      queue log generation request. Generated asynchronously (player is
      logging off — no wait).
- [ ] On next session start: new entry ready. Log browser shows notification
      dot. Player can read, approve, regenerate, or ignore it.
- [ ] Unapproved entries auto-approve after 7 days (player might not care
      but the research data accumulates).

### 6. Metrics collection

- [ ] `captains_log_events` table: generation frequency, style distribution,
      narrator voice distribution, regeneration rate, share rate.
- [ ] Research questions: what style/narrator combinations get highest share
      rate? How often do players regenerate? Does log reading correlate with
      session frequency (retention)? What event types produce the longest
      entries? What narrator voice is most popular?

## Acceptance gates

```
cargo test -p reachlock-core agency::log::
# key moment detection, significance scoring, template fallback determinism
make check
```

Manual: play a session with crew events → quit → relaunch → log entry ready
→ read it → switch narrator to a crew member → regenerate → share the entry
→ browse community feed → import a public entry into your log.

## Non-goals

- Session replay (full event-by-event playback of a session). That's a
  debug/admin feature. The log is a narrative, not a replay.
- Video/audio log entries. Text only.
- Cross-player collaborative logs (co-authored entries for fleet actions).
  Post-S23 social features.
- Log entry translation/localization. English only for this sprint.

## Gotchas

- The log generation LLM call is a creative generation, not a deliberation.
  It should NOT be rate-limited the same way. Use a separate quota bucket
  for log generation (once per session = negligible rate). If quota is
  exhausted, fall back to the template summary.
- Log entries can contain NPC names, faction names, system names — all
  generated from seeds. These are safe to share. They must NOT contain
  other players' character names (PII/privacy). Scrub other player names
  from the event data before log generation.
- The auto-log trigger on session end needs a graceful shutdown path. Bevy's
  `AppExit` event fires; capture events BEFORE the app fully tears down.
  Register an `OnExit(AppState::InGame)` system that snapshots the session
  events and fires the log generation request.
- Shared entries that reference a system the viewer hasn't discovered yet
  should show the system as "Unknown Region" in the shared view. The data
  is there but the label is hidden — the seed ledger's discovery state
  determines visibility.
