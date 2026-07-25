# S83 — Captain's Log & Consequence Memory

**Spec:** §16 (dialogue UX), §18 (LLM agency) · **Wave B (the systems you already paid for)** · **Depends on:** S81 (content dispatch — `ContentDispatcher` must exist before new client systems can hook into session capture)

**Closes: D5, P3** — Captain's Log exists in core (S37) with types, key moment detection, significance scoring, and log generation. It has exactly zero references outside `reachlock-core` and zero determinism goldens. That gets fixed here.

## Outcome

`detect_key_moments` / `score_significance` / `generate_log_entry` are wired into the live game loop. As the player flies, fights, deliberates, and chooses, visible moments accumulate in a session buffer. When they open the log panel, those moments appear as narrated entries — most recent first. Each entry can be regenerated in a different narrator voice or exported as plain text. The system is under the determinism gate: a fixed seed and fixed event sequence produce an identical `LogEntry`.

## Context

- S37 defined every type, wrote every pure function, and tested them. The code at `agency/log.rs` and `agency/log_generation.rs` is complete and unit-tested. It has never run in a game.
- D5 (MASTER-PLAN.md): "Captain's Log dark and outside the determinism gate." Zero goldens means a refactor of `LogEntry` or `NarratorVoice` would silently change the template output with no CI catch.
- P3: "The world simulates but never remembers the player." The log is the player's memory — a persistent artifact that accumulates across sessions.
- The `LogSession` captures events as `Vec<LoggableEvent>`. This sprint wires the producers: combat outcomes, deliberation completions, dilemma resolutions, discoveries, faction milestones, crew relationship changes. Each produces a `LoggableEvent` pushed to the session buffer.
- Offline-first: the log captures during local play. Events are stored in the session buffer. Generation uses the template fallback when no LLM proxy is available. Online adds the LLM generation path.
- No new types in core — S37's types are the frozen surface. This sprint touches only the client and determinism manifest.

## Freeze first

No new types in core. S37's `LogSession`, `LogEntry`, `LogMoment`, `LogMomentType`, `NarratorVoice`, `LoggableEvent`, `RelationshipDelta`, `LogGenerationRequest`, and `LogGenError` are the frozen surface. Any change to these is a wire-shape revision (iron rule #4 — pinned format).

### LogCapture resource (`client/src/systems/log_capture.rs`)

```rust
#[derive(Resource)]
pub struct LogCapture {
    pub session_id: String,
    pub start_tick: u64,
    pub raw_events: Vec<LoggableEvent>,
    pub relationship_changes: Vec<RelationshipDelta>,
}

impl LogCapture {
    pub fn push_event(&mut self, event: LoggableEvent);
    pub fn push_relationship_delta(&mut self, delta: RelationshipDelta);
    pub fn flush(&mut self) -> LogSession;
}
```

Initialized at game start. Pushed to by capturing systems. `flush()` produces a `LogSession` and clears the buffer for a new session.

### LogViewer resource + marker (`client/src/systems/log_ui.rs`)

```rust
#[derive(Resource, Default)]
pub struct LogViewerVisible(pub bool);

#[derive(Resource, Default)]
pub struct LogEntries(pub Vec<LogEntry>);

/// Which entry index is currently selected / displayed.
#[derive(Resource, Default)]
pub struct LogSelection(pub Option<usize>);
```

Follows the same `ActivePanel` pattern as `culture_view` and the market.

## Deliverables

### 1. Log capture system (`client/src/systems/log_capture.rs`)

- [ ] `LogCapture` resource with `session_id`, `start_tick`, event/delta buffers. Initialized in `Startup` when entering `AppState::InGame`.
- [ ] Consumer systems that push events:
  - Combat outcome → `LoggableEvent { kind: "combat", crew_involved, summary, tick }`
  - Deliberation completion (visible state change) → `LoggableEvent { kind: "deliberation", … }`
  - Dilemma resolution → `LoggableEvent { kind: "dilemma", … }`
  - Discovery (new system, new species) → `LoggableEvent { kind: "discovery", … }`
  - Faction milestone (rep threshold crossed) → `LoggableEvent { kind: "faction_milestone", … }`
  - Crew relationship change → `RelationshipDelta` pushed separately
  - Player choice (manual override, key decision) → `LoggableEvent { kind: "choice", … }`
- [ ] On game exit (`OnExit(AppState::InGame)`): call `flush()`, run `detect_key_moments`, run `generate_log_entry` (template path or LLM), store the entry.
- [ ] On session start: load previous entries from the save file's log history. New entries are prepended to the `LogEntries` vec.
- [ ] `LogSession` serialized into the save file alongside crew state. Loaded on `load_save`, sealed on exit.
- [ ] Test: simulate a sequence of events → flush → verify correct `LogMoment` detection and a valid `LogEntry` produced.

### 2. Log entry generation — LLM + template (`client/src/systems/log_generation.rs` or inline LLM dispatch)

- [ ] On exit: events flushed, key moments detected → if LLM proxy is available, call `build_prompt_context` → dispatch LLM request → parse response into `LogEntry`. If unavailable, call `template_narrative` / `template_summary` for deterministic fallback.
- [ ] Style hints and narrator voice: default to `Captain` + `["concise"]`. Player can change in the log viewer (deliverable 3).
- [ ] Regeneration: player can regenerate any existing entry with a different narrator voice or style. Old entry kept as draft; player picks the canonical entry.
- [ ] Quota respect: log generation uses a separate rate-limit bucket (once per session). If quota exhausted, template fallback.

