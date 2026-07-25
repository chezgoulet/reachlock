# S34 — Contract Crafting

**Spec:** §6 (contract system), §18 (LLM agency model) · **Wave 8 (LLM gameplay) ·
Depends on:** S13 (souls), S15 (agency), S16 (dialogue)

## Outcome

Players write contracts that define their crew's personality. Not for
optimization — for characterization. A contract is character creation through
rules. The system provides a workshop to write, test, tune, and share
contracts, but sharing is about the stories those contracts produce, not their
tactical effectiveness. A contract library browsable by "what stories did this
contract generate?" rather than "what's the DPS?"

This is the mechanic that answers: who ARE your crew? The player creates them.

## Context

- The contract system (S06) and contract types (`contract/types.rs`) are the
  foundation. Contracts have triggers, rules, priorities, and LLM fallback
  config. They're already serializable and shareable.
- The Loup-Garou crew (Boris, Tove, Tib, etc.) are the demo/tutorial —
  pre-authored contracts that teach the system. Players create their own
  crew with their own contracts. The crafting tool is where that creation
  happens.
- The research surface: what contract designs produce narratively interesting
  behavior? What do players PRIORITIZE when they're designing for story
  rather than optimization? What contract patterns emerge across the
  community?
- V1 precedent: the contract builder in the Godot prototype was a text UI.
  This sprint builds the Bevy version with the S06 contract engine as the
  compile target.
- Progression tie-in: a well-crafted contract that produces interesting
  outcomes earns "crew trust" bonuses — the game rewards good character
  design, not just effective automation.

## Freeze first

### Contract metadata for sharing (`contract/metadata.rs`)

