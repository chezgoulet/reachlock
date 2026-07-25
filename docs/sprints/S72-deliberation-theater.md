# S72 — Deliberation Theater & Onboarding

**Spec:** §6 (contract system), §9 (game modes), §14 (HUD), §15 (agency), §18 (LLM outcome model) ·
**Wave C (make it legible, accessible, and fun) · Depends on:** S70 (client UI framework), S81 (content dispatch & contract pipeline)

**Closes findings:** C5, C6, C7, C8, P1

## Outcome

The contract engine — *the* novel mechanic — is no longer buried behind a console.
Every deliberation is a visible, staged event: the player sees what's being weighed,
which of *their* rules matched (and which ran out), the crew member's current mood
and history, the verdict animation, and the cost. The player can interject at a
relationship cost (`co_deliberation.rs`). After each resolution, a "your auto-helm
left N gaps this cycle" counter surfaces `recent_uncovered` so rule-writing becomes
a skill with feedback.

The first run includes an onboarding sequence that teaches contract rules,
deliberation, and consequences. Every HUD element has a contextual hint. Damage and
threats are ordered by severity with distinct visual treatment. Mode transitions
(GameMode beats like Docking, Landed, Hyperspace) animate instead of swapping text.
A diegetic Help mode highlights interactable elements and shows their keybind.

The dead-code `deliberation_renderer.rs` is rewritten into a working visible
deliberation state system that replaces the single-line grey text.

## Context

- **C5 — `show_tutorial_hints` is dead.** The settings field exists at
  `settings.rs:133`, togglable from the settings UI at `settings_ui.rs:434`,
  but no system reads it. No tutorial, no tooltips, no contextual hints exist
  anywhere in the client. The player learns nothing.
- **C6 — Deliberation is one line of grey text** (`hud.rs:291-301`):
  `"⟳ Boris is considering the situation…"`. The signature moment of ReachLock
  reads as a log line. There is no panel, no crew portrait, no mood, no matched
  rules, no verdict animation, no cost display.
- **C7 — Mode transitions are text swaps** (`hud.rs:282-284`):
  `"DOCKING…"`, `"UNDOCKING…"`, `"HYPERSPACE…"`. These are raw `GameMode`
  enum names rendered as location-banner text with no animation, no easing
  curve, no sound, no camera movement.
- **C8 — No damage/threat feedback hierarchy.** State competes as equal text
  in a single line (`hud.rs:256`): fuel percentage, hull percentage, breach
  indicator, speed — all the same colour, same weight. A 2% fuel shortage
  and a hull breach look equally important.
- **P1 — The contract engine is buried behind a console.** The game's one
  novel mechanic — "you write the rules your ship runs on" — is invoked
  through a text string that only appears when the auto-helm has no covering
  rule. The crafting workshop (S34), contract editor, and library (S81) now
  exist, but the runtime's *output* is invisible to the player.
- The current `deliberation_renderer.rs` has dead `DeliberationStatus`/
  `DeliberationTrack` types marked `#[expect(dead_code)]` and a render loop
  that just sets the same single-line text. This sprint replaces it entirely.
- `recent_uncovered` (`contract.rs:86`) tracks the rolling count of evaluations
  where no rule matched. It feeds `agency::contract_quality_modifier` — the
  "write tighter rules" lever — but is never exposed. This sprint surfaces it.
- `co_deliberation.rs` fully models `player_override()` with relationship
  deltas (trust gain/loss, tension) — the player interjection mechanic is
  ready in core. The client just needs the UI to invoke it.
- `agency/log.rs` has `detect_key_moments` with `LogMomentType::CrewDeliberation`
  — wired in S83, but the client-side deliberation renderer can emit events
  that the log picks up.
- S70 provides the focus/input stack, panel z-order, and widget kit this
  sprint builds on. S81 provides the contract pipeline (multiple contracts,
  install paths). This sprint consumes both.

## Freeze first

### DeliberationStage (`core/src/contract/stage.rs` or extend `types.rs`)