### 3. Log viewer UI (`client/src/systems/log_ui.rs`)

- [ ] `LogViewerVisible` toggle on dedicated keybinding `InputAction::OpenCaptainsLog`. Bound to `L` by default (same slot S31's `OpenShipLog` — reassign: ship log is now captain's log).
- [ ] Panel shows a scrollable list of entries, most recent first. Each entry card shows: title, narrator voice badge, date/session label, and the first 80 characters of narrative as a preview.
- [ ] Entry detail view: full narrative text. Syntax-highlighted key moments listed below with significance scores (transparency — player sees what the generator worked from). Summary stats: "Session: 3 combat, 1 dilemma, 2 discoveries. 7 relationship changes."
- [ ] Narrator voice selector: dropdown (Captain / ShipLog / CrewMember / Omniscient). Changing regenerates the currently viewed entry.
- [ ] Style selector: dropdown (Concise / Detailed / Dramatic / Technical / Personal). Changing regenerates.
- [ ] Approve/Reject: player approves an entry (marks it as canonical). Unapproved entries show a subtle indicator.
- [ ] Export: "Copy to clipboard" button (narrative text). "Save to file" button (saves to `export/captains_log/` as `.txt` with metadata header).
- [ ] Same `ActivePanel` keyboard navigation pattern as culture view and market.

### 4. Determinism goldens — log generation (`core/src/determinism.rs`)

- [ ] Add `log_generation` entry to the determinism manifest. Fixed seed + fixed `Vec<LoggableEvent>` + fixed `Vec<RelationshipDelta>` → fixed checksum of the resulting `LogEntry`.
- [ ] The golden sequence: 3 events (deliberation, dilemma, combat) + 2 relationship deltas (trust +50, trust -30). Assert identical `LogEntry` (title, narrative, narrator, model_used, approved flag).
- [ ] The template fallback (`template_narrative`) is the deterministic path. The LLM path is not under determinism (it depends on model response).
- [ ] Wire the determinism entry so a change to `score_significance`, `detect_key_moments`, `template_narrative`, or `build_title` produces a golden change that must be recaptured deliberately (iron rule #3).

**Gate:** `cargo test determinism::log_generation` — a fixed session produces a fixed `LogEntry`. Breaking the template text changes the checksum.

### 5. Export / share — text file

- [ ] Export to file: `export/captains_log/YYYY-MM-DD-HHMMSS.txt`. Format:
  ```
  == Captain's Log ==
  Session: {session_id}
  Narrator: {narrator_voice}
  Generated: {timestamp}
  Model: {model_used}
  Style: {style_hints}

  {narrative}

  ---
  Key moments: {moment_count}
  Events: {event_count}
  ```
- [ ] Export to clipboard: narrative text only (no metadata). Uses the `arboard` crate or a `wgpu` clipboard fallback (same pattern as S34's contract share).
- [ ] Share (server): optional — if the player opts in, push the entry to the shared log feed (same `shared_log_entries` table S37 defined). Server endpoint already exists from S37 design; wire the client call.

## Acceptance gates

```
cargo test -p reachlock-core determinism::log_generation
cargo test -p reachlock-client log_capture::
make check
```

Manual:
1. Launch game → fly to a system → engage combat → open captain's log (L) → see entry with combat moment
2. Complete a deliberation → open log → entry has the deliberation listed as a key moment
3. Change narrator to Omniscient → regenerate → narrative text changes
4. Export to clipboard → paste into text editor → correct content
5. Quit → relaunch → previous log entry is in the list (loaded from save)
6. Delete LLM key → play session → quit → log entry uses template fallback (no crash, no hang)

## Non-goals

- Session replay / full event timeline — the log is a narrative, not a debug player
- Video/audio log entries — text only
- Cross-player collaborative logs — post-MMO social
- Log translation / localization — English only
- Community log feed browser — S37 defined it but S37 was never wired. Community feed is a future sprint

## Gotchas

- The `LogCapture` flush on exit must happen BEFORE the save file is written. Register the capture system at `OnExit(AppState::InGame)` with a higher priority than the save system. The ordering chain is: flush → generate → save.
- S31's `OpenShipLog` InputAction binds to `L`. This sprint re-binds it to `OpenCaptainsLog`. Update the default keybind name and all references — the captain's log is what that key opens now. The old `OpenShipLog` variant can remain in the enum (deprecated) for settings-file backward compat.
- The determinism test for `log_generation` must use the template path, not the LLM path. The test constructs a `generate_log_entry` call with empty style hints — the template builder is the deterministic fallback. Document this: the LLM path is intentionally NOT under determinism.
- Log entries reference crew names, faction names, system names — all safe to export (seed-derived). Scrub other player names if any appear in shared events (privacy). The S37 shared-entry server endpoint already filters PII; the local export bypasses that filter entirely — warn the player before export if other-player data is present.
- The `LogEntries` resource is part of the save file. When `load_save` runs, it deserializes previous entries. The save format is a `.ron` file; large log histories could bloat the save. Cap history at 100 entries. Older entries are dropped with a `tracing::info!` message.
