# S113 — Determinism: Float & Seed Fixes (H5, H6, M8)

**Wave: Hotfix · Depends on:** None (standalone fixes in core and client)

## Outcome

Three determinism/iron-rule violations fixed:
- **H5**: Dilemma seed uses `Timer::elapsed_secs() as u64` — replaced with game tick
- **H6**: Majority vote uses `f64` comparison — replaced with integer math
- **M8**: `DeliberationStage` uses `f32` for timing — converted to `u32` centiseconds

## Fix 1 — H5: Dilemma Seed Non-Determinism

**File:** `reachlock-client/src/systems/dilemma.rs` line 53

**Current (buggy):**
```rust
let seed = location
    .system_seed
    .wrapping_add(cooldown.0.elapsed_secs() as u64);
```

`Timer::elapsed_secs()` returns `f32`. Casting `f32` → `u64` is non-deterministic: the same wall-clock time can produce different integer values depending on floating-point precision, rounding mode, and frame timing. Iron rule #2 prohibits floats in gameplay values.

**Fix:** Use the game tick from `UniverseTicker`:

1. Add `ticker: Res<UniverseTicker>` to the system parameter list
2. Replace with:

```rust
let seed = location
    .system_seed
    .wrapping_add(ticker.state.tick_no);
```

**Full system signature change:**

Before:
```rust
pub fn dilemma_trigger_system(
    time: Res<Time>,
    mut cooldown: ResMut<DilemmaCooldown>,
    location: Res<CurrentLocation>,
    mut active: ResMut<ActiveDilemma>,
    mut panel: ResMut<ActivePanel>,
) {
```

After:
```rust
pub fn dilemma_trigger_system(
    time: Res<Time>,
    mut cooldown: ResMut<DilemmaCooldown>,
    location: Res<CurrentLocation>,
    ticker: Res<UniverseTicker>,
    mut active: ResMut<ActiveDilemma>,
    mut panel: ResMut<ActivePanel>,
) {
```

Add import if needed: `use crate::systems::ticker::UniverseTicker;`

## Fix 2 — H6: f64 in Theater Majority Vote

**File:** `reachlock-core/src/contract/theater.rs` line 293

**Current (buggy):**
```rust
if for_action > against && for_action as f64 > self.participants.len() as f64 * 0.6 {
```

Both `for_action` and `self.participants.len()` are `usize`. Convert to integer math: majority = more than 60% of participants. Multiply both sides by 5 to avoid fractions:
- `for_action > len * 0.6` ⇔ `for_action * 5 > len * 3`

**Fix:**
```rust
if for_action > against && for_action * 5 > self.participants.len() * 3 {
```

No overflow concern: `len()` is number of crew members (≤ ~20). `for_action * 5` is at most 100. Safe.

## Fix 3 — M8: f32 in DeliberationStage

**File:** `reachlock-core/src/contract/stage.rs` lines 49-50

**Current (buggy):**
```rust
    pub remaining_secs: f32,
    pub total_secs: f32,
```

`DeliberationStage` is in `reachlock-core` and holds `f32` fields. Iron rule #2 ("No floats in gameplay values") prohibits floats in core. Convert to centiseconds (1/100 s) as `u32`.

**Fix:**

```rust
    pub remaining_cs: u32,
    pub total_cs: u32,
```

### Update all readers of these fields

Find every use of `remaining_secs` and `total_secs`:

1. **`deliberation_renderer.rs`** — replaces `stage.remaining_secs` with `stage.remaining_cs as f32 / 100.0` for display
2. **`onboarding.rs`** — `demo_deliberation_stage()` sets `total_secs: 4.0` → `total_cs: 400`
3. **Any other file** referencing these fields

### Update `demo_deliberation_stage()` in onboarding.rs

**File:** `reachlock-client/src/systems/onboarding.rs` ~line 130

Find:
```rust
        remaining_secs: 0.0,
        total_secs: 4.0,
```

Replace with:
```rust
        remaining_cs: 0,
        total_cs: 400,
```

### Update deliberation renderer display

**File:** `reachlock-client/src/systems/deliberation_renderer.rs`

Find the line that reads `stage.remaining_secs` (in `render_deliberation_panel` or `advance_deliberation_stage`):

Replace `stage.remaining_secs` with `stage.remaining_cs as f32 / 100.0` where display is needed.

### Search for all references

Run before committing:
```bash
cd /home/c/git/chezgoulet/reachlock && rg "remaining_secs|total_secs" --include '*.rs'
```

Fix every match. None should remain except in documentation or test golden files.

### Determinism check

If `DeliberationStage` is part of the determinism manifest (check `determinism.rs`), **regenerate goldens**:

```bash
cargo run -p reachlock-cli -- determinism emit
```

Compare the output to the existing golden files and update.

## Acceptance gates

```bash
# H5: Dilemma seed uses game tick
cargo test -p reachlock-client dilemma
# Verify: same tick number → same seed → same dilemma generated

# H6: Theater vote uses integer math
cargo test -p reachlock-core
# Existing theater tests must still pass

# M8: DeliberationStage uses centiseconds
cargo test -p reachlock-core
# Determinism gate: if manifest changed, recapture goldens
cargo run -p reachlock-cli -- determinism check

cargo clippy --workspace --all-targets -- -D warnings
make check
```

## Non-goals

- Changing the cooldown timer mechanism (the cooldown still uses `Timer` for wall-time gating — that's fine, it's not a gameplay value)
- Converting ALL `f32` in the codebase (only these three items)
- Eliminating all `Timer` usage (Timer is a Bevy render-layer type, acceptable in client systems)

## Gotchas

- **H5: The dilemma system `dilemma_trigger_system` must import `UniverseTicker`.** Check `systems/mod.rs` — it's already exported as `pub use ticker::UniverseTicker;`.
- **H6: The condition `for_action > against` remains as-is.** This is a simple count comparison (integers). Only the `f64` part is removed.
- **M8: Serialization format changes from `f32` to `u32`.** Existing RON/JSON files with `remaining_secs: 0.0` will fail to deserialize. `DeliberationStage` is not persisted to disk — it's a transient struct built by the deliberation engine and consumed by the renderer. Verify this by checking save file format (`SaveFile` struct in `inventory.rs`). If `DeliberationStage` appears nowhere in a serialized `SaveFile`, no migration needed.
- **M8: Centisecond → seconds conversion for display.** Show `{remaining_cs / 100}.{remaining_cs % 100:02}s` for human-readable timing. Rounding from centiseconds is acceptable for display.
- **M8: The `total_secs` 4.0 in onboarding becomes `total_cs: 400`.** This is exact: `4.0 * 100 = 400`.