The deliberation renderer needs a structured representation of what the
player sees — not just the raw `DeliberationState` timer, but the staged
presentation data. New type in core (pure, no rendering deps):

```rust
/// The staged presentation of a deliberation moment. Constructed by the
/// client bridge from the contract engine's output; consumed by the
/// deliberation panel UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliberationStage {
    pub phase: StagePhase,
    /// The crew member deliberating.
    pub crew_member: String,
    pub crew_portrait_id: String,
    /// The crew member's current mood (from SoulFile mood bridge).
    pub crew_mood: String,
    /// The player's trust with this crew member (from SoulFile).
    pub trust_with_player: i64,
    /// What situation triggered deliberation.
    pub context_summary: String,
    /// Which rules from the contract DID match (rule index, label, action).
    pub matched_rules: Vec<RuleSnapshot>,
    /// Which rules were checked and DID NOT match (so the player sees gaps).
    pub unmatched_rules: Vec<RuleSnapshot>,
    /// The verdict, once the phase reaches Verdict.
    pub verdict: Option<VerdictSnapshot>,
    /// Relationship consequences of the verdict.
    pub cost: Option<CostSnapshot>,
    /// Rolling count of uncovered evaluations from ContractRuntime.
    pub recent_uncovered: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StagePhase {
    /// "Boris is examining the situation…"
    Examining,
    /// "Boris checked the rules: fuel_warning ✓, maintain_course ✓, unknown_signal ✗"
    Weighing,
    /// "Boris is deciding…" (animation, timer)
    Deciding,
    /// Verdict delivered.
    Verdict,
    /// Deliberation completed, showing cost + summary.
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSnapshot {
    pub index: usize,
    pub label: String,
    pub action: String,
    pub condition_summary: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerdictSnapshot {
    pub outcome_label: String,   // "success", "misinterpretation", etc.
    pub action_taken: String,
    pub reasoning: String,
    pub escalation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSnapshot {
    pub relationship_deltas: Vec<(String, i64)>,
    pub hull_damage: Option<i64>,
    pub cargo_loss: Option<Vec<String>>,
}
```

### TutorialHint (`core/src/tutorial.rs` or `client/src/help.rs`)

