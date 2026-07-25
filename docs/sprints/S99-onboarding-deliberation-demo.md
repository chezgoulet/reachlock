# S99 — Onboarding Deliberation Demo

**Wave: UX-Polish · Depends on:** S08 (Onboarding), S38 (Deliberation theater), S72 (Deliberation renderer)

## Outcome

Step 3 of the onboarding tutorial ("Watching Deliberation") actually renders a deliberation stage using the existing `deliberation_renderer` module. The placeholder text `"(Demonstration deliberation would render here)"` is replaced with a live (but static, timed) rendering of a deliberation panel showing crew member, mood, context, matched/unmatched rules, and a verdict.

## Context

The onboarding system (`onboarding.rs`) contains a fully constructed `DeliberationStage` for the demo:

```rust
fn demo_deliberation_stage() -> DeliberationStage {
    DeliberationStage {
        phase: StagePhase::Weighing,
        crew_member: "Boris".into(),
        crew_portrait_id: "boris_default".into(),
        crew_mood: "ANXIOUS".into(),
        trust_with_player: 256,
        context_summary: "Unknown signal detected — none of my rules cover it.".into(),
        matched_rules: vec![],
        unmatched_rules: vec![...],
        verdict: None,
        cost: None,
        recent_uncovered: 2,
        remaining_secs: 0.0,
        total_secs: 4.0,
    }
}
```

But `render_onboarding` only writes:

```rust
if step.demo_stage.is_some() {
    content.push_str("\n\n(Demonstration deliberation would render here)");
}
```

The `deliberation_renderer` module has a full `render_deliberation_panel` system that takes a `DeliberationPanel` resource and renders crew name, mood, rules table, verdict. This sprint pipes the demo stage into that renderer so the onboarding step actually shows deliberation.

### Key files

| File | Role |
|------|------|
| `reachlock-client/src/systems/onboarding.rs` | Onboarding flow — pipe demo stage into renderer |
| `reachlock-client/src/systems/deliberation_renderer.rs` | `render_deliberation_panel`, `DeliberationPanel` resource |
| `reachlock-client/src/main.rs` | System ordering between onboarding and deliberation renderer |

## Freeze first

### Demo stage lifecycle

The demo follows a timed sequence:

1. **t=0s:** Stage appears — `Phase::Examining` — Boris reads the situation
2. **t=1.5s:** Phase transitions to `Weighing` — unmatched rules shown, Boris deliberates
3. **t=3.5s:** Phase transitions to `Verdict` — `"Boris: I don't have a procedure for this. Recommend we run silent and listen."`
4. **t=5.0s:** Demo ends — the player presses Enter/Tab to continue to Step 4

The demo uses a local `Timer` (not the full deliberation engine), making it a purely visual sequence.

### Resource for demo state

```rust
/// Active during onboarding Step 3. Holds the demo stage and phase timer.
#[derive(Resource)]
pub struct OnboardingDemo {
    pub stage: DeliberationStage,
    pub timer: Timer,
    pub phase_index: usize,
}
```

### Phase sequence

```rust
const DEMO_PHASES: [(f32, StagePhase, Option<&str>); 3] = [
    (1.5, StagePhase::Examining, None),
    (3.5, StagePhase::Weighing, None),
    (5.0, StagePhase::Verdict, Some("run silent and listen")),
];
```

## Deliverables

### 1. Create `OnboardingDemo` resource and phase timer

- [ ] Add to `onboarding.rs`:
```rust
#[derive(Resource)]
pub struct OnboardingDemo {
    pub stage: DeliberationStage,
    pub timer: Timer,
    pub phase_index: usize,
}
```
- [ ] Timer mode: `TimerMode::Once` at each phase boundary

### 2. Initialize demo on Step 3 entry

- [ ] In `onboarding.rs`: when `advance_onboarding` moves to step 2 (0-indexed), and `demo_deliberation_stage()` returns `Some`
- [ ] Create `OnboardingDemo` resource with `phase_index = 0`, timer at 1.5s

### 3. Implement demo phase tick

- [ ] New system `tick_onboarding_demo`:
  - Reads `OnboardingDemo` resource
  - Ticks the timer
  - When timer finishes:
    - Advance to next phase
    - Update `stage.phase` and `stage.verdict`
    - If `phase_index >= DEMO_PHASES.len()`, the sequence is done
- [ ] Wire `DEMO_PHASES` to update `stage.phase` and optionally `stage.verdict`

### 4. Route demo stage into deliberation renderer

- [ ] In `render_onboarding`:
  - If demo resource exists and `rendering_demo` flag is set:
    - Set `DeliberationPanel.stage = Some(demo.stage.clone())`
    - The `render_deliberation_panel` system (already registered) picks it up
- [ ] OR: directly set `DeliberationPanel` resource from the onboarding system, then restore it after the demo ends

### 5. Despawn demo on step advance or onboarding close

- [ ] When player advances past Step 3 (to Step 4 or Esc): remove `OnboardingDemo` resource
- [ ] Clear `DeliberationPanel.stage` after demo ends
- [ ] Remove the `"(Demonstration deliberation would render here)"` placeholder text

### 6. Test

- [ ] Verify demo shows on Step 3
- [ ] Verify phase transitions happen at timed intervals
- [ ] Verify verdict text appears
- [ ] Verify Enter/Tab advances past the demo step

## Acceptance gates

```bash
cargo clippy -p reachlock-client -- -D warnings

# Manual:
# 1. Delete save/onboarding_completed.flag
# 2. Launch game → onboarding starts
# 3. Advance to Step 3 → deliberation panel renders with Boris, rules, mood
# 4. Wait ~5s → phases cycle: Examining → Weighing → Verdict
# 5. Press Enter → advance to Step 4 → deliberation panel disappears

make check
```

## Non-goals

- Interactive deliberation (this is a playback demo)
- Real LLM calls during onboarding
- Different demo for different origins
- Crew conference demo (solo deliberation only)
- Rendered sprites/portraits for the demo (text-based only)

## Gotchas

- **The `DeliberationPanel` resource is already used by the live game.** Setting it from onboarding must not conflict with real deliberation. Solution: set it when `OnboardingState.active && step.demo_stage.is_some()`; the real deliberation engine is gated behind `in_state(AppState::InGame)` while onboarding happens during InGame.
- **System ordering.** `render_onboarding` and `render_deliberation_panel` both write to the same render target. Ensure `render_onboarding` runs BEFORE `render_deliberation_panel` so the demo stage is set before the renderer reads it. Use `.chain()` or `.before()`.
- **Timer reset.** The demo timer should NOT tick while the player is on the previous step. Only tick when the demo is active (Step 3).
- **Phase transition shouldn't skip.** `DEMO_PHASES` cumulatively defines when each phase starts: Examining at 0s, Weighing at 1.5s, Verdict at 3.5s. The timer duration for each phase is `next_start - current_start`, not the cumulative time.
- **The demo stage uses hardcoded crew member "Boris".** This is acceptable for a tutorial demo — it's meant to show the mechanic, not reflect the player's actual crew.