```rust
pub struct ContractMetadata {
    pub author: String,                  // player ID
    pub created: chrono::DateTime<chrono::Utc>,
    pub updated: chrono::DateTime<chrono::Utc>,
    pub crew_member_name: String,        // "Boris" — who this contract is for
    pub crew_role: CrewRole,             // Engineer, Medic, Pilot, Tactical, etc.
    pub description: String,             // what this contract is DESIGNED to do
    pub personality_tags: Vec<String>,   // "cautious", "protective", "reckless"
    pub story_tags: Vec<String>,         // what kind of stories this produces
    pub usage_notes: String,             // author's notes on what makes it tick
    pub shareable: bool,                 // can other players see this?
}

pub struct ContractStory {
    pub contract_id: String,
    pub story: String,                   // player-written anecdote
    pub event_type: String,              // what kind of situation triggered it
    pub outcome_type: String,            // "triumph", "disaster", "comedy", "drama"
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

### Contract validation rules (same module)

A contract is valid if it compiles to a `Contract` struct that the engine can
evaluate. But contracts can also have *crafting* issues — detectable patterns
that produce uninteresting behavior:

```rust
pub enum CraftingWarning {
    AlwaysResolvesWithoutLLM,    // deterministic-only — misses the point
    AlwaysRequiresLLM,           // no rules cover any common situation
    AllSamePriority,             // no conflict resolution — rules fight
    NoFallbackBehavior,          // LLM timeout means nothing happens
    OverSpecificTrigger,         // trigger will almost never fire
    CircularRule,                // rule A's action triggers rule B which triggers rule A
}
```

## Deliverables

### 1. Contract workshop UI (`client/src/systems/contract_crafting.rs`)

- [ ] Accessible from the ship's onboard consoles (contracts console, new
      interactable) and from the main menu (design offline, test later).
- [ ] Rule builder: visual table of rules. Each row: trigger dropdown, condition
      builder (S25's shared widget), action picker, priority spinner. Add/remove/
      reorder rows. A "test this rule" button runs the contract engine against
      a sample game state and highlights which rule fired.
- [ ] LLM fallback config: enabled toggle, system prompt text area. Preview:
      given a sample context, what would the LLM see? (Shows the assembled
      prompt without making a call — useful for tuning.)
- [ ] Persona alignment: the system prompt gets a "persona" section auto-filled
      from the crew member's soul file (S13) — speaking style, background,
      role. The player writes the rules; the soul provides the voice.
- [ ] Contract import/export: save to a `.ron` file (same format as the
      content pipeline). Share via text (copy to clipboard) or file. Import
      from clipboard or file. Validate on import.
- [ ] Offline simulation: run the contract against a battery of predefined
      scenarios (combat, crisis, transit, social) and show the outcome
      summary. "In 20 combat scenarios, your contract fired weapons 14 times,
      retreated 4 times, and hit LLM deliberation 2 times. Average
      deliberation time: 3.2s."

### 2. Contract library browser (`client/src/systems/contract_library.rs`)

- [ ] Browse view: grid of contract cards. Each card shows: crew member name,
      role icon, personality tags, story count, author. Sort by: newest, most
      stories, "most interesting" (high deliberation rate + high outcome
      variance), author reputation.
- [ ] NOT sorted by effectiveness. No DPS score. No win rate.
- [ ] Filter by: crew role, personality tag, situation type, contract version
      (game version it was written for).
- [ ] Detail view: full contract rules, author's description, personality
      tags, usage notes. "Stories this contract produced" section — a feed
      of player-submitted anecdotes, newest first. Each anecdote has a
      "this happened to me too" button (aggregation, not voting).
- [ ] Import: one click to add this contract to your library. Preview the
      simulation results against your ship configuration before confirming.
- [ ] Library is local-first (offline). Online syncs with the server's
      contract directory (S23 content distribution infrastructure).
- [ ] The player's OWN contracts appear in a "My Contracts" tab — private
      by default, shareable with a toggle per contract.

### 3. Story submission & aggregation

- [ ] After a significant deliberation event, the comms panel shows a
      "Share this story?" prompt. Opt-in. Player writes a one-line
      description. The contract ID, game event data (anonymized), and
      deliberation trace are attached. Submitted to the contract library
      server.
- [ ] Server aggregates: per-contract story count, outcome type distribution,
      average crew relationship delta. Published on the contract card.
- [ ] No ratings. No upvotes. Just "this contract produced these 47 stories."
      The stories themselves are the quality signal.
- [ ] Moderation: stories are text submitted by players. Apply S23's chat
      moderation patterns (length cap, rate limit, report mechanism).

### 4. Contract meta-game

- [ ] Crew trust bonus: a contract that has been in service for N sessions
      and produced M deliberation events accrues a "seasoned" bonus. Crew
      respond faster (shorter deliberation latency) and with more
      personality (system prompt gains a "history" section auto-filled from
      relationship memory).
- [ ] Contract evolution: after a major story event (crew member saves
      another's life, a crisis they caused, a faction turning hostile), the
      player can "evolve" the contract — small tweaks justified by the story.
      "After the Veil incident, Boris now prioritizes crew safety over mission
      objectives." The rule change is logged. The contract version increments.
      This is a story mechanic, not a balance mechanic.

### 5. Metrics collection

- [ ] `contract_library_events` table: creation, import, simulation run, share,
      story submission. No PII — structural data.
- [ ] Research questions: what contract patterns have the highest
      deliberation rate? What personality tags correlate with "most
      interesting" outcomes? What percentage of players share contracts
      vs keep them private? What triggers the most story submissions?

## Acceptance gates

```
cargo test -p reachlock-core contract::metadata contract::validation::
# contract validation produces correct crafting warnings
# contract metadata round-trips through serialization
make check
```

Manual: write a contract for a new engineer crew member → simulation shows
75% rule coverage → publish → friend imports it → friend's ship log shows
the contract firing with the same persona → friend submits a story about
their engineer saving the ship → your contract card shows "4 stories."

## Non-goals

- Contract rating/ranking system. No stars. No leaderboard. Stories are the
  signal.
- Contract marketplace with in-game currency. Sharing is free.
- "Optimal" contract detection. The system warns about structural issues
  (never-reaches-LLM, always-needs-LLM) but never suggests tactical
  improvements. The craft is the player's.
- Contract scripting language. Rules use the existing condition/action DSL
  from S06. No embedded Lua/JS/WASM.
- Version control / forking / merge for contracts. Simple import/export +
  version increment on evolve. Git for contracts is Phase 4.

## Gotchas

- The "most interesting" sort metric is computed from: deliberation rate
  (how often the contract hits the LLM edge) × outcome variance (how
  different the outcomes are across events). It's NOT a quality metric —
  it's an entertainment metric. Document this clearly in the UI.
- Story submission is a moderation surface. Every story is player-written
  text. The server MUST apply the same length cap, rate limit, and report
  mechanism as S23's chat system. Add a profanity filter if one exists.
- Contract import from untrusted sources is a security surface. A contract
  `.ron` file is deserialized into a `Contract` struct. The deserialization
  is safe (serde + ron), but the contract's system prompt is injected into
  the LLM context. A malicious contract could contain a prompt injection
  payload. Mitigate: strip known injection patterns, cap system prompt
  length (already in S15), and the LLM proxy's role isolation (S14) ensures
  the LLM can only produce contract actions, never game commands.
- The "evolve" mechanic requires the contract to be mutable. The original
  contract is versioned; the evolved contract is a new version. The old
  version's stories don't transfer to the new version — they're part of
  the old version's history. This is intentional. The story of the
  evolution IS the story of the crew member's change.