A data-driven hint definition — not hardcoded strings scattered in systems.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorialHint {
    pub hint_id: String,
    pub trigger: HintTrigger,
    pub title: String,
    pub body: String,
    pub position: HintPosition,
    pub dismiss_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HintTrigger {
    FirstRun,
    FirstDeliberation,
    FirstDocking,
    FirstUndocking,
    FirstContractEdit,
    FirstUncoveredGap { threshold: u8 },
    GameModeEntered(GameModeData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameModeData {
    pub mode: String,
    pub transition_animation: String,
}
```

Wire tests: `DeliberationStage` serializes round-trip through JSON (matches
the contract engine's JSON transport). `StagePhase` advances in order, never
skips. At least one `TutorialHint` exists for each `HintTrigger` variant
(excluding test-only variants).

## Deliverables

### 1. Deliberation panel (`reachlock-client/src/systems/deliberation_panel.rs`)

Rewrites `deliberation_renderer.rs` entirely (the old file is deleted, not
patched — the dead-code types are replaced by the panel system).

- [ ] `DeliberationPanel` resource: owns the current `DeliberationStage`,
      advances `StagePhase` on a timer (Examining → Weighing → Deciding →
      Verdict → Complete), and manages the panel's visible state. Panel
      spawns when `deliberation.active` flips to `Some` and despawns when
      Complete is dismissed.
- [ ] **Examining phase:** shows the crew member's portrait (from
      `crew_portrait_id`, resolved through S70's image pipeline), their name,
      their current mood as a coloured badge (e.g. "ANXIOUS" in yellow,
      "RESOLUTE" in green), and a one-line description of the situation:
      `"Unknown signal detected — none of my rules cover it."`
- [ ] **Weighing phase:** a split list. Left side: "RULES THAT MATCHED" in
      green — each rule's label, action, and condition summary. Right side:
      "GAPS" in orange/red — rules that were checked and didn't match. This
      is the key feedback surface: the player sees *which* rules exhausted.
      Both lists collapse to a scrollable panel if the contract has many rules.
- [ ] **Deciding phase:** a pulsing "thinking" animation (not the old grey
      text pulse — use S70's animation primitives: a spinning symbol, or a
      crew-portrait shimmer). The remaining `Timer` bar is visible as a
      progress bar at the bottom of the panel: `"Boris has 3.2s to decide…"`.
- [ ] **Verdict phase:** the verdict card slides in (or fades, using S70's
      transition system). Shows: the outcome label (Success/Misinterpretation/
      Confabulation/etc.), the action taken, the reasoning text, and any
      escalation consequence. The outcome label is colour-coded per the
      `LlmOutcome` table (green → Success, yellow → Misinterpretation,
      red → Catastrophic, etc.).
- [ ] **Cost phase:** shows relationship deltas as coloured badges per crew
      member: `"Boris (+trust) · Tove (-trust)"`. If there's hull damage or
      cargo loss, those appear as red badges with a damage icon. The panel
      stays visible until dismissed (Enter/Space/click) or auto-dismisses
      after 5s. The `recent_uncovered` counter appears at the bottom:
      `"Your rules left 4 gaps this cycle. Write tighter rules to improve
      deliberation odds."`
- [ ] Wire the panel to `DeliberationState` and `ContractRuntime.recent_uncovered`.
      Read both; advance `DeliberationStage` through phases; log the result
      via `ShipLog::log` (for the ship log, S83).
- [ ] Delete `deliberation_renderer.rs` entirely. Remove `DeliberationUi`,
      `DeliberationRenderState`, `DeliberationStatus`, `DeliberationTrack`.
      The `spawn_deliberation_ui` in HUD (`hud.rs:115-129`) is replaced by
      the panel spawn; the `DeliberationOverlay` text component is removed
      from `spawn_hud`.

### 2. Player interjection during deliberation

- [ ] During the Deciding or Verdict phase, the deliberation panel shows an
      interjection prompt: `"[Tab] Interject — make the call yourself"`.
      Pressing Tab opens a sub-panel with a text input (or action picker,
      for keyboard-only) where the player types or selects the action they
      want to override with.
- [ ] On submission, calls `CoDeliberation::player_override(action)` (or the
      equivalent direct path when no full co-deliberation is running —
      single-crew deliberation also needs an override). The relationship
      deltas from `player_override` are applied to the soul save and shown
      in the Cost phase.
- [ ] The interjection cost is visualised: a warning panel shows which crew
      members will lose trust if the player overrides, before the player
      confirms. `"Boris will lose trust (overruled). Tove will gain trust
      (agreed with your call). Confirm?"`
- [ ] If the player does NOT interject, the deliberation runs to its natural
      resolution (existing timer + outcome path). The panel shows a subtle
      note: `"The captain let the crew decide — small trust gain."`

### 3. Recent uncovered counter (`recent_uncovered` surface)

- [ ] After every deliberation completes, the HUD shows a flash counter for
      3 seconds: `"Your auto-helm left 4 gaps this cycle."` The counter
      value is `ContractRuntime.recent_uncovered`. Position: top-right of the
      HUD, near the OfflineBadge, in a warm yellow colour.
- [ ] The counter accumulates across deliberations — it does not reset every
      resolution. It only resets when the contract is edited (installed,
      rules changed) or when `recent_uncovered` naturally decays (the
      existing `saturating_sub(1)` in `contract.rs:210` on rule match).
- [ ] A dedicated tooltip on the counter explains the mechanic:
      `"Uncovered evaluations = situations your rules didn't cover. Write
      more rules to reduce this number and improve your crew's deliberation
      odds."` (From S70's tooltip system).
- [ ] The counter is also shown in the deliberation panel's Cost phase
      (deliverable 1 already includes this).

### 4. Onboarding sequence (`reachlock-client/src/systems/onboarding.rs`)

- [ ] **First-run detection:** check for `save/settings.ron` or a dedicated
      `save/onboarding_completed.flag`. On first run, the onboarding flag
      is false. After completion, write the flag.
- [ ] **Onboarding steps** (linear sequence, step-forward with Enter/Tab):
      - Step 1: "Welcome to ReachLock. This is your ship's auto-helm
        console. The rules you write here control your ship when you're not
        at the controls."
      - Step 2: "Each rule says: IF [condition] THEN [action]. Your crew
        follows these rules. If no rule covers a situation, the crew
        deliberates." (Shows a mock contract panel with two example rules.)
      - Step 3: "Watch what happens when a situation isn't covered."
        (Triggers a simulated deliberation — the same flow as deliverable 1,
        but scripted: `unknown_signal` detected, rules checked, Boris
        deliberates, verdict delivered.)
      - Step 4: "You can interject at any time. The crew reacts. Override
        too often and they stop trusting your judgment. Let them decide and
        they grow." (Shows the interjection UI.)
      - Step 5: "Your rules left N gaps this cycle. Write tighter rules and
        your crew deliberates less, decides faster, and trusts your vision."
        (Mock counter display.)
      - Step 6: "That's the core loop. Write rules. Watch them run. Fill
        the gaps. You're in command." → Dismiss to enter the real game.
- [ ] The onboarding overlay is a full-screen panel (using S70's overlay
      system) that pauses the game underneath. The player cannot interact
      with the game world during onboarding.
- [ ] Onboarding can be replayed from the settings menu (the existing
      `GameplaySettings.show_tutorial_hints` toggles a "Replay tutorial"
      button in the settings UI).
- [ ] `show_tutorial_hints` is now read by the onboarding system. When true
      (and first-run flag is true, or replay is triggered), the onboarding
      runs. When false, the onboarding is skipped and the game starts
      directly.

### 5. Contextual hints (`reachlock-client/src/systems/hints.rs`)

- [ ] `HintSystem` resource: a registry of `TutorialHint` definitions (from
      the freeze-first schema), keyed by `HintTrigger`. Initialised from
      authored data (or hardcoded defaults for launch).
- [ ] Each HUD element that has ambiguous meaning gets a registered hint:
      - Fuel gauge: "Current fuel level. When fuel drops below 15%, Boris
        will warn you. At 0%, the ship drifts."
      - Hull integrity: "Your ship's structural health. Below 30%, systems
        start failing. At 0%, hull breach."
      - Speed indicator: "Current velocity relative to nearby objects.
        Use W/S to adjust."
      - Deliberation panel: "Your crew member is deciding. Watch their
        reasoning and the rules that run out."
      - Rule-matched/gaps list: "Green rules matched the situation. Red
        gaps are conditions your rules didn't cover."
      - Interjection prompt: "Override the deliberation. Quick way to take
        command — but costs trust."
- [ ] Hints are toggled by the `show_tutorial_hints` setting. When on,
      hovering (mouse) or focusing (keyboard tab-to) an element shows a
      tooltip with the hint text. When off, no tooltips.
- [ ] Hint text is the S70 tooltip system, rendered above/below the element
      with a subtle background box. No animation — just appear/disappear on
      focus change.

### 6. Feedback hierarchy (`reachlock-client/src/systems/hud.rs` refactor)

- [ ] Redesign the HUD status line (`hud.rs:250-262`). Instead of one line
      with all values at equal weight, create a hierarchically structured
      HUD:
      - **Top priority** (red/orange): hull breach, active fire, imminent
        collision, catastrophic failure. These flash, pulse, or use S70's
        alert widget. Only these appear in the topmost priority slot.
      - **Medium priority** (yellow): low fuel, moderate hull damage, nearby
        hostiles, cargo bay door open. Second line, coloured yellow.
      - **Normal priority** (white/grey): speed, fuel %, hull %, system id.
        Normal HUD data in a compact line.
- [ ] The hierarchy is data-driven: `ShipSystems` gets a `fn threats() -> Vec<Threat>`
      that returns threats sorted by severity. The HUD renders the top 3
      only, preventing the screen from being a wall of text.
- [ ] Damage/threat indicators use glyphs (• ▲ ■) and colour, not colour
      alone (accessibility — C9 is S71, but this sprint doesn't introduce
      new colour-only states). Each threat type has a dedicated glyph.

### 7. Mode transitions (animated GameMode beats)

- [ ] Replace the text-swap mode labels (`hud.rs:282-284`: `"DOCKING…"`,
      `"UNDOCKING…"`, `"HYPERSPACE…"`) with animated transitions.
- [ ] **Docking:** the space scene smoothly scrolls the station into centre
      view (1.5s ease-in-out camera pan, using S70's tween system). The
      HUD shows a subtle banner: `"DOCKING WITH KESSEL STATION"` — animated
      slide-in from top, hold 1s, slide-out.
- [ ] **Undocking:** reverse animation: camera pull-back from station, banner
      `"UNDOCKING — CLEAR SPACE"`. 1.5s.
- [ ] **Hyperspace:** star-field stretch effect (the existing `hyperspace`
      module already has a visual; the text swap was redundant). Banner:
      `"JUMP TO AETHON — ETA 14m"` with a countdown animation.
- [ ] **Landed/OnBoard toggle:** no camera animation (the viewport switches
      scene), but a brief fade-to-black + fade-in (0.3s each) with a banner
      showing the location name.
- [ ] All transition durations are driven by a `TransitionTimer` resource,
      not hardcoded. The timer advances in the `tick_deliberation` or
      equivalent system. `Paused` transitions are instant (no beat for
      opening the pause menu).
- [ ] The `ModeScope` teardown already handles entity despawning; the
      animation layer must finish before the teardown runs. Wire the
      transition system as an `OnEnter` system that runs BEFORE the
      teardown (add ordering in the schedule: `TransitionAnimation →
      DespawnPreviousScene → SpawnNewScene`).
- [ ] Accessibility: if `settings.accessibility.reduce_motion` is true (S71),
      all transition animations are instant (0s duration, just the banner
      flash). The existing S71 gate ensures `reduce_motion` is a consumed
      setting.

### 8. Diegetic help (`reachlock-client/src/systems/help.rs`)

- [ ] A dedicated "Help" mode activated by pressing `F1` (or the configured
      `InputAction::OpenHelp` — add this variant to `InputAction`). When
      Help mode is active:
      - All interactable elements in the current view (docking ports,
        consoles, airlocks, cargo hatches, crew members) are highlighted
        with a subtle coloured outline/glow.
      - Next to each highlighted element, a small label shows its keybind:
        `[E] Activate`, `[Tab] Open panel`, `[F] Toggle`.
      - The current HUD help text at `hud.rs:326-335` (the 12px grey line)
        is hidden during Help mode — it's replaced by the inline labels.
- [ ] Help mode is diegetic: the labels render as in-world UI (not a separate
      panel). They use the same glyph system as S70's tooltip layer, spawned
      as `ModeScope` entities so they clean up on mode exit.
- [ ] Exiting Help mode (F1 again, or Esc) despawns all label entities and
      restores the normal HUD help text.
- [ ] Help mode is available in every GameMode. Different elements highlight
      per mode: SpaceFlight shows weapons, thrusters, scanner; Landed shows
      NPCs, shops, airlocks; OnBoard shows consoles, cryo pods, cargo.
- [ ] The keybind displayed on each label comes from the settings keybind
      registry (`settings.controls.keybinds`) — respects player rebinding
      (S31). If an action has no key bound, the label shows `[—] Unbound`.
- [ ] The 12px HUD help text (`HelpText` entity in `spawn_hud`) is reduced
      to a single line: `"Press F1 for help"` (or the binding for Help).
      The old multi-line help dump is deleted. This is the diegetic
      replacement the finding asked for.

### 9. Deliberation renderer rewrite (delete + replace)

- [ ] Delete `reachlock-client/src/systems/deliberation_renderer.rs` entirely.
- [ ] Remove `mod deliberation_renderer` from the client's `systems/mod.rs`.
- [ ] Remove `render_deliberation`, `cleanup_completed_deliberations`,
      `spawn_deliberation_ui` from the app builder in `main.rs`.
- [ ] Add the new deliberation panel system (deliverable 1) as a replacement.
      The four systems in the new module are:
      `advance_deliberation_stage` (advances the phase timer),
      `spawn_deliberation_panel` (OnEnter Deliberation),
      `despawn_deliberation_panel` (OnExit Deliberation),
      `handle_interjection_input` (reads Tab, processes override).
- [ ] The dead `DeliberationStatus`/`DeliberationTrack` types and their
      `#[expect(dead_code)]` annotations are gone. The new code is not
      dead — it is wired into the schedule at the right priority.

## Acceptance gates

```
cargo test -p reachlock-core contract::stage::   # DeliberationStage round-trip, phase ordering
cargo test -p reachlock-client deliberation::  # panel lifecycle, stage transitions
cargo test -p reachlock-core tutorial::         # HintTrigger definitions exist for every variant
make check
```

Manual:

1. **Deliberation panel:** fly the ship until the unknown signal triggers
   deliberation → see the panel appear with Boris's portrait, mood badge
   → watch it advance through Examining → Weighing (2 matched, 1 gap) →
   Deciding (timer bar) → Verdict (outcome with colour) → Cost (relationship
   deltas, "4 gaps this cycle" counter). Press Enter/click to dismiss.

2. **Player interjection:** during deliberation, press Tab → choose an action
   → confirm → see the Cost phase show "Boris lost trust (overruled)" and
   "Tove gained trust (agreed)". Crew relationship saves reflect the change.

3. **Onboarding:** delete `save/onboarding_completed.flag` → launch game →
   see the 6-step onboarding sequence. Step through with Enter. After step 6,
   the game starts normally. The flag file exists. Replay from settings.

4. **Contextual hints:** `show_tutorial_hints = true` → hover/focus-tab to
   the fuel gauge → see tooltip: "Current fuel level…". Toggle the setting
   to false → tooltips disappear.

5. **Feedback hierarchy:** take hull damage below 50% → see the HUD flash a
   red "HULL STRESS" alert in the top priority slot, while speed and fuel
   stay in the normal slot. Take a hull breach → "⚠ BREACH" pulses red.
   Fuel at 8% → yellow "LOW FUEL" in the medium slot.

6. **Mode transitions:** fly to a station and start docking → see the camera
   pan to the station (1.5s) with a slide-in banner. Undock → reverse.
   Gate jump → star-field stretch + banner. Set `reduce_motion = true` →
   all transitions are instant.

7. **Diegetic help:** press F1 → see all interactable elements glow with key
   labels (`[E] Activate`, `[F] Scanner`, `[L] Ship Log`). Press F1 again →
   labels disappear. Rebind `Activate` to `G` in settings → press F1 → see
   `[G] Activate`. Unbind it → see `[—] Unbound`.

8. **No dead code:** the old `deliberation_renderer.rs` is gone. The
   `#[expect(dead_code)]` on `DeliberationStatus`/`DeliberationTrack` is
   gone. `git diff --name-only` shows no reference to the deleted file.

## Non-goals

- Full crew co-deliberation theater (S38's sequential group deliberation with
  multiple speakers, reaction overlays, relationship bar, replay) — that is
  S38, which waits for S70+S81+S72 to land first. This sprint stages the
  *single-crew* deliberation that already exists, making it visible. Group
  theater is a superset built on this panel infrastructure.
- Voice synthesis for deliberation lines (S62).
- Captain's log integration (S83) — the panel logs to `ShipLog` in text,
  but the structured `LogMoment` integration is S83.
- Full tutorial suite (character creation, ship editor, combat) — this
  sprint's onboarding covers only the contract/deliberation loop. Other
  tutorial sequences are separate sprints.
- Controller/gamepad for specific UI (S70 covers the input stack; gamepad
  navigation of the deliberation panel is S70's responsibility).
- Settings completeness enforcement gate (S71 owns this).

## Gotchas

- The `DeliberationStage` must be constructed from the contract engine's
  output, which means the stage builder (`DeliberationStage::from_outcome`)
  needs access to the matched/unmatched rule list. Contract engine's
  `evaluate()` currently returns only the *first* matching rule (descending
  priority). To show all evaluated rules (matched + unmatched), the stage
  builder needs either (a) a separate `evaluate_all` that runs every rule
  without short-circuiting, or (b) a modified `evaluate` that returns the
  full evaluation trace. Option (a) is cleaner: `fn evaluate_all(contract,
  ctx) -> Vec<RuleResult>` where `RuleResult` has `matched: bool`. Add this
  to `engine.rs` as a pure function. The deliberation panel calls it, not
  the existing `evaluate`. The existing `evaluate` stays for the runtime
  decision path; `evaluate_all` is a display-only function.
- The onboarding sequence triggers a *simulated* deliberation. The simulation
  must produce deterministic output (it's not a real LLM call). Write a
  `generate_demo_deliberation` helper that builds a `DeliberationStage` with
  hardcoded rules/outcome. The onboarding step function returns this stage
  to the panel, which renders identically to a real deliberation.
- Mode transition animations must NOT block the simulation tick. The camera
  animation runs in parallel with the sim. Use a `TransitionState` resource
  that the animation system reads and the mode transition writes. The
  `OnEnter(GameMode)` system sets `TransitionState::Animating { duration, elapsed }`.
  The animation system advances `elapsed` each frame and kicks to
  `TransitionState::Complete` when done. The scene-swap system checks
  `TransitionState::Complete` before despawning the old scene (under
  `reduce_motion`, instant-complete).
- The diegetic help labels must not interfere with click/hover on the
  underlying interactable elements. The labels render as a transparent
  overlay layer that absorbs no input (S70's layer system: `Interaction`
  is never set on the label entities). The player clicks through labels
  to the element underneath.
- The HUD help text (`HelpText` entity) is replaced by the diegetic labels,
  but only while Help mode is active. When Help mode is off, the entity
  still exists — it just shows a single line: `"F1 Help"`. Remove the old
  multi-line help strings (`cache.flight`, `cache.interior` from
  `HelpTextCache`) and the rebuild logic in `refresh_help_cache` — those
  strings were 12px grey keybind dumps that this sprint expressly replaces.
  The `HelpText` entity now shows only the single line; `HelpTextCache`
  can be simplified or removed (the keybind display function is still
  needed for the diegetic labels).
- `recent_uncovered` is a `u8` clamped at `min(8)` in `contract.rs:222`.
  The counter display format `"N gaps this cycle"` uses `u8::MAX` (which
  is 8 in practice) as the worst-case display. When `recent_uncovered` is
  0, the counter is not shown at all (no gaps = no feedback needed).
- The interjection prompt (`[Tab] Interject`) must only appear when the
  game is in online mode OR offline mode with a configured override. In
  offline mode, the `player_override()` call on `CoDeliberation` works
  identically — no LLM needed. The guard is: always show the prompt.
  The player can always interject. The cost (trust changes) applies
  regardless of online/offline mode.
- S70's tooltip system must support the hint registry. If S70's tooltip
  module doesn't exist yet (S70 is a parallel sprint), this sprint defines
  a minimal `HoverTooltip` component and wire it directly to the hint
  registry — the S70 can adopt it later. The tooltip component: spawning
  a `Text` entity adjacent to the target on focus, despawning on blur.
